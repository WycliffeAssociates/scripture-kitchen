//! A compact structural tree over the scanner's stamped token stream.
//!
//! One generic scope loop (displacement by stamped context, same-kind
//! eviction) plus the rules it cannot express: `\X*` by name, `\esbe` by scope
//! kind, note peers, the sidebar barrier, milestone points, the U25003
//! list/table containers, unknown-marker pop-all recovery. One scratch tail
//! feeds the shared child-id arena; [`Cst::in_order`] reads document order
//! back out. Positional context is lint's.

use std::ops::Range;

use crate::tables::generated;
use crate::tables::schema::{Category, ClosingBehavior, MarkerKind, ScopeKind, SpecContext};
use crate::{Token, TokenKind};

pub(crate) const NODE_ID_BIT: u32 = 1 << 31;
pub(crate) const ROOT_TOKEN: u32 = u32::MAX;

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
/// `children` indexes [`Cst::child_ids`], not `nodes`: the arena stores a mixed
/// document-order stream of token ids and tagged node ids. The opening marker is
/// the first token in its own child list, and copied into `token` as well.
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
    pub fn close_reason(&self) -> CloseReason {
        CloseReason::from_u8(self.reason)
    }

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

    /// One node's BYTE extent: its opening marker's start through the end of
    /// its LAST DESCENDANT token.
    ///
    /// Both ends are WALKED rather than read off `token`, because a child id is
    /// as likely to be a node as a token: the last child of `\f` is usually the
    /// `\ft` node, whose own last child may be another node again.
    ///
    /// The extent INCLUDES the node's explicit closer (it is the node's last
    /// child), so a "replace this whole note" edit needs nothing else. The ROOT
    /// covers every token; an empty document is `0..0`.
    pub fn extent(&self, node: u32, tokens: &[Token]) -> Range<u32> {
        let children = &self.nodes[node as usize].children;
        if children.is_empty() {
            // Only the root can be childless, and only on an empty document:
            // every other node holds at least its own opening marker.
            let start = self.nodes[node as usize].token;
            return match tokens.get(start as usize) {
                Some(token) => token.start..token.start,
                None => 0..0,
            };
        }
        let mut spine = children.clone();
        let first = loop {
            let child = self.child_ids[spine.start as usize];
            if child & NODE_ID_BIT == 0 {
                break child;
            }
            spine = self.nodes[(child & !NODE_ID_BIT) as usize].children.clone();
        };
        let mut spine = children.clone();
        let last = loop {
            let child = self.child_ids[spine.end as usize - 1];
            if child & NODE_ID_BIT == 0 {
                break child;
            }
            spine = self.nodes[(child & !NODE_ID_BIT) as usize].children.clone();
        };
        tokens[first as usize].start..tokens[last as usize].end()
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
pub(crate) enum FrameRole {
    /// Paragraphs, characters, notes, rows, cells — the generic case.
    Plain,
    /// A sidebar is a POP BARRIER: the displacement loop and the closer
    /// searches stop underneath it — only `\esbe` (via `closes_scope`) may
    /// end it. A `\c` inside `\esb` therefore stays INSIDE the sidebar,
    /// which is lint's to flag, not the walker's to fix.
    Sidebar,
    /// A milestone POINT (`\zaln-s |…\*`, `\ts-e\*`): the tiny frame from the
    /// milestone token to its own `\*`. Content BETWEEN paired points is in
    /// neither — they are points, as in usfm-grammar's `_milestoneStart`, and
    /// -s/-e pairing belongs to post-processing. `ends` on a container's `-e`
    /// point closes that container when the point's `\*` lands.
    Point { ends: Option<ScopeKind> },
    /// A U25003 list/table container, opened by `\list-s`/`\table-s` (keyed on
    /// CATEGORY, not `opens_scope` — the row says Milestone). A WALL for the
    /// same-kind and closer searches, but NOT a displacement barrier: a `\p` may
    /// still end an unterminated container, legal in 3.2.
    Container(ScopeKind),
}

pub(crate) struct Frame {
    pub(crate) node: u32,
    /// The open frame owns the scratch tail beginning at this index.
    pub(crate) mark: u32,
    /// The row of the frame's OPENING token, COPIED at push time. Holding it
    /// here rather than re-reading `tokens[node.token]` is what makes the
    /// Builder streaming — see the no-slice contract on [`Builder`]. The root
    /// frame's [`generated::UNRESOLVED`] is never read: searches start at
    /// depth 1.
    pub(crate) marker_idx: generated::MarkerIdx,
    /// The context resolved once at push time; transparent frames inherit it.
    pub(crate) ctx: u8,
    pub(crate) role: FrameRole,
}

impl Frame {
    pub(crate) fn barrier(&self) -> bool {
        self.role == FrameRole::Sidebar
    }

    /// Frames that structural SEARCHES never cross (barriers and
    /// containers), as opposed to the displacement loop's barrier-only stop.
    pub(crate) fn wall(&self) -> bool {
        matches!(self.role, FrameRole::Sidebar | FrameRole::Container(_))
    }
}

/// The in-flight build state, driven ONE TOKEN AT A TIME: [`Builder::feed`]
/// per token in document order, then [`Builder::finish`]. [`build`] is that
/// loop over a slice; a scanner sink is the same loop off `push_token`.
///
/// # The feed contract
///
/// **The Builder never sees the token stream — only the token in hand.** No
/// `&[Token]`, no lookahead, no look-BACK: what the closer rules need from an
/// earlier token (its row) was copied into [`Frame::marker_idx`] when that
/// token opened its frame, so a driver may retain no tokens at all.
///
/// In return: **do not feed token N until token N is final.** The scanner's
/// `whitespace_arm` extends the PREVIOUS token's len on a LATER iteration, so a
/// `push_token` sink feeds N only once N+1 exists (or at EOF). The Builder
/// reads `kind()` and `marker_idx`, not `len`, but consumers downstream do.
pub(crate) struct Builder {
    nodes: Vec<Node>,
    child_ids: Vec<u32>,
    scratch: Vec<u32>,
    frames: Vec<Frame>,
}

/// Builds a CST from scanner-stamped tokens.
///
/// Opening markers are the first leaf in their own node, which keeps the token
/// partition complete while `Node::token` still hands consumers the opener
/// directly. Only a scope-opening row, or a chapter/verse point with a non-empty
/// context mask, may displace the current stack.
///
/// # Cost
///
/// ~6 ns/token, about 40% of a lex (`playground --cst-only`); heaviest aligned
/// book ≈ 0.6ms, lex+build ≈ 2.1ms. Corpus health via `--cst-stats`: zero
/// Recovery except three genuinely unclosed `\f` in the wild (en_ulb ISA/MRK,
/// bsb GEN).
pub fn build(tokens: &[Token]) -> Cst {
    assert!(
        tokens.len() < NODE_ID_BIT as usize,
        "token ids must fit the tag bit"
    );

    // One cheap counting pre-pass buys EXACT allocations for both vecs — only
    // affordable for a caller holding the whole slice.
    let mut b = Builder::with_capacity(tokens.len(), exact_scope_openers(tokens));
    for (token_idx, token) in tokens.iter().enumerate() {
        b.feed(token_idx as u32, token);
    }
    b.finish()
}

/// The count that makes [`build`]'s allocations EXACT: nodes = openers + root,
/// child_ids = every token once + every non-root node once. Every milestone
/// token opens a point (row 0 included — the spelling override), and a container
/// start opens a second node, the container itself.
fn exact_scope_openers(tokens: &[Token]) -> usize {
    tokens
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
        .sum()
}

impl CloseReason {
    pub(crate) fn for_displacement(marker_idx: generated::MarkerIdx) -> Self {
        match generated::closing(marker_idx) {
            ClosingBehavior::None | ClosingBehavior::OptionalExplicitUntilNoteEnd => Self::Implicit,
            ClosingBehavior::RequiredExplicit | ClosingBehavior::SelfClosingMilestone => {
                Self::Recovery
            }
        }
    }
}

impl Builder {
    /// Reserves nothing and grows: for a driver with no token count in hand.
    #[allow(dead_code)] // No in-crate caller: the entry point for a stream driver.
    pub(crate) fn new() -> Self {
        Self::with_capacity(0, 0)
    }

    /// `tokens` and `scope_openers` are HINTS; being wrong costs only a realloc.
    /// A stream driver estimates from the source length — over the corpora,
    /// tokens ≈ bytes/16, openers ≈ tokens/8 on prose, tokens/4 when aligned.
    pub(crate) fn with_capacity(tokens: usize, scope_openers: usize) -> Self {
        let mut b = Self {
            nodes: Vec::with_capacity(scope_openers + 1),
            child_ids: Vec::with_capacity(tokens + scope_openers),
            scratch: Vec::with_capacity(tokens),
            frames: vec![Frame {
                node: 0,
                mark: 0,
                marker_idx: generated::UNRESOLVED,
                ctx: SpecContext::Scripture as u8,
                role: FrameRole::Plain,
            }],
        };
        // The root is reserved here; `finish` patches its range once every child
        // tail has reached the arena.
        b.nodes.push(Node {
            token: ROOT_TOKEN,
            children: 0..0,
            reason: CloseReason::Eof as u8,
            ctx: SpecContext::Scripture as u8,
        });
        b
    }

    /// One token, in document order. `token_idx` is the id it will carry in the
    /// finished tree — the driver's own running count.
    #[inline]
    pub(crate) fn feed(&mut self, token_idx: u32, token: &Token) {
        debug_assert!(token_idx < NODE_ID_BIT, "token ids must fit the tag bit");
        let marker_idx = match token.kind() {
            TokenKind::Marker { .. } => token.marker_idx,
            TokenKind::Milestone { end } => {
                self.milestone_point(token_idx, token.marker_idx, end);
                return;
            }
            TokenKind::MilestoneTerminator => {
                self.milestone_close(token_idx);
                return;
            }
            TokenKind::ClosingMarker { .. } => {
                self.explicit_close(token_idx, token.marker_idx);
                return;
            }
            _ => {
                self.scratch.push(token_idx);
                return;
            }
        };

        // A KNOWN milestone row in its BARE spelling (`\ts \*`) is a point too —
        // the row already says so. Otherwise every uW chunk marker opens a plain
        // frame only displacement can kill (thousands of `\ts` Recovery stamps
        // across en_ult).
        if generated::kind(marker_idx) == MarkerKind::Milestone {
            self.milestone_point(token_idx, marker_idx, false);
            return;
        }

        // Unknown/illegal markers (row 0) RECOVER: no open scope is trustworthy
        // across a marker the walker cannot classify. Each popped row keeps its
        // own verdict, and the unknown marker stays an ordinary leaf for lint.
        // Unknown CLOSERS and row-0 MILESTONES do NOT recover: the closer is an
        // orphan leaf, the milestone is a point by spelling.
        if marker_idx == generated::UNRESOLVED {
            while self.frames.len() > 1 {
                let reason = CloseReason::for_displacement(self.top_marker_idx());
                self.close_top(reason);
            }
            self.scratch.push(token_idx);
            return;
        }

        if let Some(kind) = generated::closes_scope(marker_idx) {
            self.scope_close(token_idx, kind);
            return;
        }

        // Note peers: `\fr` then `\ft` are siblings, not parent/child, and
        // displacement cannot say so — both sit in the same Footnote context, so
        // the mask never pops one for the other.
        if generated::closing(marker_idx) == ClosingBehavior::OptionalExplicitUntilNoteEnd
            && self.frames.len() > 1
            && generated::closing(self.top_marker_idx())
                == ClosingBehavior::OptionalExplicitUntilNoteEnd
        {
            self.close_top(CloseReason::Implicit);
        }

        let marker_kind = generated::kind(marker_idx);
        let opens_scope = generated::opens_scope(marker_idx);
        let mask = generated::context_mask(marker_idx);
        if opens_scope.is_some() {
            // Like kinds never nest: without this, `\li` inside a list
            // container (List IS in its mask, so displacement abstains) would
            // nest under its own sibling forever.
            self.same_kind_evict(marker_kind);
        }
        if displaces(marker_kind, opens_scope, mask) {
            while self.frames.len() > 1
                && !self.top().barrier()
                && mask & context_bit(self.top().ctx) == 0
            {
                let reason = CloseReason::for_displacement(self.top_marker_idx());
                self.close_top(reason);
            }
        }

        if opens_scope.is_some() {
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
                marker_idx,
                ctx,
                role: if opens_scope == Some(ScopeKind::Sidebar) {
                    FrameRole::Sidebar
                } else {
                    FrameRole::Plain
                },
            });
        } else {
            // Empty-mask adjacency markers are ordinary leaves: they must not
            // displace the enclosing paragraph.
            self.scratch.push(token_idx);
        }
    }

    /// Input ended: close survivors as Eof — lint judges each by its row (a
    /// paragraph at EOF is silent, a footnote at EOF is a finding) — then
    /// patch the root's child range.
    pub(crate) fn finish(mut self) -> Cst {
        while self.frames.len() > 1 {
            self.close_top(CloseReason::Eof);
        }

        let root_start = self.child_ids.len() as u32;
        self.child_ids.extend_from_slice(&self.scratch);
        self.nodes[0].children = root_start..self.child_ids.len() as u32;

        Cst {
            nodes: self.nodes,
            child_ids: self.child_ids,
        }
    }

    fn top(&self) -> &Frame {
        self.frames.last().expect("root frame remains")
    }

    fn top_marker_idx(&self) -> generated::MarkerIdx {
        self.top().marker_idx
    }

    /// Flush the top frame's scratch tail into the arena and stamp its verdict.
    /// The closed node's own id replaces its tail on the scratch, where it now
    /// belongs to the parent frame.
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

    /// `\X*` pops THROUGH to the frame of the same name: anything open above it
    /// closes by the row-keyed verdict displacement uses, and the match closes
    /// Explicit with the closer as its last child — which is what puts the
    /// closer's bytes inside the node's extent. The search stops under a
    /// barrier, and no match (an orphan closer, or one reaching past a sidebar)
    /// leaves the token an ordinary leaf for lint.
    fn explicit_close(&mut self, token_idx: u32, closer_idx: generated::MarkerIdx) {
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            if self.frames[depth].wall() {
                break;
            }
            if self.frames[depth].marker_idx == closer_idx {
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

    /// A milestone-shaped token opens its POINT frame — the tiny scope from the
    /// token to its own `\*` — for KNOWN rows and equally for ROW 0, where the
    /// `-s`/`-e` SPELLING overrides the table and so pairs an unknown
    /// `\zaln-s` with its `\*`. Start spellings on known rows displace first,
    /// per their mask; row 0 and `-e` spellings displace nothing (an `-e` opens
    /// no content).
    ///
    /// The U25003 containers ride here: `\list-s`/`\table-s` open the container
    /// frame around their point, and `\list-e`/`\table-e` pop back to a
    /// reachable container so its `\*` closes the container too — Explicit,
    /// with the whole `-e` point as its last child.
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
                // Non-destructive search first: pop back only to a container
                // that is really open and not behind a barrier. An orphan
                // `\list-e` stays a plain point.
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
                    marker_idx,
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
            marker_idx,
            ctx,
            role: FrameRole::Point { ends },
        });
    }

    /// `\*` ends the topmost open POINT — normally the frame directly on top, a
    /// point's interior being only its attribute list. An orphan `\*` is an
    /// ordinary leaf; closing a container's `-e` point closes the container.
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
        if let Some(kind) = ends
            && self.top().role == FrameRole::Container(kind)
        {
            self.close_top(CloseReason::Explicit);
        }
    }

    /// Like kinds never nest: a paragraph ends an open paragraph, a cell an open
    /// cell, and a row both. The mask cannot express this — a container item
    /// legally lives in the container's context, so its sibling shares a context
    /// the mask must allow.
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
                let kind = generated::kind(self.frames[depth].marker_idx);
                if evicts(kind) {
                    target = Some(depth);
                    // A row that found a CELL keeps looking for the row
                    // beneath it: ending the cell but nesting inside the stale
                    // row would be worse than either.
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
            let opener = self.frames[depth].marker_idx;
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

pub(crate) fn displaces(kind: MarkerKind, opens_scope: Option<ScopeKind>, mask: u32) -> bool {
    opens_scope.is_some() || (matches!(kind, MarkerKind::Chapter | MarkerKind::Verse) && mask != 0)
}

/// The U25003 containers, keyed on CATEGORY: the rows' `opens_scope` says
/// Milestone, and the `-s`/`-e` spelling picks open vs close.
pub(crate) fn container_kind(marker_idx: generated::MarkerIdx) -> Option<ScopeKind> {
    match generated::category(marker_idx) {
        Category::MilestoneList => Some(ScopeKind::List),
        Category::MilestoneTable => Some(ScopeKind::Table),
        _ => None,
    }
}

pub(crate) fn context_bit(ctx: u8) -> u32 {
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

    /// The streaming Builder — no slice, no pre-count, one token at a time — is
    /// the SAME walker `build` runs, so a scanner sink is a re-driving of it
    /// rather than a rewrite.
    fn feeds_the_same_tree(source: &str) {
        let tokens = lex(source);
        let mut b = Builder::new();
        for (idx, token) in tokens.iter().enumerate() {
            b.feed(idx as u32, token);
        }
        assert_eq!(
            b.finish(),
            build(&tokens),
            "streaming differs on {source:?}"
        );
    }

    #[test]
    fn streaming_matches_the_slice_build() {
        for source in [
            "",
            "text",
            "\\p one\\p two",
            "\\p before\\f + \\ft note\\c 1",
            "\\p \\add deep\\add* after",
            "\\p out\\esb \\p in \\c 1 more\\esbe\\p after",
            "\\list-s\\*\n\\li one\n\\li two\n\\list-e\\*",
            "\\table-s\\*\n\\tr \\tc1 a\\tc2 b\n\\tr \\tc1 c\n\\table-e\\*",
            "\\p a \\f + \\ft n \\zfoo b",
            "\\zaln-s |x-strong=\"G1\"\\*\\w In|lemma=\"in\"\\w*\\zaln-e\\*",
            "\\p text \\* more\\w* tail",
            "\\id GEN\n\\c 1\n\\p \\v 1 text\n",
        ] {
            feeds_the_same_tree(source);
        }
    }

    /// The same equivalence over one REAL book — every shape the unit snippets
    /// miss. Skips when the (gitignored) corpora are absent.
    #[test]
    fn streaming_matches_the_slice_build_on_a_corpus_book() {
        let mut paths: Vec<std::path::PathBuf> = match std::fs::read_dir("example-corpora/en_ult") {
            Ok(dir) => dir
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.extension().is_some_and(|ext| ext == "usfm"))
                .collect(),
            Err(_) => {
                eprintln!("streaming equivalence SKIPPED: no example-corpora/en_ult");
                return;
            }
        };
        paths.sort();
        let Some(path) = paths.first() else {
            eprintln!("streaming equivalence SKIPPED: no *.usfm in en_ult");
            return;
        };
        feeds_the_same_tree(&std::fs::read_to_string(path).expect("readable book"));
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
        // their paragraph. Footnote is NOT in the mask: a bare `\v` means the
        // note is definitively unclosed.
        let tokens = lex("\\p \\v 1 one \\v 2 two");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");
        assert_eq!(p.close_reason(), CloseReason::Eof);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );

        // `\q1` resolves to the shared `q` row, so the node is found by the ROW
        // name.
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
        // The closer token is the node's LAST child, so its bytes are inside
        // the extent.
        let ids = &cst.child_ids[add.children.start as usize..add.children.end as usize];
        let closer = tokens
            .iter()
            .position(|t| matches!(t.kind(), TokenKind::ClosingMarker { .. }))
            .unwrap() as u32;
        assert_eq!(*ids.last().unwrap(), closer);

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
        // `\ft` ends its `\fr` PEER; `\f*` then pops through the open `\ft` to
        // the note itself.
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
    fn a_character_marker_nests_inside_its_note_instead_of_displacing_it() {
        // bsb GEN 2:4's shape in miniature: character rows carry Footnote, so
        // `\+nd` NESTS inside the note instead of displacing it.
        let tokens = lex("\\p \\f + \\fr 2:4 \\fq \\+nd Lord\\+nd*\\ft rest.\\f* after");
        let cst = build(&tokens);

        let f = node_for(&tokens, &cst, "f");
        let fq = node_for(&tokens, &cst, "fq");
        let nd = node_for(&tokens, &cst, "nd");
        assert_eq!(f.close_reason(), CloseReason::Explicit);
        assert_eq!(nd.close_reason(), CloseReason::Explicit);
        assert_eq!(f.context(), SpecContext::Footnote);
        assert_eq!(fq.context(), SpecContext::Footnote);
        assert_eq!(nd.context(), SpecContext::Footnote);

        // note:f → char:fq → char:nd, the tree usfmtc reads off these bytes.
        let id_of = |name: &str| {
            cst.nodes
                .iter()
                .position(|n| {
                    n.token != ROOT_TOKEN
                        && generated::name(tokens[n.token as usize].marker_idx) == name
                })
                .unwrap() as u32
        };
        let children = |node: &Node| {
            cst.child_ids[node.children.start as usize..node.children.end as usize].to_vec()
        };
        assert!(children(f).contains(&(NODE_ID_BIT | id_of("fq"))));
        assert!(children(fq).contains(&(NODE_ID_BIT | id_of("nd"))));
        assert!(children(node_for(&tokens, &cst, "p")).contains(&(NODE_ID_BIT | id_of("f"))));
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_sidebar_is_a_pop_barrier_only_esbe_ends() {
        // `\c` inside `\esb` must NOT unwind the sidebar; `\esbe` reaches the
        // barrier frame itself.
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

        // Both items are siblings INSIDE the container, and the whole
        // `\list-e …\*` point is the container's LAST child.
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

        // root + container + two points + 2 rows + 3 cells = 9: no synthesized
        // second table frame, and rows sit DIRECTLY in the container.
        assert_eq!(cst.nodes.len(), 9);
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
        // tc1 evicted by tc2 (same kind), tc2 + row by the second \tr, all
        // Implicit (their rows close None).
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
        // Legal in 3.2 (closing milestone optional), so displacement stamps
        // Recovery and lint keys severity on the declared version.
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
        let point = node_for(&tokens, &cst, "list");
        assert_eq!(point.close_reason(), CloseReason::Explicit);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_unknown_marker_recovers_by_popping_all_frames() {
        let tokens = lex("\\p a \\f + \\ft n \\zfoo b");
        let cst = build(&tokens);
        assert_eq!(
            node_for(&tokens, &cst, "f").close_reason(),
            CloseReason::Recovery
        );
        assert_eq!(
            node_for(&tokens, &cst, "ft").close_reason(),
            CloseReason::Implicit
        );
        assert_eq!(
            node_for(&tokens, &cst, "p").close_reason(),
            CloseReason::Implicit
        );
        // The unknown marker lands at ROOT: a fresh start.
        let root_kids = &cst.child_ids
            [cst.nodes[0].children.start as usize..cst.nodes[0].children.end as usize];
        let zfoo = tokens
            .iter()
            .position(|t| {
                t.marker_idx == generated::UNRESOLVED
                    && matches!(t.kind(), TokenKind::Marker { .. })
            })
            .unwrap() as u32;
        assert!(root_kids.contains(&zfoo));
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unknown_milestones_pair_with_their_star() {
        // The aligned-corpus idiom: row-0 milestones, where the SPELLING opens
        // the point. Each point holds exactly its own attrs + `\*`, and the
        // word between the pair is inside NEITHER.
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
        // usfmtc's USJ keeps `\ts-s` INSIDE its paragraph and lands `\list-s`
        // content BESIDE it; the row masks encode exactly that split.
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
        // `cp` is the last empty-mask row (a published chapter label: no closer,
        // no frame). Its emptiness is safe because it opens nothing — see
        // `every_scope_opening_row_has_a_nonempty_context_mask`.
        let tokens = lex("\\p before\n\\cp \u{5d0}\n after");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");

        assert_eq!(p.close_reason(), CloseReason::Eof);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    /// `ca`/`va`/`vp` open Character scopes and carry the character class's
    /// context mask, so they NEST at both of the places the spec puts them and
    /// displace nothing there.
    #[test]
    fn chapter_and_verse_annotations_nest_like_character_markers() {
        // Chapter level: `\c` is a point, so the stack is just the root and the
        // pop loop never touches it.
        let tokens = lex("\\c 1\n\\ca 2\\ca*\n\\p \\v 1 text");
        let cst = build(&tokens);
        let ca = node_for(&tokens, &cst, "ca");
        let p = node_for(&tokens, &cst, "p");
        assert_eq!(ca.close_reason(), CloseReason::Explicit);
        assert_eq!(ca.context(), SpecContext::Scripture);
        assert_eq!(p.close_reason(), CloseReason::Eof);
        assert_eq!(p.context(), SpecContext::Para);
        assert_eq!(cst.nodes.len(), 3);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );

        // Verse level: `Para` in the mask is what keeps the paragraph open
        // across both annotations.
        let tokens = lex("\\p \\v 1 \\va 3\\va* \\vp 3b\\vp* text");
        let cst = build(&tokens);
        let p = node_for(&tokens, &cst, "p");
        let va = node_for(&tokens, &cst, "va");
        let vp = node_for(&tokens, &cst, "vp");
        assert_eq!(va.close_reason(), CloseReason::Explicit);
        assert_eq!(vp.close_reason(), CloseReason::Explicit);
        assert_eq!(va.context(), SpecContext::Para);
        assert_eq!(vp.context(), SpecContext::Para);
        assert_eq!(p.close_reason(), CloseReason::Eof);
        // Both are CHILDREN of the paragraph, not its successors.
        let children = &cst.child_ids[p.children.start as usize..p.children.end as usize];
        assert_eq!(
            children.iter().filter(|id| *id & NODE_ID_BIT != 0).count(),
            2
        );
        assert_eq!(cst.nodes.len(), 4);
        assert_eq!(
            cst.in_order().collect::<Vec<_>>(),
            (0..tokens.len() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_unclosed_chapter_annotation_is_displaced_as_recovery() {
        // `RequiredExplicit` + displacement = Recovery, the same verdict any
        // other unclosed character marker earns.
        let tokens = lex("\\c 1\n\\ca 2\n\\c 2");
        let cst = build(&tokens);
        let ca = node_for(&tokens, &cst, "ca");
        assert_eq!(ca.close_reason(), CloseReason::Recovery);
    }

    /// An extent as the SOURCE TEXT it covers — the only form checkable by eye.
    fn extent_text<'a>(source: &'a str, tokens: &[Token], cst: &Cst, name: &str) -> &'a str {
        let node = node_for(tokens, cst, name);
        let id = cst
            .nodes
            .iter()
            .position(|candidate| candidate == node)
            .expect("the node came from this tree") as u32;
        let extent = cst.extent(id, tokens);
        &source[extent.start as usize..extent.end as usize]
    }

    #[test]
    fn a_node_extends_to_its_last_descendant_token() {
        // Ending in a LEAF: the paragraph's last child is the text token.
        let source = "\\p one two";
        let tokens = lex(source);
        let cst = build(&tokens);
        assert_eq!(extent_text(source, &tokens, &cst, "p"), "\\p one two");

        // Ending in a NESTED NODE, twice over: `\p`'s last child is the `\f`
        // node, whose last child is the `\ft` node — neither node's own `token`
        // could answer this.
        let source = "\\p one \\f + \\ft note\\f* tail\n\\p next";
        let tokens = lex(source);
        let cst = build(&tokens);
        assert_eq!(extent_text(source, &tokens, &cst, "ft"), "\\ft note");
        // The closer is INSIDE the note's extent — it is the note's last child.
        assert_eq!(
            extent_text(source, &tokens, &cst, "f"),
            "\\f + \\ft note\\f*"
        );
        assert_eq!(
            extent_text(source, &tokens, &cst, "p"),
            "\\p one \\f + \\ft note\\f* tail\n"
        );

        // The container's last descendant is two levels down.
        let source = "\\list-s\\*\n\\li item\n\\list-e\\*";
        let tokens = lex(source);
        let cst = build(&tokens);
        assert_eq!(extent_text(source, &tokens, &cst, "li"), "\\li item\n");
        assert_eq!(extent_text(source, &tokens, &cst, "list"), source);
    }

    #[test]
    fn the_root_extends_over_the_whole_document() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 text\n";
        let tokens = lex(source);
        let cst = build(&tokens);
        assert_eq!(cst.extent(0, &tokens), 0..source.len() as u32);

        // A point's extent is its own tiny span, terminator included.
        let source = "\\p a \\qt-s |who=\"Levi\"\\* b";
        let tokens = lex(source);
        let cst = build(&tokens);
        assert_eq!(
            extent_text(source, &tokens, &cst, "qt"),
            "\\qt-s |who=\"Levi\"\\*"
        );

        // An empty document has an empty root extent rather than a panic.
        let tokens = lex("");
        let cst = build(&tokens);
        assert_eq!(cst.extent(0, &tokens), 0..0);
    }
}
