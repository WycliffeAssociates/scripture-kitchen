//! Findings over an already-built document: `lex → cst::build → lint`.
//!
//! Phases 1-2 ship the STRUCTURAL and ORDERING subsystems plus the four
//! Payload rules that need only a span and a table (adjacency, form, the
//! remaining payload and attribute rules, and the fix model, are later phases
//! — see planning/lint-sketch.md "Build phases").
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
//! the consumed-closer bitset), one sweep over `tokens` (row-keyed and
//! span-keyed token findings), one in-order tree walk carrying two depth
//! counters (sidebar and paragraph), and one more sweep over `tokens` for
//! ORDERING, which is separate because it is the only rule family carrying
//! cross-token sequence state. All linear, no recursion.
//!
//! PERF (measured 2026-08-19, `playground --lint-only`, min-of-8 on the same
//! machine in the same window, so trust the deltas over the absolutes):
//!
//! | corpus | phase 1 | phase 2 | delta |
//! |--------|---------|---------|-------|
//! | en_ult (6.57M tokens) | 7.4 ns/token | 9.9 | +2.5 |
//! | en_ulb (255k tokens)  | 6.7 ns/token | 9.1 | +2.4 |
//!
//! The whole +2.4 is `ordering_pass` and nothing else: skipping just that call
//! puts both corpora back on their phase-1 numbers to within noise (7.7 / 6.8),
//! so the BookCode arm added to the token sweep is free. Two-plus ns for a
//! fourth linear sweep is honest rather than surprising — a bare second pass
//! over token rows floors at ~1 ns/token (measured, cst.rs) and the state
//! machine costs the rest — and en_ulb, which fits in cache, pays the same, so
//! it is work and not memory traffic.
//!
//! Split by pass, phase 2: the tree walk ~3.8, ordering ~2.4, the node sweep
//! plus the two scratch vecs ~2.0, the token sweep ~1.4. The tree walk is
//! still the biggest single lead (ancestry is the one fact no node carries, so
//! it chases arena ids out of order); the cheap SECOND lead, if lint ever
//! needs to get under ~8 again, is fusing the two token sweeps into one — they
//! have identical shape and only the ruled "one family, one pass" tidiness
//! keeps them apart.

use crate::cst::{CloseReason, Cst, Node};
use crate::designator::{self, Designator};
use crate::tables::books;
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
    // --- phase 2: Ordering ------------------------------------------------
    DesignatorMalformed,
    ChapterDuplicate,
    ChapterOutOfOrder,
    ChapterGap,
    VerseDuplicate,
    VerseOutOfOrder,
    VerseGap,
    MissingVerseOne,
    VerseBeforeFirstChapter,
    MissingChapter,
    // --- phase 2: Payload -------------------------------------------------
    MissingId,
    BookCodeUnknown,
    BookCodeNotUppercase,
    ChapterWithoutDesignator,
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
/// Phases 1 and 2: the Structure family, the Ordering family, and the four
/// Payload rules that need nothing but a span and a table. Rows for the
/// adjacency, form, attribute and version families land with their passes.
pub const LINT_ROWS: [LintRow; 26] = [
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
    // ---- Ordering ---------------------------------------------------------
    // A `\c`/`\v` payload that fails its pattern (see [`crate::designator`]).
    // ALWAYS the first thing said about a designator, and the last: a
    // malformed one is excluded from the sequence entirely, so it never
    // cascades into a gap or a duplicate on the verses that follow. Flag,
    // never reinterpret — guessing that `1O` meant `10` would be synthesis.
    LintRow {
        code: Code::DesignatorMalformed,
        name: "designator-malformed",
        category: Category::Ordering,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "{anchor} is not a valid chapter/verse number",
        fix_label: None,
    },
    // The same chapter number twice in one book. `second` is the previous
    // chapter's designator token, `aux` the number the sequence expected.
    LintRow {
        code: Code::ChapterDuplicate,
        name: "chapter-duplicate",
        category: Category::Ordering,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::ExpectedNumber,
        template: "chapter {anchor} repeats the chapter at {second}; expected {aux}",
        fix_label: Some("renumber to the expected chapter"),
    },
    // A chapter number BELOW the previous one.
    LintRow {
        code: Code::ChapterOutOfOrder,
        name: "chapter-out-of-order",
        category: Category::Ordering,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::ExpectedNumber,
        template: "chapter {anchor} goes backwards from {second}; expected {aux}",
        fix_label: Some("renumber to the expected chapter"),
    },
    // A jump of more than one. Chapters, unlike verses, have no tradition of
    // legitimate holes — but the sequence is still only CONTIGUITY, never a
    // versification scheme (ruled), so this stays a warning.
    LintRow {
        code: Code::ChapterGap,
        name: "chapter-gap",
        category: Category::Ordering,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::ExpectedNumber,
        template: "chapter {anchor} skips ahead; expected {aux}",
        fix_label: None,
    },
    // A verse whose first number is exactly the previous designator's LAST
    // covered verse — `\v 12-14` then `\v 14`. One code for the plain repeat
    // and the range overlap, because they are the same fact.
    LintRow {
        code: Code::VerseDuplicate,
        name: "verse-duplicate",
        category: Category::Ordering,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::ExpectedNumber,
        template: "verse {anchor} repeats the verse at {second}; expected {aux}",
        fix_label: Some("renumber to the expected verse"),
    },
    // A verse starting BELOW the previous designator's last covered verse.
    LintRow {
        code: Code::VerseOutOfOrder,
        name: "verse-out-of-order",
        category: Category::Ordering,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::ExpectedNumber,
        template: "verse {anchor} goes backwards from {second}; expected {aux}",
        fix_label: Some("renumber to the expected verse"),
    },
    // A hole in the verse sequence. The rule the versification question lands
    // on: traditions that legitimately omit a verse produce this warning, and
    // the ruled answer for now is a per-rule off switch when a consumer asks,
    // never a scheme table inside the linter.
    LintRow {
        code: Code::VerseGap,
        name: "verse-gap",
        category: Category::Ordering,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::ExpectedNumber,
        template: "verse {anchor} skips ahead; expected {aux}",
        fix_label: None,
    },
    // A chapter whose FIRST verse is not 1. Fires INSTEAD of `verse-gap`,
    // never beside it: there is no previous verse to have skipped from, and
    // two findings for one authoring fact is noise. `aux` is always 1.
    LintRow {
        code: Code::MissingVerseOne,
        name: "missing-verse-one",
        category: Category::Ordering,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::ExpectedNumber,
        template: "this chapter starts at verse {anchor}; expected {aux}",
        fix_label: None,
    },
    // Scripture verses before the book's first `\c`. ONE finding, at the first
    // such `\v` — the whole run is one misplacement — and only when a `\c`
    // does arrive later; a book with no chapter at all is `missing-chapter`'s
    // to report, and saying both about the same token helps nobody.
    LintRow {
        code: Code::VerseBeforeFirstChapter,
        name: "verse-before-first-chapter",
        category: Category::Ordering,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} comes before the book's first \\c",
        fix_label: None,
    },
    // Verses but no `\c` anywhere. Anchored at the first `\v`, and silent for
    // a book with no verses either — front matter (FRT, GLO) is chapter-less
    // by design and must never be nagged.
    LintRow {
        code: Code::MissingChapter,
        name: "missing-chapter",
        category: Category::Ordering,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "this book has verses but no \\c",
        fix_label: None,
    },
    // ---- Payload ----------------------------------------------------------
    // No `\id` line at all (real in the wild: BSB Ecclesiastes). Anchored at
    // token 0 because the missing line belongs at the top of the file, and
    // raised only for a file with markers in it — an empty or prose-only
    // buffer is not a book and gets no opinion. `LintReport::book` still
    // reports `None`; the observation is the message, not the state.
    LintRow {
        code: Code::MissingId,
        name: "missing-id",
        category: Category::Payload,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "this book has no \\id line",
        fix_label: None,
    },
    // An `\id` payload that is not one of the spec's 116 identifiers, in any
    // casing (see [`crate::tables::books`]).
    LintRow {
        code: Code::BookCodeUnknown,
        name: "book-code-unknown",
        category: Category::Payload,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "{anchor} is not a book identifier",
        fix_label: None,
    },
    // A real identifier, written `gen` or `Gen`. Fires INSTEAD of
    // `book-code-unknown` — the code IS known, only its casing is wrong, and
    // reporting both would double-count one typo.
    LintRow {
        code: Code::BookCodeNotUppercase,
        name: "book-code-not-uppercase",
        category: Category::Payload,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "book identifier {anchor} should be uppercase",
        fix_label: Some("uppercase the book identifier"),
    },
    // A `\c` with no number after it. The scanner carves a `Designator` from
    // the first content region after `\c` and abandons the expectation at a
    // newline or a marker, so this is exactly "the `\c` line was empty".
    // Attribute lists are stepped over: `\c |x="y"| 1` still has its number.
    LintRow {
        code: Code::ChapterWithoutDesignator,
        name: "chapter-without-designator",
        category: Category::Payload,
        severity: Severity::Error,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} has no chapter number",
        fix_label: None,
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
    token_pass(source, tokens, &consumed, &mut report.observations);
    tree_pass(tokens, cst, &mut report.observations);
    ordering_pass(source, tokens, &mut report.observations);

    // A file with no `\id` at all. Raised here rather than in a pass because
    // the fact is the ABSENCE of a token, which no sweep can see, and because
    // `report.book` is the state it reads. Markers-only guard: a plain-prose
    // or empty buffer is not a book that owes an `\id`.
    if report.book.is_none()
        && tokens.iter().any(|token| {
            matches!(
                token.kind(),
                TokenKind::Marker { .. }
                    | TokenKind::ClosingMarker { .. }
                    | TokenKind::Milestone { .. }
            )
        })
    {
        report
            .observations
            .push(Observation::one(Code::MissingId, 0));
    }

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

/// One sweep over the tokens: the rules that are a row lookup, or a span
/// against an auxiliary table, and nothing else.
fn token_pass(source: &[u8], tokens: &[Token], consumed: &[bool], out: &mut Vec<Observation>) {
    for (idx, token) in tokens.iter().enumerate() {
        let idx = idx as u32;
        match token.kind() {
            // Byte-exact membership first, then the case-folded retry: "known
            // but lowercase" and "unknown" are different findings and exactly
            // one of them fires.
            TokenKind::BookCode => {
                let span = span_of(source, token);
                if books::is_book_code(span) {
                    continue;
                }
                let known_folded =
                    books::upper3(span).is_some_and(|upper| books::is_book_code(&upper));
                out.push(Observation::one(
                    if known_folded {
                        Code::BookCodeNotUppercase
                    } else {
                        Code::BookCodeUnknown
                    },
                    idx,
                ));
            }
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

/// A token's bytes. The only place lint reads `source`, and it reads it only
/// through spans the scanner already carved.
fn span_of<'a>(source: &'a [u8], token: &Token) -> &'a [u8] {
    &source[token.start as usize..token.end() as usize]
}

/// The Ordering subsystem: one linear sweep over the tokens, ignoring the CST
/// entirely (NEXT-STEPS: "tokens only").
///
/// It is its own pass because it is the only part of lint that carries
/// CROSS-TOKEN state — the previous chapter, the previous verse, whether a
/// `\c` has been seen at all — and threading that through the row-lookup
/// sweep would entangle two unrelated shapes of rule.
///
/// Two adjacency facts it depends on, both read off the scanner (scanner.rs
/// `text_arm`/`newline_arm`): a `\c`/`\v` marker and its `Designator` are
/// SEPARATE, adjacent tokens; and the only thing that can sit between them is
/// an attribute list (`\v |script="Arab"| 1`), because a newline or any other
/// marker abandons the payload expectation.
///
/// Sequence policy, in one place:
///
/// - Malformed designators are FLAGGED and then excluded — they update no
///   state, so a single typo never cascades into a gap plus a duplicate.
/// - Exactly ONE code per anomaly: equal → duplicate, smaller → out of order,
///   larger by more than one → gap.
/// - Verse state resets at every `\c`; chapter state runs for the whole book.
/// - Ranges count as their span: after `\v 12-14` the sequence expects 15.
fn ordering_pass(source: &[u8], tokens: &[Token], out: &mut Vec<Observation>) {
    /// Which marker is still owed its `Designator` token.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Awaiting {
        None,
        /// The `\c` marker's token index — the anchor if no number arrives.
        Chapter(u32),
        Verse,
    }

    let mut awaiting = Awaiting::None;
    // (number, the designator token that carried it) — `second` on a finding.
    let mut prev_chapter: Option<(u32, u32)> = None;
    let mut prev_verse: Option<(u32, u32)> = None;
    // True until this chapter's first verse has been read (well-formed or
    // not): the window in which `missing-verse-one` can fire. Starts FALSE —
    // the rule is about a CHAPTER's first verse, so a verse ahead of any `\c`
    // is verse-before-first-chapter's story alone, never also this one's.
    let mut first_verse_slot = false;
    let mut seen_chapter = false;
    let mut first_verse_token: Option<u32> = None;
    let mut first_pre_chapter_verse: Option<u32> = None;

    for (idx, token) in tokens.iter().enumerate() {
        let idx = idx as u32;
        match token.kind() {
            // An attribute list does not end the payload expectation, and it
            // is not the payload either — step over it.
            TokenKind::AttrList => {}
            TokenKind::Designator => match awaiting {
                Awaiting::Chapter(_) => {
                    awaiting = Awaiting::None;
                    match designator::chapter(span_of(source, token)) {
                        Designator::Malformed => {
                            out.push(Observation::one(Code::DesignatorMalformed, idx));
                            // RESYNC, not just skip: the number is unknown, so
                            // the NEXT chapter has nothing legitimate to be
                            // compared against either. Dropping the state is
                            // what keeps one bad number to one finding.
                            prev_chapter = None;
                        }
                        Designator::Wellformed { first: number, .. } => {
                            if let Some((previous, previous_token)) = prev_chapter {
                                let expected = previous.saturating_add(1);
                                let code = if number == previous {
                                    Some(Code::ChapterDuplicate)
                                } else if number < previous {
                                    Some(Code::ChapterOutOfOrder)
                                } else if number > expected {
                                    Some(Code::ChapterGap)
                                } else {
                                    None
                                };
                                if let Some(code) = code {
                                    out.push(Observation {
                                        code,
                                        anchor: idx,
                                        second: previous_token,
                                        aux: expected,
                                    });
                                }
                            }
                            prev_chapter = Some((number, idx));
                        }
                    }
                }
                Awaiting::Verse => {
                    awaiting = Awaiting::None;
                    match designator::verse(span_of(source, token)) {
                        Designator::Malformed => {
                            out.push(Observation::one(Code::DesignatorMalformed, idx));
                            // RESYNC. Both slots are dropped, so the verse
                            // AFTER the bad one is compared against nothing
                            // and the sequence restarts from it. Skipping only
                            // the malformed token itself is not enough: real
                            // data proves it. en_ulb ZEC 12:7 is written
                            // `\v 7"` (no space before the quote), so the
                            // designator is `7"`; leaving `prev_verse` at 6
                            // then made the perfectly good `\v 8` look like a
                            // gap — one typo, two findings, the second of them
                            // a lie.
                            prev_verse = None;
                            first_verse_slot = false;
                        }
                        Designator::Wellformed { first, last } => {
                            if let Some((previous_last, previous_token)) = prev_verse {
                                let expected = previous_last.saturating_add(1);
                                let code = if first == previous_last {
                                    Some(Code::VerseDuplicate)
                                } else if first < previous_last {
                                    Some(Code::VerseOutOfOrder)
                                } else if first > expected {
                                    Some(Code::VerseGap)
                                } else {
                                    None
                                };
                                if let Some(code) = code {
                                    out.push(Observation {
                                        code,
                                        anchor: idx,
                                        second: previous_token,
                                        aux: expected,
                                    });
                                }
                            } else if first_verse_slot && first != 1 {
                                out.push(Observation {
                                    code: Code::MissingVerseOne,
                                    anchor: idx,
                                    second: NO_TOKEN,
                                    aux: 1,
                                });
                            }
                            first_verse_slot = false;
                            prev_verse = Some((last, idx));
                        }
                    }
                }
                Awaiting::None => {}
            },
            TokenKind::Marker { .. } => {
                if let Awaiting::Chapter(marker) = awaiting {
                    out.push(Observation::one(Code::ChapterWithoutDesignator, marker));
                }
                awaiting = match generated::kind(token.marker_idx) {
                    MarkerKind::Chapter => {
                        seen_chapter = true;
                        prev_verse = None;
                        first_verse_slot = true;
                        Awaiting::Chapter(idx)
                    }
                    MarkerKind::Verse => {
                        first_verse_token.get_or_insert(idx);
                        if !seen_chapter {
                            first_pre_chapter_verse.get_or_insert(idx);
                        }
                        Awaiting::Verse
                    }
                    _ => Awaiting::None,
                };
            }
            // Text, a newline, a closer, anything else: the payload window is
            // over, exactly as the scanner's is. Guarded rather than
            // unconditional because this is the arm nearly every token in the
            // corpus takes, and a perfectly-predicted branch beats a store.
            _ if awaiting != Awaiting::None => {
                if let Awaiting::Chapter(marker) = awaiting {
                    out.push(Observation::one(Code::ChapterWithoutDesignator, marker));
                }
                awaiting = Awaiting::None;
            }
            _ => {}
        }
    }
    // A `\c` as the very last token of the file.
    if let Awaiting::Chapter(marker) = awaiting {
        out.push(Observation::one(Code::ChapterWithoutDesignator, marker));
    }

    match (seen_chapter, first_verse_token, first_pre_chapter_verse) {
        // Verses and no chapter anywhere: one finding at the first verse.
        // (A book with neither — front matter, a glossary — says nothing.)
        (false, Some(verse), _) => out.push(Observation::one(Code::MissingChapter, verse)),
        // Verses ahead of the first `\c`: one finding for the whole run.
        (true, _, Some(verse)) => out.push(Observation::one(Code::VerseBeforeFirstChapter, verse)),
        _ => {}
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
    ///
    /// A snippet that does not open with `\id` gets one: a file with markers
    /// and no `\id` line is itself a finding (`missing-id`), and a structural
    /// test should not have to restate that in every expectation. Anchors are
    /// always resolved by name or by kind, never by a literal index, so the
    /// prefix never leaks into an assertion.
    fn findings(usfm: &str) -> (Vec<Token>, Vec<Observation>) {
        let usfm = if usfm.starts_with("\\id") {
            usfm.to_string()
        } else {
            format!("\\id GEN\n{usfm}")
        };
        let tokens = lex(&usfm);
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
            // Phases 1-2 ship three families; a stray row from a family whose
            // pass has not landed means a phase arrived half-built.
            assert!(
                matches!(
                    row.category,
                    Category::Structure | Category::Ordering | Category::Payload
                ),
                "{} belongs to a family with no pass yet",
                row.name
            );
            // The only `aux` meaning in use so far is the expected number, and
            // only the sequence rules carry one.
            assert!(
                matches!(row.aux, AuxKind::None | AuxKind::ExpectedNumber),
                "{} uses an aux kind nothing writes yet",
                row.name
            );
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

        // No `\id`: the `None` state is KEPT (it is the fact the consumer
        // reads) and `missing-id` is raised alongside it, anchored at token 0.
        let usfm = "\\c 1\n\\p \\v 1 text\n";
        let tokens = lex(usfm);
        let cst = build(&tokens);
        let report = lint(usfm.as_bytes(), &tokens, &cst);
        assert_eq!(report.book, None);
        assert_eq!(
            report.observations,
            vec![Observation::one(Code::MissingId, 0)]
        );
    }

    #[test]
    fn unclosed_note() {
        // `\c` is not allowed inside a footnote, so the walker stamps the note
        // Recovery — the shape of all three live corpus findings.
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

    // -----------------------------------------------------------------
    // Phase 2: ordering
    // -----------------------------------------------------------------

    /// The index of the nth `Designator` token — the anchor every sequence
    /// rule uses (the marker and its number are separate tokens).
    fn designator_at(tokens: &[Token], nth: usize) -> u32 {
        tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| token.kind() == TokenKind::Designator)
            .map(|(idx, _)| idx as u32)
            .nth(nth)
            .unwrap_or_else(|| panic!("no designator #{nth}"))
    }

    #[test]
    fn a_clean_ordered_book_yields_nothing() {
        let (_, obs) = findings(
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b \\v 3-4 c \\v 5 d\n\\c 2\n\\p \\v 1 e\n",
        );
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn designator_malformed_flags_once_and_resyncs() {
        // The en_ulb ZEC 12:7 shape: `\v 2"` glues the quote to the number,
        // so the carved span is `2"`. ONE finding — the perfectly good `\v 3`
        // that follows must NOT be reported as a gap.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\v 2\" b\n\\v 3 c\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DesignatorMalformed,
                designator_at(&tokens, 2)
            )]
        );

        // A chapter designator is judged by the CHAPTER pattern: no segments.
        let (tokens, obs) = findings("\\c 12b\n\\p \\v 1 a\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DesignatorMalformed,
                designator_at(&tokens, 0)
            )]
        );
    }

    #[test]
    fn chapter_sequence_reports_one_code_per_anomaly() {
        let cases = [
            ("\\c 1\n\\c 1\n", Code::ChapterDuplicate, 2),
            ("\\c 2\n\\c 1\n", Code::ChapterOutOfOrder, 3),
            ("\\c 1\n\\c 4\n", Code::ChapterGap, 2),
        ];
        for (usfm, code, expected) in cases {
            let (tokens, obs) = findings(usfm);
            assert_eq!(
                obs,
                vec![Observation {
                    code,
                    anchor: designator_at(&tokens, 1),
                    second: designator_at(&tokens, 0),
                    aux: expected,
                }],
                "{usfm:?}"
            );
        }
    }

    #[test]
    fn verse_sequence_reports_one_code_per_anomaly() {
        let cases = [
            ("\\c 1\n\\p \\v 1 a \\v 1 b", Code::VerseDuplicate, 2),
            (
                "\\c 1\n\\p \\v 1 a \\v 3 b \\v 2 c",
                Code::VerseOutOfOrder,
                4,
            ),
            ("\\c 1\n\\p \\v 1 a \\v 3 b", Code::VerseGap, 2),
        ];
        for (usfm, code, expected) in cases {
            let (tokens, obs) = findings(usfm);
            let last = obs.last().copied().unwrap_or_else(|| panic!("{usfm:?}"));
            assert_eq!(last.code, code, "{usfm:?}");
            assert_eq!(last.aux, expected, "{usfm:?}");
            // `second` always names the designator this one was compared to.
            let count = tokens
                .iter()
                .filter(|t| t.kind() == TokenKind::Designator)
                .count();
            assert_eq!(last.anchor, designator_at(&tokens, count - 1));
            assert_eq!(last.second, designator_at(&tokens, count - 2));
        }
    }

    #[test]
    fn ranges_count_as_their_whole_span() {
        // 12-14 covers 15's predecessor, so `\v 15` is contiguous.
        let (_, obs) = findings("\\c 1\n\\p \\v 1-11 a \\v 12-14 b \\v 15 c");
        assert_eq!(obs, vec![]);

        // Landing ON the range's last verse is a DUPLICATE…
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1-11 a \\v 12-14 b \\v 14 c");
        assert_eq!(codes(&obs), vec![Code::VerseDuplicate]);
        assert_eq!(obs[0].anchor, designator_at(&tokens, 3));
        assert_eq!(obs[0].aux, 15);

        // …and landing INSIDE it is out of order.
        let (_, obs) = findings("\\c 1\n\\p \\v 1-11 a \\v 12-14 b \\v 13 c");
        assert_eq!(codes(&obs), vec![Code::VerseOutOfOrder]);

        // A list covers its endpoints the same way.
        let (_, obs) = findings("\\c 1\n\\p \\v 1,3 a \\v 4 b");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn segments_and_rtl_marks_are_ordinary_designators() {
        // Two segments of one verse are the SAME verse, not a duplicate: the
        // suffix takes no part in the comparison.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a \\v 2a b \\v 3 c");
        assert_eq!(obs, vec![]);

        // U+200F before the separator, as RTL scripts write it.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a \\v 2\u{200F}-3 b \\v 4 c");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn missing_verse_one_fires_instead_of_verse_gap() {
        let (tokens, obs) = findings("\\c 1\n\\p \\v 2 a \\v 3 b");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::MissingVerseOne,
                anchor: designator_at(&tokens, 1),
                second: NO_TOKEN,
                aux: 1,
            }]
        );

        // Starting well above 1 is still the SAME single finding — never a
        // gap as well.
        let (_, obs) = findings("\\c 1\n\\p \\v 7 a");
        assert_eq!(codes(&obs), vec![Code::MissingVerseOne]);
    }

    #[test]
    fn a_chapter_resets_the_verse_sequence() {
        // Chapter 2 starting again at 1 is not a duplicate or a step back.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a \\v 2 b\n\\c 2\n\\p \\v 1 c \\v 2 d");
        assert_eq!(obs, vec![]);

        // …and the missing-verse-one window reopens with it.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\c 2\n\\p \\v 4 b");
        assert_eq!(codes(&obs), vec![Code::MissingVerseOne]);
        assert_eq!(obs[0].anchor, designator_at(&tokens, 3));
    }

    #[test]
    fn verse_before_first_chapter_fires_once_and_only_with_a_chapter() {
        // ONE finding for the whole run, at its first verse.
        let (tokens, obs) = findings("\\p \\v 1 a \\v 2 b\n\\c 1\n\\p \\v 1 c");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::VerseBeforeFirstChapter,
                token_named(&tokens, "v", 0)
            )]
        );

        // With no `\c` at all it is `missing-chapter`'s story instead — the
        // two never both describe the same token.
        let (tokens, obs) = findings("\\p \\v 1 a \\v 2 b");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingChapter,
                token_named(&tokens, "v", 0)
            )]
        );

        // A pre-chapter verse numbered other than 1 is still ONLY this rule's
        // story — `missing-verse-one` is about a CHAPTER's first verse and
        // stays quiet until a `\c` exists.
        let (tokens, obs) = findings("\\p \\v 5 a\n\\c 1\n\\p \\v 1 c");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::VerseBeforeFirstChapter,
                token_named(&tokens, "v", 0)
            )]
        );
    }

    #[test]
    fn a_book_with_no_verses_is_never_asked_for_a_chapter() {
        // Front matter: no `\c`, no `\v`, and nothing to say.
        let (_, obs) = findings("\\id FRT\n\\mt1 Front Matter\n\\p Some introduction.\n");
        assert_eq!(obs, vec![]);
    }

    // -----------------------------------------------------------------
    // Phase 2: payload
    // -----------------------------------------------------------------

    #[test]
    fn missing_id_is_anchored_at_the_top_of_the_file() {
        let (_, obs) = findings("\\id GEN\n\\c 1\n\\p \\v 1 a");
        assert_eq!(obs, vec![]);

        let usfm = "\\c 1\n\\p \\v 1 a";
        let tokens = lex(usfm);
        let cst = build(&tokens);
        let report = lint(usfm.as_bytes(), &tokens, &cst);
        assert_eq!(
            report.observations,
            vec![Observation::one(Code::MissingId, 0)]
        );
        // The state is kept as well as reported.
        assert_eq!(report.book, None);

        // A file with no markers at all is not a book and owes no `\id`.
        let usfm = "just prose\n";
        let tokens = lex(usfm);
        let cst = build(&tokens);
        assert_eq!(lint(usfm.as_bytes(), &tokens, &cst).observations, vec![]);
    }

    #[test]
    fn book_codes_are_checked_against_the_books_table() {
        let book_code = |tokens: &[Token]| {
            tokens
                .iter()
                .position(|t| t.kind() == TokenKind::BookCode)
                .unwrap() as u32
        };

        // Known, uppercase, with a description after it: silent.
        let (_, obs) = findings("\\id 1JN Some description\n\\c 1\n\\p \\v 1 a");
        assert_eq!(obs, vec![]);
        // Peripherals count as book identifiers too.
        let (_, obs) = findings("\\id XXA\n\\p a");
        assert_eq!(obs, vec![]);

        // Known but mis-cased: exactly ONE finding, and not `unknown`.
        let (tokens, obs) = findings("\\id gen\n\\c 1\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::BookCodeNotUppercase,
                book_code(&tokens)
            )]
        );
        let (_, obs) = findings("\\id Gen\n\\c 1\n\\p \\v 1 a");
        assert_eq!(codes(&obs), vec![Code::BookCodeNotUppercase]);

        // Not a code in any casing.
        let (tokens, obs) = findings("\\id ZZZ\n\\c 1\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(Code::BookCodeUnknown, book_code(&tokens))]
        );
        // `\id GENESIS` carves `GENESIS` as the code (one span up to the
        // first space), which is simply not an identifier.
        let (_, obs) = findings("\\id GENESIS\n\\c 1\n\\p \\v 1 a");
        assert_eq!(codes(&obs), vec![Code::BookCodeUnknown]);
    }

    #[test]
    fn chapter_without_designator() {
        // The scanner abandons the payload expectation at a newline, so this
        // `\c` really does own no number.
        let (tokens, obs) = findings("\\c\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::ChapterWithoutDesignator,
                token_named(&tokens, "c", 0)
            )]
        );

        // `\c` as the very last token of the file.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\c");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::ChapterWithoutDesignator,
                token_named(&tokens, "c", 1)
            )]
        );

        // An attribute list is stepped over, not mistaken for the payload.
        let (_, obs) = findings("\\c 1\n\\p \\v |script=\"Arab\"| 1 a");
        assert_eq!(obs, vec![]);
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
