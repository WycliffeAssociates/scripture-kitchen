//! The driver: [`Doc`], the [`Emit`] sink, and THE WALK that feeds the four
//! machines.

use super::fix::Fix;
use super::{Ancestry, Flat, LintReport, NO_FIX, Observation, Ordering, Structure, UsfmVersion};
use crate::Token;
use crate::cst::Cst;
use crate::edit::{Edit, FixStr};

pub(super) const NODE_ID_BIT: u32 = 1 << 31;

/// The sink every pass writes findings through, and the only place a fix is
/// attached to one.
///
/// It exists because an [`Observation`] cannot carry its fix: the four-u32 shape
/// is pinned, so the link is the parallel `fix_of` vec, and a parallel vec must
/// be permuted with its partner when the findings are sorted. One sink keeps
/// that pairing in a single place instead of at forty push sites.
#[derive(Default)]
pub(crate) struct Emit {
    pub(crate) observations: Vec<Observation>,
    pub(crate) fix_of: Vec<u32>,
    pub(crate) fixes: Vec<Fix>,
    pub(crate) edit_list: Vec<Edit>,
}

impl Emit {
    /// A finding with no repair — the common case, and the shape every phase
    /// 1-3 call site already had.
    pub(crate) fn push(&mut self, observation: Observation) {
        self.observations.push(observation);
        self.fix_of.push(NO_FIX);
    }

    /// A finding AND the byte splice that repairs it: `from..to` replaced by
    /// `text` (equal ends = a pure insertion, empty `text` = a pure deletion).
    ///
    /// Text longer than a [`FixStr`] becomes a chain of same-position edits
    /// which concatenate; only the FIRST carries the replaced range, so applying
    /// right to left splices once and then inserts the tail behind it.
    ///
    /// The label comes from the row, which is therefore the single declaration
    /// of "this rule offers a fix" — a code that emits one without declaring it
    /// is a table bug and fails loudly here rather than in a consumer.
    pub(crate) fn push_fixed(&mut self, observation: Observation, from: u32, to: u32, text: &[u8]) {
        let label = observation
            .code
            .row()
            .fix_label
            .expect("a code that emits a fix declares its label");
        let start = self.edit_list.len() as u32;
        let mut rest = text;
        loop {
            let take = rest.len().min(FixStr::CAP);
            let (from, to) = if self.edit_list.len() as u32 == start {
                (from, to)
            } else {
                (to, to)
            };
            self.edit_list.push(Edit {
                from,
                to,
                insert: FixStr::new(&rest[..take]),
            });
            rest = &rest[take..];
            if rest.is_empty() {
                break;
            }
        }
        let fix = self.fixes.len() as u32;
        self.fixes.push(Fix {
            label,
            edits: start..self.edit_list.len() as u32,
        });
        self.observations.push(observation);
        self.fix_of.push(fix);
    }

    /// Document order, applied to the observations and their fix links at once.
    ///
    /// Findings are rare relative to tokens, so one sort at the end is cheaper
    /// than threading document order through four passes that each have a
    /// natural order of their own. It sorts a PERMUTATION because `fix_of` has
    /// to travel with its partner; both gathers are over a vec whose length is
    /// the finding count, not the token count.
    pub(crate) fn finish(
        self,
        book: Option<u32>,
        declared_version: Option<UsfmVersion>,
    ) -> LintReport {
        let mut order: Vec<u32> = (0..self.observations.len() as u32).collect();
        order.sort_unstable_by_key(|slot| {
            let obs = self.observations[*slot as usize];
            (obs.anchor, obs.code as u16)
        });
        LintReport {
            book,
            declared_version,
            observations: order
                .iter()
                .map(|slot| self.observations[*slot as usize])
                .collect(),
            fix_of: order
                .iter()
                .map(|slot| self.fix_of[*slot as usize])
                .collect(),
            fixes: self.fixes,
            edit_list: self.edit_list,
        }
    }
}

/// The three read-only slices every machine reads through — one argument
/// instead of three at every event, and the thing a future Builder-driven
/// pipeline replaces with its own live view.
pub(crate) struct Doc<'a> {
    pub(crate) source: &'a [u8],
    pub(crate) tokens: &'a [Token],
    pub(crate) cst: &'a Cst,
}

/// One frame of the driver's own stack: where this node's child list has got
/// to, and which node it is (so the close event can name it).
struct Frame {
    next: u32,
    end: u32,
    node: u32,
    /// [`Ancestry`]'s two bits for this frame, handed back at close.
    ///
    /// The machines keep no copy of this stack — but a fact a machine computed
    /// at OPEN and needs again at CLOSE has to live somewhere, and the frame is
    /// where it is already paid for. A Builder-driven pipeline carries the same
    /// byte on its own frames, which is why this is a machine-agnostic scratch
    /// and not an ancestry field.
    scratch: u8,
}

/// THE WALK. One in-order pass over the CST, feeding four state machines.
///
/// Every token index is delivered exactly once, in document order, as an
/// `on_leaf` event — the lint-side reading of the lifted partition oracle
/// (`cst.in_order()` recovers `0..tokens.len()`), and asserted as such by the
/// debug counter below. A node's OPENING marker is one of those
/// leaves: the walker files it as the node's own first child, so it arrives
/// after that node's open event and inside its frame. Its explicit closer,
/// symmetrically, is the LAST child and arrives immediately before the close.
///
/// That last fact is why no consumed-closer bitset exists any more: a closer
/// that closed something is by construction the last child of an `Explicit`
/// node, so the very next event after it is that node's close. [`Structure`]
/// holds one token of pending state and judges the closer on the following
/// event instead of on a `tokens.len()`-sized side table.
///
/// The machines are plain structs with explicit state and no view of the walk's
/// stack: the driver owns the stack, and the one per-frame fact a machine needs
/// twice rides [`Frame::scratch`], a byte the driver hands back at close. That
/// is what makes them FEEDABLE — tomorrow's driver is the CST Builder's own
/// frame stack (planning/investigate-later.md, "Single-pass pipeline"), which
/// can carry the same byte — and nothing here assumes a slice of tokens exists
/// ahead of the cursor except the one token of lookahead
/// `attr-terminator-mismatch` still wants, which is noted where it lives.
pub(super) fn walk(doc: &Doc, version: Option<UsfmVersion>, out: &mut Emit) {
    let mut structure = Structure::new();
    let mut ancestry = Ancestry::new();
    let mut ordering = Ordering::new();
    let mut flat = Flat::new(version);

    let root = &doc.cst.nodes[0];
    // The CURRENT frame lives in locals and only the ancestors live in the vec:
    // every iteration touches `cur.next`, and re-deriving it through
    // `stack.last_mut()` each time costs a load and a bounds check on the
    // hottest line in lint.
    let mut cur = Frame {
        next: root.children.start,
        end: root.children.end,
        node: 0,
        scratch: 0,
    };
    let mut stack: Vec<Frame> = Vec::new();
    // Debug-only: the partition oracle, echoed on the lint side. Every leaf
    // event is the next token index, so the walk delivers each token exactly
    // once and in order — the property the whole fusion rests on.
    #[cfg(debug_assertions)]
    let mut expected_leaf = 0u32;

    loop {
        if cur.next == cur.end {
            // The root is never closed: its "close" is `finish`.
            let Some(parent) = stack.pop() else { break };
            let node = &doc.cst.nodes[cur.node as usize];
            structure.on_node_close(doc, cur.node, node, out);
            ancestry.on_node_close(cur.scratch);
            cur = parent;
            continue;
        }
        let child = doc.cst.child_ids[cur.next as usize];
        cur.next += 1;

        if child & NODE_ID_BIT != 0 {
            let id = child & !NODE_ID_BIT;
            let node = &doc.cst.nodes[id as usize];
            let scratch = ancestry.on_node_open(doc, node);
            stack.push(cur);
            cur = Frame {
                next: node.children.start,
                end: node.children.end,
                node: id,
                scratch,
            };
            continue;
        }

        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(
                child, expected_leaf,
                "the walk must deliver every token index exactly once, in order"
            );
            expected_leaf += 1;
        }
        let token = &doc.tokens[child as usize];
        // Decoded ONCE and handed round: four machines that each asked the
        // token for its kind would pay for four decodes and four dispatch
        // trees over the same byte.
        let kind = token.kind();
        structure.on_leaf(doc, child, kind, out);
        ancestry.on_leaf(doc, child, token, kind, out);
        ordering.on_leaf(doc, child, token, kind, out);
        flat.on_leaf(doc, child, token, kind, out);
    }

    #[cfg(debug_assertions)]
    debug_assert_eq!(
        expected_leaf as usize,
        doc.tokens.len(),
        "the walk must deliver every token"
    );

    structure.finish(doc, out);
    ordering.finish(out);
}

/// A token's bytes. Lint reads `source` only through spans the scanner already
/// carved, plus the single byte on either side of one (the two Form rules).
pub(super) fn span_of<'a>(source: &'a [u8], token: &Token) -> &'a [u8] {
    &source[token.start as usize..token.end() as usize]
}

/// Space, tab, CR or LF — the four bytes the scanner treats as whitespace.
/// Deliberately ASCII-only: it is the SPEC's structural whitespace, and a
/// no-break space failing this test is the finding, not a gap.
pub(super) fn is_structural_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}
