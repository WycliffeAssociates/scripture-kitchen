//! Findings over an already-built document: `lex → cst::build → lint`.
//!
//! Phase 1 ships the skeleton and the STRUCTURAL subsystem only (ordering,
//! adjacency, form, payload and attribute rules, and the fix model, are later
//! phases — see planning/lint-sketch.md "Build phases").
//!
//! Three laws shape everything here:
//!
//! - **Lint reads verdicts, it never re-derives them.** [`CloseReason`] is the
//!   walker's judgement about how a frame ended; this module matches on it and
//!   asks the row only "did that row want a closer" — the exact predicate the
//!   walker used at pop time ([`wants_closer`]), never a second notion of it.
//! - **Flag, never repair.** No token is reordered, inserted or dropped
//!   (the editor session's token→span→UTF-16 mapping depends on it), and no
//!   text is rewritten. Phase 4's fixes are *offered* byte edits, not applied.
//! - **No strings, anywhere.** An [`Observation`] is four u32s. Everything a
//!   message needs textually is already a span reachable through
//!   `anchor`/`second`; everything else is a small integer in `aux`, whose
//!   meaning per code is the [`LintRow::aux`] column. That is what lets a
//!   report cross wasm as one flat `[code, anchor, second, aux] × n` array.
//!
//! Shape of the pass: one sweep over `cst.nodes` (per-node close verdicts and
//! the consumed-closer bitset), one sweep over `tokens` (row-keyed token
//! findings), and one in-order tree walk carrying two depth counters (sidebar
//! and paragraph). All linear, no recursion.
//!
//! PERF (measured 2026-08-19, `playground --lint-only`, min-of-8 at load ~14 so
//! trust the ratios): ~7.2 ns/token over en_ult's 6.57M tokens (47ms on top of
//! the 140ms lex + build), ~6.3 ns/token over en_ulb's 255k. Split by pass:
//! the tree walk is ~3.8, the token sweep ~1.4, the node sweep plus the two
//! scratch vecs ~2.0. The tree walk dominates because ancestry is the one fact
//! no single node carries — it touches every arena id and chases nodes out of
//! order — and it is therefore the only lead worth pulling if lint ever needs
//! to get cheaper.

use crate::cst::{CloseReason, Cst, Node};
use crate::tables::generated;
use crate::tables::schema::{Category as MarkerCategory, ClosingBehavior, MarkerKind, ScopeKind};
use crate::{Token, TokenKind};

/// `Observation::second` when there is no second party.
pub const NO_TOKEN: u32 = u32::MAX;

const NODE_ID_BIT: u32 = 1 << 31;
const ROOT_TOKEN: u32 = u32::MAX;

/// CodeMirror's exact `Diagnostic.severity` ladder; the editor session maps
/// these 1:1 with no translation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// The subsystem a code belongs to. Consumers group by this; it is also how a
/// future per-rule config addresses whole families at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Nesting, closers, barriers, recovery — everything phase 1 ships.
    Structure,
    Ordering,
    Attributes,
    Payload,
    Form,
    Version,
}

/// What [`Observation::aux`] MEANS for a given code — the column that keeps a
/// bare u32 from being opaque. Phase 1's codes are all [`Self::None`]; the
/// variants exist because the enum is part of the ruled row shape and the
/// later phases fill them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxKind {
    /// `aux` is unused and always 0.
    None,
    /// The verse/chapter number the sequence expected here.
    ExpectedNumber,
    /// The row's `numbered_max` cap.
    NumberingCap,
    /// A plain count (occurrences folded into one aggregate finding).
    Count,
    /// A [`UsfmVersion`] discriminant.
    Version,
}

/// The declared `\usfm` versions a rule's severity can key on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsfmVersion {
    V3_0,
    V3_2,
    V4_0,
}

/// One finding. Four u32s, `Copy`, no allocation: `anchor` and `second` are
/// TOKEN indices into the linted slice (per-build, like `marker_idx`), and
/// `second` is [`NO_TOKEN`] when the finding has only one party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub code: Code,
    pub anchor: u32,
    pub second: u32,
    pub aux: u32,
}

impl Observation {
    fn one(code: Code, anchor: u32) -> Self {
        Self {
            code,
            anchor,
            second: NO_TOKEN,
            aux: 0,
        }
    }

    fn pair(code: Code, anchor: u32, second: u32) -> Self {
        Self {
            code,
            anchor,
            second,
            aux: 0,
        }
    }
}

/// Everything one lint run learned about one document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintReport {
    /// The `\id` line's BookCode token index. `None` IS the missing-`\id`
    /// state (real in the wild: BSB Ecclesiastes) — never a crash, and in
    /// phase 1 never an [`Observation`] either: the `missing-id` code is
    /// phase 2's, and until then this field carries the fact by itself.
    pub book: Option<u32>,
    /// Sorted by `anchor`, then by code — one document order for consumers,
    /// independent of which internal pass produced a finding.
    pub observations: Vec<Observation>,
}

// ---------------------------------------------------------------------------
// The codes and their table
// ---------------------------------------------------------------------------

/// One finding kind. DECLARATION ORDER IS THE DISCRIMINANT and indexes
/// [`LINT_ROWS`] directly (asserted in tests) — but the number is per-build
/// wire data only. The durable identity of a rule is its kebab-case
/// [`LintRow::name`]; config and humans use that, never the integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum Code {
    UnclosedNote,
    UnclosedChar,
    UnclosedAtEof,
    UnterminatedContainer,
    UnterminatedMilestone,
    OrphanCloser,
    OrphanTerminator,
    OrphanContainerEnd,
    ContentOutsideSidebarRule,
    UnknownMarker,
    NestedSpellingMisuse,
    MissingParagraph,
}

impl Code {
    /// The row is a table lookup, not a match — declaration order is the index.
    pub fn row(self) -> &'static LintRow {
        &LINT_ROWS[self as usize]
    }
}

/// The per-code data. Everything a consumer needs to present a finding, and
/// nothing that varies per occurrence.
pub struct LintRow {
    pub code: Code,
    /// The durable identity. Kebab-case, unique, never renamed lightly.
    pub name: &'static str,
    pub category: Category,
    /// The severity when no `\usfm` version has been declared, or when the
    /// declared version is below `escalation`'s.
    pub severity: Severity,
    /// At/after this declared `\usfm` version, use THIS severity instead.
    /// `None` = flat. This column owns the version facts no `MarkerRow` owns:
    /// the spec deprecates and then removes forms, and the table records the
    /// marker page, never a version ladder.
    pub escalation: Option<(UsfmVersion, Severity)>,
    /// What [`Observation::aux`] means for this code.
    pub aux: AuxKind,
    /// Default-English message with `{anchor}`/`{second}` standing for the
    /// marker text at those token spans. Rendering and localization are the
    /// consumer's; the library never allocates a message.
    pub template: &'static str,
    /// The label a phase-4 fix would carry. Present here already so the row is
    /// the single place a rule's affordances are declared; no fix machinery
    /// exists yet.
    pub fix_label: Option<&'static str>,
}

/// The authored rules table — one row per [`Code`], in the enum's order.
///
/// Phase 1: the Structure family only. Rows for the ordering, payload,
/// adjacency, attribute, form and version families land with their passes.
pub const LINT_ROWS: [LintRow; 12] = [
    // A note frame the walker had to end without its `\f*`/`\x*`: something
    // that cannot live inside a note (a `\c`, a bare `\v`, an unknown marker)
    // arrived while it was open. The three live corpus instances (en_ulb ISA
    // and MRK, bsb GEN) are all genuinely truncated footnotes.
    LintRow {
        code: Code::UnclosedNote,
        name: "unclosed-note",
        category: Category::Structure,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} was never closed",
        fix_label: Some("insert the note closer"),
    },
    // Same event for a character marker: `\add` still open when its enclosing
    // scope was forced to end. Character markers REQUIRE their `\X*`.
    LintRow {
        code: Code::UnclosedChar,
        name: "unclosed-char",
        category: Category::Structure,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} was never closed",
        fix_label: Some("insert the closer"),
    },
    // The document simply ended with a closer-wanting frame open. Weaker than
    // Recovery — nothing displaced it, the file just stopped. Paragraphs,
    // cells and rows want no closer and are silent here.
    LintRow {
        code: Code::UnclosedAtEof,
        name: "unclosed-at-eof",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} is still open at the end of the book",
        fix_label: Some("insert the closer"),
    },
    // A U25003 `\list-s`/`\table-s` container ended by displacement instead of
    // its `\list-e\*`. The closing milestone is OPTIONAL in 3.2 and REQUIRED
    // in 4, which is exactly what the escalation column is for.
    LintRow {
        code: Code::UnterminatedContainer,
        name: "unterminated-container",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: Some((UsfmVersion::V4_0, Severity::Error)),
        aux: AuxKind::None,
        template: "\\{anchor} container was not closed by its end milestone",
        fix_label: Some("insert the container end milestone"),
    },
    // A milestone point never met its `\*`. Both Recovery and Eof mean the
    // same thing for a point — its span is only its attribute list, so
    // anything at all reaching it is the terminator going missing.
    LintRow {
        code: Code::UnterminatedMilestone,
        name: "unterminated-milestone",
        category: Category::Structure,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} milestone is missing its \\*",
        fix_label: Some("insert \\*"),
    },
    // A `\X*` that closed nothing: no open frame of that name was in reach
    // (or the only one was behind a sidebar barrier). The walker leaves it an
    // ordinary leaf; the finding is ours.
    LintRow {
        code: Code::OrphanCloser,
        name: "orphan-closer",
        category: Category::Structure,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} closes nothing",
        fix_label: Some("delete the closer"),
    },
    // A bare `\*` with no open milestone point in reach.
    LintRow {
        code: Code::OrphanTerminator,
        name: "orphan-terminator",
        category: Category::Structure,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "\\* terminates no milestone",
        fix_label: Some("delete \\*"),
    },
    // A `\list-e`/`\table-e` whose container was not open (or sat behind a
    // barrier). Distinct from `orphan-terminator`: the `-e` point's own `\*`
    // DID close the point, so nothing about the terminator is orphaned — what
    // is missing is the container the `-e` claims to end.
    LintRow {
        code: Code::OrphanContainerEnd,
        name: "orphan-container-end",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} ends no open container",
        fix_label: None,
    },
    // A sidebar is a POP BARRIER, so a `\c` or `\v` written inside one stays
    // inside it — structurally consistent, and exactly what the spec says must
    // not happen. The walker deliberately does not "fix" it by unwinding;
    // this is the finding that pays for that choice.
    LintRow {
        code: Code::ContentOutsideSidebarRule,
        name: "content-outside-sidebar-rule",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} is inside the sidebar opened by \\{second}",
        fix_label: None,
    },
    // A marker that resolved to row 0. Today that covers unknown names,
    // illegal spellings AND every custom `\z` extension, because there is no
    // configuration channel yet — when one lands, configured `\z` markers get
    // real rows and stop reaching this rule. It is also the pop-all recovery
    // event: the walker cannot trust any open scope across a marker it cannot
    // classify.
    LintRow {
        code: Code::UnknownMarker,
        name: "unknown-marker",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} is not a known marker",
        fix_label: None,
    },
    // The `\+X` nested SPELLING on a row that is not a character marker. The
    // lexer records the spelling wherever it appears precisely so this stays a
    // table question; row 0 is excluded because `unknown-marker` already says
    // everything there is to say about it.
    LintRow {
        code: Code::NestedSpellingMisuse,
        name: "nested-spelling-misuse",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\+{anchor} is not a character marker; the nested spelling does not apply",
        fix_label: None,
    },
    // A `\v` with no paragraph anywhere above it. usfmtc FABRICATES an
    // implicit `\p` here (usfmparser.py:891); we flag and never repair —
    // synthesizing a token would break the partition and lie to the editor.
    // ONE finding per paragraph-less RUN, anchored at its first verse: that is
    // where the single repairing `\p` belongs, and per-verse reporting turns
    // one authoring slip into a chapter of noise.
    LintRow {
        code: Code::MissingParagraph,
        name: "missing-paragraph",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} is not inside a paragraph",
        fix_label: Some("insert \\p"),
    },
];

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

/// Lints one already-lexed, already-built document.
///
/// INVARIANT: `lint` never reorders, inserts or drops tokens, and never
/// touches `source` bytes destructively — the caller's `tokens` slice is the
/// same slice afterwards. The editor session's token→span→UTF-16 mapping is
/// built on that.
///
/// `source` is taken because the signature is the library's one entry point
/// (NEXT-STEPS, ruled 2026-08-19) and later phases read bytes through it; the
/// structural rules ask only the CST and the marker table.
pub fn lint(source: &[u8], tokens: &[Token], cst: &Cst) -> LintReport {
    // Phase 1 is entirely structural — every fact it needs is a verdict or a
    // row. Kept in the signature so phase 2/3 add rules, not a new entry point.
    let _ = source;

    let mut report = LintReport {
        book: tokens
            .iter()
            .position(|token| token.kind() == TokenKind::BookCode)
            .map(|idx| idx as u32),
        observations: Vec::new(),
    };

    // `consumed[t]` = token t is a closer that actually closed a frame. Built
    // from the nodes rather than re-walked: a closer that closed something is
    // by construction the LAST child of an Explicit node.
    let mut consumed = vec![false; tokens.len()];
    node_pass(tokens, cst, &mut consumed, &mut report.observations);
    token_pass(tokens, &consumed, &mut report.observations);
    tree_pass(tokens, cst, &mut report.observations);

    // Findings are rare relative to tokens, so one sort at the end is cheaper
    // than threading document order through three passes that each have a
    // natural order of their own.
    report
        .observations
        .sort_unstable_by_key(|obs| (obs.anchor, obs.code as u16));
    report
}

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

/// One sweep over the nodes: the close-verdict rules, plus the consumed-closer
/// bitset the token pass needs.
fn node_pass(tokens: &[Token], cst: &Cst, consumed: &mut [bool], out: &mut Vec<Observation>) {
    // A `-e` point that actually ended a container is that container's LAST
    // child; anything else is an orphan. Marked here so the verdict rules
    // below can stay a single pass.
    let mut ended_a_container = vec![false; cst.nodes.len()];

    for (id, node) in cst.nodes.iter().enumerate().skip(1) {
        if node.close_reason() == CloseReason::Explicit
            && let Some(&last) = last_child(cst, node)
        {
            if last & NODE_ID_BIT == 0 {
                consumed[last as usize] = true;
            } else if shape_of(tokens, cst, id) == Shape::Container {
                ended_a_container[(last & !NODE_ID_BIT) as usize] = true;
            }
        }
    }

    for (id, node) in cst.nodes.iter().enumerate().skip(1) {
        let anchor = node.token;
        let marker_idx = tokens[anchor as usize].marker_idx;
        let shape = shape_of(tokens, cst, id);

        if shape == Shape::Point
            && tokens[anchor as usize].kind() == (TokenKind::Milestone { end: true })
            && is_container_row(marker_idx)
            && !ended_a_container[id]
        {
            out.push(Observation::one(Code::OrphanContainerEnd, anchor));
        }

        match node.close_reason() {
            // The walker judged these normal. Note peers and displaced
            // paragraphs both land here, and both are silent by ruling.
            CloseReason::Explicit | CloseReason::Implicit => {}
            CloseReason::Recovery => match shape {
                Shape::Container => out.push(Observation::one(Code::UnterminatedContainer, anchor)),
                Shape::Point => out.push(Observation::one(Code::UnterminatedMilestone, anchor)),
                Shape::Plain => match generated::kind(marker_idx) {
                    MarkerKind::Note => out.push(Observation::one(Code::UnclosedNote, anchor)),
                    MarkerKind::Character => out.push(Observation::one(Code::UnclosedChar, anchor)),
                    // Defensive: only RequiredExplicit/SelfClosingMilestone
                    // rows are ever stamped Recovery, and on a Plain node that
                    // means a note or a character marker. A row that grows a
                    // third closer-wanting kind lands here rather than
                    // panicking on real data.
                    _ => out.push(Observation::one(Code::UnclosedAtEof, anchor)),
                },
            },
            CloseReason::Eof => match shape {
                Shape::Point => out.push(Observation::one(Code::UnterminatedMilestone, anchor)),
                _ if wants_closer(marker_idx) => {
                    out.push(Observation::one(Code::UnclosedAtEof, anchor))
                }
                _ => {}
            },
        }
    }
}

fn last_child<'a>(cst: &'a Cst, node: &Node) -> Option<&'a u32> {
    if node.children.is_empty() {
        return None;
    }
    cst.child_ids.get(node.children.end as usize - 1)
}

/// One sweep over the tokens: the rules that are a row lookup and nothing else.
fn token_pass(tokens: &[Token], consumed: &[bool], out: &mut Vec<Observation>) {
    for (idx, token) in tokens.iter().enumerate() {
        let idx = idx as u32;
        match token.kind() {
            TokenKind::Marker { nested } => {
                if token.marker_idx == generated::UNRESOLVED {
                    out.push(Observation::one(Code::UnknownMarker, idx));
                } else if nested && generated::kind(token.marker_idx) != MarkerKind::Character {
                    out.push(Observation::one(Code::NestedSpellingMisuse, idx));
                }
            }
            TokenKind::ClosingMarker { nested } => {
                if !consumed[idx as usize] {
                    out.push(Observation::one(Code::OrphanCloser, idx));
                }
                if nested
                    && token.marker_idx != generated::UNRESOLVED
                    && generated::kind(token.marker_idx) != MarkerKind::Character
                {
                    out.push(Observation::one(Code::NestedSpellingMisuse, idx));
                }
            }
            TokenKind::MilestoneTerminator if !consumed[idx as usize] => {
                out.push(Observation::one(Code::OrphanTerminator, idx))
            }
            _ => {}
        }
    }
}

/// One in-order walk carrying the two ancestry facts no single node knows:
/// "am I inside a sidebar" and "is there a paragraph above me".
fn tree_pass(tokens: &[Token], cst: &Cst, out: &mut Vec<Observation>) {
    struct Cursor {
        next: u32,
        end: u32,
        sidebar: bool,
        para: bool,
        /// Restored on pop, so nested sidebars name the innermost opener.
        prev_sidebar_token: u32,
    }

    let root = &cst.nodes[0];
    let mut stack = vec![Cursor {
        next: root.children.start,
        end: root.children.end,
        sidebar: false,
        para: false,
        prev_sidebar_token: ROOT_TOKEN,
    }];
    let mut sidebars = 0u32;
    let mut paragraphs = 0u32;
    let mut sidebar_token = ROOT_TOKEN;
    // One `\p` repairs a whole run of paragraph-less verses, so the run gets
    // ONE finding, anchored where that `\p` would go. Reported per verse this
    // rule cries wolf: `\c 2` with no `\p` lights up every verse in the
    // chapter (614 → 34 across en_ult), which buries the real signal.
    let mut run_reported = false;

    while let Some(cursor) = stack.last_mut() {
        if cursor.next == cursor.end {
            let done = stack.pop().expect("cursor was borrowed from the stack");
            sidebars -= u32::from(done.sidebar);
            paragraphs -= u32::from(done.para);
            if done.sidebar {
                sidebar_token = done.prev_sidebar_token;
            }
            continue;
        }
        let child = cst.child_ids[cursor.next as usize];
        cursor.next += 1;

        if child & NODE_ID_BIT != 0 {
            let node = &cst.nodes[(child & !NODE_ID_BIT) as usize];
            let marker_idx = tokens[node.token as usize].marker_idx;
            let sidebar = generated::opens_scope(marker_idx) == Some(ScopeKind::Sidebar);
            // Cells count: a verse inside a table cell is inside a paragraph
            // for every purpose this rule has.
            let para = matches!(
                generated::kind(marker_idx),
                MarkerKind::Paragraph | MarkerKind::TableCell
            );
            sidebars += u32::from(sidebar);
            paragraphs += u32::from(para);
            if para {
                run_reported = false;
            }
            let prev_sidebar_token = sidebar_token;
            if sidebar {
                sidebar_token = node.token;
            }
            stack.push(Cursor {
                next: node.children.start,
                end: node.children.end,
                sidebar,
                para,
                prev_sidebar_token,
            });
            continue;
        }

        let token = &tokens[child as usize];
        if !matches!(token.kind(), TokenKind::Marker { .. }) {
            continue;
        }
        match generated::kind(token.marker_idx) {
            MarkerKind::Chapter => {
                if sidebars > 0 {
                    out.push(Observation::pair(
                        Code::ContentOutsideSidebarRule,
                        child,
                        sidebar_token,
                    ));
                }
                // A run cannot cross `\c`: the chapter displaces any open
                // paragraph, so a `\p` inserted in the previous chapter
                // repairs nothing here — each offending chapter is its own
                // run and gets its own finding.
                run_reported = false;
            }
            MarkerKind::Verse => {
                if sidebars > 0 {
                    out.push(Observation::pair(
                        Code::ContentOutsideSidebarRule,
                        child,
                        sidebar_token,
                    ));
                }
                if paragraphs == 0 && !run_reported {
                    run_reported = true;
                    out.push(Observation::one(Code::MissingParagraph, child));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::build;
    use crate::lex;

    /// Every test states its snippet and the exact findings it expects —
    /// codes AND anchors, so a rule that fires on the right marker for the
    /// wrong reason still fails.
    fn findings(usfm: &str) -> (Vec<Token>, Vec<Observation>) {
        let tokens = lex(usfm);
        let cst = build(&tokens);
        let report = lint(usfm.as_bytes(), &tokens, &cst);
        (tokens, report.observations)
    }

    /// The index of the nth token whose marker ROW has this name (`qt` has
    /// two rows, so the row name — not a spelling lookup — is the key).
    fn token_named(tokens: &[Token], name: &str, nth: usize) -> u32 {
        tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| {
                generated::name(token.marker_idx) == name
                    && matches!(
                        token.kind(),
                        TokenKind::Marker { .. } | TokenKind::Milestone { .. }
                    )
            })
            .map(|(idx, _)| idx as u32)
            .nth(nth)
            .unwrap_or_else(|| panic!("no \\{name} token #{nth}"))
    }

    fn codes(observations: &[Observation]) -> Vec<Code> {
        observations.iter().map(|obs| obs.code).collect()
    }

    #[test]
    fn rows_are_declared_in_code_order_with_unique_kebab_names() {
        let mut seen: Vec<&str> = Vec::new();
        for (idx, row) in LINT_ROWS.iter().enumerate() {
            assert_eq!(row.code as usize, idx, "{} is out of order", row.name);
            assert!(!row.name.is_empty());
            assert!(
                row.name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-'),
                "{} is not kebab-case",
                row.name
            );
            assert!(!seen.contains(&row.name), "duplicate name {}", row.name);
            seen.push(row.name);
            assert_eq!(row.code.row().name, row.name);
            // Phase 1 ships one family; a stray row here means a later phase
            // landed without its pass.
            assert_eq!(row.category, Category::Structure);
            assert_eq!(row.aux, AuxKind::None);
        }
    }

    #[test]
    fn observation_is_four_words_and_carries_no_strings() {
        assert_eq!(core::mem::size_of::<Observation>(), 16);
    }

    #[test]
    fn a_clean_book_yields_nothing() {
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p \\v 1 In the beginning\\f + \\ft note\\f*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn the_book_code_is_reported_without_an_observation() {
        let usfm = "\\id GEN\n\\c 1\n\\p \\v 1 text\n";
        let tokens = lex(usfm);
        let cst = build(&tokens);
        let report = lint(usfm.as_bytes(), &tokens, &cst);
        let book = report.book.expect("\\id GEN has a book code");
        assert_eq!(tokens[book as usize].kind(), TokenKind::BookCode);

        // No `\id` is the None state and NOT a phase-1 finding.
        let usfm = "\\c 1\n\\p \\v 1 text\n";
        let tokens = lex(usfm);
        let cst = build(&tokens);
        let report = lint(usfm.as_bytes(), &tokens, &cst);
        assert_eq!(report.book, None);
        assert_eq!(report.observations, vec![]);
    }

    #[test]
    fn unclosed_note() {
        // `\c` is not allowed inside a footnote, so the walker stamps the note
        // Recovery — the shape of all three live corpus findings.
        let (tokens, obs) = findings("\\p \\v 1 a\\f + \\ft note\\c 2\n\\p b");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::UnclosedNote,
                token_named(&tokens, "f", 0)
            )]
        );
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
        // The `-e` point closes Explicit at its own `\*` — nothing about the
        // terminator is orphaned; the CONTAINER it claims to end never opened.
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
    fn content_outside_sidebar_rule() {
        let (tokens, obs) = findings("\\p out\\esb \\p in \\c 1 more\\esbe\\p after");
        assert_eq!(
            obs,
            vec![Observation::pair(
                Code::ContentOutsideSidebarRule,
                token_named(&tokens, "c", 0),
                token_named(&tokens, "esb", 0),
            )]
        );
    }

    #[test]
    fn unknown_marker() {
        let (tokens, obs) = findings("\\p a \\zfoo b");
        let unknown = tokens
            .iter()
            .position(|t| {
                t.marker_idx == generated::UNRESOLVED
                    && matches!(t.kind(), TokenKind::Marker { .. })
            })
            .unwrap() as u32;
        assert_eq!(obs, vec![Observation::one(Code::UnknownMarker, unknown)]);
    }

    #[test]
    fn nested_spelling_misuse() {
        // `\+f` resolves to the `f` row (shape Any), which is a NOTE — the
        // nested spelling belongs to character markers alone.
        let (tokens, obs) = findings("\\p a \\+f + \\ft n\\+f*");
        assert_eq!(
            codes(&obs),
            vec![Code::NestedSpellingMisuse, Code::NestedSpellingMisuse]
        );
        assert_eq!(obs[0].anchor, token_named(&tokens, "f", 0));

        // A real nested character pair is silent.
        let (_, obs) = findings("\\p \\add a \\+nd b\\+nd* c\\add*");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn missing_paragraph() {
        let (tokens, obs) = findings("\\c 1\n\\v 1 no paragraph here");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingParagraph,
                token_named(&tokens, "v", 0)
            )]
        );

        // Inside a paragraph, and inside a table cell, both silent.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 fine\n\\tr \\tc1 \\v 2 also fine");
        assert_eq!(obs, vec![]);

        // A run never crosses `\c` — one `\p` cannot repair both chapters,
        // so consecutive offending chapters each get their own finding.
        let (tokens, obs) = findings("\\c 1\n\\v 1 a \\v 2 b\n\\c 2\n\\v 1 c");
        assert_eq!(
            obs,
            vec![
                Observation::one(Code::MissingParagraph, token_named(&tokens, "v", 0)),
                Observation::one(Code::MissingParagraph, token_named(&tokens, "v", 2)),
            ]
        );
    }

    #[test]
    fn observations_come_back_in_document_order() {
        let (_, obs) = findings("\\p \\add a\\w* \\zfoo \\p \\nd b");
        let anchors: Vec<u32> = obs.iter().map(|o| o.anchor).collect();
        let mut sorted = anchors.clone();
        sorted.sort_unstable();
        assert_eq!(anchors, sorted);
        assert!(anchors.len() >= 3);
    }
}
