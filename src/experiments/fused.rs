//! The SINGLE-PASS pipeline experiment (planning/investigate-later.md,
//! "Single-pass pipeline", step 3): one traversal that lexes, builds the CST
//! and lints, with the scanner's `push_token` feeding a sink instead of a
//! bare vec.
//!
//! NOT production. The staged path (`lex` → `cst::build` → `lint`) stays the
//! only public API and stays THE ORACLE: `tests/fused_identity.rs` asserts
//! `analyze_fused(src) == (lex, build, lint)` over every corpus book and a
//! snippet zoo, token for token, node for node, observation for observation.
//!
//! # What is copied and what is reused
//!
//! - **The scanner's ARMS are copied** (they emit, so they must call the
//!   sink), but every PURE piece is reused from `crate::scanner` through
//!   `pub(crate)`: the boundary finders, `classify_marker`,
//!   `resolve_marker_idx`, `escape_len`, `attr_list_end`, `folds_delimiter`,
//!   `opens_attrs_frame`, `HotIdx`, `ScanState`. The copy can therefore only
//!   diverge in EMISSION, which is exactly what the oracle checks.
//! - **The CST Builder is copied** into [`Sink`], because the lint machines
//!   need `node_open` / `leaf` / `node_close` events and those are its
//!   push/pop/leaf sites. The production `cst::Builder` is left alone: hooks
//!   or a sink generic would complicate a type that is already shipping.
//! - **The four lint machines are REUSED verbatim** (`Structure`,
//!   `Ancestry`, `Ordering`, `Flat`, `Emit`, `Doc`, `header_scan`), which is
//!   the whole point of step 1's feedable-state-machine refactor.
//!
//! # The delay is TWO tokens, not one
//!
//! Two separate late mutations touch the token a sink has already been handed
//! a pointer to. `scanner::whitespace_arm` extends the PREVIOUS token's `len`
//! on a LATER iteration, and `push_marker` / `marker_arm` stamp `marker_idx`
//! on the token they JUST pushed — after `push_token` (and therefore after
//! this sink's `advance`) has returned. So a token is settled only once the
//! NEXT token exists.
//!
//! One token of delay would be enough if the pipeline only ever read the token
//! in hand — the Builder does — but `Flat`'s attr-terminator-mismatch rule
//! reads `tokens[idx + 1]`'s kind and row, and an unsettled successor there
//! reads as row 0 and reports a mismatch that is not there (found the hard way:
//! every `\w …|…\w*` in en_ult NAM). The rule is therefore **feed N only once
//! N+2 exists**, which settles N and N's lookahead alike; `finish` flushes the
//! two-token tail. `push_token`'s `u16::MAX` split pushes several rows in one
//! call and is handled by the same loop.
//!
//! # The header warm-up, and why there is no divergence
//!
//! `header_scan` reads the `\id` book code and the `\usfm` version from the
//! token stream BOUNDED AT THE FIRST `\c`, and `Flat` takes the version at
//! construction. In a fused world those tokens do not exist ahead of time.
//!
//! Resolved by a WARM-UP BUFFER rather than by judging with a half-known
//! version: the sink delivers no events at all until it has settled the first
//! `Chapter` marker (or the input ends), then calls the REAL `header_scan` on
//! that prefix — which is identical to running it on the whole slice, because
//! `header_scan` breaks at that very token — builds `Flat` with the answer,
//! and flushes the prefix through the pipeline in order. Event order,
//! emission order and the version fact are therefore bit-identical to staged,
//! with no adversarial-input caveat: an `AttrList` ahead of the `\usfm` line
//! is judged with the same version staged judges it with. The cost is a
//! buffered book header (tens of tokens); a document with no `\c` at all
//! buffers whole, which is the honest limit of the trick.
//!
//! # The one REAL divergence: renumber lookahead
//!
//! `lint::next_number` — the only unbounded lookahead in lint — walks
//! FORWARD from a sequence finding to the next `\c`/`\v` designator to refuse
//! a renumber fix that would only move the problem along (bdf_reg ROM 3). In
//! one pass those tokens do not exist yet, so the fused pass would offer the
//! fix staged refuses. Handled by a FINISH-TIME CORRECTION over the emitted
//! findings ([`Sink::correct_renumbers`]): fused is a strict SUPERSET (a
//! shorter slice can only make `next_number` return `None`, which always
//! permits), so the correction only ever REMOVES a fix, and it has every
//! input it needs on the observation itself (anchor, aux, code). Cost is one
//! pass over the FINDINGS, not the tokens.

use memchr::memchr;
use memchr::memchr3;
use memchr::memmem;

use crate::cst::{
    CloseReason, Cst, Frame, FrameRole, NODE_ID_BIT, Node, ROOT_TOKEN, container_kind, context_bit,
    displaces,
};
use crate::lint::{
    Ancestry, Code, Doc, Emit, Flat, LintReport, NO_FIX, Observation, Ordering, Structure,
    UsfmVersion, header_scan,
};
use crate::scanner::{
    AttrScan, BACKSLASH, CR, HotIdx, LF, PIPE, SPACE, ScanState, TAB, attr_list_end,
    classify_marker, designator_gated, escape_len, folds_delimiter, marker_end, newline_end,
    opens_attrs_frame, payload_end, resolve_marker_idx, ws_run_end,
};
use crate::tables::generated;
use crate::tables::schema::{ClosingBehavior, MarkerKind, Payload, ScopeKind, SpecContext};
use crate::{Token, TokenKind};

/// Lex, build and lint one source in ONE traversal.
pub fn analyze_fused(source: &str) -> (Vec<Token>, Cst, LintReport) {
    let mut scanner = FusedScanner::<true>::new(source, true);
    scanner.run();
    scanner.sink.finish()
}

/// The same traversal with the pipeline switched OFF — a sink that only
/// collects tokens. Prices what hanging the sink off `push_token` costs the
/// LEX itself, separately from what the pipeline does.
pub fn lex_noop_sink(source: &str) -> Vec<Token> {
    let mut scanner = FusedScanner::<false>::new(source, false);
    scanner.run();
    scanner.sink.tokens
}

/// The middle rung of the ladder: the traversal builds the CST during the
/// scan but runs no lint. Prices lex+cst fused against `lex` + `cst::build`
/// staged, which is where "did fusing hurt, or did lint's machine state hurt"
/// gets an answer.
pub fn analyze_fused_cst(source: &str) -> (Vec<Token>, Cst) {
    let mut scanner = FusedScanner::<false>::new(source, true);
    scanner.run();
    let (tokens, cst, _) = scanner.sink.finish();
    (tokens, cst)
}

// ---------------------------------------------------------------------------
// The sink: the CST Builder + the lint machines, fed one settled token at a time
// ---------------------------------------------------------------------------

/// The Builder's frame, plus the byte `Ancestry` computed at open and wants
/// back at close (the walk driver keeps it on its own `Frame::scratch`; the
/// module doc of `lint::walk` calls this out as the one fact a Builder-driven
/// pipeline must carry).
struct SinkFrame {
    frame: Frame,
    ancestry_bits: u8,
}

struct Sink<'a, const LINT: bool> {
    source: &'a [u8],
    /// The scanner's output vec, owned HERE so the sink can hand the machines
    /// a `&[Token]` covering everything emitted so far.
    tokens: Vec<Token>,
    /// How many tokens have gone through the pipeline. Stays two behind the
    /// vec until `finish` — that is the delay, see the module doc.
    fed: usize,
    /// False for the noop-sink variant: tokens are collected, nothing runs.
    pipeline: bool,
    /// True until the header prefix has been settled and `header_scan` run.
    buffering: bool,
    /// How far the header probe has looked for the first `\c`.
    probe: usize,

    // --- the copied Builder ---
    cst: Cst,
    scratch: Vec<u32>,
    frames: Vec<SinkFrame>,

    // --- the reused lint machines ---
    structure: Structure,
    ancestry: Ancestry,
    ordering: Ordering,
    flat: Flat,
    out: Emit,
    book: Option<u32>,
    version: Option<UsfmVersion>,
    /// Debug-only: the partition oracle echoed on the fused side. Every leaf
    /// event must be the next token index, so the sink delivers each token
    /// exactly once and in document order — the property the whole fusion
    /// rests on, and the same counter `lint::walk` carries.
    #[cfg(debug_assertions)]
    expected_leaf: u32,
    /// The `missing-id` guard: staged asks `tokens.iter().any(marker-ish)`,
    /// which is a whole extra pass; one bool over the stream is the same fact.
    saw_marker: bool,
}

impl<'a, const LINT: bool> Sink<'a, LINT> {
    fn new(source: &'a str, pipeline: bool) -> Self {
        let bytes = source.as_bytes();
        // The driver has only the source length, so both Builder capacities
        // are HINTS (cst.rs documents the measured densities); being wrong
        // costs a realloc, never a different tree. The noop variant reserves
        // NOTHING for the pipeline — it exists to price the `push_token` hook,
        // and charging it for three arenas it never writes would answer a
        // different question.
        let tokens_hint = bytes.len() / 6;
        let openers_hint = if pipeline { tokens_hint / 4 } else { 0 };
        let scratch_hint = if pipeline { tokens_hint } else { 0 };
        let mut cst = Cst {
            nodes: Vec::with_capacity(openers_hint + 1),
            child_ids: Vec::with_capacity(scratch_hint + openers_hint),
        };
        cst.nodes.push(Node {
            token: ROOT_TOKEN,
            children: 0..0,
            reason: CloseReason::Eof as u8,
            ctx: SpecContext::Scripture as u8,
        });
        Sink {
            source: bytes,
            tokens: Vec::with_capacity(tokens_hint),
            fed: 0,
            pipeline,
            buffering: LINT,
            probe: 0,
            cst,
            scratch: Vec::with_capacity(scratch_hint),
            frames: vec![SinkFrame {
                frame: Frame {
                    node: 0,
                    mark: 0,
                    marker_idx: generated::UNRESOLVED,
                    ctx: SpecContext::Scripture as u8,
                    role: FrameRole::Plain,
                },
                ancestry_bits: 0,
            }],
            structure: Structure::new(),
            ancestry: Ancestry::new(),
            ordering: Ordering::new(),
            flat: Flat::new(None),
            out: Emit::default(),
            book: None,
            version: None,
            saw_marker: false,
            #[cfg(debug_assertions)]
            expected_leaf: 0,
        }
    }

    // ---- emit (the scanner's half) ----------------------------------------

    /// Pushes one token, splitting anything longer than `u16::MAX` — verbatim
    /// from `scanner::push_token` — and then hands over whatever has settled.
    fn push_token(&mut self, kind: TokenKind, start: usize, end: usize) {
        debug_assert!(end >= start);
        let mut at = start;
        while end - at > u16::MAX as usize {
            self.tokens.push(Token {
                start: at as u32,
                len: u16::MAX,
                kind_bits: kind.to_bits(),
                marker_idx: 0,
            });
            at += u16::MAX as usize;
        }
        self.tokens.push(Token {
            start: at as u32,
            len: (end - at) as u16,
            kind_bits: kind.to_bits(),
            marker_idx: 0,
        });
        self.advance();
    }

    /// Every token but the last TWO is settled, itself and its one-token
    /// lookahead — see the module doc. While the header is still buffering
    /// nothing is delivered at all.
    #[inline]
    fn advance(&mut self) {
        if !self.pipeline {
            return;
        }
        if LINT && self.buffering {
            self.probe_header();
            if self.buffering {
                return;
            }
        }
        while self.fed + 2 < self.tokens.len() {
            let idx = self.fed as u32;
            self.fed += 1;
            self.feed(idx);
        }
    }

    /// Look for the first `\c` among the settled tokens; when it lands, run
    /// the real `header_scan` over exactly the prefix it would have read.
    #[cold]
    fn probe_header(&mut self) {
        while self.probe + 2 < self.tokens.len() {
            let token = self.tokens[self.probe];
            if matches!(token.kind(), TokenKind::Marker { .. })
                && generated::kind(token.marker_idx) == MarkerKind::Chapter
            {
                self.resolve_header(self.probe + 1);
                return;
            }
            self.probe += 1;
        }
    }

    fn resolve_header(&mut self, upto: usize) {
        let (book, version) = header_scan(self.source, &self.tokens[..upto]);
        self.book = book;
        self.version = version;
        self.flat = Flat::new(version);
        self.buffering = false;
    }

    // ---- the copied Builder, with the machine calls inlined ---------------

    /// `cst::Builder::feed`, verbatim, with a leaf event at every
    /// `scratch.push` and a node-open event at every `nodes.push`.
    #[inline]
    fn feed(&mut self, token_idx: u32) {
        let token = self.tokens[token_idx as usize];
        let marker_idx = match token.kind() {
            TokenKind::Marker { .. } => {
                self.saw_marker = true;
                token.marker_idx
            }
            TokenKind::Milestone { end } => {
                self.saw_marker = true;
                self.milestone_point(token_idx, token.marker_idx, end);
                return;
            }
            TokenKind::MilestoneTerminator => {
                self.milestone_close(token_idx);
                return;
            }
            TokenKind::ClosingMarker { .. } => {
                self.saw_marker = true;
                self.explicit_close(token_idx, token.marker_idx);
                return;
            }
            _ => {
                self.leaf(token_idx);
                return;
            }
        };

        if generated::kind(marker_idx) == MarkerKind::Milestone {
            self.milestone_point(token_idx, marker_idx, false);
            return;
        }

        if marker_idx == generated::UNRESOLVED {
            while self.frames.len() > 1 {
                let reason = CloseReason::for_displacement(self.top_marker_idx());
                self.close_top(reason);
            }
            self.leaf(token_idx);
            return;
        }

        if let Some(kind) = generated::closes_scope(marker_idx) {
            self.scope_close(token_idx, kind);
            return;
        }

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
            let node = self.cst.nodes.len() as u32;
            self.cst.nodes.push(Node {
                token: token_idx,
                children: 0..0,
                reason: CloseReason::Eof as u8,
                ctx,
            });
            let mark = self.scratch.len() as u32;
            let role = if opens_scope == Some(ScopeKind::Sidebar) {
                FrameRole::Sidebar
            } else {
                FrameRole::Plain
            };
            self.open(node, mark, marker_idx, ctx, role);
            self.leaf(token_idx);
        } else {
            self.leaf(token_idx);
        }
    }

    /// A node-open event, then the frame push. The order matters: the walk
    /// delivers `node_open` before the node's first child, and the opening
    /// token IS that first child.
    #[inline]
    fn open(
        &mut self,
        node: u32,
        mark: u32,
        marker_idx: generated::MarkerIdx,
        ctx: u8,
        role: FrameRole,
    ) {
        let doc = Doc {
            source: self.source,
            tokens: &self.tokens,
            cst: &self.cst,
        };
        let ancestry_bits = if LINT {
            self.ancestry
                .on_node_open(&doc, &self.cst.nodes[node as usize])
        } else {
            0
        };
        self.frames.push(SinkFrame {
            frame: Frame {
                node,
                mark,
                marker_idx,
                ctx,
                role,
            },
            ancestry_bits,
        });
    }

    /// A leaf event, then the scratch push — the walk's `on_leaf` order
    /// (structure, ancestry, ordering, flat) exactly.
    #[inline]
    fn leaf(&mut self, token_idx: u32) {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(
                token_idx, self.expected_leaf,
                "the sink must deliver every token index exactly once, in order"
            );
            self.expected_leaf += 1;
        }
        self.scratch.push(token_idx);
        if !LINT {
            return;
        }
        let token = self.tokens[token_idx as usize];
        let kind = token.kind();
        let doc = Doc {
            source: self.source,
            tokens: &self.tokens,
            cst: &self.cst,
        };
        self.structure.on_leaf(&doc, token_idx, kind, &mut self.out);
        self.ancestry
            .on_leaf(&doc, token_idx, &token, kind, &mut self.out);
        self.ordering
            .on_leaf(&doc, token_idx, &token, kind, &mut self.out);
        self.flat
            .on_leaf(&doc, token_idx, &token, kind, &mut self.out);
    }

    #[inline]
    fn top(&self) -> &Frame {
        &self.frames.last().expect("root frame remains").frame
    }

    #[inline]
    fn top_marker_idx(&self) -> generated::MarkerIdx {
        self.top().marker_idx
    }

    /// `cst::Builder::close_top` plus the node-close event, delivered AFTER
    /// the arena flush so the node the machines see is the finished one.
    fn close_top(&mut self, reason: CloseReason) {
        let popped = self.frames.pop().expect("a frame is open");
        let frame = popped.frame;
        let start = self.cst.child_ids.len() as u32;
        self.cst
            .child_ids
            .extend_from_slice(&self.scratch[frame.mark as usize..]);
        self.scratch.truncate(frame.mark as usize);
        self.scratch.push(NODE_ID_BIT | frame.node);
        self.cst.nodes[frame.node as usize].children = start..self.cst.child_ids.len() as u32;
        self.cst.nodes[frame.node as usize].reason = reason as u8;

        if !LINT {
            return;
        }
        let doc = Doc {
            source: self.source,
            tokens: &self.tokens,
            cst: &self.cst,
        };
        self.structure.on_node_close(
            &doc,
            frame.node,
            &self.cst.nodes[frame.node as usize],
            &mut self.out,
        );
        self.ancestry.on_node_close(popped.ancestry_bits);
    }

    fn explicit_close(&mut self, token_idx: u32, closer_idx: generated::MarkerIdx) {
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            if self.frames[depth].frame.wall() {
                break;
            }
            if self.frames[depth].frame.marker_idx == closer_idx {
                target = Some(depth);
                break;
            }
        }
        let Some(depth) = target else {
            self.leaf(token_idx);
            return;
        };
        while self.frames.len() > depth + 1 {
            let reason = CloseReason::for_displacement(self.top_marker_idx());
            self.close_top(reason);
        }
        self.leaf(token_idx);
        self.close_top(CloseReason::Explicit);
    }

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
                let reachable = (1..self.frames.len())
                    .rev()
                    .take_while(|&d| !self.frames[d].frame.barrier())
                    .any(|d| self.frames[d].frame.role == FrameRole::Container(kind));
                if reachable {
                    while self.top().role != FrameRole::Container(kind) {
                        let reason = CloseReason::for_displacement(self.top_marker_idx());
                        self.close_top(reason);
                    }
                    ends = Some(kind);
                }
            } else {
                let ctx = match kind {
                    ScopeKind::List => SpecContext::List as u8,
                    _ => SpecContext::Table as u8,
                };
                let node = self.cst.nodes.len() as u32;
                self.cst.nodes.push(Node {
                    token: token_idx,
                    children: 0..0,
                    reason: CloseReason::Eof as u8,
                    ctx,
                });
                let mark = self.scratch.len() as u32;
                self.open(node, mark, marker_idx, ctx, FrameRole::Container(kind));
            }
        }

        let inherited = self.top().ctx;
        let ctx = generated::contributes_context(marker_idx)
            .map(|context| context as u8)
            .unwrap_or(inherited);
        let node = self.cst.nodes.len() as u32;
        self.cst.nodes.push(Node {
            token: token_idx,
            children: 0..0,
            reason: CloseReason::Eof as u8,
            ctx,
        });
        let mark = self.scratch.len() as u32;
        self.open(node, mark, marker_idx, ctx, FrameRole::Point { ends });
        self.leaf(token_idx);
    }

    fn milestone_close(&mut self, token_idx: u32) {
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            if let FrameRole::Point { ends } = self.frames[depth].frame.role {
                target = Some((depth, ends));
                break;
            }
            if self.frames[depth].frame.wall() {
                break;
            }
        }
        let Some((depth, ends)) = target else {
            self.leaf(token_idx);
            return;
        };
        while self.frames.len() > depth + 1 {
            let reason = CloseReason::for_displacement(self.top_marker_idx());
            self.close_top(reason);
        }
        self.leaf(token_idx);
        self.close_top(CloseReason::Explicit);
        if ends.is_some_and(|kind| self.top().role == FrameRole::Container(kind)) {
            self.close_top(CloseReason::Explicit);
        }
    }

    fn same_kind_evict(&mut self, incoming: MarkerKind) {
        let evicts = |kind: MarkerKind| match incoming {
            MarkerKind::Paragraph => kind == MarkerKind::Paragraph,
            MarkerKind::TableRow => matches!(kind, MarkerKind::TableRow | MarkerKind::TableCell),
            MarkerKind::TableCell => kind == MarkerKind::TableCell,
            _ => false,
        };
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            if self.frames[depth].frame.wall() {
                break;
            }
            if self.frames[depth].frame.role == FrameRole::Plain {
                let kind = generated::kind(self.frames[depth].frame.marker_idx);
                if evicts(kind) {
                    target = Some(depth);
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

    fn scope_close(&mut self, token_idx: u32, kind: ScopeKind) {
        let mut target = None;
        for depth in (1..self.frames.len()).rev() {
            let opener = self.frames[depth].frame.marker_idx;
            if generated::opens_scope(opener) == Some(kind) {
                target = Some(depth);
                break;
            }
            if self.frames[depth].frame.wall() {
                break;
            }
        }
        let Some(depth) = target else {
            self.leaf(token_idx);
            return;
        };
        while self.frames.len() > depth + 1 {
            let reason = CloseReason::for_displacement(self.top_marker_idx());
            self.close_top(reason);
        }
        self.leaf(token_idx);
        self.close_top(CloseReason::Explicit);
    }

    // ---- end of input -----------------------------------------------------

    fn finish(mut self) -> (Vec<Token>, Cst, LintReport) {
        assert!(
            self.tokens.len() < NODE_ID_BIT as usize,
            "token ids must fit the tag bit"
        );
        if self.pipeline {
            // A document that never reached a `\c` resolves its header here,
            // over everything — the same prefix `header_scan` would have read.
            if LINT && self.buffering {
                self.resolve_header(self.tokens.len());
            }
            // The tail the one-token delay was holding back.
            while self.fed < self.tokens.len() {
                let idx = self.fed as u32;
                self.fed += 1;
                self.feed(idx);
            }
            while self.frames.len() > 1 {
                self.close_top(CloseReason::Eof);
            }
        }

        #[cfg(debug_assertions)]
        if self.pipeline {
            debug_assert_eq!(
                self.expected_leaf as usize,
                self.tokens.len(),
                "the sink must deliver every token"
            );
        }

        let root_start = self.cst.child_ids.len() as u32;
        self.cst.child_ids.extend_from_slice(&self.scratch);
        self.cst.nodes[0].children = root_start..self.cst.child_ids.len() as u32;

        let doc = Doc {
            source: self.source,
            tokens: &self.tokens,
            cst: &self.cst,
        };
        if LINT {
            self.structure.finish(&doc, &mut self.out);
            self.ordering.finish(&mut self.out);
            if self.book.is_none() && self.saw_marker {
                self.out.push(Observation::one(Code::MissingId, 0));
            }
            self.correct_renumbers();
        }

        let report = self.out.finish(self.book, self.version);
        (self.tokens, self.cst, report)
    }

    /// The finish-time half of the renumber rule — see the module doc. A
    /// sequence finding judged mid-stream saw no tokens after itself, so
    /// `next_number` answered `None` and the fix was offered; with the whole
    /// stream in hand some of those must be withdrawn.
    fn correct_renumbers(&mut self) {
        let mut drop: Vec<u32> = Vec::new();
        for (slot, observation) in self.out.observations.iter().enumerate() {
            let verse = match observation.code {
                Code::VerseDuplicate | Code::VerseOutOfOrder => true,
                Code::ChapterDuplicate | Code::ChapterOutOfOrder => false,
                _ => continue,
            };
            let fix = self.out.fix_of[slot];
            if fix == NO_FIX {
                continue;
            }
            if next_number(self.source, &self.tokens, observation.anchor, verse)
                .is_some_and(|next| next <= observation.aux)
            {
                self.out.fix_of[slot] = NO_FIX;
                drop.push(fix);
            }
        }
        if drop.is_empty() {
            return;
        }
        // Withdrawing a fix means compacting both arenas and re-basing every
        // surviving index — rare enough (a handful per corpus) to be written
        // for clarity rather than speed.
        drop.sort_unstable();
        let mut fixes = Vec::with_capacity(self.out.fixes.len() - drop.len());
        let mut edits = Vec::with_capacity(self.out.edit_list.len());
        let mut remap = vec![NO_FIX; self.out.fixes.len()];
        for (old, fix) in self.out.fixes.iter().enumerate() {
            if drop.binary_search(&(old as u32)).is_ok() {
                continue;
            }
            let start = edits.len() as u32;
            edits.extend_from_slice(
                &self.out.edit_list[fix.edits.start as usize..fix.edits.end as usize],
            );
            remap[old] = fixes.len() as u32;
            fixes.push(crate::lint::Fix {
                label: fix.label,
                edits: start..edits.len() as u32,
            });
        }
        for link in self.out.fix_of.iter_mut() {
            if *link != NO_FIX {
                *link = remap[*link as usize];
            }
        }
        self.out.fixes = fixes;
        self.out.edit_list = edits;
    }
}

/// `lint::next_number`, copied because it is the one piece of lint the fused
/// pass cannot run at its natural moment (module doc). Behaviour must match
/// exactly; the oracle is what says it does.
fn next_number(source: &[u8], tokens: &[Token], from: u32, verse: bool) -> Option<u32> {
    use crate::designator::{self, Designator};
    let wanted = if verse {
        MarkerKind::Verse
    } else {
        MarkerKind::Chapter
    };
    let mut awaiting = false;
    for token in &tokens[from as usize + 1..] {
        match token.kind() {
            TokenKind::AttrList => {}
            TokenKind::Designator if awaiting => {
                let span = &source[token.start as usize..token.end() as usize];
                let parsed = if verse {
                    designator::verse(span)
                } else {
                    designator::chapter(span)
                };
                return match parsed {
                    Designator::Wellformed { first, .. } => Some(first),
                    Designator::Malformed => None,
                };
            }
            TokenKind::Marker { .. } => {
                let kind = generated::kind(token.marker_idx);
                if verse && kind == MarkerKind::Chapter {
                    return None;
                }
                awaiting = kind == wanted;
            }
            _ => awaiting = false,
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The scanner copy: arms only, every pure decision reused
// ---------------------------------------------------------------------------

struct FusedScanner<'a, const LINT: bool> {
    bytes: &'a [u8],
    sink: Sink<'a, LINT>,
    mode: ScanState,
    opt_break_finder: memmem::Finder<'static>,
    hot: HotIdx,
}

impl<'a, const LINT: bool> FusedScanner<'a, LINT> {
    fn new(source: &'a str, pipeline: bool) -> Self {
        FusedScanner {
            bytes: source.as_bytes(),
            sink: Sink::new(source, pipeline),
            mode: ScanState {
                awaiting_delimiter_ws: false,
                pending_payload: Payload::None,
                designator_gated: false,
                after_marker: false,
                attr_frames: 0,
                attr_list_ends_at_line: false,
            },
            opt_break_finder: memmem::Finder::new(b"//"),
            hot: HotIdx::resolve(),
        }
    }

    fn run(&mut self) {
        let bytes = self.bytes;
        let mut index = 0usize;
        while index < bytes.len() {
            // `common_marker_checks` is unconditional here — the fused
            // scanner has no `FAST` const to switch it off, since
            // `lex_general_path_only` is the real scanner's oracle, not this
            // one's.
            let fast = if bytes[index] == BACKSLASH {
                self.common_marker_checks(index)
            } else {
                None
            };
            if let Some(next) = fast {
                index = next;
                continue;
            }
            index = match bytes[index] {
                SPACE | TAB if self.mode.awaiting_delimiter_ws => self.whitespace_arm(index),
                CR | LF => self.newline_arm(index),
                BACKSLASH if escape_len(bytes, index).is_some() => self.text_arm(index),
                BACKSLASH => self.marker_arm(index),
                PIPE => match self.try_attr_list(index, self.mode.after_marker, index, None) {
                    Ok(end) => end,
                    Err(_) => self.text_arm(index),
                },
                _ => self.text_arm(index),
            };
        }
    }

    #[inline(always)]
    fn push_marker(&mut self, start: usize, end: usize, idx: generated::MarkerIdx) {
        self.sink
            .push_token(TokenKind::Marker { nested: false }, start, end);
        if let Some(last) = self.sink.tokens.last_mut() {
            last.marker_idx = idx;
        }
    }

    #[inline(always)]
    fn common_marker_checks(&mut self, index: usize) -> Option<usize> {
        let bytes = self.bytes;
        match *bytes.get(index + 1)? {
            b'v' if bytes.get(index + 2) == Some(&SPACE) => {
                let digits_from = index + 3;
                let mut end = digits_from;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                if end == digits_from
                    || !matches!(
                        bytes.get(end),
                        None | Some(&SPACE | &TAB | &CR | &LF | &BACKSLASH | &PIPE)
                    )
                {
                    return None;
                }
                let designator_end = ws_run_end(bytes, end);
                self.push_marker(index, digits_from, self.hot.v.idx);
                self.sink
                    .push_token(TokenKind::Designator, digits_from, designator_end);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                self.mode.after_marker = false;
                Some(designator_end)
            }
            b'q' => self.fused_leveled(index, index + 2, self.hot.q),
            b's' => self.fused_leveled(index, index + 2, self.hot.s),
            b'p' => self.fused_plain(index, index + 2, self.hot.p),
            b'b' => self.fused_plain(index, index + 2, self.hot.b),
            b'f' => match bytes.get(index + 2) {
                Some(&b't') => self.fused_plain(index, index + 3, self.hot.ft),
                Some(&b'r') => self.fused_plain(index, index + 3, self.hot.fr),
                _ => self.fused_plain(index, index + 2, self.hot.f),
            },
            b'x' if bytes.get(index + 2) == Some(&b't') => {
                self.fused_plain(index, index + 3, self.hot.xt)
            }
            _ => None,
        }
    }

    #[inline(always)]
    fn fused_leveled(
        &mut self,
        index: usize,
        name_end: usize,
        hot: crate::scanner::Hot,
    ) -> Option<usize> {
        let name_end = match self.bytes.get(name_end) {
            Some(&d) if d.is_ascii_digit() => {
                if !(b'1'..=b'0' + hot.level_max).contains(&d) {
                    return None;
                }
                name_end + 1
            }
            _ => name_end,
        };
        self.fused_plain(index, name_end, hot)
    }

    #[inline(always)]
    fn fused_plain(
        &mut self,
        index: usize,
        name_end: usize,
        hot: crate::scanner::Hot,
    ) -> Option<usize> {
        match self.bytes.get(name_end) {
            Some(&SPACE | &TAB) if hot.folds => {
                let end = ws_run_end(self.bytes, name_end + 1);
                self.push_marker(index, end, hot.idx);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                self.mode.after_marker = true;
                self.mode.pending_payload = hot.payload;
                self.mode.designator_gated = hot.designator_gated;
                if hot.attrs_frame {
                    self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                }
                Some(end)
            }
            Some(&SPACE | &TAB) => {
                self.push_marker(index, name_end, hot.idx);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                self.mode.after_marker = true;
                self.mode.pending_payload = hot.payload;
                self.mode.designator_gated = hot.designator_gated;
                if hot.attrs_frame {
                    self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                }
                Some(name_end)
            }
            Some(&CR | &LF) => {
                self.push_marker(index, name_end, hot.idx);
                let end = newline_end(self.bytes, name_end);
                self.sink.push_token(TokenKind::Newline, name_end, end);
                self.mode.awaiting_delimiter_ws = false;
                self.mode.pending_payload = Payload::None;
                self.mode.after_marker = false;
                self.mode.attr_frames = 0;
                self.mode.attr_list_ends_at_line = false;
                Some(end)
            }
            _ => None,
        }
    }

    #[inline(always)]
    fn whitespace_arm(&mut self, index: usize) -> usize {
        let end = ws_run_end(self.bytes, index);
        self.mode.awaiting_delimiter_ws = false;
        if let Some(last) = self.sink.tokens.last_mut() {
            last.len = (end as u32 - last.start) as u16;
        }
        end
    }

    #[inline(always)]
    fn newline_arm(&mut self, index: usize) -> usize {
        self.mode.awaiting_delimiter_ws = false;
        self.mode.pending_payload = Payload::None;
        self.mode.after_marker = false;
        self.mode.attr_frames = 0;
        self.mode.attr_list_ends_at_line = false;
        let end = newline_end(self.bytes, index);
        self.sink.push_token(TokenKind::Newline, index, end);
        end
    }

    #[inline(always)]
    fn marker_arm(&mut self, index: usize) -> usize {
        let bytes = self.bytes;
        let end = marker_end(bytes, index);
        let slice = &bytes[index..end];
        let kind = classify_marker(slice);
        self.sink.push_token(kind, index, end);
        let idx = resolve_marker_idx(slice, kind);
        if let Some(last) = self.sink.tokens.last_mut() {
            last.marker_idx = idx;
        }
        self.mode.awaiting_delimiter_ws = folds_delimiter(kind, idx);
        self.mode.after_marker =
            matches!(kind, TokenKind::Marker { .. } | TokenKind::Milestone { .. });
        self.mode.pending_payload = if matches!(kind, TokenKind::Marker { .. }) {
            generated::payload(idx)
        } else {
            Payload::None
        };
        self.mode.designator_gated = designator_gated(idx);
        match kind {
            TokenKind::Marker { .. } if opens_attrs_frame(idx) => {
                self.mode.attr_frames = self.mode.attr_frames.saturating_add(1);
                if generated::kind(idx) == MarkerKind::Periph {
                    self.mode.attr_list_ends_at_line = true;
                }
            }
            TokenKind::ClosingMarker { .. } | TokenKind::MilestoneTerminator => {
                self.mode.attr_frames = self.mode.attr_frames.saturating_sub(1)
            }
            _ => {}
        }
        end
    }

    #[inline(always)]
    fn try_attr_list(
        &mut self,
        pipe_at: usize,
        front: bool,
        text_from: usize,
        first_stop: Option<usize>,
    ) -> Result<usize, usize> {
        let (end, absorbs_trailing_ws) = match attr_list_end(self.bytes, pipe_at, front, first_stop)
        {
            AttrScan::NodeInitial(end) => (end, true),
            AttrScan::Trailing(end) => (end, false),
            // `\periph`'s list ends with the line — mirrors scanner.rs.
            AttrScan::NotAList(stop)
                if self.mode.attr_list_ends_at_line
                    && matches!(self.bytes.get(stop), None | Some(&CR) | Some(&LF)) =>
            {
                (stop, false)
            }
            AttrScan::NotAList(stop) => return Err(stop),
        };
        if pipe_at > text_from {
            self.sink.push_token(TokenKind::Text, text_from, pipe_at);
        }
        self.sink.push_token(TokenKind::AttrList, pipe_at, end);
        self.mode.awaiting_delimiter_ws = absorbs_trailing_ws;
        self.mode.after_marker = false;
        Ok(end)
    }

    #[inline(always)]
    fn text_arm(&mut self, index: usize) -> usize {
        let bytes = self.bytes;
        self.mode.awaiting_delimiter_ws = false;
        self.mode.after_marker = false;
        let payload_kind = match self.mode.pending_payload {
            Payload::Designator
                if !self.mode.designator_gated
                    || bytes.get(index).is_some_and(u8::is_ascii_digit) =>
            {
                Some(TokenKind::Designator)
            }
            Payload::Designator => None,
            Payload::NoteCaller => Some(TokenKind::NoteCaller),
            Payload::BookCode => Some(TokenKind::BookCode),
            Payload::None | Payload::Version => None,
        };
        self.mode.pending_payload = Payload::None;
        if let Some(kind) = payload_kind {
            let end = payload_end(bytes, index);
            if end > index {
                let end = ws_run_end(bytes, end);
                self.sink.push_token(kind, index, end);
                return end;
            }
        }
        let mut segment_start = index;
        let mut cursor = index;

        loop {
            let rest = &bytes[cursor..];
            let control = memchr3(BACKSLASH, CR, LF, rest);
            let bound = control.unwrap_or(rest.len());
            let opt_break = self.opt_break_finder.find(&rest[..bound]);
            let pipe = if self.mode.attr_frames > 0 {
                memchr(PIPE, &rest[..bound])
            } else {
                None
            };
            let Some(offset) = [control, opt_break, pipe].into_iter().flatten().min() else {
                cursor = bytes.len();
                break;
            };
            let pos = cursor + offset;

            match bytes[pos] {
                BACKSLASH => match escape_len(bytes, pos) {
                    Some(len) => cursor = pos + len,
                    None => {
                        cursor = pos;
                        break;
                    }
                },
                PIPE => {
                    match self.try_attr_list(pos, false, segment_start, control.map(|c| cursor + c))
                    {
                        Ok(end) => return end,
                        Err(stop) => cursor = stop,
                    }
                }
                crate::scanner::SLASH => {
                    if pos > segment_start {
                        self.sink.push_token(TokenKind::Text, segment_start, pos);
                    }
                    self.sink.push_token(TokenKind::OptBreak, pos, pos + 2);
                    segment_start = pos + 2;
                    cursor = pos + 2;
                }
                _ => {
                    cursor = pos;
                    break;
                }
            }
        }

        if cursor > segment_start {
            self.sink.push_token(TokenKind::Text, segment_start, cursor);
        }

        cursor
    }
}
