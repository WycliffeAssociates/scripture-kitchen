//! A compact structural tree over the scanner's stamped token stream.
//!
//! This module owns the CST walker: one generic scope loop (displacement by
//! stamped context), the explicit-closer rules (`\X*` by name, `\esbe` by
//! scope kind, note peers, the sidebar barrier), milestone points
//! (`\zaln-s |…\*` — spelling pairs row-0 milestones with their `\*`), one
//! scratch tail feeding the shared child-id arena, and document order
//! through [`Cst::in_order`]. Recovery pop-all, list/table containers, and
//! positional context are deliberately separate follow-up stages.

use std::ops::Range;

use crate::tables::generated;
use crate::tables::schema::{Category, ClosingBehavior, MarkerKind, ScopeKind, SpecContext};
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
        self.in_order_of(0)
    }

    /// Walks one node's SUBTREE the same way (`node` is an index into
    /// [`Self::nodes`] — the node id).
    pub fn in_order_of(&self, node: u32) -> InOrder<'_> {
        InOrder {
            cst: self,
            stack: vec![ChildCursor::for_node(self, node)],
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

/// What a frame IS to the walker's rules, beyond its row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameRole {
    /// Paragraphs, characters, notes, rows, cells — the generic case.
    Plain,
    /// A sidebar is a POP BARRIER: the displacement loop and the closer
    /// searches stop underneath it — only `\esbe` (via `closes_scope`) may
    /// end it. A `\c` inside `\esb` therefore stays INSIDE the sidebar,
    /// which is lint's to flag, not the walker's to fix.
    Sidebar,
    /// A milestone POINT (`\zaln-s |…\*`, `\ts-e\*`): the tiny frame from
    /// the milestone token to its own `\*`. Content BETWEEN paired points
    /// is deliberately not inside either — structurally they are points
    /// (usfm-grammar's `_milestoneStart` node exactly; its comment defers
    /// -s/-e pairing to post-processing, as does our sid/eid ruling).
    /// `ends` is set on a container's `-e` point: when its `\*` closes the
    /// point, the enclosing container of that kind closes with it.
    Point { ends: Option<ScopeKind> },
    /// A U25003 list/table container, opened by `\list-s`/`\table-s` (keyed
    /// on CATEGORY, not `opens_scope` — the row says Milestone). Also a
    /// WALL for the same-kind and closer searches, but NOT a displacement
    /// barrier: a `\p` may still end an unterminated container (legal in
    /// 3.2, version-keyed lint severity thereafter).
    Container(ScopeKind),
}

struct Frame {
    node: u32,
    /// The open frame owns the scratch tail beginning at this index.
    mark: u32,
    /// The context resolved once at push time; transparent frames inherit it.
    ctx: u8,
    role: FrameRole,
}

impl Frame {
    fn barrier(&self) -> bool {
        self.role == FrameRole::Sidebar
    }

    /// Frames that structural SEARCHES never cross (barriers and
    /// containers), as opposed to the displacement loop's barrier-only stop.
    fn wall(&self) -> bool {
        matches!(self.role, FrameRole::Sidebar | FrameRole::Container(_))
    }
}

/// The in-flight build state. A struct for the same reason the lexer grew
/// `Scanner`: the closer rules each need the same four mutable pieces, and
/// threading them through free functions buries the logic in signatures.
struct Builder<'a> {
    tokens: &'a [Token],
    nodes: Vec<Node>,
    child_ids: Vec<u32>,
    scratch: Vec<u32>,
    frames: Vec<Frame>,
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
    // non-root node once (the doc'd arena-length identity). Every milestone
    // token opens a point (row 0 included — the spelling override), and a
    // container start opens a second node (the container itself).
    let scope_openers: usize = tokens
        .iter()
        .map(|token| match token.kind() {
            TokenKind::Milestone { end } => {
                1 + usize::from(!end && container_kind(token.marker_idx).is_some())
            }
            TokenKind::Marker { .. } => {
                usize::from(generated::opens_scope(token.marker_idx).is_some())
            }
            _ => 0,
        })
        .sum();

    let mut b = Builder {
        tokens,
        nodes: Vec::with_capacity(scope_openers + 1),
        child_ids: Vec::with_capacity(tokens.len() + scope_openers),
        scratch: Vec::with_capacity(tokens.len()),
        frames: vec![Frame {
            node: 0,
            mark: 0,
            ctx: SpecContext::Scripture as u8,
            role: FrameRole::Plain,
        }],
    };
    // Reserve the root before the walk. Its range is patched after all child
    // tails have reached the arena; it is never a synthesized token.
    b.nodes.push(Node {
        token: ROOT_TOKEN,
        children: 0..0,
        reason: CloseReason::Eof as u8,
        ctx: SpecContext::Scripture as u8,
    });

    for (token_idx, token) in tokens.iter().enumerate() {
        let marker_idx = match token.kind() {
            TokenKind::Marker { .. } => token.marker_idx,
            TokenKind::Milestone { end } => {
                b.milestone_point(token_idx as u32, token.marker_idx, end);
                continue;
            }
            TokenKind::MilestoneTerminator => {
                b.milestone_close(token_idx as u32);
                continue;
            }
            TokenKind::ClosingMarker { .. } => {
                b.explicit_close(token_idx as u32, token.marker_idx);
                continue;
            }
            _ => {
                b.scratch.push(token_idx as u32);
                continue;
            }
        };

        // `\esbe`-shaped rows close a scope by KIND rather than by name.
        if let Some(kind) = generated::closes_scope(marker_idx) {
            b.scope_close(token_idx as u32, kind);
            continue;
        }

        // Note peers: an incoming `\ft`-class marker first ends an open
        // sibling of the same class (`\fr` then `\ft` are peers, not
        // parent/child). Displacement can't do this — both siblings sit in
        // the same Footnote context, so the mask never pops one for the
        // other.
        if generated::closing(marker_idx) == ClosingBehavior::OptionalExplicitUntilNoteEnd
            && b.frames.len() > 1
            && generated::closing(b.top_marker_idx())
                == ClosingBehavior::OptionalExplicitUntilNoteEnd
        {
            b.close_top(CloseReason::Implicit);
        }

        let marker_kind = generated::kind(marker_idx);
        let opens_scope = generated::opens_scope(marker_idx);
        let mask = generated::context_mask(marker_idx);
        if opens_scope.is_some() {
            // Same-kind eviction: like kinds never nest — without this,
            // `\li` inside a list container (List IS in its mask, so
            // displacement abstains) would nest under its own sibling
            // forever. A new row also ends the previous row's open cells.
            b.same_kind_evict(marker_kind);
        }
        if displaces(marker_kind, opens_scope, mask) {
            while b.frames.len() > 1 && !b.top().barrier() && mask & context_bit(b.top().ctx) == 0 {
                let reason = CloseReason::for_displacement(b.top_marker_idx());
                b.close_top(reason);
            }
        }

        if opens_scope.is_some() {
            let inherited = b.top().ctx;
            let ctx = generated::contributes_context(marker_idx)
                .map(|context| context as u8)
                .unwrap_or(inherited);
            let node = b.nodes.len() as u32;
            b.nodes.push(Node {
                token: token_idx as u32,
                children: 0..0,
                reason: CloseReason::Eof as u8,
                ctx,
            });
            let mark = b.scratch.len() as u32;
            b.scratch.push(token_idx as u32);
            b.frames.push(Frame {
                node,
                mark,
                ctx,
                role: if opens_scope == Some(ScopeKind::Sidebar) {
                    FrameRole::Sidebar
                } else {
                    FrameRole::Plain
                },
            });
        } else {
            // Empty-mask adjacency markers such as `ca` are ordinary leaves;
            // they must not accidentally displace the enclosing paragraph.
            b.scratch.push(token_idx as u32);
        }
    }

    // Input ended: close survivors as Eof — lint judges each by its row
    // (a paragraph at EOF is silent, a footnote at EOF is a finding).
    while b.frames.len() > 1 {
        b.close_top(CloseReason::Eof);
    }

    let root_start = b.child_ids.len() as u32;
    b.child_ids.extend_from_slice(&b.scratch);
    b.nodes[0].children = root_start..b.child_ids.len() as u32;

    Cst {
        nodes: b.nodes,
        child_ids: b.child_ids,
    }
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

impl Builder<'_> {
    fn top(&self) -> &Frame {
        self.frames.last().expect("root frame remains")
    }

    /// The marker row of the frame's OPENING token.
    fn top_marker_idx(&self) -> generated::MarkerIdx {
        self.frame_marker_idx(self.frames.len() - 1)
    }

    fn frame_marker_idx(&self, depth: usize) -> generated::MarkerIdx {
        let node = self.frames[depth].node;
        self.tokens[self.nodes[node as usize].token as usize].marker_idx
    }

    /// Flush the top frame's scratch tail into the arena and stamp its
    /// verdict. The closed node's own id replaces its tail on the scratch,
    /// where it belongs to the parent frame.
    fn close_top(&mut self, reason: CloseReason) {
        let frame = self.frames.pop().expect("a frame is open");
        let start = self.child_ids.len() as u32;
        self.child_ids
            .extend_from_slice(&self.scratch[frame.mark as usize..]);
        self.scratch.truncate(frame.mark as usize);
        self.scratch.push(NODE_ID_BIT | frame.node);
        self.nodes[frame.node as usize].children = start..self.child_ids.len() as u32;
        self.nodes[frame.node as usize].reason = reason as u8;
    }

    /// `\X*`: search the open frames top-down for the matching frame and pop
    /// THROUGH to it — anything still open above the match closes by the
    /// same row-keyed verdict displacement uses; the match itself closes
    /// Explicit with its closer token as its own last child (which is what
    /// puts the closer's bytes inside the node's extent). The search stops
    /// under a barrier. No match — an orphan closer, or one reaching past a
    /// sidebar — leaves the token an ordinary leaf: flag-never-repair is
    /// lint's, keyed on a ClosingMarker leaf.
    fn explicit_close(&mut self, token_idx: u32, closer_idx: generated::MarkerIdx) {
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            if self.frames[depth].wall() {
                break;
            }
            if self.frame_marker_idx(depth) == closer_idx {
                target = Some(depth);
                break;
            }
        }
        let Some(depth) = target else {
            self.scratch.push(token_idx);
            return;
        };
        while self.frames.len() > depth + 1 {
            let reason = CloseReason::for_displacement(self.top_marker_idx());
            self.close_top(reason);
        }
        self.scratch.push(token_idx);
        self.close_top(CloseReason::Explicit);
    }

    /// A milestone-shaped token opens its POINT frame — the tiny scope from
    /// the token to its own `\*`. This happens for KNOWN rows (their
    /// `opens_scope` says Milestone) and equally for ROW 0: the `-s`/`-e`
    /// SPELLING overrides the table for unknown rows, which is what pairs an
    /// unknown `\zaln-s` with its `\*`. Start spellings on known rows
    /// displace first, per their mask, like any opener; row 0 and `-e`
    /// spellings displace nothing (an `-e` closes things, it opens no
    /// content).
    ///
    /// The U25003 containers ride here, keyed on CATEGORY: `\list-s` /
    /// `\table-s` open the container frame first and their point inside it;
    /// `\list-e` / `\table-e` pop back to the open container (if one is in
    /// reach) so that when their point's `\*` lands, the container closes
    /// with it — Explicit, with the whole `-e` point as its last child.
    fn milestone_point(&mut self, token_idx: u32, marker_idx: generated::MarkerIdx, end: bool) {
        let container = container_kind(marker_idx);

        if !end && generated::opens_scope(marker_idx).is_some() {
            let mask = generated::context_mask(marker_idx);
            while self.frames.len() > 1
                && !self.top().barrier()
                && mask & context_bit(self.top().ctx) == 0
            {
                let reason = CloseReason::for_displacement(self.top_marker_idx());
                self.close_top(reason);
            }
        }

        let mut ends = None;
        if let Some(kind) = container {
            if end {
                // Non-destructive search first: only pop back to the
                // container if it is actually open (and not behind a
                // barrier). An orphan `\list-e` stays a plain point.
                let reachable = (1..self.frames.len())
                    .rev()
                    .take_while(|&d| !self.frames[d].barrier())
                    .any(|d| self.frames[d].role == FrameRole::Container(kind));
                if reachable {
                    while self.top().role != FrameRole::Container(kind) {
                        let reason = CloseReason::for_displacement(self.top_marker_idx());
                        self.close_top(reason);
                    }
                    ends = Some(kind);
                }
            } else {
                // The container node shares the `-s` token as its opener
                // handle; the token itself lives once, inside the point.
                let ctx = match kind {
                    ScopeKind::List => SpecContext::List as u8,
                    _ => SpecContext::Table as u8,
                };
                let node = self.nodes.len() as u32;
                self.nodes.push(Node {
                    token: token_idx,
                    children: 0..0,
                    reason: CloseReason::Eof as u8,
                    ctx,
                });
                self.frames.push(Frame {
                    node,
                    mark: self.scratch.len() as u32,
                    ctx,
                    role: FrameRole::Container(kind),
                });
            }
        }

        let inherited = self.top().ctx;
        let ctx = generated::contributes_context(marker_idx)
            .map(|context| context as u8)
            .unwrap_or(inherited);
        let node = self.nodes.len() as u32;
        self.nodes.push(Node {
            token: token_idx,
            children: 0..0,
            reason: CloseReason::Eof as u8,
            ctx,
        });
        let mark = self.scratch.len() as u32;
        self.scratch.push(token_idx);
        self.frames.push(Frame {
            node,
            mark,
            ctx,
            role: FrameRole::Point { ends },
        });
    }

    /// `\*` ends the topmost open POINT — normally the frame directly on
    /// top, since a point's interior is only its attribute list. Pops
    /// through anything unclosed above it by the usual row-keyed verdicts;
    /// an orphan `\*` (no open point in reach) is an ordinary leaf for
    /// lint. Closing a container's `-e` point also closes the container.
    fn milestone_close(&mut self, token_idx: u32) {
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            if let FrameRole::Point { ends } = self.frames[depth].role {
                target = Some((depth, ends));
                break;
            }
            if self.frames[depth].wall() {
                break;
            }
        }
        let Some((depth, ends)) = target else {
            self.scratch.push(token_idx);
            return;
        };
        while self.frames.len() > depth + 1 {
            let reason = CloseReason::for_displacement(self.top_marker_idx());
            self.close_top(reason);
        }
        self.scratch.push(token_idx);
        self.close_top(CloseReason::Explicit);
        if let Some(kind) = ends {
            if self.top().role == FrameRole::Container(kind) {
                self.close_top(CloseReason::Explicit);
            }
        }
    }

    /// Like kinds never nest: a paragraph ends an open paragraph, a cell an
    /// open cell, and a row both. Searches to the nearest WALL and pops
    /// through the match inclusively. The mask cannot express this — a
    /// container item legally lives in the container's context, so its
    /// sibling shares a context the mask must allow.
    fn same_kind_evict(&mut self, incoming: MarkerKind) {
        let evicts = |kind: MarkerKind| match incoming {
            MarkerKind::Paragraph => kind == MarkerKind::Paragraph,
            MarkerKind::TableRow => matches!(kind, MarkerKind::TableRow | MarkerKind::TableCell),
            MarkerKind::TableCell => kind == MarkerKind::TableCell,
            _ => false,
        };
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            if self.frames[depth].wall() {
                break;
            }
            if self.frames[depth].role == FrameRole::Plain {
                let kind = generated::kind(self.frame_marker_idx(depth));
                if evicts(kind) {
                    target = Some(depth);
                    // An incoming row that found a CELL keeps looking for
                    // the row beneath it — ending the cell but nesting
                    // inside the stale row would be worse than either.
                    if !(incoming == MarkerKind::TableRow && kind == MarkerKind::TableCell) {
                        break;
                    }
                }
            }
        }
        let Some(depth) = target else { return };
        while self.frames.len() > depth {
            let reason = CloseReason::for_displacement(self.top_marker_idx());
            self.close_top(reason);
        }
    }

    /// `\esbe`-shaped closers (`closes_scope`) end the topmost frame of a
    /// SCOPE KIND rather than a marker name. The target being the barrier
    /// itself is what lets this reach a sidebar nothing else may pop.
    fn scope_close(&mut self, token_idx: u32, kind: ScopeKind) {
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            let opener = self.frame_marker_idx(depth);
            if generated::opens_scope(opener) == Some(kind) {
                target = Some(depth);
                break;
            }
            if self.frames[depth].wall() {
                break;
            }
        }
        let Some(depth) = target else {
            self.scratch.push(token_idx);
            return;
        };
        while self.frames.len() > depth + 1 {
            let reason = CloseReason::for_displacement(self.top_marker_idx());
            self.close_top(reason);
        }
        self.scratch.push(token_idx);
        self.close_top(CloseReason::Explicit);
    }
}

fn displaces(kind: MarkerKind, opens_scope: Option<ScopeKind>, mask: u32) -> bool {
    opens_scope.is_some() || (matches!(kind, MarkerKind::Chapter | MarkerKind::Verse) && mask != 0)
}

/// The U25003 containers, keyed on CATEGORY per the 2026-08-17 ruling — the
/// rows' `opens_scope` says Milestone, and the `-s`/`-e` spelling picks
/// open vs close.
fn container_kind(marker_idx: generated::MarkerIdx) -> Option<ScopeKind> {
    match generated::category(marker_idx) {
        Category::MilestoneList => Some(ScopeKind::List),
        Category::MilestoneTable => Some(ScopeKind::Table),
        _ => None,
    }
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
    fn explicit_closers_close_their_frame() {
        let tokens = lex("\\p \\add deep\\add* after");
        let cst = build(&tokens);
        let add = node_for(&tokens, &cst, "add");
        assert_eq!(add.close_reason(), CloseReason::Explicit);
        // The closer token is the node's LAST child — its bytes are inside
        // the extent.
        let ids = &cst.child_ids[add.children.start as usize..add.children.end as usize];
        let closer = tokens
            .iter()
            .position(|t| matches!(t.kind(), TokenKind::ClosingMarker { .. }))
            .unwrap() as u32;
        assert_eq!(*ids.last().unwrap(), closer);

        // A nested-spelling pair closes its own frame.
        let tokens = lex("\\p \\add a \\+nd b\\+nd* c\\add*");
        let cst = build(&tokens);
        assert_eq!(
            node_for(&tokens, &cst, "nd").close_reason(),
            CloseReason::Explicit
        );
        assert_eq!(
            node_for(&tokens, &cst, "add").close_reason(),
            CloseReason::Explicit
        );
    }

    #[test]
    fn a_closer_pops_through_unclosed_frames() {
        let tokens = lex("\\p \\add a \\w b\\add*");
        let cst = build(&tokens);
        assert_eq!(
            node_for(&tokens, &cst, "w").close_reason(),
            CloseReason::Recovery
        );
        assert_eq!(
            node_for(&tokens, &cst, "add").close_reason(),
            CloseReason::Explicit
        );
    }

    #[test]
    fn an_orphan_closer_is_an_ordinary_leaf() {
        let tokens = lex("\\p text\\w* more");
        let cst = build(&tokens);
        assert!(
            !cst.nodes.iter().any(|n| n.token != ROOT_TOKEN
                && generated::name(tokens[n.token as usize].marker_idx) == "w"),
            "no \\w frame may exist"
        );
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn note_peers_are_siblings_and_the_note_closes_explicitly() {
        let tokens = lex("\\f + \\fr 1:1 \\ft note\\f*");
        let cst = build(&tokens);
        // `\ft` ends its `\fr` PEER (both OptionalExplicitUntilNoteEnd);
        // `\f*` then pops through the open `\ft` to the note itself.
        assert_eq!(
            node_for(&tokens, &cst, "fr").close_reason(),
            CloseReason::Implicit
        );
        assert_eq!(
            node_for(&tokens, &cst, "ft").close_reason(),
            CloseReason::Implicit
        );
        assert_eq!(
            node_for(&tokens, &cst, "f").close_reason(),
            CloseReason::Explicit
        );
    }

    #[test]
    fn the_sidebar_is_a_pop_barrier_only_esbe_ends() {
        // `\c` inside `\esb` must NOT unwind the sidebar — it stays inside
        // (lint's to flag). `\esbe` reaches the barrier frame itself.
        let tokens = lex("\\p out\\esb \\p in \\c 1 more\\esbe\\p after");
        let cst = build(&tokens);
        let esb_id = cst
            .nodes
            .iter()
            .position(|n| {
                n.token != ROOT_TOKEN
                    && generated::name(tokens[n.token as usize].marker_idx) == "esb"
            })
            .unwrap() as u32;
        assert_eq!(
            cst.nodes[esb_id as usize].close_reason(),
            CloseReason::Explicit
        );
        let c = tokens
            .iter()
            .position(|t| {
                generated::name(t.marker_idx) == "c" && matches!(t.kind(), TokenKind::Marker { .. })
            })
            .unwrap() as u32;
        let inside_esb: Vec<u32> = cst.in_order_of(esb_id).collect();
        assert!(inside_esb.contains(&c), "\\c stays inside the sidebar");
    }

    /// The container node and its `-s` point SHARE the opening token; the
    /// container is the one stamped with the container context.
    fn container_for<'a>(tokens: &[Token], cst: &'a Cst, name: &str, ctx: SpecContext) -> &'a Node {
        cst.nodes
            .iter()
            .find(|node| {
                node.token != ROOT_TOKEN
                    && generated::name(tokens[node.token as usize].marker_idx) == name
                    && node.context() == ctx
                    && node.children.end - node.children.start > 2
            })
            .unwrap_or_else(|| panic!("container for \\{name} not found"))
    }

    #[test]
    fn a_list_container_holds_its_items_and_closes_at_list_e() {
        let tokens = lex("\\list-s\\*\n\\li one\n\\li two\n\\list-e\\*");
        let cst = build(&tokens);
        let container = container_for(&tokens, &cst, "list", SpecContext::List);
        assert_eq!(container.close_reason(), CloseReason::Explicit);

        // Both items are siblings INSIDE the container (same-kind eviction
        // ends the first; closing the container ends the second), and the
        // whole `\list-e …\*` point is the container's LAST child.
        let li: Vec<&Node> = cst
            .nodes
            .iter()
            .filter(|n| {
                n.token != ROOT_TOKEN
                    && generated::name(tokens[n.token as usize].marker_idx) == "li"
            })
            .collect();
        assert_eq!(li.len(), 2);
        for item in &li {
            assert_eq!(item.close_reason(), CloseReason::Implicit);
        }
        let kids =
            &cst.child_ids[container.children.start as usize..container.children.end as usize];
        assert!(
            kids.last().unwrap() & NODE_ID_BIT != 0,
            "last child is the -e point"
        );
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_table_container_does_not_double_nest_rows() {
        let tokens = lex("\\table-s\\*\n\\tr \\tc1 a\\tc2 b\n\\tr \\tc1 c\n\\table-e\\*");
        let cst = build(&tokens);
        let container = container_for(&tokens, &cst, "table", SpecContext::Table);
        assert_eq!(container.close_reason(), CloseReason::Explicit);

        // Exactly: root + container + two points + 2 rows + 3 cells = 9
        // nodes — no synthesized second table frame.
        assert_eq!(cst.nodes.len(), 9);
        // Rows sit DIRECTLY in the container.
        let kids =
            &cst.child_ids[container.children.start as usize..container.children.end as usize];
        let row_nodes = kids
            .iter()
            .filter(|id| {
                *id & NODE_ID_BIT != 0
                    && generated::kind(
                        tokens[cst.nodes[(*id & !NODE_ID_BIT) as usize].token as usize].marker_idx,
                    ) == crate::tables::schema::MarkerKind::TableRow
            })
            .count();
        assert_eq!(row_nodes, 2);
    }

    #[test]
    fn bare_rows_and_cells_evict_their_own_kind() {
        let tokens = lex("\\p x\n\\tr \\tc1 a\\tc2 b\n\\tr \\tc1 c\n\\p y");
        let cst = build(&tokens);
        // First row's cells: tc1 evicted by tc2 (same kind), tc2 + row
        // evicted by the second \tr, all Implicit (their rows close None).
        let cells: Vec<&Node> = cst
            .nodes
            .iter()
            .filter(|n| {
                n.token != ROOT_TOKEN
                    && generated::kind(tokens[n.token as usize].marker_idx)
                        == crate::tables::schema::MarkerKind::TableCell
            })
            .collect();
        assert_eq!(cells.len(), 3);
        for cell in &cells {
            assert_eq!(cell.close_reason(), CloseReason::Implicit);
        }
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_paragraph_displaces_an_unterminated_container() {
        // Legal in 3.2 (closing milestone optional); the container's closing
        // behavior is SelfClosingMilestone, so displacement stamps Recovery
        // and lint keys severity on the declared version.
        let tokens = lex("\\list-s\\*\n\\li a\n\\p prose");
        let cst = build(&tokens);
        let container = container_for(&tokens, &cst, "list", SpecContext::List);
        assert_eq!(container.close_reason(), CloseReason::Recovery);
        assert_eq!(
            node_for(&tokens, &cst, "p").close_reason(),
            CloseReason::Eof
        );
    }

    #[test]
    fn an_orphan_container_end_is_a_plain_point() {
        let tokens = lex("\\p text\n\\list-e\\*");
        let cst = build(&tokens);
        // The -e point exists and closes at its star; nothing else closes.
        let point = node_for(&tokens, &cst, "list");
        assert_eq!(point.close_reason(), CloseReason::Explicit);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unknown_milestones_pair_with_their_star() {
        // The aligned-corpus idiom: row-0 milestones. The SPELLING opens the
        // point frame; each point contains exactly its own attrs + `\*`, and
        // the word between the pair is NOT inside either point.
        let tokens = lex("\\zaln-s |x-strong=\"G1\"\\*\\w In|lemma=\"in\"\\w*\\zaln-e\\*");
        let cst = build(&tokens);
        let points: Vec<&Node> = cst
            .nodes
            .iter()
            .filter(|n| {
                n.token != ROOT_TOKEN
                    && matches!(tokens[n.token as usize].kind(), TokenKind::Milestone { .. })
            })
            .collect();
        assert_eq!(points.len(), 2);
        for point in &points {
            assert_eq!(point.close_reason(), CloseReason::Explicit);
        }
        let w = node_for(&tokens, &cst, "w");
        assert_eq!(w.close_reason(), CloseReason::Explicit);
        for point in &points {
            assert!(!node_tokens(&cst, point).contains(&w.token));
        }
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_known_point_closes_at_its_star_and_owns_no_content() {
        let tokens = lex("\\p \\v 1 a \\ts-s\\* b");
        let cst = build(&tokens);
        let ts = node_for(&tokens, &cst, "ts");
        let p = node_for(&tokens, &cst, "p");
        assert_eq!(ts.close_reason(), CloseReason::Explicit);
        assert_eq!(p.close_reason(), CloseReason::Eof);
        // "b" (the last token) is p's child, not the point's.
        let last = tokens.len() as u32 - 1;
        assert!(node_tokens(&cst, p).contains(&last));
        assert!(!node_tokens(&cst, ts).contains(&last));
    }

    #[test]
    fn an_orphan_star_is_an_ordinary_leaf() {
        let tokens = lex("\\p text \\* more");
        let cst = build(&tokens);
        assert_eq!(cst.nodes.len(), 2, "root and \\p only — no point frame");
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
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
