//! Findings over an already-built document: `lex → cst::build → lint`.
//!
//! Phases 1-3 ship the STRUCTURAL, ORDERING, PAYLOAD, FORM and ADJACENCY
//! families plus the SHAPE-ONLY half of Attributes. Still owed, each waiting on
//! a piece that does not exist yet: the two attribute rules that need a k/v
//! interpreter, the Version family, and the whole fix model (phase 4) — see
//! planning/lint-sketch.md "Build phases".
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
//! Shape of the pass, and it is still FOUR sweeps after phase 3: a short
//! bounded prologue over the header ([`header_scan`]), one sweep over
//! `cst.nodes` (close verdicts, the consumed-closer bitset, empty paragraphs),
//! one sweep over `tokens` — [`token_pass`], which is now THE token walk and
//! carries three pieces of one-token lookbehind — one in-order tree walk
//! carrying two depth counters (sidebar and paragraph), and one more sweep over
//! `tokens` for ORDERING, separate because its state is a SEQUENCE and not the
//! previous token. All linear, no recursion.
//!
//! PERF (measured 2026-08-19, `playground --lint-only`, min-of-8 on the same
//! machine in the same window, so trust the deltas over the absolutes; en_ulb
//! is small enough that its numbers need `--iters 30` to settle):
//!
//! | corpus | phase 1 | phase 2 | phase 3 |
//! |--------|---------|---------|---------|
//! | en_ult (6.57M tokens) | 7.4 ns/token | 9.4 | 12.4 |
//! | en_ulb (255k tokens)  | 6.7 ns/token | 8.8 | 12.5 |
//!
//! (The phase-2 column is re-measured here — it read 9.9 / 9.1 in its own
//! window. Same code, different afternoon.)
//!
//! Phase 2's +2.4 was `ordering_pass` and nothing else. Phase 3's +3.0 / +3.7
//! is the token walk growing, and the number that decided its shape: giving
//! adjacency and the attribute rules a FIFTH pass of their own cost **+3.6
//! ns/token on en_ult by itself**, and disabling each of its arms' bodies in
//! turn moved that by only 0.4-0.9 — the cost was the sweep, not the work. A
//! token sweep over this corpus is ~2.5 ns before any rule runs (the kind
//! dispatch and its branch mispredictions), which is why the sketch's "one
//! token walk" is the right architecture and not merely a tidy one. Fusing it
//! into [`token_pass`] gave the whole family back for ~1 ns.
//!
//! What the remaining phase-3 growth buys, in rough order of cost: the two Form
//! rules read the source byte on either side of every opening marker (the first
//! rule in lint to touch `source` per token rather than per finding), the
//! attribute machine carries four locals across the walk, and `numbering-mix`
//! zeroes two 153-entry arrays per document.
//!
//! Split by pass, phase 3: the token walk ~4.5, the tree walk ~3.8, ordering
//! ~2.4, the node sweep plus the two scratch vecs ~2.0. The tree walk is now
//! the biggest single lead (ancestry is the one fact no node carries, so it
//! chases arena ids out of order); the second is fusing ordering into the token
//! walk, which the phase-3 measurement says is worth ~1.5 and which only the
//! "sequence state is not lookbehind" reading keeps apart.

use crate::cst::{CloseReason, Cst, Node};
use crate::designator::{self, Designator};
use crate::tables::books;
use crate::tables::generated;
use crate::tables::schema::{
    Category as MarkerCategory, ClosingBehavior, MarkerKind, Numbering, ScopeKind, SpellingShape,
    StructuralWhitespaceRequirement as Ws,
};
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
    /// The version the `\usfm` line declares, `None` when there is no such
    /// line (the corpus majority) or its payload is not a version.
    ///
    /// Phase 3 gives the [`LintRow::escalation`] column the fact it has always
    /// needed: a consumer maps a finding's severity through that column using
    /// THIS value, and one rule (`attr-trailing-form-deprecated`) uses it as a
    /// gate rather than a dial, because "deprecated" is a claim about a
    /// declared version and is false without one.
    pub declared_version: Option<UsfmVersion>,
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
    // --- phase 3: adjacency (filed Structure — see the rows) --------------
    CaCpPlacement,
    VaVpPlacement,
    // --- phase 3: Payload -------------------------------------------------
    CallerShape,
    NumberingMix,
    // --- phase 3: Form ----------------------------------------------------
    MarkerNotWsPreceded,
    DelimiterShape,
    EmptyParagraph,
    // --- phase 3: Attributes (shape only — no k/v interpreter) ------------
    AttrTrailingFormDeprecated,
    AttrBothLists,
    AttrTerminatorMismatch,
    AttrPipeHint,
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
/// Phases 1-3: Structure, Ordering, Payload, Form, and the SHAPE-ONLY half of
/// Attributes. Still absent, each waiting on a piece that does not exist yet:
/// `attr-unknown-name` and `attr-required-if` want the k/v attribute
/// interpreter, and the whole Version family wants a consumer for the
/// `deprecated` column.
pub const LINT_ROWS: [LintRow; 37] = [
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
    // ---- Adjacency --------------------------------------------------------
    // `\ca`/`\cp` may only follow `\c`'s designator, or the other of the pair.
    // Filed under STRUCTURE rather than Payload: the fact reported is a
    // marker sitting where it may not sit, which is exactly what
    // `content-outside-sidebar-rule` reports and nothing like a malformed
    // span. The sketch lists these separately only because they need a token
    // of LOOKBEHIND — the CST cannot see them, since `ca`/`cp` open no scope
    // — and "which pass computes it" is not a category.
    //
    // Whitespace between the parties is fine, newlines included: `\c 1\n\ca
    // 2\ca*\n\cp א` is the shape the spec's own examples use, and the scanner
    // emits a Newline token between them, so the rule steps over Newlines and
    // whitespace-only Text and nothing else.
    LintRow {
        code: Code::CaCpPlacement,
        name: "ca-cp-placement",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} must follow the \\c it re-numbers",
        fix_label: None,
    },
    // The same rule for `\va`/`\vp` after `\v`.
    LintRow {
        code: Code::VaVpPlacement,
        name: "va-vp-placement",
        category: Category::Structure,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} must follow the \\v it re-numbers",
        fix_label: None,
    },
    // ---- Payload ----------------------------------------------------------
    // A note caller that is none of the three conventional values (`+`, `-`,
    // `?`) AND longer than three bytes. The spec pattern is `/[^\\\s]+/`, so a
    // custom caller is perfectly legal and this can only ever be a HINT — the
    // rule exists for the ACCIDENT (`\f + \ft` written `\f +\ft`, a quotation
    // mark glued to the marker), not for the deliberate custom caller.
    //
    // Three bytes is the width that keeps it honest: every real custom caller
    // seen in the wild is a mark or a short symbol, and the corpus's one
    // non-`+` caller (`",` — examples.bsb, a stray quote) is two bytes and is
    // deliberately NOT reported. Widening this rule to "not one of the three"
    // would report that and every legitimate custom caller with it.
    LintRow {
        code: Code::CallerShape,
        name: "caller-shape",
        category: Category::Payload,
        severity: Severity::Hint,
        escalation: None,
        aux: AuxKind::None,
        template: "{anchor} is an unusual note caller",
        fix_label: None,
    },
    // One book spelling a numbered family both ways — `\q` and `\q2`. The
    // spec's own rule is that the bare form is a valid spelling ALWAYS and
    // should be used when the text has a single level, so this is a
    // consistency observation and never an error. ONE finding per family per
    // book, anchored at the token that revealed the mix; `second` is the
    // family's first occurrence, `aux` the row's numbering cap (`liv`, the one
    // uncapped family, reports 0). The LEVELS are not in `aux` because they
    // are not integers a message needs: both spellings are spans, reachable
    // through `anchor` and `second`, which is the message-params rule.
    //
    // NOTE for the reader looking for `numbering-out-of-range`: it is not
    // here, because it cannot fire. `generated::marker_idx` validates the
    // level against the row's cap DURING resolution (`digits_ok`), so `\q7`
    // never reaches the `q` row at all — it is row 0, and `unknown-marker`
    // has already said everything there is to say about it. A rule for it
    // would be dead code; see the report in git history.
    LintRow {
        code: Code::NumberingMix,
        name: "numbering-mix",
        category: Category::Payload,
        severity: Severity::Info,
        escalation: None,
        aux: AuxKind::NumberingCap,
        template: "\\{anchor} mixes numbered and bare spellings with \\{second} (levels 1-{aux})",
        fix_label: None,
    },
    // ---- Form -------------------------------------------------------------
    // The byte before a marker's backslash is neither whitespace nor the start
    // of the file — `content\s1`. NARROWED to PARAGRAPH rows, and the
    // narrowing is not a nicety: character markers legitimately hug, and
    // aligned USFM is built out of hugging (`\zaln-s |…\*\w In|…\w*\zaln-e\*`
    // — 6.5M tokens of it in en_ult), so an un-narrowed rule reports ~1.5M
    // findings on a corpus with nothing wrong with it. What the spec actually
    // states is the PARA railroad's requirement of a newline (or at least
    // whitespace) before a paragraph marker, and that is what this reports.
    LintRow {
        code: Code::MarkerNotWsPreceded,
        name: "marker-not-ws-preceded",
        category: Category::Form,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} needs whitespace before it",
        fix_label: None,
    },
    // The delimiter after a marker NAME is written as something that is not
    // structural whitespace — the NBSP-after-name case, and every other
    // exotic separator with it (`\p\u{00A0}text`, `\v\u{2007}1`).
    //
    // What this HONESTLY covers, and why it is not more: after the lex, the
    // delimiter is not a token — it is folded into the marker's own span, so
    // "how was it written" survives only as the marker's LENGTH and the byte
    // that follows it. Two derivations exist there, and this rule ships the
    // one that is unambiguous: the row requires a delimiter, the span absorbed
    // none (`ws_run_end` folds space/tab only), and the next byte is not
    // whitespace, not a marker or pipe (`TAGEND`'s other alternatives), and
    // not the end of the file. The other derivation — "the delimiter is
    // several spaces where one would do" — is deliberately left out: extra
    // horizontal whitespace is legal in every row's pattern (`HS` is `+`, not
    // `?`), so it is a FORMATTER's business, and phase 4's formatter bundle is
    // where it belongs.
    LintRow {
        code: Code::DelimiterShape,
        name: "delimiter-shape",
        category: Category::Form,
        severity: Severity::Hint,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} is not followed by structural whitespace",
        fix_label: None,
    },
    // A paragraph node with no content at all — nothing but Newlines under it,
    // or nothing whatever. `\b` is EXCLUDED, because a blank line is a
    // paragraph that is empty BY DESIGN; it is identified by the one column
    // that says so, `ws_after_name: SingleNewline` (it is the table's only
    // such row, and it is that value precisely because `\b` takes no content).
    // `\pb` never reaches here at all — it is a Character row that opens no
    // scope, so it is never a node.
    LintRow {
        code: Code::EmptyParagraph,
        name: "empty-paragraph",
        category: Category::Form,
        severity: Severity::Info,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} has no content",
        fix_label: None,
    },
    // ---- Attributes (shape only) ------------------------------------------
    // The 3.1 TRAILING attribute form — `\w grace|lemma="x"\w*` — which 3.2
    // deprecates and 4 removes.
    //
    // Two gates, both load-bearing:
    //
    // - CHARACTER rows only. `\zaln-s |x-strong="G1"\*` is a milestone's
    //   NORMAL syntax and never deprecated; `\fig |src="a.png"…\fig*` is how
    //   the spec's own figure examples are written. Flagging either lights up
    //   every alignment corpus for nothing.
    // - The document must DECLARE `\usfm 3.2` or later. A rule that says "this
    //   form is deprecated" to a file that declares 3.0 is simply wrong: the
    //   trailing form is the correct spelling of the version in force. en_ult
    //   declares 3.0 and contains 792,414 trailing lists — every one of them
    //   right, and every one of them a false positive without this gate. The
    //   `escalation` column then does the rest: Warning at 3.2, Error at 4.
    LintRow {
        code: Code::AttrTrailingFormDeprecated,
        name: "attr-trailing-form-deprecated",
        category: Category::Attributes,
        severity: Severity::Warning,
        escalation: Some((UsfmVersion::V4_0, Severity::Error)),
        aux: AuxKind::Version,
        template: "the trailing attribute form is deprecated in USFM {aux}",
        fix_label: None,
    },
    // Two attribute lists on one marker — the proposal's own "ridiculous but
    // legal" `\w |Fred|Jésus|Jesus\w*`. Legal back-compat, so a warning and
    // not an error; which one wins is the interpreter's merge rule ("later
    // definition wins"), which is exactly why saying it twice is worth
    // reporting. `second` is the first list.
    LintRow {
        code: Code::AttrBothLists,
        name: "attr-both-lists",
        category: Category::Attributes,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "a second attribute list overrides the one at {second}",
        fix_label: None,
    },
    // A trailing-form list ended by the WRONG terminator: `\w a|k="v"\add*`,
    // or a milestone list closed by a named closer instead of `\*`. This is
    // the finding scanner.rs promises when it says the terminator is checked
    // "only for BEING a closer, never for matching the open frame" — the list
    // is well-formed, its owner's closer is not the one that arrived.
    //
    // Node-initial lists are never reported: their terminator is their own
    // closing pipe, which the scanner found or the bytes would not be a list.
    // That is also why "the list was never closed" has no code — a list that
    // runs to a newline is not lexed as a list at all, so its pipe survives as
    // Text and `attr-pipe-hint` below is what speaks.
    LintRow {
        code: Code::AttrTerminatorMismatch,
        name: "attr-terminator-mismatch",
        category: Category::Attributes,
        severity: Severity::Warning,
        escalation: None,
        aux: AuxKind::None,
        template: "the attribute list on \\{second} is closed by the wrong marker",
        fix_label: None,
    },
    // A raw `|` left in the content of a marker that DEFINES attributes: the
    // author meant an attribute list and the list was refuted (it ran past the
    // end of its line, or its closer never came), so the bytes stayed content.
    // Hint, and gated on `defined_attributes` being non-empty, because a pipe
    // in ordinary prose is ordinary prose.
    LintRow {
        code: Code::AttrPipeHint,
        name: "attr-pipe-hint",
        category: Category::Attributes,
        severity: Severity::Hint,
        escalation: None,
        aux: AuxKind::None,
        template: "did you mean an attribute list on \\{second}?",
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
/// `source` is read through spans the scanner already carved — plus, since
/// phase 3, the single byte on either side of an opening marker's span, which
/// is where the Form family's whole evidence lives. The structural rules still
/// ask only the CST and the marker table.
pub fn lint(source: &[u8], tokens: &[Token], cst: &Cst) -> LintReport {
    let (book, declared_version) = header_scan(source, tokens);
    let mut report = LintReport {
        book,
        declared_version,
        observations: Vec::new(),
    };

    // `consumed[t]` = token t is a closer that actually closed a frame. Built
    // from the nodes rather than re-walked: a closer that closed something is
    // by construction the LAST child of an Explicit node.
    let mut consumed = vec![false; tokens.len()];
    node_pass(tokens, cst, &mut consumed, &mut report.observations);
    token_pass(
        source,
        tokens,
        &consumed,
        declared_version,
        &mut report.observations,
    );
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

/// The two header facts every later pass wants: the `\id` line's BookCode
/// token, and the version the `\usfm` line declares.
///
/// BOUNDED AT THE FIRST `\c`, and that bound is the point: both markers live in
/// the book header, so sweeping 6.5M tokens for a `\usfm` line that three of
/// the four corpora do not have would cost more than every rule that reads it.
/// Nothing after the first chapter can be an `\id` or a `\usfm` line — and a
/// file that puts one there has a structural finding already, not a header.
fn header_scan(source: &[u8], tokens: &[Token]) -> (Option<u32>, Option<UsfmVersion>) {
    let usfm = generated::marker_idx(b"usfm", SpellingShape::PlainOnly);
    let mut book = None;
    let mut version = None;
    let mut awaiting_version = false;
    for (idx, token) in tokens.iter().enumerate() {
        match token.kind() {
            TokenKind::BookCode => {
                book.get_or_insert(idx as u32);
                if version.is_some() {
                    break;
                }
            }
            // `\usfm` carves no payload (the scanner leaves the version string
            // as ordinary Text, isolated by its line ending), so the fact is
            // read off the ADJACENT token — the same adjacency shape the
            // `ca`/`cp` rules use, and the reason scanner.rs carves nothing.
            TokenKind::Text if awaiting_version => {
                version = parse_version(span_of(source, token));
                awaiting_version = false;
                if book.is_some() && version.is_some() {
                    break;
                }
            }
            TokenKind::Marker { .. } => {
                if generated::kind(token.marker_idx) == MarkerKind::Chapter {
                    break;
                }
                awaiting_version = token.marker_idx == usfm;
            }
            _ => awaiting_version = false,
        }
    }
    (book, version)
}

/// `3.0`, `3.2`, `4.0` → the ladder rung a rule keys on. Anything else is
/// `None`: an undeclared version is not a declaration of 3.0, and a rule that
/// escalates on one must not fire on the other.
fn parse_version(span: &[u8]) -> Option<UsfmVersion> {
    let mut parts = span.split(|b| *b == b'.');
    let number = |part: Option<&[u8]>| -> Option<u32> {
        let part = part?;
        (!part.is_empty() && part.iter().all(u8::is_ascii_digit))
            .then(|| part.iter().fold(0u32, |n, b| n * 10 + u32::from(b - b'0')))
    };
    let major = number(parts.next())?;
    let minor = number(parts.next()).unwrap_or(0);
    Some(match (major, minor) {
        (4.., _) => UsfmVersion::V4_0,
        (3, 2..) => UsfmVersion::V3_2,
        _ => UsfmVersion::V3_0,
    })
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

        // A paragraph with nothing under it but line endings. `\b` abstains by
        // its `SingleNewline` delimiter — the column that says "this row takes
        // no content" (see the row).
        if generated::kind(marker_idx) == MarkerKind::Paragraph
            && generated::ws_after_name(marker_idx) != Ws::SingleNewline
            && is_empty_paragraph(tokens, cst, node)
        {
            out.push(Observation::one(Code::EmptyParagraph, anchor));
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

/// Does this node hold anything a reader would see? A child NODE always
/// counts; among token children only the node's own OPENING marker (which the
/// walker files as its first child) and a `Newline` do not. Attribute lists DO
/// count — `\p|cat="x"|` with nothing after it carries metadata and is a
/// different authoring fact from a bare `\p`, and lumping the two together
/// would report the deliberate one.
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

/// THE token walk: every rule whose evidence is a token row, its span, or the
/// one token before it — the row-lookup rules, the two Form rules, the caller
/// and numbering payload rules, adjacency, and the shape-only attribute
/// family. One pass, exactly as the sketch draws it ("adjacency + form +
/// payload + attributes: single token walk").
///
/// It carries THREE small pieces of lookbehind state, and they are here rather
/// than in passes of their own for a measured reason: a token sweep over this
/// corpus costs ~2.5 ns/token in dispatch alone, however little each arm does,
/// so a family that needs no more than the last marker earns no pass of its
/// own. `ordering_pass` stays separate because its state is a SEQUENCE (the
/// previous chapter and verse), not the previous token.
///
/// - **The adjacency window.** `\ca`/`\cp` are legal immediately after `\c`'s
///   designator or after each other, `\va`/`\vp` likewise after `\v` — where
///   "immediately" means "with nothing but whitespace between", because the
///   scanner emits a Newline token at every line break and the spec's own
///   examples put `\cp` on its own line. These markers open no scope, so the
///   CST cannot see their misplacement; this is the only place the fact exists.
/// - **The attribute owner.** Attributes belong to the last marker, exactly as
///   [`TokenKind::AttrList`] documents, so the owner is the nearest preceding
///   opener and a Newline ends its reach — the scanner bounds lists to a line,
///   so nothing else would be honest.
/// - **The numbering-mix bitmasks**, closed out by the pass simply ending.
fn token_pass(
    source: &[u8],
    tokens: &[Token],
    consumed: &[bool],
    version: Option<UsfmVersion>,
    out: &mut Vec<Observation>,
) {
    /// Which designator family may be re-numbered at this point.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Window {
        Closed,
        Chapter,
        Verse,
    }

    let idx_of = |name: &[u8]| generated::marker_idx(name, SpellingShape::PlainOnly);
    let (ca, cp) = (idx_of(b"ca"), idx_of(b"cp"));
    let (va, vp) = (idx_of(b"va"), idx_of(b"vp"));

    let mut window = Window::Closed;
    // The owning marker of any attribute list that arrives now: its token
    // index, its row, whether its terminator is `\*` rather than a named
    // closer, and whether its row defines any attributes at all. `NO_TOKEN` =
    // no marker is in reach.
    let mut owner = NO_TOKEN;
    let mut owner_idx = generated::UNRESOLVED;
    let mut owner_is_point = false;
    let mut owner_has_attrs = false;
    // The first attribute list already seen on that owner.
    let mut first_list = NO_TOKEN;
    // Levels-seen per numbered family, indexed by ROW: bit 0 = the bare
    // spelling, bit n = `\q<n>`, bit 15 = "already reported". A fixed array
    // rather than a map because the row index IS the family key and there are
    // only 153 rows — 306 bytes of stack, no hashing, no allocation.
    // `first_seen` is only ever read when the mask says the family has been
    // seen, so it needs no sentinel initialization.
    let mut levels = [0u16; generated::ROW_COUNT];
    let mut first_seen = [0u32; generated::ROW_COUNT];
    const REPORTED: u16 = 1 << 15;

    for (idx, token) in tokens.iter().enumerate() {
        let idx = idx as u32;
        let kind = token.kind();
        match kind {
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
            // Openers and milestones share this arm because they are the same
            // thing to every rule below: the marker a list, a level or a
            // designator belongs to. `nested` binds the two kinds' one
            // spelling bit (`\+w`'s `+`, `\qt-e`'s `e`), and only the opener
            // half ever reads it.
            TokenKind::Marker { nested } | TokenKind::Milestone { end: nested } => {
                let marker_idx = token.marker_idx;
                let opener = matches!(kind, TokenKind::Marker { .. });
                if opener {
                    if marker_idx == generated::UNRESOLVED {
                        out.push(Observation::one(Code::UnknownMarker, idx));
                    } else if nested && generated::kind(marker_idx) != MarkerKind::Character {
                        out.push(Observation::one(Code::NestedSpellingMisuse, idx));
                    }
                    if generated::kind(marker_idx) == MarkerKind::Paragraph
                        && token.start > 0
                        && !is_structural_ws(source[token.start as usize - 1])
                    {
                        out.push(Observation::one(Code::MarkerNotWsPreceded, idx));
                    }
                    // "The span absorbed no delimiter" is ONE byte to check: a
                    // marker name never ends in whitespace, so a trailing
                    // space or tab can only be the folded delimiter run.
                    if wants_delimiter(marker_idx)
                        && !span_of(source, token)
                            .last()
                            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                        && source
                            .get(token.end() as usize)
                            .is_some_and(|byte| !is_delimiter_byte(*byte))
                    {
                        out.push(Observation::one(Code::DelimiterShape, idx));
                    }
                }

                // --- adjacency ------------------------------------------
                let (code, opens) = if marker_idx == ca || marker_idx == cp {
                    (Some(Code::CaCpPlacement), Window::Chapter)
                } else if marker_idx == va || marker_idx == vp {
                    (Some(Code::VaVpPlacement), Window::Verse)
                } else {
                    (None, Window::Closed)
                };
                window = match code {
                    Some(code) => {
                        if window != opens {
                            out.push(Observation::one(code, idx));
                        }
                        // The window stays open either way: `\ca 2\ca*\cp א`
                        // is one legal run, and re-reporting every member of a
                        // misplaced run turns one slip into three findings.
                        opens
                    }
                    None => match generated::kind(marker_idx) {
                        MarkerKind::Chapter => Window::Chapter,
                        MarkerKind::Verse => Window::Verse,
                        _ => Window::Closed,
                    },
                };

                // --- numbering-mix --------------------------------------
                if has_levels(marker_idx) {
                    let family = &mut levels[marker_idx as usize];
                    if *family == 0 {
                        first_seen[marker_idx as usize] = idx;
                    }
                    *family |= 1 << spelled_level(span_of(source, token)).min(14);
                    // Bare AND numbered, said once per family per book.
                    if *family & REPORTED == 0 && *family & 1 != 0 && *family & !(REPORTED | 1) != 0
                    {
                        *family |= REPORTED;
                        out.push(Observation {
                            code: Code::NumberingMix,
                            anchor: idx,
                            second: first_seen[marker_idx as usize],
                            aux: match generated::numbering(marker_idx) {
                                Numbering::UpTo(cap) => u32::from(cap),
                                // `liv` alone: numbered with no cap stated.
                                _ => 0,
                            },
                        });
                    }
                }

                owner = idx;
                owner_idx = marker_idx;
                owner_is_point = !opener || generated::kind(marker_idx) == MarkerKind::Milestone;
                // Resolved HERE, once per marker, rather than per Text token:
                // it is the flag that keeps the pipe scan off ordinary prose,
                // so it must not itself cost a table read per token.
                owner_has_attrs =
                    !owner_is_point && !generated::defined_attributes(marker_idx).is_empty();
                first_list = NO_TOKEN;
            }
            // Not an enumeration of `+`/`-`/`?` — those are the conventional
            // values of one general run, and a caller of up to three bytes is
            // taken as deliberate. See the row for why the width is what it is.
            TokenKind::NoteCaller => {
                if span_of(source, token).len() > 3 {
                    out.push(Observation::one(Code::CallerShape, idx));
                }
                window = Window::Closed;
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
                // A closer ends the owner's reach and leaves the adjacency
                // window alone: `\ca 2\ca*\cp א` is one legal run.
                owner = NO_TOKEN;
                owner_has_attrs = false;
                first_list = NO_TOKEN;
            }
            TokenKind::MilestoneTerminator => {
                if !consumed[idx as usize] {
                    out.push(Observation::one(Code::OrphanTerminator, idx));
                }
                owner = NO_TOKEN;
                owner_has_attrs = false;
                first_list = NO_TOKEN;
            }
            TokenKind::AttrList => {
                if first_list != NO_TOKEN {
                    out.push(Observation::pair(Code::AttrBothLists, idx, first_list));
                } else {
                    first_list = idx;
                }
                if owner == NO_TOKEN {
                    continue;
                }
                // NODE-INITIAL is "in front position AND self-closed": the
                // list is the token right after its marker, and its span ends
                // with the closing pipe (plus any HS the U25001 production
                // puts inside the list). Everything else is the 3.1 trailing
                // form. The one shape this would read as node-initial and is
                // not is `\w a|b|\w*`, where a raw pipe ENDS a back-position
                // value — but that list is not in front position either, so
                // the front test already excludes it.
                let span = span_of(source, token);
                let trailing = idx != owner + 1
                    || !span
                        .iter()
                        .rev()
                        .find(|byte| !matches!(byte, b' ' | b'\t'))
                        .is_some_and(|byte| *byte == b'|');
                if trailing {
                    if generated::kind(owner_idx) == MarkerKind::Character
                        && version >= Some(UsfmVersion::V3_2)
                    {
                        out.push(Observation {
                            code: Code::AttrTrailingFormDeprecated,
                            anchor: idx,
                            second: owner,
                            aux: version.map_or(0, |declared| declared as u32),
                        });
                    }
                    // A trailing list stops AT its terminator, so the next
                    // token IS the closer the scanner accepted without
                    // checking whose it was. This is where that is checked.
                    let matched = match tokens.get(idx as usize + 1).map(Token::kind) {
                        Some(TokenKind::MilestoneTerminator) => owner_is_point,
                        Some(TokenKind::ClosingMarker { .. }) => {
                            !owner_is_point && tokens[idx as usize + 1].marker_idx == owner_idx
                        }
                        _ => true,
                    };
                    if !matched {
                        out.push(Observation::pair(Code::AttrTerminatorMismatch, idx, owner));
                    }
                }
            }
            // A raw pipe in the content of an attrs-capable marker. The owner
            // flag comes first on purpose: it is one already-loaded boolean,
            // and it keeps the byte scan off the ~85% of tokens that are
            // ordinary prose.
            //
            // MILESTONE owners abstain, and that is the same line the scanner
            // draws when it arms its back-position pipe needle for Character
            // and Figure rows alone. A milestone has no content, so a pipe
            // that stayed content there means its `\*` never came — which
            // `unterminated-milestone` already reports, exactly, and this hint
            // would only guess at.
            TokenKind::Text => {
                if owner_has_attrs && span_of(source, token).contains(&b'|') {
                    out.push(Observation::pair(Code::AttrPipeHint, idx, owner));
                }
                // GUARDED, and the guard is load-bearing: `\c 1 \ca` is the
                // only shape that cares whether a Text run is blank, so asking
                // the question unconditionally means reading every content byte
                // in the document for a fact that matters after roughly one
                // token in ten thousand.
                if window != Window::Closed
                    && !span_of(source, token).iter().all(|b| is_structural_ws(*b))
                {
                    window = Window::Closed;
                }
            }
            // A line ending ends the ATTRIBUTE machine's reach, and
            // deliberately leaves the adjacency window open: `\c 1` and its
            // `\cp` are conventionally written on separate lines.
            TokenKind::Newline => {
                owner = NO_TOKEN;
                owner_has_attrs = false;
                first_list = NO_TOKEN;
            }
            // The designator of the `\c`/`\v`/`\ca`/`\vp` that opened the
            // window belongs to the run; an optional break does not.
            TokenKind::Designator => {}
            TokenKind::OptBreak => window = Window::Closed,
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

/// A token's bytes. Lint reads `source` only through spans the scanner already
/// carved, plus the single byte on either side of one (the two Form rules).
fn span_of<'a>(source: &'a [u8], token: &Token) -> &'a [u8] {
    &source[token.start as usize..token.end() as usize]
}

/// Space, tab, CR or LF — the four bytes the scanner treats as whitespace.
/// Deliberately ASCII-only: it is the SPEC's structural whitespace, and a
/// no-break space failing this test is the finding, not a gap.
fn is_structural_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// What may legally follow a marker name: structural whitespace, or one of
/// `TAGEND`'s two other alternatives (a marker, an attribute list).
fn is_delimiter_byte(byte: u8) -> bool {
    is_structural_ws(byte) || matches!(byte, b'\\' | b'|')
}

/// Does this row's `ws_after_name` REQUIRE something after the name? The
/// optional forms (milestones' `Hs`, row 0's `NotRequired`) can never be
/// missing one.
fn wants_delimiter(marker_idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::ws_after_name(marker_idx),
        Ws::AtLeastOneHorizontalWhitespace
            | Ws::AtLeastOneWhitespace
            | Ws::SingleNewline
            | Ws::AtLeastOneNewline
            | Ws::TagEndDelimiter
    )
}

/// The level digit an occurrence was SPELLED with — `\q2` → 2, `\q` → 0,
/// `\qt3-s` → 3. Read off the span because that is the only place it exists:
/// rows are canonical (`q`, not `q1`), so the token's own bytes are the sole
/// record of which spelling was used.
fn spelled_level(span: &[u8]) -> u8 {
    let from = usize::from(span.get(1) == Some(&b'+')) + 1;
    let mut level = 0u8;
    for byte in &span[from.min(span.len())..] {
        match byte {
            b'0'..=b'9' => level = level.saturating_mul(10).saturating_add(byte - b'0'),
            b'a'..=b'z' if level == 0 => {}
            _ => break,
        }
    }
    level
}

/// Is this row numbered in the sense `numbering-mix` cares about — a family
/// whose digit is a LEVEL? `TableColumns` rows (`\tc1`, `\tc1-2`) are excluded:
/// their digits are a column number, i.e. payload, so `\tc` beside `\tc2` is
/// not two spellings of one thing.
fn has_levels(marker_idx: generated::MarkerIdx) -> bool {
    matches!(
        generated::numbering(marker_idx),
        Numbering::UpTo(_) | Numbering::Unbounded
    )
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
            // Phases 1-3 ship five families; a stray row from a family whose
            // pass has not landed means a phase arrived half-built. Version is
            // the one still missing.
            assert!(
                !matches!(row.category, Category::Version),
                "{} belongs to a family with no pass yet",
                row.name
            );
            // `Count` is the one aux meaning nothing writes yet.
            assert!(
                !matches!(row.aux, AuxKind::Count),
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
        // (The line break before the last `\p` is load-bearing since phase 3:
        // a paragraph marker glued to the preceding word is a Form finding of
        // its own, and this test is not about that.)
        let (tokens, obs) = findings("\\p out\\esb \\p in \\c 1 more\\esbe\n\\p after");
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

    // -----------------------------------------------------------------
    // Phase 3: adjacency
    // -----------------------------------------------------------------

    /// The placement findings alone.
    ///
    /// Filtered rather than compared whole because `\ca*`/`\va*` currently
    /// draw an `orphan-closer` of their own: the `ca`/`va`/`vp` rows carry
    /// `closing: RequiredExplicit` but `opens_scope: None`, so the walker
    /// pushes no frame for them and their closers close nothing. That is a
    /// TABLE/walker disagreement, not this rule's business, and the corpus
    /// contains no `\ca` at all — flagged for the spec-diff, never patched
    /// here.
    fn placement(observations: &[Observation]) -> Vec<Observation> {
        observations
            .iter()
            .filter(|obs| matches!(obs.code, Code::CaCpPlacement | Code::VaVpPlacement))
            .copied()
            .collect()
    }

    #[test]
    fn ca_and_cp_must_follow_their_chapter() {
        // The spec's own shape: `\ca` on the `\c` line, `\cp` on the next one.
        // A Newline between them is a token, and the rule steps over it.
        let (_, obs) = findings("\\c 1 \\ca 2\\ca*\n\\cp \u{5d0}\n\\p \\v 1 a");
        assert_eq!(placement(&obs), vec![]);

        // …and the pair the other way round is equally legal.
        let (_, obs) = findings("\\c 1\n\\cp \u{5d0}\n\\ca 2\\ca*\n\\p \\v 1 a");
        assert_eq!(placement(&obs), vec![]);

        // Real content between them closes the window.
        let (tokens, obs) = findings("\\c 1\n\\p text\n\\ca 2\\ca*\n");
        assert_eq!(
            placement(&obs),
            vec![Observation::one(
                Code::CaCpPlacement,
                token_named(&tokens, "ca", 0)
            )]
        );

        // A whole run out of place is ONE finding, not one per member.
        let (tokens, obs) = findings("\\p text\n\\ca 2\\ca*\\cp \u{5d0}\n");
        assert_eq!(
            placement(&obs),
            vec![Observation::one(
                Code::CaCpPlacement,
                token_named(&tokens, "ca", 0)
            )]
        );
    }

    #[test]
    fn va_and_vp_must_follow_their_verse() {
        let (_, obs) = findings("\\c 1\n\\p \\v 1 \\va 2\\va* \\vp 1-2\\vp* text");
        assert_eq!(placement(&obs), vec![]);

        // The designator and a line break both keep the window open.
        let (_, obs) = findings("\\c 1\n\\p \\v 1\n\\va 2\\va*\n");
        assert_eq!(placement(&obs), vec![]);

        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 text \\va 2\\va*");
        assert_eq!(
            placement(&obs),
            vec![Observation::one(
                Code::VaVpPlacement,
                token_named(&tokens, "va", 0)
            )]
        );

        // A `\va` after a CHAPTER is still misplaced — the two windows are
        // separate machines, not one "designator" window.
        let (tokens, obs) = findings("\\c 1 \\va 2\\va*\n\\p \\v 1 a");
        assert_eq!(
            placement(&obs),
            vec![Observation::one(
                Code::VaVpPlacement,
                token_named(&tokens, "va", 0)
            )]
        );
    }

    // -----------------------------------------------------------------
    // Phase 3: payload
    // -----------------------------------------------------------------

    #[test]
    fn caller_shape_only_reports_the_accidents() {
        // The three conventional values, and a short custom one, are silent.
        for caller in ["+", "-", "?", "*", "\",", "abc"] {
            let (_, obs) = findings(&format!("\\p a\\f {caller} \\ft n\\f*"));
            assert_eq!(obs, vec![], "caller {caller:?}");
        }

        // `\f +note` — the space after the caller was forgotten, so the whole
        // word lexed as the caller.
        let (tokens, obs) = findings("\\p a\\f +note \\ft n\\f*");
        let caller = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::NoteCaller)
            .unwrap() as u32;
        assert_eq!(obs, vec![Observation::one(Code::CallerShape, caller)]);
    }

    /// `numbering-out-of-range` is NOT a code, and this is why: an over-cap
    /// level never reaches its family's row. `generated::marker_idx` validates
    /// the digits during resolution, so `\q7` IS row 0 and `unknown-marker`
    /// has already said everything there is to say about it. If this test ever
    /// fails, the rule became reachable and should be written.
    #[test]
    fn an_over_cap_level_is_an_unknown_marker_not_a_range_finding() {
        let (tokens, obs) = findings("\\c 1\n\\q7 poetry\n");
        let over_cap = tokens
            .iter()
            .position(|t| {
                matches!(t.kind(), TokenKind::Marker { .. })
                    && t.marker_idx == generated::UNRESOLVED
            })
            .unwrap() as u32;
        assert_eq!(codes(&obs), vec![Code::UnknownMarker]);
        assert_eq!(obs[0].anchor, over_cap);
        // …and the highest legal level resolves to the `q` row as it should.
        let (_, obs) = findings("\\c 1\n\\q4 poetry\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn the_declared_version_is_reported() {
        let version = |usfm: &str| {
            let tokens = lex(usfm);
            let cst = build(&tokens);
            lint(usfm.as_bytes(), &tokens, &cst).declared_version
        };
        assert_eq!(
            version("\\id GEN\n\\usfm 3.0\n\\p a"),
            Some(UsfmVersion::V3_0)
        );
        assert_eq!(
            version("\\id GEN\n\\usfm 3.2\n\\p a"),
            Some(UsfmVersion::V3_2)
        );
        assert_eq!(
            version("\\id GEN\n\\usfm 4\n\\p a"),
            Some(UsfmVersion::V4_0)
        );
        assert_eq!(
            version("\\id GEN\n\\usfm 3.2.1\n\\p a"),
            Some(UsfmVersion::V3_2)
        );
        // No declaration, and a declaration that is not a version, are the
        // same state: unknown, never assumed.
        assert_eq!(version("\\id GEN\n\\p a"), None);
        assert_eq!(version("\\id GEN\n\\usfm three\n\\p a"), None);
        // The header scan stops at the first `\c`, so a `\usfm` line written
        // below one is not a declaration this report will claim.
        assert_eq!(version("\\id GEN\n\\c 1\n\\usfm 3.2\n\\p a"), None);
    }

    #[test]
    fn numbering_mix_is_one_finding_per_family_per_book() {
        // One family, both spellings: anchored at the token that revealed it,
        // `second` at the family's first occurrence, `aux` the row's cap.
        let (tokens, obs) = findings("\\c 1\n\\q a\n\\q1 b\n\\q2 c\n\\q d\n");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::NumberingMix,
                anchor: token_named(&tokens, "q", 1),
                second: token_named(&tokens, "q", 0),
                aux: 4,
            }]
        );

        // Numbered-only and bare-only are both consistent.
        let (_, obs) = findings("\\c 1\n\\q1 a\n\\q2 b\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\c 1\n\\q a\n\\q b\n");
        assert_eq!(obs, vec![]);

        // Two families mixing is two findings — never one per occurrence.
        let (_, obs) = findings("\\c 1\n\\q a\n\\q1 b\n\\q c\n\\s d\n\\s1 e\n\\s f\n");
        assert_eq!(codes(&obs), vec![Code::NumberingMix, Code::NumberingMix]);

        // `\tc1` is a COLUMN, not a level: `\tc` beside it is not a mix.
        let (_, obs) = findings("\\c 1\n\\tr \\tc a \\tc2 b\n");
        assert_eq!(obs, vec![]);
    }

    // -----------------------------------------------------------------
    // Phase 3: form
    // -----------------------------------------------------------------

    #[test]
    fn marker_not_ws_preceded_is_paragraphs_only() {
        let (tokens, obs) = findings("\\p text\\s1 heading\n\\p more");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MarkerNotWsPreceded,
                token_named(&tokens, "s", 0)
            )]
        );

        // Character markers legitimately hug — the nested spelling included,
        // and aligned USFM is built out of exactly this.
        let (_, obs) = findings("\\p \\w grace\\+nd deep\\+nd*\\w*\\add x\\add*");
        assert_eq!(obs, vec![]);

        // Start of file is not a finding either (the `\id` prefix this helper
        // adds is itself the case).
        let (_, obs) = findings("\\id GEN\n\\p a");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn delimiter_shape_reports_a_non_whitespace_separator() {
        // The live corpus finding: en_ulb REV writes `\m(for fine linen…`.
        let (tokens, obs) = findings("\\p a\n\\m(for fine linen)\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DelimiterShape,
                token_named(&tokens, "m", 0)
            )]
        );

        // A no-break space after the name is the same finding.
        let (tokens, obs) = findings("\\p a\n\\q\u{00A0}poetry\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DelimiterShape,
                token_named(&tokens, "q", 0)
            )]
        );

        // Everything `TAGEND` allows is silent: whitespace, a line ending, a
        // marker, an attribute list, end of file.
        for usfm in [
            "\\p text",
            "\\p\ttext",
            "\\p\n\\p text",
            "\\p\\v 1 text",
            "\\p|cat=\"x\"| text",
            "\\c 1\n\\p a\n\\b\n\\p b",
        ] {
            let (_, obs) = findings(usfm);
            assert!(
                !codes(&obs).contains(&Code::DelimiterShape),
                "{usfm:?} reported a delimiter"
            );
        }
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

    // -----------------------------------------------------------------
    // Phase 3: attributes (shape only)
    // -----------------------------------------------------------------

    #[test]
    fn the_trailing_attribute_form_is_reported_only_against_a_declared_32() {
        // 3.0 declared, and no declaration at all: the trailing form is the
        // correct spelling and nothing is said.
        let (_, obs) = findings("\\id GEN\n\\usfm 3.0\n\\p \\w grace|lemma=\"x\"\\w*\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\p \\w grace|lemma=\"x\"\\w*\n");
        assert_eq!(obs, vec![]);

        // 3.2 declared: deprecated, and the row escalates it to an Error at 4.
        let usfm = "\\id GEN\n\\usfm 3.2\n\\p \\w grace|lemma=\"x\"\\w*\n";
        let (tokens, obs) = findings(usfm);
        let list = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::AttrList)
            .unwrap() as u32;
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::AttrTrailingFormDeprecated,
                anchor: list,
                second: token_named(&tokens, "w", 0),
                aux: UsfmVersion::V3_2 as u32,
            }]
        );
        assert_eq!(
            Code::AttrTrailingFormDeprecated.row().escalation,
            Some((UsfmVersion::V4_0, Severity::Error))
        );

        // The node-initial form is what 3.2 wants, and says nothing.
        let (_, obs) = findings("\\id GEN\n\\usfm 3.2\n\\p \\w |lemma=\"x\"|grace\\w*\n");
        assert_eq!(obs, vec![]);

        // A MILESTONE's trailing list is its normal syntax, never deprecated —
        // this is the shape that would light up every alignment corpus.
        let (_, obs) =
            findings("\\id GEN\n\\usfm 3.2\n\\p a \\qt-s |who=\"Levi\"\\* b \\qt-e\\*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn two_attribute_lists_on_one_marker() {
        // The proposal's own "ridiculous but legal" case.
        let (tokens, obs) = findings("\\p \\w |Fred|J\u{e9}sus|Jesus\\w*\n");
        let lists: Vec<u32> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind() == TokenKind::AttrList)
            .map(|(idx, _)| idx as u32)
            .collect();
        assert_eq!(
            obs,
            vec![Observation::pair(Code::AttrBothLists, lists[1], lists[0])]
        );

        // One list per marker, twice over, is not two lists on one marker.
        let (_, obs) = findings("\\p \\w a|k=\"v\"\\w* \\w b|k=\"v\"\\w*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn an_attribute_list_closed_by_the_wrong_marker() {
        // `\add*` ends `\w`'s list — the case scanner.rs names when it says
        // the terminator is checked only for BEING a closer.
        let (tokens, obs) = findings("\\p \\w grace|lemma=\"x\"\\add*\n");
        let list = tokens
            .iter()
            .position(|t| t.kind() == TokenKind::AttrList)
            .unwrap() as u32;
        assert!(codes(&obs).contains(&Code::AttrTerminatorMismatch));
        assert_eq!(
            obs.iter()
                .find(|o| o.code == Code::AttrTerminatorMismatch)
                .copied(),
            Some(Observation::pair(
                Code::AttrTerminatorMismatch,
                list,
                token_named(&tokens, "w", 0)
            ))
        );

        // A milestone list closed by `\*`, and a character list closed by its
        // own `\X*`, are both matched.
        let (_, obs) = findings("\\p a \\qt-s |who=\"Levi\"\\* b \\qt-e\\*\n");
        assert_eq!(obs, vec![]);
        let (_, obs) = findings("\\p \\w grace|lemma=\"x\"\\w*\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn a_pipe_in_content_hints_only_inside_an_attrs_capable_marker() {
        // The list was refuted (no closer before the end of the line), so the
        // pipe survived as content — which is the whole reason for the hint.
        let (tokens, obs) = findings("\\p \\w gracious|lemma=\"grace\"\n\\p more\n");
        let hints: Vec<Observation> = obs
            .iter()
            .filter(|o| o.code == Code::AttrPipeHint)
            .copied()
            .collect();
        assert_eq!(hints.len(), 1, "{obs:?}");
        assert_eq!(hints[0].second, token_named(&tokens, "w", 0));

        // A pipe in ordinary prose is ordinary prose: `\p` defines no
        // attributes, so nothing is said.
        let (_, obs) = findings("\\p a | b\n");
        assert_eq!(obs, vec![]);

        // Neither does a marker that opens no attributes of its own.
        let (_, obs) = findings("\\p \\add a | b\\add*\n");
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
