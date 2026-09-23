//! A target's block structure made equal to a source's, as one edit
//! transaction.
//!
//! ```text
//! source  \p            target  \v 21 So the LORD God …
//!         \v 21 So the LORD God …        \v 22 And from the rib …
//!         \v 22 And from the rib …       \v 23 And the man said: “This is now …”
//!         \v 23 And the man said:        \v 24 For this reason …
//!         \q1 “This is now bone …
//!         \q2 and flesh of my flesh;
//!         \q1 she shall be called …
//!         \q2 for out of man …”
//!         \p
//!         \v 24 For this reason …
//!
//! skeleton(source)  \p(GEN 2:21 leading 1)  \q1(GEN 2:23 inside 1)
//!                   \q2(GEN 2:23 inside 2)  \q1(GEN 2:23 inside 3)
//!                   \q2(GEN 2:23 inside 4)  \p(GEN 2:24 leading 1)
//!
//! overlay(target, source)   insert "\\p\n"  at the target's `\v 21`
//!                           insert "\n\\q1" ×2 and "\n\\q2" ×2 after v23's text
//!                           insert "\\p\n"  at the target's `\v 24`
//! ```
//!
//! An address — (verse sid, leading or inside, ordinal) — names equivalent
//! nodes on both sides. Leading blocks sit immediately before their verse's
//! `\v`, so they land exactly; where a verse's text SPLITS is unknowable
//! across languages, so inside blocks arrive EMPTY after the verse's text and
//! the translator pastes each line into place. `overlay.md` beside this file
//! is the contract: the marker set, what an empty block does, and why a
//! target block the source lacks is removed rather than kept.

use core::ops::Range;

use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::{AlignmentFact, Corpus, InputError, VerseKey, align};

use crate::onion::tables::generated::{self, MarkerIdx};
use crate::onion::tables::schema::{Category, MarkerKind};
use crate::onion::{Action, Edit, Filter, Mask, Sid, Token, TokenKind, edit::FixStr, lint::Code};
use crate::pantry::{BookId, Pantry, PantryError};
use crate::sous::OnionBook;

// ------------------------------------------------------------------ the shapes

/// Where a block sits relative to its verse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[cfg_attr(feature = "wasm", serde(rename_all = "lowercase"))]
pub enum Placement {
    /// Immediately before the verse's `\v`, with no verse text in between.
    Leading,
    /// After verse text, inside the verse's own extent.
    Inside,
}

/// One block marker's address. The POSITION — sid, placement, ordinal — names
/// equivalent nodes on both sides; `marker` is the spelling that position held
/// when the address was taken.
///
/// The name is a CHECK, not a key. A door that is handed an address whose
/// position still exists but now spells something else refuses it as stale
/// rather than answering about a different node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct BlockAddress {
    pub sid: String,
    #[cfg_attr(feature = "wasm", serde(rename = "where"))]
    pub placement: Placement,
    pub ordinal: u32,
    /// As SPELLED — `"q1"`, not the row's canonical `"q"`.
    pub marker: String,
}

impl core::fmt::Display for BlockAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let placement = match self.placement {
            Placement::Leading => "leading",
            Placement::Inside => "inside",
        };
        write!(f, "{} {placement} {}", self.sid, self.ordinal)
    }
}

/// One verse's two spans: its `\v` marker, and its own text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SkeletonVerse {
    pub sid: String,
    /// The `\v` marker through its designator.
    pub from: u32,
    pub to: u32,
    /// The verse's own text, first retained byte through last. Equal to `to`
    /// when the verse has no text at all.
    pub text_from: u32,
    pub text_to: u32,
}

/// One block marker in the skeleton: its address, its spelling, its span.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct SkeletonRow {
    pub sid: String,
    #[cfg_attr(feature = "wasm", serde(rename = "where"))]
    pub placement: Placement,
    pub ordinal: u32,
    /// As SPELLED — `"q1"`, not the row's canonical `"q"`.
    pub marker: String,
    /// The marker token's span, delimiter included: deleting it joins this
    /// block's text to the block before.
    pub from: u32,
    pub to: u32,
    /// Where the block itself ends: `from..end` is the whole paragraph node,
    /// closed by onion's grammar — the next paragraph-kind marker of ANY
    /// category (a `\s1` or `\r` the marker set leaves out still closes it),
    /// a `\c`, or the end of the book. Never the next row's `from` by rule.
    pub end: u32,
    /// What onion lints as an empty paragraph — a block marker with nothing
    /// under it but line endings. `\b` is empty by design and is never one.
    /// An empty row shares the ordinal of the row it folds into.
    pub empty: bool,
    /// The one delimiter byte the marker token folded — a space when text
    /// follows on the line, a newline when it does not, `0` at end of file.
    #[cfg_attr(feature = "wasm", serde(skip))]
    delimiter: u8,
}

impl SkeletonRow {
    /// Whatever this token ends in, so a replacement ends in it too and a
    /// respelled block does not also move its text.
    fn tail(&self) -> &'static str {
        match self.delimiter {
            b'\n' => "\n",
            0 => "",
            _ => " ",
        }
    }

    pub fn address(&self) -> BlockAddress {
        BlockAddress {
            sid: self.sid.clone(),
            placement: self.placement,
            ordinal: self.ordinal,
            marker: self.marker.clone(),
        }
    }
}

/// Every address in one skeleton, and the blocks at it in document order —
/// [`Skeleton::groups`], built once and read by [`Skeleton::rows`].
type Groups<'s> = FxHashMap<(&'s str, Placement), Vec<u32>>;

/// One book's block structure: its verses, and the blocks addressed to them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct Skeleton {
    /// The `\id` code the sids above render, kept so a sid never has to be
    /// read back apart.
    #[cfg_attr(feature = "wasm", serde(skip))]
    pub book: [u8; 3],
    /// Runs of block markers with no verse text between them, at least one of
    /// which is empty — what a source-side diff folds to one.
    #[cfg_attr(feature = "wasm", serde(skip))]
    pub collapsed: Vec<Collapsed>,
    pub verses: Vec<SkeletonVerse>,
    pub blocks: Vec<SkeletonRow>,
}

impl Skeleton {
    /// The blocks at one (sid, placement), in document order, renumbered from
    /// one.
    ///
    /// `keep_empty` is the whole asymmetry between the two sides: a TARGET
    /// keeps every block it has, because the overlay has to account for each;
    /// a SOURCE drops what onion lints as an empty paragraph, because an empty
    /// block does not propagate.
    fn rows(
        &self,
        groups: &Groups<'_>,
        sid: &str,
        placement: Placement,
        keep_empty: bool,
    ) -> Vec<SkeletonRow> {
        groups
            .get(&(sid, placement))
            .into_iter()
            .flatten()
            .map(|at| &self.blocks[*at as usize])
            .filter(|row| keep_empty || !row.empty)
            .enumerate()
            .map(|(at, row)| SkeletonRow {
                ordinal: at as u32 + 1,
                ..row.clone()
            })
            .collect()
    }

    /// One pass over `blocks`, so a diff that asks about every verse does not
    /// re-scan the whole list once per verse per placement.
    fn groups(&self) -> Groups<'_> {
        let mut groups: Groups<'_> = FxHashMap::default();
        for (at, row) in self.blocks.iter().enumerate() {
            groups
                .entry((row.sid.as_str(), row.placement))
                .or_default()
                .push(at as u32);
        }
        groups
    }

    fn verse(&self, sid: &str) -> Option<&SkeletonVerse> {
        self.verses.iter().find(|verse| verse.sid == sid)
    }
}

/// Which side of the pair a report line is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[cfg_attr(feature = "wasm", serde(rename_all = "lowercase"))]
pub enum Side {
    Target,
    Source,
}

/// Why a verse got no edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[cfg_attr(feature = "wasm", serde(rename_all = "lowercase"))]
pub enum Reason {
    /// The other side has no verse with this sid.
    Absent,
    /// A bridge met a constituent run rather than the same bridge.
    Bridge,
    /// The sid occurs more than once, so an address cannot name one node.
    Ambiguous,
}

/// One block the overlay adds to the target.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct Inserted {
    pub address: BlockAddress,
    pub marker: String,
    pub at: u32,
    /// An inside block arrives with no text under it, awaiting the
    /// translator's.
    pub empty: bool,
}

/// One target block the source has no equivalent for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct Removed {
    pub address: BlockAddress,
    pub marker: String,
    pub from: u32,
    pub to: u32,
}

/// A run of source block markers that folded to its last, surviving one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct Collapsed {
    pub sid: String,
    pub marker: String,
    pub count: u32,
}

/// One verse that paired with nothing an address could cross.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct Unpaired {
    pub sid: String,
    pub side: Side,
    pub reason: Reason,
}

/// Everything the overlay did, and everything it declined to do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
pub struct OverlayReport {
    pub inserted: Vec<Inserted>,
    pub removed: Vec<Removed>,
    pub collapsed: Vec<Collapsed>,
    pub unpaired: Vec<Unpaired>,
}

/// The transaction, and the account of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    /// Sorted by `from`, non-overlapping, in the target's byte offsets — one
    /// Undo step for an editor that applies them together.
    pub edits: Vec<Edit>,
    pub report: OverlayReport,
}

/// The answer to "where does this block sit on the other side".
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize))]
#[cfg_attr(feature = "wasm", serde(untagged))]
pub enum Equivalent {
    /// The same address names a block over there.
    Found { found: SkeletonRow },
    /// It does not, and this is where the overlay would put one.
    #[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
    Absent {
        absent: bool,
        insert_at: u32,
        #[cfg_attr(feature = "wasm", serde(rename = "where"))]
        placement: Placement,
    },
    /// The verse itself crosses nothing.
    Unpaired { unpaired: bool, reason: Reason },
}

/// Which verses an overlay may touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Chapter(u16),
    Sid(String),
}

/// The two knobs, plus the coordinate opt-in the doors read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayOptions {
    /// Block markers by NAME, not by class. `None` is onion's paragraph and
    /// poetry rows with the section titles left out; a numbered spelling names
    /// its ROW, so `"q1"` and `"q"` both admit every `\q` level.
    pub markers: Option<Vec<String>>,
    /// `None` is the whole book.
    pub scope: Option<Scope>,
}

/// Why an overlay could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayError {
    /// No book is registered under this id.
    UnknownBook { id: BookId },
    /// The book retains no text, or no projection to read its verse text off.
    Pantry(PantryError),
    /// A name in `markers` names no marker row.
    UnknownMarker { name: String },
    /// The book's projection is not an analyzable pairing input.
    Input { id: BookId, error: InputError },
    /// The address's position still exists, but it holds a different marker
    /// than the address names — a node the caller is no longer looking at.
    StaleAddress {
        address: BlockAddress,
        found: String,
    },
}

impl From<PantryError> for OverlayError {
    fn from(error: PantryError) -> Self {
        Self::Pantry(error)
    }
}

impl core::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownBook { id } => write!(f, "no book is registered as {id}"),
            Self::Pantry(error) => write!(f, "{error}"),
            Self::UnknownMarker { name } => write!(f, "no marker row is named {name:?}"),
            Self::Input { id, error } => write!(f, "book {id} cannot be paired: {error}"),
            Self::StaleAddress { address, found } => write!(
                f,
                "{address} names {} but the node there is {found} — the address is stale",
                address.marker
            ),
        }
    }
}

impl std::error::Error for OverlayError {}

// ------------------------------------------------------------- the marker set

/// The block markers an address may name.
struct MarkerSet {
    /// `None` is the default set, which is a predicate rather than a list.
    rows: Option<FxHashSet<MarkerIdx>>,
}

impl MarkerSet {
    fn resolve(names: Option<&[String]>) -> Result<Self, OverlayError> {
        let Some(names) = names else {
            return Ok(Self { rows: None });
        };
        // Onion's own name resolution, so `"qt-s"` and `"qt"` land where the
        // mask recipes put them and an unknown name fails here rather than
        // filtering nothing.
        let filter = Filter {
            markers: names
                .iter()
                .map(|name| (name.clone(), Action::Keep))
                .collect(),
            ..Filter::structure()
        };
        let rows = filter
            .resolve()
            .map_err(|unknown| OverlayError::UnknownMarker {
                name: unknown.0.clone(),
            })?;
        Ok(Self {
            rows: Some(rows.into_iter().map(|(idx, _)| idx).collect()),
        })
    }

    fn contains(&self, idx: MarkerIdx) -> bool {
        match &self.rows {
            Some(rows) => rows.contains(&idx),
            // Paragraphs and poetry; identification, introductions, titles and
            // sections, lists, tables and peripherals all stay home.
            None => {
                generated::kind(idx) == MarkerKind::Paragraph
                    && matches!(
                        generated::category(idx),
                        Category::ParaBody | Category::ParaPoetry
                    )
            }
        }
    }
}

// -------------------------------------------------------------- the extraction

/// One book's skeleton under the default marker set.
pub fn skeleton(pantry: &mut Pantry, id: &BookId) -> Result<Skeleton, OverlayError> {
    skeleton_with(pantry, id, &OverlayOptions::default())
}

/// The same under a caller's marker set.
pub fn skeleton_with(
    pantry: &mut Pantry,
    id: &BookId,
    opts: &OverlayOptions,
) -> Result<Skeleton, OverlayError> {
    let set = MarkerSet::resolve(opts.markers.as_deref())?;
    extract(pantry, id, &set)
}

fn extract(pantry: &mut Pantry, id: &BookId, set: &MarkerSet) -> Result<Skeleton, OverlayError> {
    if pantry.role(id).is_none() {
        return Err(OverlayError::UnknownBook { id: id.clone() });
    }
    let (tokens, cst, lint) = pantry.skeleton_input(id)?;
    let owners = cst.owners(tokens.len());
    // Onion's own empty-paragraph rule, not a second copy of it: the anchors
    // are the opening markers' token indices.
    let empties: FxHashSet<u32> = lint
        .observations
        .iter()
        .filter(|row| row.code == Code::EmptyParagraph)
        .map(|row| row.anchor)
        .collect();

    let entry = pantry.book(id).expect("the role above answered");
    let text = entry.text()?;
    let source = text.as_bytes();
    let mask = entry.mask()?;
    let toc = entry.toc();
    let toc_book = toc.book;

    // Verse rows first: a keyed anchor's extent is its `\v` through the next
    // anchor, capped at its chapter's end — the same extent `OnionBook` cuts.
    let mut verses: Vec<SkeletonVerse> = Vec::new();
    // Token index of each verse's `\v`, index-aligned with `verses`.
    let mut verse_token: Vec<u32> = Vec::new();
    let mut text_at = 0usize;
    for (at, anchor) in toc.verses.iter().enumerate() {
        if anchor.first == 0 {
            continue;
        }
        let chapter_row = toc
            .chapters
            .partition_point(|chapter| chapter.start <= anchor.at)
            .saturating_sub(1);
        let chapter_end = toc.chapters[chapter_row].end;
        let end = toc
            .verses
            .get(at + 1)
            .map_or(chapter_end, |next| next.at.min(chapter_end));
        let marker_end = marker_span_end(&tokens, anchor.token as usize);
        let (text_from, text_to) =
            text_extent(mask, source, anchor.at..end, marker_end, &mut text_at);
        verses.push(SkeletonVerse {
            sid: Sid {
                book: toc.book,
                chapter: anchor.chapter,
                first: anchor.first,
                last: anchor.last,
            }
            .to_string(),
            from: anchor.at,
            to: marker_end,
            text_from,
            text_to,
        });
        verse_token.push(anchor.token);
    }
    let by_token: FxHashMap<u32, usize> = verse_token
        .iter()
        .enumerate()
        .map(|(row, token)| (*token, row))
        .collect();

    // Then the walk. A block marker waits in `pending` until the document
    // places it. Verse text makes every waiting block INSIDE the current
    // verse; a `\v` makes the LAST waiting block leading for it — the others
    // closed before the `\v` and belong to the verse behind them, which is
    // why a verse has at most one leading block.
    let mut blocks: Vec<SkeletonRow> = Vec::new();
    let mut collapsed: Vec<Collapsed> = Vec::new();
    let mut pending: Vec<Pending> = Vec::new();
    let mut current: Option<usize> = None;
    let mut inside = 1u32;
    let mut range_at = 0usize;
    let mut scanned = 0u32;

    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.kind(), TokenKind::Marker { nested: false }) {
            continue;
        }
        let kind = generated::kind(token.marker_idx);
        let block = kind == MarkerKind::Paragraph && set.contains(token.marker_idx);
        let verse = kind == MarkerKind::Verse;
        if !block && !verse {
            continue;
        }
        // Verse text before this marker places everything waiting. The mask
        // keeps line endings, so a run of whitespace is not text.
        let saw_text = text_between(mask, source, &mut range_at, &mut scanned, token.start);
        if saw_text {
            note_run(&mut collapsed, &pending, sid_of_row(&verses, current));
            place(
                &mut pending,
                &mut blocks,
                sid_of_row(&verses, current),
                Placement::Inside,
                &mut inside,
            );
        }
        if verse {
            let Some(row) = by_token.get(&(index as u32)).copied() else {
                continue;
            };
            // An EMPTY block cannot be the one this `\v` sits inside: a block
            // with the verse under it has the verse's text under it too. So an
            // empty tail belongs to the verse behind, which is also what makes
            // an inserted, text-less inside block read back as one.
            let leads = pending.last().is_some_and(|row| !row.empty);
            note_run(
                &mut collapsed,
                &pending,
                match leads {
                    true => Some(&verses[row].sid),
                    false => sid_of_row(&verses, current),
                },
            );
            let leader = leads.then(|| pending.pop()).flatten();
            place(
                &mut pending,
                &mut blocks,
                sid_of_row(&verses, current),
                Placement::Inside,
                &mut inside,
            );
            if let Some(leader) = leader {
                blocks.push(leader.into_row(&verses[row].sid, Placement::Leading, 1));
            }
            current = Some(row);
            inside = 1;
            continue;
        }
        let marker = spelling(source, token);
        pending.push(Pending {
            span: token.start..token.end(),
            // The opening marker is its own node's first child, so its owner
            // is the paragraph it opens.
            end: cst.extent(owners[index], &tokens).end,
            delimiter: *source
                .get(token.start as usize + marker.len() + 1)
                .unwrap_or(&0),
            marker,
            empty: empties.contains(&(index as u32)),
        });
    }
    note_run(&mut collapsed, &pending, sid_of_row(&verses, current));
    place(
        &mut pending,
        &mut blocks,
        sid_of_row(&verses, current),
        Placement::Inside,
        &mut inside,
    );

    Ok(Skeleton {
        book: toc_book,
        collapsed,
        verses,
        blocks,
    })
}

/// A block marker the walk has seen and the document has not yet placed.
struct Pending {
    span: Range<u32>,
    end: u32,
    marker: String,
    delimiter: u8,
    empty: bool,
}

impl Pending {
    fn into_row(self, sid: &str, placement: Placement, ordinal: u32) -> SkeletonRow {
        SkeletonRow {
            sid: sid.to_string(),
            placement,
            ordinal,
            marker: self.marker,
            from: self.span.start,
            to: self.span.end,
            end: self.end,
            empty: self.empty,
            delimiter: self.delimiter,
        }
    }
}

/// Whether any masked, non-whitespace byte sits between the last marker and
/// this one. Each byte is looked at once: `scanned` only moves forward.
fn text_between(
    mask: &Mask,
    source: &[u8],
    range_at: &mut usize,
    scanned: &mut u32,
    upto: u32,
) -> bool {
    let mut saw = false;
    let mut at = *range_at;
    while at < mask.ranges.len() && mask.ranges[at].start < upto {
        let range = &mask.ranges[at];
        let lo = range.start.max(*scanned) as usize;
        let hi = range.end.min(upto) as usize;
        if lo < hi
            && source[lo..hi]
                .iter()
                .any(|byte| !byte.is_ascii_whitespace())
        {
            saw = true;
        }
        if range.end <= upto {
            at += 1;
        } else {
            break;
        }
    }
    *range_at = at;
    *scanned = upto;
    saw
}

fn sid_of_row(verses: &[SkeletonVerse], row: Option<usize>) -> Option<&str> {
    row.map(|row| verses[row].sid.as_str())
}

/// Drains `pending` into `blocks` at one address, numbering sequentially.
///
/// Blocks with no verse before them have no address and are dropped: nothing
/// in a book's front matter can be placed against a verse.
fn place(
    pending: &mut Vec<Pending>,
    blocks: &mut Vec<SkeletonRow>,
    sid: Option<&str>,
    placement: Placement,
    ordinal: &mut u32,
) {
    let Some(sid) = sid else {
        pending.clear();
        return;
    };
    for row in pending.drain(..) {
        blocks.push(row.into_row(sid, placement, *ordinal));
        *ordinal += 1;
    }
}

/// A run of block markers with no verse text between them, at least one of
/// which onion lints as empty — the fold a source-side diff performs.
fn note_run(collapsed: &mut Vec<Collapsed>, pending: &[Pending], sid: Option<&str>) {
    let (Some(sid), Some(last)) = (sid, pending.last()) else {
        return;
    };
    if pending.len() > 1 && pending.iter().any(|row| row.empty) {
        collapsed.push(Collapsed {
            sid: sid.to_string(),
            marker: last.marker.clone(),
            count: pending.len() as u32,
        });
    }
}

/// A marker token's spelling without its backslash: `\q1 ` → `"q1"`.
fn spelling(source: &[u8], token: &Token) -> String {
    let span = &source[token.start as usize + 1..token.end() as usize];
    let len = span
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric() && *byte != b'-')
        .unwrap_or(span.len());
    String::from_utf8_lossy(&span[..len]).into_owned()
}

/// The `\v` marker span, extended over the designator that follows it.
fn marker_span_end(tokens: &[Token], at: usize) -> u32 {
    let end = tokens[at].end();
    match tokens.get(at + 1) {
        Some(next) if next.kind() == TokenKind::Designator => next.end(),
        _ => end,
    }
}

/// One verse's own text: its first SUBSTANTIVE masked byte through its last.
///
/// The mask keeps line endings, which are not what "after the verse text"
/// means; whitespace is skipped here so an inside block lands after the
/// words. A verse with no text at all answers `(end, end)` at the end of its
/// `\v` marker, which is where a block would go instead.
fn text_extent(
    mask: &Mask,
    source: &[u8],
    extent: Range<u32>,
    marker_end: u32,
    range_at: &mut usize,
) -> (u32, u32) {
    let mut from = None;
    let mut to = None;
    let mut at = *range_at;
    while at < mask.ranges.len() && mask.ranges[at].start < extent.end {
        let range = &mask.ranges[at];
        let lo = range.start.max(extent.start) as usize;
        let hi = range.end.min(extent.end) as usize;
        for (step, byte) in source[lo..hi.max(lo)].iter().enumerate() {
            if !byte.is_ascii_whitespace() {
                let here = (lo + step) as u32;
                from.get_or_insert(here);
                to = Some(here + 1);
            }
        }
        if range.end <= extent.end {
            at += 1;
            *range_at = at;
        } else {
            break;
        }
    }
    let end = to.unwrap_or(marker_end);
    (from.unwrap_or(end), end)
}

// ------------------------------------------------------------------ the pairing

/// Which sids cross, and which do not and why.
struct Pairing {
    paired: FxHashSet<String>,
    unpaired: Vec<Unpaired>,
}

fn pair(
    pantry: &mut Pantry,
    target_id: &BookId,
    source_id: &BookId,
    target: &Skeleton,
    source: &Skeleton,
) -> Result<Pairing, OverlayError> {
    let target_book = [projected(pantry, target_id)?];
    let source_book = [projected(pantry, source_id)?];
    let alignment = align(
        &Corpus::try_new(&target_book).map_err(|error| OverlayError::Input {
            id: target_id.clone(),
            error,
        })?,
        &Corpus::try_new(&source_book).map_err(|error| OverlayError::Input {
            id: source_id.clone(),
            error,
        })?,
    );
    let code = target.book;

    let mut paired = FxHashSet::default();
    let mut unpaired: Vec<Unpaired> = Vec::new();
    let note = |sid: &str, side: Side, reason: Reason, rows: &mut Vec<Unpaired>| {
        if !rows.iter().any(|row| row.sid == sid && row.side == side) {
            rows.push(Unpaired {
                sid: sid.to_string(),
                side,
                reason,
            });
        }
    };

    for unit in alignment.units() {
        let sid = sid_of(code, unit.key());
        // One range each is the pairing an address can cross. A bridge met by
        // a constituent run is a real pairing and a hopeless address: which
        // constituent a block belongs to is exactly what is unknowable.
        if unit.target().len() == 1 && unit.source().len() == 1 {
            paired.insert(sid);
        } else {
            note(&sid, Side::Target, Reason::Bridge, &mut unpaired);
            note(&sid, Side::Source, Reason::Bridge, &mut unpaired);
        }
    }
    for fact in alignment.facts() {
        match *fact {
            AlignmentFact::TargetOnly { key, .. } => note(
                &sid_of(code, key),
                Side::Target,
                Reason::Absent,
                &mut unpaired,
            ),
            AlignmentFact::SourceOnly { key, .. } => note(
                &sid_of(code, key),
                Side::Source,
                Reason::Absent,
                &mut unpaired,
            ),
            AlignmentFact::AmbiguousDuplicate { key, .. } => {
                let sid = sid_of(code, key);
                note(&sid, Side::Target, Reason::Ambiguous, &mut unpaired);
                note(&sid, Side::Source, Reason::Ambiguous, &mut unpaired);
            }
            AlignmentFact::PartialOverlap { target, source, .. } => {
                note(
                    &sid_of(code, target),
                    Side::Target,
                    Reason::Bridge,
                    &mut unpaired,
                );
                note(
                    &sid_of(code, source),
                    Side::Source,
                    Reason::Bridge,
                    &mut unpaired,
                );
            }
        }
    }

    // A sid an address cannot resolve on either side is no pairing at all,
    // however the aligner counted its occurrences.
    for skeleton in [target, source] {
        let mut seen = FxHashSet::default();
        for verse in &skeleton.verses {
            if !seen.insert(verse.sid.as_str()) {
                paired.remove(&verse.sid);
                note(&verse.sid, Side::Target, Reason::Ambiguous, &mut unpaired);
                note(&verse.sid, Side::Source, Reason::Ambiguous, &mut unpaired);
            }
        }
    }
    unpaired.retain(|row| !paired.contains(&row.sid));

    Ok(Pairing { paired, unpaired })
}

/// One registered book as the neutral projection the aligner reads.
fn projected(pantry: &mut Pantry, id: &BookId) -> Result<OnionBook, OverlayError> {
    let entry = pantry
        .book(id)
        .ok_or_else(|| OverlayError::UnknownBook { id: id.clone() })?;
    let text = entry.text()?.to_string();
    let mask = entry.mask()?.clone();
    let toc = entry.toc().clone();
    OnionBook::from_parts(&text, mask, toc).map_err(|error| OverlayError::Input {
        id: id.clone(),
        error,
    })
}

fn sid_of(book: [u8; 3], key: VerseKey) -> String {
    Sid {
        book,
        chapter: key.chapter(),
        first: key.first(),
        last: key.last(),
    }
    .to_string()
}

// --------------------------------------------------------------------- the diff

/// Every edit that makes the target's skeleton the source's, and the account.
pub fn overlay(
    pantry: &mut Pantry,
    target_id: &BookId,
    source_id: &BookId,
    opts: &OverlayOptions,
) -> Result<Overlay, OverlayError> {
    let set = MarkerSet::resolve(opts.markers.as_deref())?;
    let target = extract(pantry, target_id, &set)?;
    let source = extract(pantry, source_id, &set)?;
    let pairing = pair(pantry, target_id, source_id, &target, &source)?;
    // The one place the diff reads the target's bytes: a leading marker has to
    // start its own line, and whether it already does is a byte question.
    let entry = pantry
        .book(target_id)
        .ok_or_else(|| OverlayError::UnknownBook {
            id: target_id.clone(),
        })?;
    let target_text = entry.text()?.as_bytes();

    let (target_groups, source_groups) = (target.groups(), source.groups());

    let mut edits: Vec<Edit> = Vec::new();
    let mut report = OverlayReport {
        collapsed: source
            .collapsed
            .iter()
            .filter(|row| pairing.paired.contains(&row.sid) && in_scope(opts, &row.sid))
            .cloned()
            .collect(),
        unpaired: pairing
            .unpaired
            .iter()
            .filter(|row| in_scope(opts, &row.sid))
            .cloned()
            .collect(),
        ..OverlayReport::default()
    };

    for verse in &target.verses {
        if !pairing.paired.contains(&verse.sid) || !in_scope(opts, &verse.sid) {
            continue;
        }
        for placement in [Placement::Leading, Placement::Inside] {
            let mine = target.rows(&target_groups, &verse.sid, placement, true);
            let theirs = source.rows(&source_groups, &verse.sid, placement, false);
            let at = match placement {
                Placement::Leading => verse.from,
                Placement::Inside => verse
                    .text_to
                    .max(mine.last().map_or(verse.text_to, |row| row.to)),
            };
            for slot in 0..mine.len().max(theirs.len()) {
                match (mine.get(slot), theirs.get(slot)) {
                    (Some(row), Some(want)) if row.marker == want.marker => {}
                    // A target block the source spells differently is one
                    // removal and one insertion at the same node.
                    (Some(row), Some(want)) => {
                        let text = format!("\\{}{}", want.marker, row.tail());
                        push(&mut edits, row.from, row.to, &text);
                        report.removed.push(removed(row));
                        report.inserted.push(Inserted {
                            address: BlockAddress {
                                marker: want.marker.clone(),
                                ..row.address()
                            },
                            marker: want.marker.clone(),
                            at: row.from,
                            empty: false,
                        });
                    }
                    (Some(row), None) => {
                        push(&mut edits, row.from, row.to, "");
                        report.removed.push(removed(row));
                    }
                    (None, Some(want)) => {
                        let text = match placement {
                            // A verse-only Bible often runs `\v` on after the
                            // previous verse's text, so a leading marker opens
                            // the line it needs rather than landing mid-line.
                            Placement::Leading => {
                                match at > 0 && target_text.get(at as usize - 1) != Some(&b'\n') {
                                    true => format!("\n\\{}\n", want.marker),
                                    false => format!("\\{}\n", want.marker),
                                }
                            }
                            Placement::Inside => format!("\n\\{}", want.marker),
                        };
                        push(&mut edits, at, at, &text);
                        report.inserted.push(Inserted {
                            address: want.address(),
                            marker: want.marker.clone(),
                            at,
                            empty: placement == Placement::Inside,
                        });
                    }
                    (None, None) => unreachable!("the loop is bounded by the longer side"),
                }
            }
        }
    }

    // Leading blocks precede their `\v` and inside blocks follow their text,
    // so document order is already ascending; the sort is stable, which is
    // what keeps several inserts at one offset in source order.
    edits.sort_by_key(|edit| edit.from);
    debug_assert!(
        edits
            .windows(2)
            .all(|pair| pair[0].from <= pair[0].to && pair[0].to <= pair[1].from),
        "overlay edits overlap"
    );
    Ok(Overlay { edits, report })
}

/// The overlay applied — the target's own bytes with the source's structure.
pub fn overlay_text(
    pantry: &mut Pantry,
    target_id: &BookId,
    source_id: &BookId,
    opts: &OverlayOptions,
) -> Result<String, OverlayError> {
    let Overlay { edits, .. } = overlay(pantry, target_id, source_id, opts)?;
    let entry = pantry
        .book(target_id)
        .ok_or_else(|| OverlayError::UnknownBook {
            id: target_id.clone(),
        })?;
    let text = entry.text()?;
    let out = crate::onion::edit::apply(text.as_bytes(), &edits);
    Ok(String::from_utf8(out).expect("splicing ASCII markers into UTF-8 keeps it UTF-8"))
}

/// A source block address, answered in the target's coordinates.
pub fn target_node_for(
    pantry: &mut Pantry,
    target_id: &BookId,
    source_id: &BookId,
    address: &BlockAddress,
    opts: &OverlayOptions,
) -> Result<Equivalent, OverlayError> {
    equivalent(pantry, target_id, source_id, address, opts, Side::Target)
}

/// A target block address, answered in the source's.
pub fn source_node_for(
    pantry: &mut Pantry,
    target_id: &BookId,
    source_id: &BookId,
    address: &BlockAddress,
    opts: &OverlayOptions,
) -> Result<Equivalent, OverlayError> {
    equivalent(pantry, target_id, source_id, address, opts, Side::Source)
}

/// `want` is the side being asked ABOUT: the target for `targetNodeFor`.
fn equivalent(
    pantry: &mut Pantry,
    target_id: &BookId,
    source_id: &BookId,
    address: &BlockAddress,
    opts: &OverlayOptions,
    want: Side,
) -> Result<Equivalent, OverlayError> {
    let set = MarkerSet::resolve(opts.markers.as_deref())?;
    let target = extract(pantry, target_id, &set)?;
    let source = extract(pantry, source_id, &set)?;
    let pairing = pair(pantry, target_id, source_id, &target, &source)?;
    if !pairing.paired.contains(&address.sid) {
        return Ok(Equivalent::Unpaired {
            unpaired: true,
            reason: pairing
                .unpaired
                .iter()
                .find(|row| row.sid == address.sid)
                .map_or(Reason::Absent, |row| row.reason),
        });
    }
    // The address was taken on the OTHER side: `targetNodeFor` is handed a
    // source address, `sourceNodeFor` a target one. Check it there before
    // answering about anything.
    let (target_groups, source_groups) = (target.groups(), source.groups());
    let (named, named_groups, named_keeps_empty) = match want {
        Side::Target => (&source, &source_groups, false),
        Side::Source => (&target, &target_groups, true),
    };
    let stale = named
        .rows(
            named_groups,
            &address.sid,
            address.placement,
            named_keeps_empty,
        )
        .into_iter()
        .find(|row| row.ordinal == address.ordinal)
        .filter(|row| row.marker != address.marker);
    if let Some(row) = stale {
        return Err(OverlayError::StaleAddress {
            address: address.clone(),
            found: row.marker,
        });
    }
    let (here, groups, keep_empty) = match want {
        Side::Target => (&target, &target_groups, true),
        Side::Source => (&source, &source_groups, false),
    };
    let rows = here.rows(groups, &address.sid, address.placement, keep_empty);
    if let Some(row) = rows.into_iter().find(|row| row.ordinal == address.ordinal) {
        return Ok(Equivalent::Found { found: row });
    }
    let verse = here
        .verse(&address.sid)
        .expect("a paired sid has a verse on both sides");
    let insert_at = match address.placement {
        Placement::Leading => verse.from,
        Placement::Inside => here
            .rows(groups, &address.sid, Placement::Inside, keep_empty)
            .iter()
            .map(|row| row.to)
            .fold(verse.text_to, u32::max),
    };
    Ok(Equivalent::Absent {
        absent: true,
        insert_at,
        placement: address.placement,
    })
}

fn removed(row: &SkeletonRow) -> Removed {
    Removed {
        address: row.address(),
        marker: row.marker.clone(),
        from: row.from,
        to: row.to,
    }
}

/// Inserts one edit, splitting text no [`FixStr`] can carry into adjacent
/// same-position edits, which concatenate on apply.
fn push(edits: &mut Vec<Edit>, from: u32, to: u32, insert: &str) {
    if insert.is_empty() {
        edits.push(Edit {
            from,
            to,
            insert: FixStr::EMPTY,
        });
        return;
    }
    let mut span = Some((from, to));
    for chunk in insert.as_bytes().chunks(FixStr::CAP) {
        let (from, to) = span.take().unwrap_or((to, to));
        edits.push(Edit {
            from,
            to,
            insert: FixStr::new(chunk),
        });
    }
}

fn in_scope(opts: &OverlayOptions, sid: &str) -> bool {
    match &opts.scope {
        None => true,
        Some(Scope::Sid(wanted)) => wanted == sid,
        Some(Scope::Chapter(chapter)) => chapter_of(sid) == Some(*chapter),
    }
}

/// `"GEN 2:21"` → `2`.
fn chapter_of(sid: &str) -> Option<u16> {
    let rest = sid.split_once(' ')?.1;
    rest.split(':').next()?.parse().ok()
}
