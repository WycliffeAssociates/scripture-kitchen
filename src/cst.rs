//! A compact structural tree over the scanner's stamped token stream.
//!
//! This module owns the first CST pass: one generic scope walker. It keeps
//! pending children in one scratch tail, moves each closed tail into the
//! shared child-id arena, and exposes document order through [`Cst::in_order`].
//! Explicit closers, recovery pop-all, milestone pairing, and positional
//! context are deliberately separate follow-up passes.

use std::ops::Range;

use crate::tables::generated;
use crate::tables::schema::{ClosingBehavior, MarkerKind, ScopeKind, SpecContext};
use crate::{Token, TokenKind};

const NODE_ID_BIT: u32 = 1 << 31;
const ROOT_TOKEN: u32 = u32::MAX;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The marker that owns the frame supplied its explicit closer.
    Explicit = 0,
    /// The grammar ends this frame when a new compatible scope begins.
    Implicit = 1,
    /// The walker had to end a frame that requires an explicit closer.
    Recovery = 2,
    /// The input ended while the frame was still open.
    Eof = 3,
}

impl CloseReason {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Explicit,
            1 => Self::Implicit,
            2 => Self::Recovery,
            3 => Self::Eof,
            _ => unreachable!("invalid close reason {value}"),
        }
    }
}

/// One structural node. The node id is its index in [`Cst::nodes`].
///
/// `children` indexes [`Cst::child_ids`], not `nodes`: the arena stores a
/// mixed document-order stream of token ids and tagged node ids. The opening
/// marker is the first token in its own child list and is also copied into
/// `token` for direct access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The opening marker's token index. `u32::MAX` identifies the root.
    pub token: u32,
    /// The node's mixed child-id range in the shared arena.
    pub children: Range<u32>,
    /// Encoded [`CloseReason`]. Use [`Self::close_reason`] at call sites.
    pub reason: u8,
    /// The stamped [`SpecContext`] encoded by its enum discriminant.
    pub ctx: u8,
}

impl Node {
    /// Returns the walker verdict without making consumers know the packed form.
    pub fn close_reason(&self) -> CloseReason {
        CloseReason::from_u8(self.reason)
    }

    /// Returns the context stamped when this node was opened.
    pub fn context(&self) -> SpecContext {
        SpecContext::from_u8(self.ctx)
    }
}

/// The flat CST and its mixed child-id arena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cst {
    pub nodes: Vec<Node>,
    pub child_ids: Vec<u32>,
}

impl Cst {
    /// Walks leaf token ids in document order, independent of close order.
    pub fn in_order(&self) -> InOrder<'_> {
        InOrder {
            cst: self,
            stack: vec![ChildCursor::for_node(self, 0)],
        }
    }
}

/// The allocation-backed depth-first iterator used by every in-order consumer.
pub struct InOrder<'a> {
    cst: &'a Cst,
    stack: Vec<ChildCursor>,
}

struct ChildCursor {
    next: u32,
    end: u32,
}

impl ChildCursor {
    fn for_node(cst: &Cst, node: u32) -> Self {
        let children = &cst.nodes[node as usize].children;
        Self {
            next: children.start,
            end: children.end,
        }
    }
}

impl Iterator for InOrder<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cursor = self.stack.last_mut()?;
            if cursor.next == cursor.end {
                self.stack.pop();
                continue;
            }

            let child_id = self.cst.child_ids[cursor.next as usize];
            cursor.next += 1;
            if child_id & NODE_ID_BIT != 0 {
                self.stack
                    .push(ChildCursor::for_node(self.cst, child_id & !NODE_ID_BIT));
            } else {
                return Some(child_id);
            }
        }
    }
}

struct Frame {
    node: u32,
    /// The open frame owns the scratch tail beginning at this index.
    mark: u32,
    /// The context resolved once at push time; transparent frames inherit it.
    ctx: u8,
}

/// Builds a CST from scanner-stamped tokens.
///
/// Opening markers are included as the first leaf in their own node. This
/// keeps the token partition complete while `Node::token` gives consumers a
/// direct opening-marker handle. Only a scope-opening row, or a chapter/verse
/// point with a non-empty context mask, may displace the current stack.
pub fn build(tokens: &[Token]) -> Cst {
    assert!(
        tokens.len() < NODE_ID_BIT as usize,
        "token ids must fit the tag bit"
    );

    // One cheap counting pre-pass buys EXACT allocations for both vecs:
    // nodes = openers + root, and child_ids = every token once + every
    // non-root node once (the doc'd arena-length identity).
    let scope_openers = tokens
        .iter()
        .filter(|token| {
            matches!(
                token.kind(),
                TokenKind::Marker { .. } | TokenKind::Milestone
            ) && generated::opens_scope(token.marker_idx).is_some()
        })
        .count();

    let mut nodes = Vec::with_capacity(scope_openers + 1);
    // Reserve the root before the walk. Its range is patched after all child
    // tails have reached the arena; it is never a synthesized token.
    nodes.push(Node {
        token: ROOT_TOKEN,
        children: 0..0,
        reason: CloseReason::Eof as u8,
        ctx: SpecContext::Scripture as u8,
    });

    let mut child_ids = Vec::with_capacity(tokens.len() + scope_openers);
    let mut scratch = Vec::with_capacity(tokens.len());
    let mut frames = vec![Frame {
        node: 0,
        mark: 0,
        ctx: SpecContext::Scripture as u8,
    }];

    for (token_idx, token) in tokens.iter().enumerate() {
        let marker_idx = match token.kind() {
            TokenKind::Marker { .. } | TokenKind::Milestone => token.marker_idx,
            _ => {
                scratch.push(token_idx as u32);
                continue;
            }
        };

        let marker_kind = generated::kind(marker_idx);
        let opens_scope = generated::opens_scope(marker_idx);
        let mask = generated::context_mask(marker_idx);
        if displaces(marker_kind, opens_scope, mask) {
            while frames.len() > 1
                && mask & context_bit(frames.last().expect("root frame remains").ctx) == 0
            {
                let displaced_node = frames.last().expect("root frame remains").node;
                let displaced_marker_idx =
                    tokens[nodes[displaced_node as usize].token as usize].marker_idx;
                close_frame(
                    &mut frames,
                    &mut scratch,
                    &mut child_ids,
                    &mut nodes,
                    CloseReason::for_displacement(displaced_marker_idx),
                );
            }
        }

        if opens_scope.is_some() {
            let inherited = frames.last().expect("root frame remains").ctx;
            let ctx = generated::contributes_context(marker_idx)
                .map(|context| context as u8)
                .unwrap_or(inherited);
            let node = nodes.len() as u32;
            nodes.push(Node {
                token: token_idx as u32,
                children: 0..0,
                reason: CloseReason::Eof as u8,
                ctx,
            });
            let mark = scratch.len() as u32;
            scratch.push(token_idx as u32);
            frames.push(Frame { node, mark, ctx });
        } else {
            // Empty-mask adjacency markers such as `ca` are ordinary leaves;
            // they must not accidentally displace the enclosing paragraph.
            scratch.push(token_idx as u32);
        }
    }

    // The first CST pass has no explicit-closer or barrier rules yet. Close
    // survivors as Eof so the returned tree is complete; later walker stages
    // replace this mechanical sweep with their richer close decisions.
    while frames.len() > 1 {
        close_frame(
            &mut frames,
            &mut scratch,
            &mut child_ids,
            &mut nodes,
            CloseReason::Eof,
        );
    }

    let root_start = child_ids.len() as u32;
    child_ids.extend_from_slice(&scratch);
    nodes[0].children = root_start..child_ids.len() as u32;

    Cst { nodes, child_ids }
}

impl CloseReason {
    fn for_displacement(marker_idx: generated::MarkerIdx) -> Self {
        match generated::closing(marker_idx) {
            ClosingBehavior::None | ClosingBehavior::OptionalExplicitUntilNoteEnd => Self::Implicit,
            ClosingBehavior::RequiredExplicit | ClosingBehavior::SelfClosingMilestone => {
                Self::Recovery
            }
        }
    }
}

fn close_frame(
    frames: &mut Vec<Frame>,
    scratch: &mut Vec<u32>,
    child_ids: &mut Vec<u32>,
    nodes: &mut [Node],
    reason: CloseReason,
) {
    let frame = frames.pop().expect("a frame is open");
    let start = child_ids.len() as u32;
    child_ids.extend_from_slice(&scratch[frame.mark as usize..]);
    scratch.truncate(frame.mark as usize);
    scratch.push(NODE_ID_BIT | frame.node);
    nodes[frame.node as usize].children = start..child_ids.len() as u32;
    nodes[frame.node as usize].reason = reason as u8;
}

fn displaces(kind: MarkerKind, opens_scope: Option<ScopeKind>, mask: u32) -> bool {
    opens_scope.is_some() || (matches!(kind, MarkerKind::Chapter | MarkerKind::Verse) && mask != 0)
}

fn context_bit(ctx: u8) -> u32 {
    1 << ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    fn node_for<'a>(tokens: &[Token], cst: &'a Cst, name: &str) -> &'a Node {
        cst.nodes
            .iter()
            .find(|node| {
                node.token != ROOT_TOKEN
                    && generated::name(tokens[node.token as usize].marker_idx) == name
            })
            .unwrap_or_else(|| panic!("node for \\{name} not found"))
    }

    fn node_tokens(cst: &Cst, node: &Node) -> Vec<u32> {
        cst.child_ids[node.children.start as usize..node.children.end as usize]
            .iter()
            .copied()
            .filter(|id| id & NODE_ID_BIT == 0)
            .collect()
    }

    #[test]
    fn reserves_and_patches_root() {
        let tokens = lex("text");
        let cst = build(&tokens);

        assert_eq!(core::mem::size_of::<Node>(), 16);
        assert_eq!(cst.nodes[0].token, ROOT_TOKEN);
        assert_eq!(cst.nodes[0].children, 0..1);
        assert_eq!(cst.child_ids, vec![0]);
        assert_eq!(cst.child_ids.len(), tokens.len() + cst.nodes.len() - 1);
        assert_eq!(cst.in_order().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn paragraph_displacement_is_implicit() {
        let tokens = lex("\\p one\\p two");
        let cst = build(&tokens);

        let first = node_for(&tokens, &cst, "p");
        assert_eq!(first.close_reason(), CloseReason::Implicit);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn chapter_unwinds_note_and_paragraph() {
        let tokens = lex("\\p before\\f + \\ft note\\c 1");
        let cst = build(&tokens);

        let p = node_for(&tokens, &cst, "p");
        let f = node_for(&tokens, &cst, "f");
        let ft = node_for(&tokens, &cst, "ft");
        assert_eq!(p.close_reason(), CloseReason::Implicit);
        assert_eq!(f.close_reason(), CloseReason::Recovery);
        assert_eq!(ft.close_reason(), CloseReason::Implicit);
    }

    #[test]
    fn paragraph_break_does_not_displace_paragraph() {
        let tokens = lex("\\p before\\pb after");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");
        let pb = tokens
            .iter()
            .position(|token| {
                token.marker_idx
                    == generated::marker_idx(b"pb", crate::tables::schema::SpellingShape::PlainOnly)
            })
            .unwrap();

        assert_eq!(p.close_reason(), CloseReason::Eof);
        assert!(node_tokens(&cst, p).contains(&(pb as u32)));
    }

    #[test]
    fn nested_character_frames_inherit_paragraph_context() {
        let tokens = lex("\\p before\\add nested\\addpn deeper");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");
        let add = node_for(&tokens, &cst, "add");
        let addpn = node_for(&tokens, &cst, "addpn");

        assert_eq!(p.context(), SpecContext::Para);
        assert_eq!(add.context(), SpecContext::Para);
        assert_eq!(addpn.context(), SpecContext::Para);
        assert_eq!(p.close_reason(), CloseReason::Eof);
        assert_eq!(add.close_reason(), CloseReason::Eof);
        assert_eq!(addpn.close_reason(), CloseReason::Eof);
        assert_eq!(
            cst.child_ids[p.children.start as usize..p.children.end as usize],
            [0, 1, NODE_ID_BIT | 2]
        );
        assert_eq!(
            cst.child_ids[add.children.start as usize..add.children.end as usize],
            [2, 3, NODE_ID_BIT | 3]
        );
    }

    #[test]
    fn verse_does_not_displace_its_paragraph_but_pops_an_unclosed_note() {
        // `\v`'s mask includes Para/List/Table, so verses are LEAVES inside
        // their paragraph. Footnote is deliberately NOT in the mask (Q16 on
        // the row): a bare `\v` means the note is definitively unclosed.
        let tokens = lex("\\p \\v 1 one \\v 2 two");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");
        assert_eq!(p.close_reason(), CloseReason::Eof);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );

        // `\q1` resolves to the shared `q` row (numbered markers), so the
        // node is found by the ROW name.
        let tokens = lex("\\q1 \\v 1 poetry line");
        let cst = build(&tokens);
        let q = node_for(&tokens, &cst, "q");
        assert_eq!(q.close_reason(), CloseReason::Eof);

        let tokens = lex("\\p \\f + \\ft note\\v 3 after");
        let cst = build(&tokens);
        let f = node_for(&tokens, &cst, "f");
        let p = node_for(&tokens, &cst, "p");
        assert_eq!(f.close_reason(), CloseReason::Recovery);
        assert_eq!(p.close_reason(), CloseReason::Eof);
    }

    #[test]
    fn milestones_nest_where_containers_displace() {
        // usfmtc's USJ (scratchpad probe, 2026-08-18): `\ts-s` stays INSIDE
        // its paragraph; `\list-s` content lands BESIDE it. The row masks
        // encode exactly that split.
        let tokens = lex("\\p \\v 1 before \\ts-s\\* after");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");
        assert_eq!(p.close_reason(), CloseReason::Eof);

        let tokens = lex("\\p \\v 1 prose\\list-s\\*");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");
        assert_eq!(p.close_reason(), CloseReason::Implicit);
    }

    #[test]
    fn every_scope_opening_row_has_a_nonempty_context_mask() {
        // An opens_scope row with an empty mask would make the pop predicate
        // true for EVERY frame — a silent pop-all the oracle cannot see.
        for idx in 0..generated::ROW_COUNT as u8 {
            if generated::opens_scope(idx).is_some() {
                assert_ne!(
                    generated::context_mask(idx),
                    0,
                    "\\{} opens a scope but allows no context",
                    generated::name(idx)
                );
            }
        }
    }

    #[test]
    fn empty_context_rows_do_not_displace() {
        let tokens = lex("\\p before\\ca 1 after");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");

        assert_eq!(p.close_reason(), CloseReason::Eof);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }
}
