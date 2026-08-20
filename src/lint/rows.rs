//! The lint codes and their authored table — the sibling of `tables::rows`.
//!
//! [`Code`] is the identity, [`LINT_ROWS`] is the per-code data, and the four
//! small enums are the columns' vocabularies. Nothing here reads a document.

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
///
/// [`Observation::aux`]: super::Observation::aux
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
    ///
    /// [`Observation::aux`]: super::Observation::aux
    pub aux: AuxKind,
    /// Default-English message with `{anchor}`/`{second}` standing for the
    /// marker text at those token spans. Rendering and localization are the
    /// consumer's; the library never allocates a message.
    pub template: &'static str,
    /// The label this code's fix carries. The row is the single place a
    /// rule's affordances are declared: a code emits a fix iff its row has a
    /// label (tested), and the generators read the label from here.
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
    // arrived while it was open. Both live corpus instances (en_ulb ISA and
    // MRK) are genuinely truncated footnotes.
    //
    // There used to be a third, and it was a TABLE bug rather than damaged
    // data — phase 4's fix preview is what showed it. bsb GEN 2:4 writes a
    // perfectly well-formed note whose `\fq` contains `\+nd`, and `nd`'s
    // context mask omitted Footnote, so the nested character marker DISPLACED
    // the whole note (taking that book's `\f*` down with it as an orphan
    // closer). Will ruled the omission class-wide on 2026-08-19 — usfmtc nests
    // character markers in notes, and the spec contradicts its own "Valid In"
    // lists — so every character row now carries Footnote and CrossReference
    // (see the note above the `add` row in tables::rows). The fix that would
    // have truncated a good footnote is no longer offered there, because there
    // is no longer a finding.
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
    //
    // Phase 4 learned the limit of that aggregation, and it is worth stating
    // where the rule is: a run can be re-opened. In en_ulb the `\s5` chunk
    // marker is row 0, so its pop-all recovery kills whatever paragraph is
    // standing — accepting the fix at the head of a run therefore repairs that
    // site and UNMASKS the next segment, which was damaged all along and merely
    // aggregated away. Nothing is created (PHM reports 36 before and 36 after)
    // and the count never rises, which is why the fix oracle judges a fix by
    // its own SITE rather than by a falling total. See `check_fixes`.
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
    // findings on a corpus with nothing wrong with it.
    //
    // HINT, not Warning (corrected 2026-08-20, Will's read of the PARA
    // railroad): the diagram's two branches into a paragraph marker are
    // `'\n\'` and `/${Ws}\\/`, and `Ws` is `/${anyws}*/` — ZERO or more —
    // so `content\p` is grammatically VALID; the newline branch is the
    // preferred/canonical spelling, not a requirement. That makes this a
    // formatting preference — exactly the formatter-bundle shape (Hint +
    // auto-fix), not a violation.
    LintRow {
        code: Code::MarkerNotWsPreceded,
        name: "marker-not-ws-preceded",
        category: Category::Form,
        severity: Severity::Hint,
        escalation: None,
        aux: AuxKind::None,
        template: "\\{anchor} needs whitespace before it",
        fix_label: Some("insert a line break"),
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
    //
    // NO FIX, deliberately (phase 4). The 3.2 rewrite MOVES the list from back
    // position to front — `\w grace|lemma="x"\w*` becomes
    // `\w |lemma="x"|grace\w*` — and a move is not one splice: the k/v interior
    // has never been read (that is the attribute interpreter's, unbuilt), the
    // content the list has to jump over is arbitrary, and deciding where inside
    // it the boundary falls is an interpretation of the author's text. A fix is
    // a mechanical splice or it is not offered.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
