//! `Mask`: WHICH source bytes survive a [`Filter`], as a range set that doubles
//! as the offset map back to the document.
//!
//! ```text
//! source   \v 1 Jesus wept.\f + \ft why\f* Then…
//!          ·····Jesus wept.················ Then…      · = dropped
//!
//! let m = mask(source, &tokens, &cst, &Filter::verse_text());
//! m.ranges          == [5..16, 31..39]
//! m.starts          == [0, 11]
//! m.text(source)    == "Jesus wept. Then…"
//! m.to_source(3)    == 8     // 5  + (3 − 0)      the "u" maps to the real "u"
//! m.to_source(13)   == 33    // 31 + (13 − 11)    past the note, still exact
//! m.from_source(20) == None  // inside the dropped \f — no home in this view
//! ```
//!
//! The map costs one `u32` per RANGE, not per byte, and the ranges are MAXIMAL:
//! adjacent survivors merge, so a consumer sees the runs a reader would see.
//!
//! CONVENTION: **public offsets are always SOURCE bytes.** A consumer finds
//! "doubled word at 12..17 of the mask", calls [`Mask::to_source`] twice, and
//! files its diagnostic in document coordinates — mask space never escapes its
//! maker. [`Mask::from_source`] serves only the reverse direction, where `None`
//! honestly means "that byte is not in this view".
//!
//! The mask walks the CST rather than the token stream because "text is not
//! verse text" is a SCOPE fact: the `why` inside a footnote dies because the
//! footnote dies, which is one skipped subtree here and a stateful guess on a
//! flat stream.

use std::ops::Range;

use crate::cst::{Cst, NODE_ID_BIT};
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::{MarkerKind, SpellingShape};
use crate::{Token, TokenKind};

/// What a filter does with one marker.
///
/// The three-way split exists because a scope-opening marker has TWO different
/// drops: `\add`'s text is wanted where its markers are not ([`Self::Unwrap`]),
/// and a footnote's text is wanted nowhere ([`Self::Remove`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The marker's own tokens survive, and its children are judged by their own
    /// rules.
    ///
    /// `Keep` is NOT "verbatim" — the three actions all speak about ONE marker's
    /// own tokens, never about its descendants' fate, which is what keeps them
    /// composable: `structure()` keeps `\p` and still drops the prose inside it,
    /// and `markers: [("f", Keep)]` on top of `structure()` keeps the note's
    /// shell (`\f +\f*`) while its `\ft` child stays Removed as a Character.
    /// Wanting the note's TEXT back means saying so — `kinds[Note] = Keep`,
    /// `kinds[Character] = Keep`, `text = All` — because there is no
    /// inside-a-note text rule to lean on.
    Keep,
    /// The children are walked; the marker's own tokens — its payload and
    /// attribute list included — drop.
    Unwrap,
    /// The whole subtree drops, in O(1) at the node.
    Remove,
}

/// Which `Text` survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRule {
    All,
    /// Text survives only inside an open verse — a verse running from its `\v`
    /// to the next `\v`/`\c`/EOF, with a sidebar its own scope (usx.rs's `vid`
    /// definition, reused). Without it a kept-all-text filter also keeps intro
    /// text, front matter, and everything before `\v 1`.
    VerseExtent,
    None,
}

/// A marker name in [`Filter::markers`] that resolves to no row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownMarker(pub String);

impl core::fmt::Display for UnknownMarker {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "no marker row is named \\{}", self.0)
    }
}

impl std::error::Error for UnknownMarker {}

/// What survives a mask. Config only — no predicate, no callback: one API for
/// Rust and for a wasm caller that can only hand over data.
///
/// Build from a recipe and mutate what you disagree with; there are no merge
/// semantics beyond the one precedence rule, **marker beats kind**:
///
/// ```text
/// let mut f = Filter::structure();            // a \c 1 \p \v 1 \v 2 … skeleton
/// f.markers.push(("f".into(), Action::Keep)); // …that keeps footnotes whole
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// A COMPLETE map, indexed by `kind as usize` — there is no "unlisted" case.
    /// The [`MarkerKind::Unknown`] slot is never read: row 0 is [`Self::unknowns`]'s
    /// business, and the recipes keep the two in agreement anyway.
    pub kinds: [Action; MarkerKind::COUNT],
    /// Per-marker overrides by NAME (`"f"`, `"qt-s"`), resolved once at
    /// [`mask`] entry. Beats [`Self::kinds`]; an unresolvable name is a loud
    /// failure, never a silent no-op — see [`Filter::resolve`].
    pub markers: Vec<(String, Action)>,
    /// Every row-0 marker: an unknown name, a `\z` extension, an illegal
    /// spelling.
    pub unknowns: Action,
    pub text: TextRule,
    /// `Newline` tokens. Dropped text leaves its line breaks behind, which is
    /// why a `structure()` skeleton has blank lines where prose was — collapsing
    /// them would be trimming, and nothing here trims.
    pub newlines: bool,
    /// Attribute lists. An attr list RIDES its marker: it survives only when
    /// this is true AND the marker it belongs to was kept.
    pub attr_lists: bool,
    /// `OptBreak` (`//`) tokens. Its own switch rather than a Paragraph-kind
    /// verdict because the two views disagree: a structural skeleton keeps the
    /// break, a reading text unwraps it away (`gr//ace` reads `grace`), and the
    /// diff's reader text wants it back WITHOUT the paragraph markers around
    /// it — three combinations one `kinds` slot cannot spell.
    pub opt_breaks: bool,
}

impl Filter {
    /// The proofreading view: the words a reader reads, and nothing else.
    ///
    /// Markers, designators, attribute lists and note/milestone/sidebar
    /// SUBTREES all drop; `\add`'s text survives while its markers do not; text
    /// outside a verse (front matter, intro, a heading, everything before
    /// `\v 1`) is not verse text and dies too.
    pub fn verse_text() -> Self {
        use Action::{Remove, Unwrap};
        let mut kinds = [Remove; MarkerKind::COUNT];
        // Unwrap = "this marker is presentation, its content is prose".
        kinds[MarkerKind::Paragraph as usize] = Unwrap;
        kinds[MarkerKind::Character as usize] = Unwrap;
        kinds[MarkerKind::TableRow as usize] = Unwrap;
        kinds[MarkerKind::TableCell as usize] = Unwrap;
        kinds[MarkerKind::Periph as usize] = Unwrap;
        Self {
            kinds,
            markers: Vec::new(),
            unknowns: Remove,
            text: TextRule::VerseExtent,
            newlines: true,
            attr_lists: false,
            opt_breaks: false,
        }
    }

    /// The diff's reader text: every byte a reader would read, wherever it
    /// sits.
    ///
    /// [`Self::verse_text`] with two differences, both because a diff UNIT is
    /// not a verse: text outside a verse extent (front matter, `\h`, a heading)
    /// belongs to some block and has to be diffable, and note prose rides in
    /// undifferentiated rather than dropping with its subtree. Nothing is
    /// [`Action::Remove`]d, so no text is unreachable; `//` survives, so a
    /// break is a run of its own instead of gluing two words.
    pub fn reader_text() -> Self {
        Self {
            kinds: [Action::Unwrap; MarkerKind::COUNT],
            markers: Vec::new(),
            unknowns: Action::Unwrap,
            text: TextRule::All,
            newlines: true,
            attr_lists: false,
            opt_breaks: true,
        }
    }

    /// The copy-a-Bible scaffolding: `\id`, `\h`/`\toc*`, `\mt*`, `\c` + its
    /// designator, the paragraph markers, `\v` + its designator, and their
    /// newlines. All text, notes, character markup and unknowns drop, leaving a
    /// `\c 1 \p \v 1 \v 2 …` skeleton to pour a translation into.
    pub fn structure() -> Self {
        use Action::{Keep, Remove};
        let mut kinds = [Keep; MarkerKind::COUNT];
        kinds[MarkerKind::Unknown as usize] = Remove;
        kinds[MarkerKind::Character as usize] = Remove;
        kinds[MarkerKind::Note as usize] = Remove;
        kinds[MarkerKind::Milestone as usize] = Remove;
        kinds[MarkerKind::Sidebar as usize] = Remove;
        kinds[MarkerKind::Figure as usize] = Remove;
        kinds[MarkerKind::Meta as usize] = Remove;
        Self {
            kinds,
            markers: Vec::new(),
            unknowns: Remove,
            text: TextRule::None,
            newlines: true,
            attr_lists: true,
            opt_breaks: true,
        }
    }

    /// [`Self::markers`] as row indices, or the first name that names no row.
    ///
    /// The pre-flight a boundary (wasm, a CLI flag) runs before calling [`mask`],
    /// which resolves the same way and PANICS on the same failure — config that
    /// silently filters nothing is the bug this refuses to have.
    ///
    /// A name is matched in the spelling it is written in: `"qt"` finds the
    /// character row, `"qt-s"` the milestone one.
    pub fn resolve(&self) -> Result<Vec<(MarkerIdx, Action)>, UnknownMarker> {
        self.markers
            .iter()
            .map(|(name, action)| {
                resolve_name(name)
                    .map(|idx| (idx, *action))
                    .ok_or_else(|| UnknownMarker(name.clone()))
            })
            .collect()
    }
}

/// The row a marker NAME names, classifying the spelling the way the scanner
/// does: a `-s`/`-e` suffix means the milestone row, anything else the plain one.
/// `qt` is the only name the table overloads, and it is exactly the name where
/// guessing would pick the wrong row.
fn resolve_name(name: &str) -> Option<MarkerIdx> {
    let spelled = match name.as_bytes() {
        [.., b'-', b's' | b'e'] => SpellingShape::MilestoneOnly,
        _ => SpellingShape::PlainOnly,
    };
    [SpellingShape::Any, spelled]
        .into_iter()
        .map(|shape| generated::marker_idx(name.as_bytes(), shape))
        .find(|idx| *idx != generated::UNRESOLVED)
}

/// The kept source bytes, and the two-way offset map they are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mask {
    /// Kept SOURCE bytes: sorted, disjoint, non-empty, and MAXIMAL — two
    /// adjacent survivors are one range, never two.
    pub ranges: Vec<Range<u32>>,
    /// `ranges`' zip-mate: `ranges[i]`'s bytes sit at `starts[i]..` in the mask,
    /// so `starts[i] == sum(len(ranges[..i]))`.
    pub starts: Vec<u32>,
}

/// Which source bytes survive `filter`.
///
/// `source` must be the bytes `tokens` were lexed from and `cst` the tree built
/// from those tokens. One in-order CST walk, with a [`Action::Remove`] node
/// skipped at its own child id.
///
/// # Panics
///
/// If a [`Filter::markers`] name resolves to no row — call [`Filter::resolve`]
/// first at a boundary that must not panic.
pub fn mask(source: &[u8], tokens: &[Token], cst: &Cst, filter: &Filter) -> Mask {
    let overrides = filter
        .resolve()
        .unwrap_or_else(|unknown| panic!("Filter::markers: {unknown}"));
    // A row index is a u8, so the override lookup is one array read rather than
    // a scan per marker token.
    let mut by_row: [Option<Action>; 256] = [None; 256];
    for (idx, action) in overrides {
        by_row[idx as usize] = Some(action);
    }
    let action_of = |marker_idx: MarkerIdx| -> Action {
        if let Some(action) = by_row[marker_idx as usize] {
            return action;
        }
        if marker_idx == generated::UNRESOLVED {
            return filter.unknowns;
        }
        filter.kinds[generated::kind(marker_idx) as usize]
    };

    let mut out = Mask::default();
    let mut keep = |start: u32, end: u32| {
        if start >= end {
            return;
        }
        match out.ranges.last_mut() {
            // Tokens partition the source in order, so "adjacent" is exactly
            // "the previous range ended where this one starts".
            Some(last) if last.end == start => last.end = end,
            _ => {
                out.starts.push(
                    out.ranges.last().map_or(0, |r| r.end - r.start)
                        + out.starts.last().copied().unwrap_or(0),
                );
                out.ranges.push(start..end);
            }
        }
    };

    let root = &cst.nodes[0];
    let mut cur = Cursor {
        next: root.children.start,
        end: root.children.end,
        own_token: u32::MAX,
        keep_own: false,
        sidebar: false,
    };
    let mut stack: Vec<Cursor> = Vec::new();
    let mut state = Extent::default();
    // The verdict of the most recent marker-class token, for the payload and
    // attribute-list tokens that RIDE it: nobody keeps `\v` and drops its `1`.
    let mut rides = false;

    loop {
        if cur.next == cur.end {
            let Some(parent) = stack.pop() else { break };
            if cur.sidebar {
                state.sidebar_depth -= 1;
            }
            cur = parent;
            continue;
        }
        let child = cst.child_ids[cur.next as usize];
        cur.next += 1;

        if child & NODE_ID_BIT != 0 {
            let id = (child & !NODE_ID_BIT) as usize;
            let node = &cst.nodes[id];
            let opener = &tokens[node.token as usize];
            // A U25003 container SHARES its `-s` token with the point inside it,
            // so it owns no tokens of its own: the token is the point's first
            // child, and this node's is a node. Transparent, never removable —
            // dropping it would take the `\li` items with it.
            let transparent = cst
                .child_ids
                .get(node.children.start as usize)
                .is_some_and(|first| first & NODE_ID_BIT != 0);
            let action = if transparent {
                Action::Unwrap
            } else {
                action_of(opener.marker_idx)
            };
            if action == Action::Remove {
                // The whole subtree, skipped at its child id. A `\v` buried in
                // it never opens an extent, which is the same blindness usx's
                // decorate() has inside a sidebar.
                continue;
            }
            let sidebar = !transparent && generated::kind(opener.marker_idx) == MarkerKind::Sidebar;
            if sidebar {
                state.sidebar_depth += 1;
            }
            stack.push(cur);
            cur = Cursor {
                next: node.children.start,
                end: node.children.end,
                own_token: if transparent { u32::MAX } else { node.token },
                keep_own: action == Action::Keep,
                sidebar,
            };
            continue;
        }

        let token = &tokens[child as usize];
        let kind = token.kind();
        // A node's own closer is its LAST child (the tree builder's invariant),
        // so it needs no side table to be recognized.
        let last_child = cur.next == cur.end;
        let from = token.start;
        let survives = match kind {
            TokenKind::Marker { .. } | TokenKind::Milestone { .. } => {
                state.observe(token, kind);
                rides = if child == cur.own_token {
                    cur.keep_own
                } else {
                    action_of(token.marker_idx) == Action::Keep
                };
                rides
            }
            TokenKind::ClosingMarker { .. } => {
                rides = if last_child {
                    cur.keep_own
                } else {
                    // An orphan closer belongs to nothing; its own row decides.
                    action_of(token.marker_idx) == Action::Keep
                };
                rides
            }
            TokenKind::MilestoneTerminator => {
                rides = if last_child {
                    cur.keep_own
                } else {
                    filter.kinds[MarkerKind::Milestone as usize] == Action::Keep
                };
                rides
            }
            // The three carved payloads ride the marker that consumed them, and
            // each owns its own delimiter, so dropping one is whitespace-clean.
            TokenKind::Designator | TokenKind::BookCode | TokenKind::NoteCaller => rides,
            // A delimiter run's surplus rides the same way: never content, so
            // a text view drops it whole; a marker-keeping view keeps the
            // bytes and the concatenation stays byte-identical.
            TokenKind::Pad => rides,
            TokenKind::AttrList => filter.attr_lists && rides,
            TokenKind::Newline => filter.newlines,
            TokenKind::OptBreak => filter.opt_breaks,
            TokenKind::Text => match filter.text {
                TextRule::All => true,
                TextRule::VerseExtent => state.in_verse_text(),
                TextRule::None => false,
            },
        };
        if survives {
            keep(from, token.end());
        }
    }

    debug_assert!(
        out.ranges.len() == out.starts.len()
            && out
                .ranges
                .windows(2)
                .all(|pair| pair[0].end < pair[1].start),
        "ranges must be maximal, disjoint and ascending"
    );
    debug_assert!(
        out.ranges
            .last()
            .is_none_or(|last| last.end as usize <= source.len()),
        "ranges must be inside the source"
    );
    out
}

/// One frame of the walk: where this node's child list has got to, and what the
/// node's own tokens were sentenced to.
struct Cursor {
    next: u32,
    end: u32,
    /// The node's opening-marker token id, `u32::MAX` for the root and for a
    /// transparent container (neither owns a token).
    own_token: u32,
    /// [`Action::Keep`] on this node: its own tokens survive.
    keep_own: bool,
    /// This frame is a sidebar, so its close decrements the depth.
    sidebar: bool,
}

/// The verse-extent state [`TextRule::VerseExtent`] reads — usx.rs's `vid`
/// machinery, minus the sids nothing here renders.
#[derive(Default)]
struct Extent {
    /// A `\v` has been seen and neither a `\v` nor a `\c` has ended it.
    open: bool,
    /// A SIDEBAR is its own scope: its paragraphs are in no verse, and the verse
    /// outside RESUMES after `\esbe` — so the extent looks away while one is
    /// open rather than closing anything.
    sidebar_depth: u32,
}

impl Extent {
    fn observe(&mut self, token: &Token, kind: TokenKind) {
        // The ROW is the authority on what bounds a verse: `\ca`/`\vp` carve a
        // designator too, and `\+v` is not a verse marker at all.
        if self.sidebar_depth > 0 || kind != (TokenKind::Marker { nested: false }) {
            return;
        }
        match generated::kind(token.marker_idx) {
            // A `\v` whose designator is missing or malformed still opens an
            // extent — otherwise the previous verse silently swallows this
            // verse's text, the same reason Toc keeps its row.
            MarkerKind::Verse => self.open = true,
            MarkerKind::Chapter => self.open = false,
            _ => {}
        }
    }

    fn in_verse_text(&self) -> bool {
        self.open && self.sidebar_depth == 0
    }
}

impl Mask {
    /// The masked bytes as one string, in ONE allocation.
    ///
    /// `source` must be the bytes the mask was built from, valid UTF-8 (the
    /// crate-wide contract).
    pub fn text(&self, source: &[u8]) -> String {
        let mut out = String::with_capacity(self.len() as usize);
        for part in self.iter(source) {
            out.push_str(part);
        }
        out
    }

    /// The masked bytes as a run of borrowed slices — the zero-alloc view.
    pub fn iter<'s>(&self, source: &'s [u8]) -> impl Iterator<Item = &'s str> {
        self.ranges.iter().map(|range| {
            std::str::from_utf8(&source[range.start as usize..range.end as usize])
                .expect("mask ranges are token spans, so they fall on character boundaries")
        })
    }

    /// Mask-space length: the total kept bytes.
    pub fn len(&self) -> u32 {
        match (self.starts.last(), self.ranges.last()) {
            (Some(start), Some(last)) => start + (last.end - last.start),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// The source byte a mask offset names. TOTAL, like `Toc::locate`: the
    /// mask's own length answers the end of the last range, and anything past
    /// it clamps there.
    pub fn to_source(&self, mask_off: u32) -> u32 {
        if self.ranges.is_empty() {
            return 0;
        }
        let off = mask_off.min(self.len());
        let row = self.starts.partition_point(|start| *start <= off) - 1;
        self.ranges[row].start + (off - self.starts[row])
    }

    /// Where a source byte sits in the mask, or `None` when this view dropped
    /// it. NEVER clamped: a diagnostic on a `\f` byte asking "where in the
    /// proofread text?" has the honest answer nowhere.
    pub fn from_source(&self, src_off: u32) -> Option<u32> {
        let row = self
            .ranges
            .partition_point(|range| range.start <= src_off)
            .checked_sub(1)?;
        let range = &self.ranges[row];
        (src_off < range.end).then(|| self.starts[row] + (src_off - range.start))
    }
}

/// Whether a marker's category is one the U25003 containers use — the fact that
/// tells a `\list-s` point from the container node around it, kept here so the
/// walk's `transparent` test has a name to be checked against in tests.
#[cfg(test)]
fn is_container(marker_idx: MarkerIdx) -> bool {
    use crate::tables::schema::Category;
    matches!(
        generated::category(marker_idx),
        Category::MilestoneList | Category::MilestoneTable
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cst, lex};

    fn built(source: &str, filter: &Filter) -> Mask {
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        mask(source.as_bytes(), &tokens, &cst, filter)
    }

    /// The masked text, plus every invariant checked on the way — the shape
    /// every case below reads through, so no case can pass on a broken map.
    fn masked(source: &str, filter: &Filter) -> String {
        let m = built(source, filter);
        check_invariants(source.as_bytes(), &m);
        m.text(source.as_bytes())
    }

    fn verse_text(source: &str) -> String {
        masked(source, &Filter::verse_text())
    }

    fn structure(source: &str) -> String {
        masked(source, &Filter::structure())
    }

    /// The sketch's Invariants section, executable. Shared with
    /// tests/mask_oracle.rs in spirit; kept here so a unit case fails on the
    /// same terms a corpus book would.
    pub(super) fn check_invariants(source: &[u8], m: &Mask) {
        assert_eq!(m.ranges.len(), m.starts.len(), "one start per range");
        let mut running = 0u32;
        let mut previous_end = 0u32;
        for (row, range) in m.ranges.iter().enumerate() {
            assert!(range.start < range.end, "range {row} is empty");
            assert!(
                range.end as usize <= source.len(),
                "range {row} leaves the source"
            );
            assert!(
                row == 0 || previous_end < range.start,
                "range {row} is adjacent to or overlaps its predecessor — ranges must be maximal"
            );
            assert_eq!(
                m.starts[row], running,
                "starts[{row}] is not the prefix sum"
            );
            running += range.end - range.start;
            previous_end = range.end;
        }
        assert_eq!(m.len(), running, "len is the sum of the range lengths");

        let text = m.text(source);
        assert_eq!(
            text,
            m.iter(source).collect::<String>(),
            "text() == concat(iter())"
        );
        assert_eq!(text.len() as u32, m.len(), "text length is the mask length");

        for offset in 0..m.len() {
            let byte = m.to_source(offset);
            assert_eq!(
                text.as_bytes()[offset as usize],
                source[byte as usize],
                "text()[{offset}] != source[to_source({offset})]"
            );
            assert_eq!(
                m.from_source(byte),
                Some(offset),
                "to_source(from_source(b)) != b at source byte {byte}"
            );
        }
        // `from_source` is None on exactly the dropped bytes.
        let mut kept = vec![false; source.len()];
        for range in &m.ranges {
            kept[range.start as usize..range.end as usize].fill(true);
        }
        for (byte, kept) in kept.iter().enumerate() {
            assert_eq!(
                m.from_source(byte as u32).is_some(),
                *kept,
                "from_source disagrees about source byte {byte}"
            );
        }
        // The end of the mask names the end of the last kept run, and past it
        // clamps there.
        if let Some(last) = m.ranges.last() {
            assert_eq!(m.to_source(m.len()), last.end);
            assert_eq!(m.to_source(u32::MAX), last.end);
        }
    }

    #[test]
    fn the_module_doc_example() {
        let source = "\\v 1 Jesus wept.\\f + \\ft why\\f* Then…";
        let m = built(source, &Filter::verse_text());
        check_invariants(source.as_bytes(), &m);
        assert_eq!(m.ranges, vec![5..16, 31..39]);
        assert_eq!(m.starts, vec![0, 11]);
        assert_eq!(m.text(source.as_bytes()), "Jesus wept. Then…");
        assert_eq!(m.to_source(3), 8);
        assert_eq!(m.to_source(13), 33);
        assert_eq!(m.from_source(20), None);
    }

    #[test]
    fn verse_text_drops_markers_designators_and_note_subtrees() {
        assert_eq!(
            verse_text("\\c 1\n\\p \\v 1 In the beginning.\n"),
            "\nIn the beginning.\n"
        );
        // The note's own text dies with the note, milestones and their attribute
        // lists leave nothing behind, and the words either side join up.
        assert_eq!(
            verse_text("\\p \\v 1 a\\f + \\ft n\\f*b \\ts-s\\* c\n"),
            "ab  c\n"
        );
    }

    #[test]
    fn a_character_markers_text_survives_its_markers() {
        assert_eq!(
            verse_text("\\p \\v 1 he \\add really\\add* went\n"),
            "he really went\n"
        );
        // Nested character markup unwraps at every depth.
        assert_eq!(
            verse_text("\\p \\v 1 \\add a \\+nd b\\+nd* c\\add*\n"),
            "a b c\n"
        );
        // An attribute list rides its marker, so it goes when the marker goes.
        assert_eq!(
            verse_text("\\p \\v 1 \\w grace|lemma=\"x\"\\w*.\n"),
            "grace.\n"
        );
    }

    /// `~` is a plain byte of a Text token, never a token of its own, so it
    /// survives as itself: bytes are bytes and nothing here normalizes. `//` IS
    /// a token, and a line-break marker, so it drops with the paragraph markers.
    #[test]
    fn a_tilde_survives_and_an_optional_break_does_not() {
        assert_eq!(verse_text("\\p \\v 1 Psalm~1 gr//ace\n"), "Psalm~1 grace\n");
        assert_eq!(structure("\\p \\v 1 gr//ace\n"), "\\p \\v 1 //\n");
    }

    /// A kept designator brings its delimiter with it, exactly as a kept marker
    /// does — which is what makes the skeleton `\v 1 ` rather than `\v 1` glued
    /// to whatever the next kept token is.
    #[test]
    fn structure_keeps_the_scaffolding_and_no_prose() {
        assert_eq!(
            structure("\\id GEN\n\\h Genesis\n\\c 1\n\\p \\v 1 In the beginning.\n"),
            "\\id GEN\n\\h \n\\c 1\n\\p \\v 1 \n"
        );
        // `\b`, the blank-line paragraph, is scaffolding like any other
        // paragraph marker, so a poetry stanza break survives.
        assert_eq!(
            structure("\\q1 \\v 1 a\n\\b\n\\q1 \\v 2 b\n"),
            "\\q1 \\v 1 \n\\b\n\\q1 \\v 2 \n"
        );
        // Notes, character markup and their contents leave nothing at all.
        assert_eq!(
            structure("\\c 2\n\\q1 \\v 3 a\\f + \\ft n\\f* \\add b\\add*\n"),
            "\\c 2\n\\q1 \\v 3 \n"
        );
    }

    #[test]
    fn verse_extent_kills_text_outside_a_verse() {
        // Front matter, an intro paragraph, a chapter heading, and the bytes
        // between `\c` and `\v 1` are all outside every verse.
        let source =
            "\\id GEN\n\\mt1 Genesis\n\\ip An intro.\n\\c 1\n\\s1 A heading\n\\p \\v 1 real text\n";
        assert_eq!(verse_text(source), "\n\n\n\nreal text\n");
        // A heading in the MIDDLE of a verse dies by KIND (Titles is a
        // Paragraph, so the marker unwraps) and by extent both — text after the
        // next `\v` returns.
        assert_eq!(
            verse_text("\\p \\v 1 one\n\\s1 mid\n\\p \\v 2 two\n"),
            "one\nmid\ntwo\n"
        );
        // …which is why `\s1`'s text needs the kind table to die, not the
        // extent: the extent is still open across it.
        let mut f = Filter::verse_text();
        f.markers.push(("s".into(), Action::Remove));
        assert_eq!(
            masked("\\p \\v 1 one\n\\s1 mid\n\\p \\v 2 two\n", &f),
            "one\ntwo\n"
        );
    }

    #[test]
    fn a_verse_survives_a_sidebar_and_resumes_after_it() {
        let source = "\\c 1\n\\p \\v 1 before\n\\esb \\p inside\n\\esbe\n\\p after\n";
        // The sidebar's own prose is in no verse; `after` is still verse 1's.
        assert_eq!(verse_text(source), "\nbefore\n\nafter\n");
        // Even with the sidebar kept, its text stays out of the verse.
        let mut f = Filter::verse_text();
        f.markers.push(("esb".into(), Action::Unwrap));
        assert_eq!(masked(source, &f), "\nbefore\n\n\nafter\n");
    }

    #[test]
    fn a_chapter_ends_the_open_verse() {
        assert_eq!(
            verse_text("\\c 1\n\\p \\v 1 one\n\\c 2\n\\p tail\n\\v 1 two\n"),
            "\none\n\n\ntwo\n"
        );
    }

    #[test]
    fn marker_beats_kind_in_both_directions() {
        // "structure but keep footnotes": the note subtree survives whole.
        let mut f = Filter::structure();
        f.markers.push(("f".into(), Action::Keep));
        assert_eq!(
            masked("\\p \\v 1 a\\f + \\ft n\\f* b\n", &f),
            // The kept caller brings its delimiter, as a kept marker does.
            "\\p \\v 1 \\f + \\f*\n"
        );
        // "verse text including footnote text": the note unwraps instead.
        let mut f = Filter::verse_text();
        f.kinds[MarkerKind::Note as usize] = Action::Unwrap;
        assert_eq!(masked("\\p \\v 1 a\\f + \\ft n\\f* b\n", &f), "an b\n");
    }

    #[test]
    fn unknown_markers_follow_the_unknowns_action() {
        let source = "\\p \\v 1 a \\zfoo b\n";
        assert_eq!(verse_text(source), "a b\n");
        let mut f = Filter::verse_text();
        f.unknowns = Action::Keep;
        assert_eq!(masked(source, &f), "a \\zfoo b\n");
        // An aligned-corpus milestone pair: row-0 points, dropped whole, and the
        // word between them (which is in NEITHER point) survives.
        assert_eq!(
            verse_text(
                "\\p \\v 1 \\zaln-s |x-strong=\"G1\"\\*\\w In|lemma=\"in\"\\w*\\zaln-e\\*\n"
            ),
            "In\n"
        );
    }

    #[test]
    fn a_list_container_is_not_removable_with_its_milestone() {
        // `\list-s`'s point and the CONTAINER share one token; Milestone =
        // Remove must take the point and leave the items.
        let source = "\\list-s\\*\n\\li one\n\\li two\n\\list-e\\*\n";
        let tokens = lex(source);
        assert!(is_container(
            tokens
                .iter()
                .find(|t| matches!(t.kind(), TokenKind::Milestone { .. }))
                .expect("a milestone")
                .marker_idx
        ));
        assert_eq!(structure(source), "\n\\li \n\\li \n\n");
        assert_eq!(verse_text(source), "\n\n\n\n");
    }

    #[test]
    fn attr_lists_ride_their_marker() {
        let source = "\\p \\v 1 \\w grace|lemma=\"x\"\\w*\n";
        // structure() keeps attr lists, but `\w` is Character = Remove, so the
        // list goes with it: riding means riding.
        assert_eq!(structure(source), "\\p \\v 1 \n");
        let mut f = Filter::structure();
        f.markers.push(("w".into(), Action::Keep));
        assert_eq!(masked(source, &f), "\\p \\v 1 \\w |lemma=\"x\"\\w*\n");
        f.attr_lists = false;
        assert_eq!(masked(source, &f), "\\p \\v 1 \\w \\w*\n");
        // A FRONT-position list rides the same way.
        let mut f = Filter::structure();
        f.attr_lists = false;
        assert_eq!(masked("\\c |x-a=\"b\"| 1\n", &f), "\\c 1\n");
        assert_eq!(structure("\\c |x-a=\"b\"| 1\n"), "\\c |x-a=\"b\"| 1\n");
    }

    #[test]
    fn newlines_are_a_flag_of_their_own() {
        let mut f = Filter::structure();
        f.newlines = false;
        assert_eq!(masked("\\c 1\n\\p \\v 1 text\n", &f), "\\c 1\\p \\v 1 ");
    }

    #[test]
    fn text_rules_span_the_three_answers() {
        let source = "\\id GEN\n\\p intro\n\\c 1\n\\p \\v 1 verse\n";
        let mut f = Filter::verse_text();
        f.text = TextRule::All;
        assert_eq!(masked(source, &f), "intro\n\nverse\n");
        f.text = TextRule::None;
        assert_eq!(masked(source, &f), "\n\n\n");
    }

    /// Multibyte text changes NUMBERS, never mechanics: the map is over BYTES,
    /// so a 3-byte script's offsets round-trip like any other's.
    #[test]
    fn multibyte_text_round_trips_by_byte() {
        let source = "\\p \\v 1 अब्राहम \\add की\\add* सन्तान\n";
        let m = built(source, &Filter::verse_text());
        check_invariants(source.as_bytes(), &m);
        assert_eq!(m.text(source.as_bytes()), "अब्राहम की सन्तान\n");
        // The first byte of the mask is the first byte of the first character.
        assert_eq!(m.to_source(0), source.find('अ').expect("the text") as u32);
    }

    #[test]
    fn an_empty_mask_answers_both_directions() {
        let m = built("\\p \\v 1 text", &{
            let mut f = Filter::verse_text();
            f.text = TextRule::None;
            f.newlines = false;
            f
        });
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert_eq!(m.text(b"\\p \\v 1 text"), "");
        assert_eq!(m.to_source(0), 0);
        assert_eq!(m.to_source(u32::MAX), 0);
        assert_eq!(m.from_source(0), None);
        // An empty DOCUMENT too.
        let m = built("", &Filter::verse_text());
        assert!(m.is_empty());
        assert_eq!(m.from_source(0), None);
    }

    #[test]
    fn an_unknown_marker_name_fails_loudly() {
        let mut f = Filter::structure();
        f.markers.push(("nosuchmarker".into(), Action::Keep));
        assert_eq!(
            f.resolve().unwrap_err(),
            UnknownMarker("nosuchmarker".into())
        );
        // A `\z` extension has no row either — `unknowns` is its lever.
        f.markers = vec![("zfoo".into(), Action::Keep)];
        assert!(f.resolve().is_err());
        let tokens = lex("\\p a");
        let cst = cst::build(&tokens);
        assert!(
            std::panic::catch_unwind(|| mask(b"\\p a", &tokens, &cst, &f)).is_err(),
            "mask() must refuse a filter naming no row"
        );
    }

    #[test]
    fn a_name_resolves_in_the_spelling_it_is_written_in() {
        // `qt` is the one overloaded name: the character row and the milestone
        // row answer to different spellings.
        let mut f = Filter::verse_text();
        f.markers.push(("qt-s".into(), Action::Keep));
        assert_eq!(f.resolve().unwrap().len(), 1);
        assert_eq!(
            masked("\\p \\v 1 a \\qt-s |who=\"Levi\"\\* b\n", &f),
            "a \\qt-s \\* b\n"
        );
        let mut f = Filter::verse_text();
        f.markers.push(("qt".into(), Action::Keep));
        assert_eq!(
            masked("\\p \\v 1 a \\qt b\\qt* c\n", &f),
            "a \\qt b\\qt* c\n"
        );
    }

    #[test]
    fn every_kind_has_an_action_in_both_recipes() {
        for recipe in [Filter::verse_text(), Filter::structure()] {
            assert_eq!(recipe.kinds.len(), MarkerKind::COUNT);
        }
    }
}
