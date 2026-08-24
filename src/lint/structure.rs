//! The STRUCTURE machine: node close verdicts, empty paragraphs, orphan
//! closers, and the insertion fixes computed at a close.

use std::ops::Range;

use super::walk::{NODE_ID_BIT, is_structural_ws, span_of};
use super::{Code, Doc, Emit, NO_TOKEN, Observation};
use crate::cst::{CloseReason, Cst, Node};
use crate::tables::generated;
use crate::tables::schema::{
    Category as MarkerCategory, ClosingBehavior, MarkerKind, StructuralWhitespaceRequirement as Ws,
};
use crate::{Token, TokenKind};

/// What a node IS to the structural rules. The CST does not store this — it is
/// derivable from the opening token plus one table read, and the walker's own
/// `FrameRole` is private to the build.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Paragraphs, characters, notes, sidebars, rows, cells.
    Plain,
    /// A milestone point: the tiny frame from the milestone token to its `\*`.
    Point,
    /// A U25003 list/table container.
    Container,
}

/// The walker's own Recovery-vs-Implicit predicate, read from the same column.
/// One notion of "this row wanted a closer" or none.
fn wants_closer(marker_idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::closing(marker_idx),
        ClosingBehavior::RequiredExplicit | ClosingBehavior::SelfClosingMilestone
    )
}

fn is_container_row(marker_idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::category(marker_idx),
        MarkerCategory::MilestoneList | MarkerCategory::MilestoneTable
    )
}

/// A container start pushes TWO nodes back to back from one token — the
/// container, then its `-s` point inside it — so the pair is identified by
/// adjacent ids sharing a token, and the container is always the lower id.
fn shape_of(tokens: &[Token], cst: &Cst, id: usize) -> Shape {
    let node = &cst.nodes[id];
    let token = &tokens[node.token as usize];
    match token.kind() {
        TokenKind::Milestone { end } => {
            if !end
                && is_container_row(token.marker_idx)
                && cst
                    .nodes
                    .get(id + 1)
                    .is_some_and(|next| next.token == node.token)
            {
                Shape::Container
            } else {
                Shape::Point
            }
        }
        // A KNOWN milestone row in its bare spelling (`\ts \*`) is a point too
        // — the row says so and the walker treats it that way.
        TokenKind::Marker { .. } if generated::kind(token.marker_idx) == MarkerKind::Milestone => {
            Shape::Point
        }
        _ => Shape::Plain,
    }
}

/// Node close verdicts, empty paragraphs, and the orphan closers.
///
/// The verdict half is a match on the walker's [`CloseReason`] — lint reads it,
/// never re-derives it — and it is where four of the five INSERTION fixes are
/// computed, because the close event is the one place both the missing text (the
/// opening token's own spelling) and its position ([`Cst::extent`]) are in hand.
///
/// The orphan half is the only state, and it is TWO u32s rather than a
/// `tokens.len()` bitset plus a `nodes.len()` one.
///
/// A `\X*`/`\*` leaf and a container's `-e` point node ask the same question —
/// "am I the last child of the thing that consumed me?" — and a last child is
/// always followed immediately by its parent's close, so the verdict is settled
/// at the NEXT NODE CLOSE and nowhere else: what is pending is consumed iff the
/// closing node is `Explicit` and its last child is the pending id. Leaves in
/// between need no work, because a leaf after the pending id is exactly what
/// makes it not-last. The one leaf that must act is a SECOND closer arriving
/// before any close: the first can no longer be anyone's last child.
pub(crate) struct Structure {
    /// A closer leaf awaiting its verdict, or [`NO_TOKEN`].
    pending_closer: u32,
    /// Whether that pending closer is a bare `\*` (which reports a different
    /// code and splices the same way).
    pending_is_terminator: bool,
    /// A container `-e` point node awaiting its verdict, or [`NO_TOKEN`].
    pending_end: u32,
    /// `(observation slot, node id)` per empty paragraph, resolved into a fix at
    /// [`Self::finish`] — the repair depends on the paragraph that DISPLACED it,
    /// which the close event has not reached. The only allocation in this
    /// machine, and it stays empty on a book with no empty paragraph.
    empty_paragraphs: Vec<(u32, u32)>,
}

impl Structure {
    pub(crate) fn new() -> Self {
        Self {
            pending_closer: NO_TOKEN,
            pending_is_terminator: false,
            pending_end: NO_TOKEN,
            empty_paragraphs: Vec::new(),
        }
    }

    /// Resolve whatever is pending against the node now closing. `closing` is
    /// [`NO_TOKEN`] at end of input, where nothing can have consumed anything.
    ///
    /// `#[inline(never)]`: the emission path is large and the walk loop wants to
    /// stay small, and this runs once per node rather than once per token.
    #[inline(never)]
    fn resolve(&mut self, doc: &Doc, closing: u32, out: &mut Emit) {
        if self.pending_closer != NO_TOKEN {
            let token = self.pending_closer;
            self.pending_closer = NO_TOKEN;
            if !consumed_by(doc, closing, token) {
                self.orphan(doc, token, out);
            }
        }
        if self.pending_end != NO_TOKEN {
            let id = self.pending_end;
            self.pending_end = NO_TOKEN;
            let ended = closing != NO_TOKEN
                && consumed_by(doc, closing, NODE_ID_BIT | id)
                && shape_of(doc.tokens, doc.cst, closing as usize) == Shape::Container;
            if !ended {
                out.push(Observation::one(
                    Code::OrphanContainerEnd,
                    doc.cst.nodes[id as usize].token,
                ));
            }
        }
    }

    /// A closer that closed nothing, and the splice that removes it.
    #[inline(never)]
    fn orphan(&mut self, doc: &Doc, token: u32, out: &mut Emit) {
        let code = if self.pending_is_terminator {
            Code::OrphanTerminator
        } else {
            Code::OrphanCloser
        };
        // A PLAIN span delete, whitespace left exactly as written: deleting
        // `\f*` out of `text \f* more` leaves two spaces, and eating one is a
        // second edit nobody asked for. Extra horizontal whitespace is legal
        // everywhere in USFM — a formatter's business, not a fix's.
        let span = &doc.tokens[token as usize];
        out.push_fixed(Observation::one(code, token), span.start, span.end(), b"");
    }

    /// The ONLY per-leaf work: remember a closer, and flush a previous one that
    /// this arrival has just disqualified.
    #[inline]
    pub(crate) fn on_leaf(&mut self, doc: &Doc, idx: u32, kind: TokenKind, out: &mut Emit) {
        let terminator = match kind {
            TokenKind::ClosingMarker { .. } => false,
            TokenKind::MilestoneTerminator => true,
            _ => return,
        };
        if self.pending_closer != NO_TOKEN {
            // Two closers with no close between them: the first can no longer
            // be any node's last child.
            let stale = self.pending_closer;
            self.orphan(doc, stale, out);
        }
        self.pending_closer = idx;
        self.pending_is_terminator = terminator;
    }

    /// The document ended with something still pending — nothing can have
    /// consumed it. (A closer that is the ROOT's last child lands here: the root
    /// is never closed.)
    pub(crate) fn finish(&mut self, doc: &Doc, out: &mut Emit) {
        self.resolve(doc, NO_TOKEN, out);
        // The FORMATTER half of `empty-paragraph`, in the one place the whole
        // token stream is in hand — the same finish-time shape the fused
        // experiment already needs for its renumber correction.
        for (slot, id) in std::mem::take(&mut self.empty_paragraphs) {
            if let Some(extent) = duplicated_by_the_next_paragraph(doc, id) {
                out.attach_fixed(slot, extent.start, extent.end, b"");
            }
        }
    }

    pub(crate) fn on_node_close(&mut self, doc: &Doc, id: u32, node: &Node, out: &mut Emit) {
        if self.pending_closer != NO_TOKEN || self.pending_end != NO_TOKEN {
            self.resolve(doc, id, out);
        }

        let (source, tokens, cst) = (doc.source, doc.tokens, doc.cst);
        let anchor = node.token;
        let opener = &tokens[anchor as usize];
        let marker_idx = opener.marker_idx;

        // A container's `-e` point claims to end a container; whether it did is
        // the PARENT's close to say, which is the very next event if it did.
        if opener.kind() == (TokenKind::Milestone { end: true }) && is_container_row(marker_idx) {
            self.pending_end = id;
        }

        // A paragraph with nothing under it but line endings. `\b` abstains by
        // its `SingleNewline` delimiter: the column that says "takes no
        // content".
        if generated::kind(marker_idx) == MarkerKind::Paragraph
            && generated::ws_after_name(marker_idx) != Ws::SingleNewline
            && is_empty_paragraph(tokens, cst, node)
        {
            self.empty_paragraphs
                .push((out.observations.len() as u32, id));
            out.push(Observation::one(Code::EmptyParagraph, anchor));
        }

        // THE FAST OUT, most of this machine's budget: 1.73M of en_ult's 1.76M
        // nodes close Explicit with no verdict to report, so the shape question
        // (a row read plus a peek at the next node) is asked only of the rest.
        let reason = node.close_reason();
        if matches!(reason, CloseReason::Explicit | CloseReason::Implicit) {
            return;
        }
        let shape = shape_of(tokens, cst, id as usize);

        let code = match reason {
            // The walker judged these normal — note peers and displaced
            // paragraphs land here, and both are silent.
            CloseReason::Explicit | CloseReason::Implicit => None,
            CloseReason::Recovery => Some(match shape {
                Shape::Container => Code::UnterminatedContainer,
                Shape::Point => Code::UnterminatedMilestone,
                Shape::Plain => match generated::kind(marker_idx) {
                    MarkerKind::Note => Code::UnclosedNote,
                    MarkerKind::Character => Code::UnclosedChar,
                    // Defensive: only RequiredExplicit/SelfClosingMilestone
                    // rows are stamped Recovery, and on a Plain node that is a
                    // note or a character marker. A third closer-wanting kind
                    // lands here rather than panicking on real data.
                    _ => Code::UnclosedAtEof,
                },
            }),
            CloseReason::Eof => match shape {
                Shape::Point => Some(Code::UnterminatedMilestone),
                _ if wants_closer(marker_idx) => Some(Code::UnclosedAtEof),
                _ => None,
            },
        };
        let Some(code) = code else { return };

        // All five findings share one repair — write the ending the author left
        // out — so the TEXT depends on the node's shape, not on the code.
        let mut buf = [0u8; CLOSER_CAP];
        let text = match shape {
            Shape::Container => container_end_text(marker_idx, &mut buf),
            Shape::Point => Some(&b"\\*"[..]),
            Shape::Plain => closer_text(span_of(source, &tokens[anchor as usize]), &mut buf),
        };
        let observation = Observation::one(code, anchor);
        match text {
            Some(text) => {
                let at = content_end(
                    source,
                    cst.extent(id, tokens),
                    tokens[anchor as usize].end(),
                );
                out.push_fixed(observation, at, at, text);
            }
            None => out.push(observation),
        }
    }
}

/// Did the node now closing consume `child` — i.e. is `child` its last child,
/// and did the walker call that an explicit close?
///
/// `child` is a raw token id or a [`NODE_ID_BIT`]-tagged node id, exactly as
/// the arena stores it. `closing` is [`NO_TOKEN`] for any event that is not a
/// node close, which can never consume anything.
#[inline]
fn consumed_by(doc: &Doc, closing: u32, child: u32) -> bool {
    if closing == NO_TOKEN {
        return false;
    }
    let node = &doc.cst.nodes[closing as usize];
    node.close_reason() == CloseReason::Explicit && last_child(doc.cst, node) == Some(&child)
}

/// The scratch every "write the missing ending" fix builds into. Enough for the
/// table's two worst cases — `\+` + the longest name (6 bytes) + `*`, and
/// `\table-e\*` — with room to spare. Unrelated to [`FixStr::CAP`]: longer text
/// splits, and this is only how much is composed at once.
const CLOSER_CAP: usize = 16;

/// The closer the author never wrote, in THEIR spelling: the opening token's own
/// bytes with the folded delimiter trimmed, plus `*`.
///
/// The row NAME would be wrong twice over — being canonical, it turns `\+nd`
/// into `\nd*` (losing the nesting) and `\q2` into `\q*` (losing the level). The
/// token's bytes are the only record of the spelling in force.
///
/// `None` — no fix — for a span we cannot re-emit as short ASCII: unreachable on
/// today's table, and cheaper than being wrong if configured rows ever arrive.
fn closer_text<'a>(span: &[u8], buf: &'a mut [u8; CLOSER_CAP]) -> Option<&'a [u8]> {
    let name = trim_end_ws(span);
    if name.len() + 1 > buf.len() || !name.is_ascii() {
        return None;
    }
    buf[..name.len()].copy_from_slice(name);
    buf[name.len()] = b'*';
    Some(&buf[..name.len() + 1])
}

/// A U25003 container's end milestone: `\list-e\*`, `\table-e\*`.
///
/// Built from the ROW name, unlike [`closer_text`]: the `-s`/`-e` pair is the
/// row's own spelling of open and close, and a bare `\list` opens a container
/// too, whose ending is still `\list-e\*`.
fn container_end_text(
    marker_idx: generated::MarkerIdx,
    buf: &mut [u8; CLOSER_CAP],
) -> Option<&[u8]> {
    let name = generated::name(marker_idx).as_bytes();
    let end = name.len() + b"\\-e\\*".len();
    if name.is_empty() || end > buf.len() {
        return None;
    }
    buf[0] = b'\\';
    buf[1..=name.len()].copy_from_slice(name);
    buf[name.len() + 1..end].copy_from_slice(b"-e\\*");
    Some(&buf[..end])
}

/// Where a missing ending BELONGS: the node's last content byte.
///
/// The extent END is a token boundary, and the last token of an unclosed node is
/// often the newline that ended the line, so inserting there strands the closer
/// on the next line's doorstep (`\ft note\n\f*\v 5`). Backing off over trailing
/// structural whitespace puts it where an author would type it
/// (`\ft note\f*\n\v 5`). The `floor` — the opening marker's span end — keeps
/// the backoff out of the marker, so `\add ` gets `\add \add*`.
fn content_end(source: &[u8], extent: Range<u32>, floor: u32) -> u32 {
    let mut at = extent.end;
    while at > floor && at > extent.start && is_structural_ws(source[at as usize - 1]) {
        at -= 1;
    }
    at
}

/// The empty paragraph's extent — its marker plus the line endings under it —
/// WHEN deleting it is unambiguous: the paragraph that displaced it is spelled
/// exactly the same, so `\p\p` is plain duplication.
///
/// A MIXED pair (`\m\p`) says nothing about which of the two was intended, so it
/// gets no fix at all: the diagnostic points and a human chooses.
fn duplicated_by_the_next_paragraph(doc: &Doc, id: u32) -> Option<Range<u32>> {
    let extent = doc.cst.extent(id, doc.tokens);
    // Tokens partition the source, so the displacer is one binary search away.
    let at = doc.tokens.partition_point(|token| token.start < extent.end);
    let next = doc.tokens.get(at)?;
    if !matches!(next.kind(), TokenKind::Marker { .. })
        || generated::kind(next.marker_idx) != MarkerKind::Paragraph
    {
        return None;
    }
    let opener = &doc.tokens[doc.cst.nodes[id as usize].token as usize];
    let same = trim_end_ws(span_of(doc.source, next)) == trim_end_ws(span_of(doc.source, opener));
    (same && holds_content(doc, at + 1)).then_some(extent)
}

/// Does the paragraph that starts at `from` hold anything?
///
/// A CHAIN of identical empties (`\p\n\p\n\q1`, real in en_ulb ISA) would
/// otherwise offer every member a fix "repaired by" the next member, which is no
/// repair at all: the fix oracle judges a fix by its own SITE, and the site is
/// still empty afterwards. Only the empty whose survivor holds something is
/// duplication.
fn holds_content(doc: &Doc, from: usize) -> bool {
    doc.tokens[from..]
        .iter()
        .find_map(|token| match token.kind() {
            TokenKind::Newline => None,
            // The markers that DISPLACE a paragraph rather than living inside
            // one: reaching one means nothing came between.
            TokenKind::Marker { .. }
                if matches!(
                    generated::kind(token.marker_idx),
                    MarkerKind::Paragraph
                        | MarkerKind::Chapter
                        | MarkerKind::Sidebar
                        | MarkerKind::Periph
                        | MarkerKind::Header
                        | MarkerKind::TableRow
                        | MarkerKind::Unknown
                ) =>
            {
                Some(false)
            }
            _ => Some(true),
        })
        // End of file: nothing came after, so nothing is in it.
        .unwrap_or(false)
}

/// A span with its trailing space/tab/newline run removed.
fn trim_end_ws(span: &[u8]) -> &[u8] {
    let mut end = span.len();
    while end > 0 && is_structural_ws(span[end - 1]) {
        end -= 1;
    }
    &span[..end]
}

/// Does this node hold anything a reader would see? A child NODE always counts;
/// among token children only the node's own OPENING marker (filed as its first
/// child) and a `Newline` do not. Attribute lists DO count — `\p|cat="x"|` with
/// nothing after it carries metadata, which is a deliberate authoring fact and
/// not an empty paragraph.
fn is_empty_paragraph(tokens: &[Token], cst: &Cst, node: &Node) -> bool {
    cst.child_ids[node.children.start as usize..node.children.end as usize]
        .iter()
        .all(|child| {
            *child & NODE_ID_BIT == 0
                && (*child == node.token || tokens[*child as usize].kind() == TokenKind::Newline)
        })
}

fn last_child<'a>(cst: &'a Cst, node: &Node) -> Option<&'a u32> {
    if node.children.is_empty() {
        return None;
    }
    cst.child_ids.get(node.children.end as usize - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::tests::{codes, findings, token_named};

    #[test]
    fn unclosed_note() {
        // `\c` cannot live inside a footnote, so the walker stamps the note
        // Recovery — the shape of both live corpus findings.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\\f + \\ft note\\c 2\n\\p b");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::UnclosedNote,
                token_named(&tokens, "f", 0)
            )]
        );
    }

    #[test]
    fn a_note_holding_nested_character_markers_is_silent() {
        // bsb GEN 2:4: the note's `\fq` holds a `\+nd`. Character rows carry
        // Footnote in their context masks, so the char NESTS instead of
        // displacing the note, and this well-formed note is clean end to end.
        let (_, obs) = findings(
            "\\c 2\n\\p \\v 1 in the beginning\n\\v 2 more\n\\v 3 more\n\\v 4 the \\nd Lord\\nd*\\f + \\fr 2:4 \\fq \\+nd Lord\\+nd*\\ft or \\fq \\+nd God\\+nd*\\ft , the proper name.\\f* God made them.",
        );
        assert_eq!(codes(&obs), Vec::<Code>::new());
    }

    #[test]
    fn unclosed_char() {
        // `\add*` pops THROUGH the still-open `\w`, which the walker stamps
        // Recovery because character rows require an explicit closer.
        let (tokens, obs) = findings("\\p \\add a \\w b\\add*");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::UnclosedChar,
                token_named(&tokens, "w", 0)
            )]
        );
    }

    #[test]
    fn unclosed_at_eof() {
        // The paragraph is Eof too and stays silent: its row wants no closer.
        let (tokens, obs) = findings("\\p \\add a");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::UnclosedAtEof,
                token_named(&tokens, "add", 0)
            )]
        );
    }

    #[test]
    fn unterminated_container() {
        let (tokens, obs) = findings("\\list-s\\*\n\\li a\n\\p prose");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::UnterminatedContainer,
                token_named(&tokens, "list", 0)
            )]
        );
    }

    #[test]
    fn unterminated_milestone() {
        let (tokens, obs) = findings("\\p a \\ts-s\\* b \\qt-s |who=\"Levi\"");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::UnterminatedMilestone,
                token_named(&tokens, "qt", 0)
            )]
        );
    }

    #[test]
    fn orphan_closer() {
        let (tokens, obs) = findings("\\p text\\w* more");
        let closer = tokens
            .iter()
            .position(|t| matches!(t.kind(), TokenKind::ClosingMarker { .. }))
            .unwrap() as u32;
        assert_eq!(obs, vec![Observation::one(Code::OrphanCloser, closer)]);
    }

    #[test]
    fn orphan_terminator() {
        let (tokens, obs) = findings("\\p text \\* more");
        let star = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::MilestoneTerminator)
            .unwrap() as u32;
        assert_eq!(obs, vec![Observation::one(Code::OrphanTerminator, star)]);
    }

    #[test]
    fn orphan_container_end() {
        // The `-e` point closes Explicit at its own `\*`: the CONTAINER it
        // claims to end never opened.
        let (tokens, obs) = findings("\\p text\n\\list-e\\*");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::OrphanContainerEnd,
                token_named(&tokens, "list", 0)
            )]
        );

        // A matched pair says nothing.
        let (_, obs) = findings("\\list-s\\*\n\\li a\n\\list-e\\*");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn empty_paragraph_excludes_the_blank_line() {
        let (tokens, obs) = findings("\\c 1\n\\p\n\\p text\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::EmptyParagraph,
                token_named(&tokens, "p", 0)
            )]
        );

        // `\b` IS an empty paragraph, and is empty by design.
        let (_, obs) = findings("\\c 1\n\\p a\n\\b\n\\p b\n");
        assert_eq!(obs, vec![]);

        // A verse, a nested node, or plain text all count as content.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a\n\\q1 b\n\\q2 \\add c\\add*\n");
        assert_eq!(obs, vec![]);
    }
}
