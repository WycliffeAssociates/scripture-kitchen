//! The lint codes and their authored table — the sibling of `tables::rows`.
//!
//! [`Code`] is the identity, [`LINT_ROWS`] is the per-code data, and the four
//! small enums are the columns' vocabularies. Nothing here reads a document.

/// CodeMirror's `Diagnostic.severity` ladder plus one channel of our own; the
/// editor session maps the first four 1:1 with no translation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
    /// The FORMATTER's channel: a row that draws no diagnostic at all.
    /// [`lint`](crate::lint::lint) never evaluates such a row and no report ever
    /// carries one; [`format_edits`](crate::format::format_edits) is its only
    /// consumer.
    ///
    /// An explicit variant rather than `severity: None`, which already means
    /// something else here — "gated until the document declares a version".
    Form,
}

/// The subsystem a code belongs to. Consumers group by this; it is also how a
/// future per-rule config addresses whole families at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Nesting, closers, barriers, recovery.
    Structure,
    Ordering,
    Attributes,
    Payload,
    Form,
    Version,
}

/// What [`Observation::aux`] MEANS for a given code — the column that keeps a
/// bare u32 from being opaque.
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
    /// A plain count (occurrences folded into one aggregate finding), or the
    /// same u32 read as a FLAG where a rule has exactly two shapes
    /// (`attr-unknown-name`: 1 = the marker has no default attribute at all).
    Count,
    /// A [`UsfmVersion`] discriminant.
    Version,
    /// A [`MalformedAttr`](crate::attributes::MalformedAttr) discriminant —
    /// which way an attribute list stopped making sense.
    MalformedShape,
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
/// [`LINT_ROWS`] directly (asserted in tests), but the number is per-build wire
/// data: a rule's durable identity is its kebab-case [`LintRow::name`].
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
    // --- Ordering ---------------------------------------------------------
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
    // --- Payload ----------------------------------------------------------
    MissingId,
    BookCodeUnknown,
    BookCodeNotUppercase,
    ChapterWithoutDesignator,
    VerseWithoutDesignator,
    // --- adjacency (filed Structure — see the rows) -----------------------
    CaCpPlacement,
    VaVpPlacement,
    // --- Payload ----------------------------------------------------------
    CallerShape,
    NumberingMix,
    // --- Form -------------------------------------------------------------
    MarkerNotWsPreceded,
    DelimiterShape,
    EmptyParagraph,
    // --- Attributes (shape only — no k/v interpreter) ---------------------
    AttrTrailingFormDeprecated,
    AttrBothLists,
    AttrTerminatorMismatch,
    AttrPipeHint,
    // --- Attributes, through the k/v interpreter --------------------------
    AttrUnknownName,
    AttrMalformed,
    AttrRequiredIf,
    // --- Version ----------------------------------------------------------
    DeprecatedMarker,
    DeprecatedAttribute,
    // --- the positional band ----------------------------------------------
    MarkerOutOfBand,
    DuplicateId,
    ParagraphBeforeFirstChapter,
    // --- Form (continued — appended so earlier row indices stay stable) ----
    DelimiterSurplus,
    // --- the FORM CHANNEL --------------------------------------------------
    // Never a diagnostic (`Severity::Form`), never evaluated by `lint`. THIS
    // ORDER IS PRECEDENCE: where two of these want the same bytes, the earlier
    // row wins the span and the later one is not emitted
    // ([`crate::format::format_edits`]).
    RemoveMarker,
    BridgeEmptyVerses,
    DedupeVerseNumber,
    BlockMarkerOwnLine,
    CharMarkerLineJoin,
    CollapseBlankLines,
    NormalizeNewlines,
    TrimTextEdges,
    DelimiterSingle,
    DesignatorWsSingle,
    MarkerWsAtLineStart,
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
    /// The severity BELOW the ladder's first rung — no `\usfm` declaration, or
    /// one under `escalation[0]`'s version.
    ///
    /// `None` is the GATE: the code says NOTHING until its first rung is
    /// declared (`attr-trailing-form-deprecated`, `deprecated-marker`). The
    /// finding is never raised and then filtered — the machine asks
    /// [`Self::severity_at`] first — because "this form is deprecated" is FALSE
    /// under an earlier version, not merely quiet.
    pub severity: Option<Severity>,
    /// The version LADDER, ascending: at or above each version, that severity.
    /// Empty = flat, and the last rung at or below the declared version wins
    /// ([`Self::severity_at`]).
    ///
    /// This column owns the version facts no `MarkerRow` owns: the spec
    /// deprecates and then removes forms, and the marker table records the
    /// spec page, never a version ladder.
    pub escalation: &'static [(UsfmVersion, Severity)],
    /// What [`Observation::aux`] means for this code.
    ///
    /// [`Observation::aux`]: super::Observation::aux
    pub aux: AuxKind,
    /// Default-English message with `{anchor}`/`{second}` standing for the
    /// marker text at those token spans. Rendering and localization are the
    /// consumer's; the library never allocates a message.
    pub template: &'static str,
    /// DUAL CITIZENSHIP: a real diagnostic whose fix is also a formatting
    /// action, so `format_edits` takes it into the transaction without the
    /// caller naming it. Always false on a [`Severity::Form`] row, where the
    /// channel IS the membership ([`Self::formats`]).
    pub formatter: bool,
    /// The label this code's fix carries. A code emits a fix iff its row has a
    /// label (tested), and the generators read the label from here.
    pub fix_label: Option<&'static str>,
}

impl LintRow {
    /// The severity this rule carries in a document that declares `version`, or
    /// `None` when it is SILENT there.
    ///
    /// ```text
    /// severity          escalation                          undeclared  3.0      3.2      4.0
    /// Some(Error)       &[]                                 Error       Error    Error    Error
    /// Some(Warning)     &[(V4_0, Error)]                    Warning     Warning  Warning  Error
    /// None (GATED)      &[(V3_2, Warning), (V4_0, Error)]   —           —        Warning  Error
    /// ```
    ///
    /// A `None` answer means the finding is never raised at all — the machine
    /// asks before it pushes, so the gate is data and not an `if` beside a rule.
    pub fn severity_at(&self, version: Option<UsfmVersion>) -> Option<Severity> {
        let mut severity = self.severity;
        if let Some(declared) = version {
            // Ascending, so the LAST rung at or below the declaration wins.
            for (rung, escalated) in self.escalation {
                if *rung > declared {
                    break;
                }
                severity = Some(*escalated);
            }
        }
        severity
    }

    /// A FORM-channel row: no diagnostic, ever. `lint` skips it and no consumer
    /// of a [`LintReport`](super::LintReport) can see one.
    pub fn is_form(&self) -> bool {
        self.severity == Some(Severity::Form)
    }

    /// Is this row part of the formatting transaction by default — either a Form
    /// row or a dual citizen? The `repairs` allowlist adds to this set; nothing
    /// removes from it.
    pub fn formats(&self) -> bool {
        self.is_form() || self.formatter
    }
}

/// The authored rules table — one row per [`Code`], in the enum's order. All
/// six families have a pass; the FORM family's tail is the format channel, which
/// `lint` never reaches (see [`Severity::Form`]).
pub const LINT_ROWS: [LintRow; 58] = [
    // Something that cannot live inside a note (a `\c`, a bare `\v`, an unknown
    // marker) arrived while the frame was open. Both live corpus instances
    // (en_ulb ISA, MRK) are genuinely truncated footnotes.
    LintRow {
        code: Code::UnclosedNote,
        name: "unclosed-note",
        category: Category::Structure,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} was never closed",
        formatter: false,
        fix_label: Some("insert the note closer"),
    },
    // The same, for a character marker, which REQUIRES its `\X*`.
    LintRow {
        code: Code::UnclosedChar,
        name: "unclosed-char",
        category: Category::Structure,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} was never closed",
        formatter: false,
        fix_label: Some("insert the closer"),
    },
    // Weaker than Recovery: nothing displaced the frame, the file just stopped.
    // Paragraphs, cells and rows want no closer and stay silent.
    LintRow {
        code: Code::UnclosedAtEof,
        name: "unclosed-at-eof",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is still open at the end of the book",
        formatter: false,
        fix_label: Some("insert the closer"),
    },
    // A U25003 container ended by displacement, not by its `\list-e\*`. The end
    // milestone is OPTIONAL in 3.2 and REQUIRED in 4 — hence the rung.
    LintRow {
        code: Code::UnterminatedContainer,
        name: "unterminated-container",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[(UsfmVersion::V4_0, Severity::Error)],
        aux: AuxKind::None,
        template: "{anchor} container was not closed by its end milestone",
        formatter: false,
        fix_label: Some("insert the container end milestone"),
    },
    // Recovery and Eof are one fact for a point: its span is only its attribute
    // list, so anything reaching it means the terminator went missing.
    LintRow {
        code: Code::UnterminatedMilestone,
        name: "unterminated-milestone",
        category: Category::Structure,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} milestone is missing its \\*",
        formatter: false,
        fix_label: Some("insert \\*"),
    },
    // No open frame of that name in reach (or only one behind a sidebar
    // barrier). The walker leaves it an ordinary leaf; the finding is ours.
    LintRow {
        code: Code::OrphanCloser,
        name: "orphan-closer",
        category: Category::Structure,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} closes nothing",
        formatter: false,
        fix_label: Some("delete the closer"),
    },
    LintRow {
        code: Code::OrphanTerminator,
        name: "orphan-terminator",
        category: Category::Structure,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "\\* terminates no milestone",
        formatter: false,
        fix_label: Some("delete \\*"),
    },
    // Not `orphan-terminator`: the `-e` point's own `\*` DID close the point —
    // what is missing is the container it claims to end.
    LintRow {
        code: Code::OrphanContainerEnd,
        name: "orphan-container-end",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} ends no open container",
        formatter: false,
        fix_label: None,
    },
    // A sidebar is a POP BARRIER, so a `\c` or `\v` written inside one stays
    // inside it: consistent, and exactly what the spec forbids. The walker
    // never unwinds to "fix" it — this finding pays for that.
    LintRow {
        code: Code::ContentOutsideSidebarRule,
        name: "content-outside-sidebar-rule",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is inside the sidebar opened by {second}",
        formatter: false,
        fix_label: None,
    },
    // Row 0: unknown names, illegal spellings, and every custom `\z` extension
    // until a configuration channel gives those real rows. Also the pop-all
    // recovery event — no open scope survives an unclassifiable marker.
    LintRow {
        code: Code::UnknownMarker,
        name: "unknown-marker",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is not a known marker",
        formatter: false,
        fix_label: None,
    },
    // The lexer records the `\+X` spelling wherever it appears, so this stays a
    // table question. Row 0 is excluded — `unknown-marker` speaks there.
    LintRow {
        code: Code::NestedSpellingMisuse,
        name: "nested-spelling-misuse",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is not a character marker; the nested spelling does not apply",
        formatter: false,
        fix_label: None,
    },
    // A `\v` with no paragraph anywhere above it. usfmtc FABRICATES an implicit
    // `\p` here (usfmparser.py:891); we flag and never repair — synthesizing a
    // token would break the partition. ONE finding per paragraph-less RUN,
    // anchored at its first verse, where the single repairing `\p` belongs.
    //
    // A run can be RE-OPENED, so accepting a fix need not lower the total: in
    // en_ulb the `\s5` chunk marker is row 0, and its pop-all recovery kills the
    // standing paragraph, so repairing one run unmasks the next segment that
    // aggregation was hiding. Hence the fix oracle judges a fix by its own SITE
    // rather than by a falling count (`check_fixes`).
    LintRow {
        code: Code::MissingParagraph,
        name: "missing-paragraph",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is not inside a paragraph",
        formatter: true,
        fix_label: Some("insert \\p"),
    },
    // ---- Ordering ---------------------------------------------------------
    // A payload that fails its pattern ([`crate::designator`]) is excluded from
    // the sequence, so one typo never cascades into a gap or duplicate later.
    // Flag, never reinterpret — reading `1O` as `10` would be synthesis.
    LintRow {
        code: Code::DesignatorMalformed,
        name: "designator-malformed",
        category: Category::Ordering,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is not a valid chapter/verse number",
        formatter: false,
        fix_label: None,
    },
    // The same chapter number twice in one book. `second` is the previous
    // chapter's designator token, `aux` the number the sequence expected.
    LintRow {
        code: Code::ChapterDuplicate,
        name: "chapter-duplicate",
        category: Category::Ordering,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::ExpectedNumber,
        template: "chapter {anchor} repeats the chapter at {second}; expected {aux}",
        formatter: false,
        fix_label: Some("renumber to the expected chapter"),
    },
    LintRow {
        code: Code::ChapterOutOfOrder,
        name: "chapter-out-of-order",
        category: Category::Ordering,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::ExpectedNumber,
        template: "chapter {anchor} goes backwards from {second}; expected {aux}",
        formatter: false,
        fix_label: Some("renumber to the expected chapter"),
    },
    // Chapters, unlike verses, have no tradition of legitimate holes — but the
    // check is CONTIGUITY, not a versification scheme, so it stays a warning.
    LintRow {
        code: Code::ChapterGap,
        name: "chapter-gap",
        category: Category::Ordering,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::ExpectedNumber,
        template: "chapter {anchor} skips ahead; expected {aux}",
        formatter: false,
        fix_label: None,
    },
    // The first number equals the previous designator's LAST covered verse
    // (`\v 12-14` then `\v 14`): repeat and range overlap are one fact.
    LintRow {
        code: Code::VerseDuplicate,
        name: "verse-duplicate",
        category: Category::Ordering,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::ExpectedNumber,
        template: "verse {anchor} repeats the verse at {second}; expected {aux}",
        formatter: false,
        fix_label: Some("renumber to the expected verse"),
    },
    // A verse starting BELOW the previous designator's last covered verse.
    LintRow {
        code: Code::VerseOutOfOrder,
        name: "verse-out-of-order",
        category: Category::Ordering,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::ExpectedNumber,
        template: "verse {anchor} goes backwards from {second}; expected {aux}",
        formatter: false,
        fix_label: Some("renumber to the expected verse"),
    },
    // Traditions that legitimately omit a verse land here; the answer is a
    // per-rule off switch, never a versification table inside the linter.
    LintRow {
        code: Code::VerseGap,
        name: "verse-gap",
        category: Category::Ordering,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::ExpectedNumber,
        template: "verse {anchor} skips ahead; expected {aux}",
        formatter: false,
        fix_label: None,
    },
    // A chapter whose FIRST verse is not 1. Fires INSTEAD of `verse-gap`: there
    // is no previous verse to have skipped from. `aux` is always 1.
    LintRow {
        code: Code::MissingVerseOne,
        name: "missing-verse-one",
        category: Category::Ordering,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::ExpectedNumber,
        template: "this chapter starts at verse {anchor}; expected {aux}",
        formatter: false,
        fix_label: None,
    },
    // ONE finding at the first such `\v`, and only when a `\c` does arrive
    // later — a book with no chapter at all is `missing-chapter`'s to report.
    LintRow {
        code: Code::VerseBeforeFirstChapter,
        name: "verse-before-first-chapter",
        category: Category::Ordering,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} comes before the book's first \\c",
        formatter: false,
        fix_label: None,
    },
    // Anchored at the first `\v`, and silent for a book with no verses: front
    // matter (FRT, GLO) is chapter-less by design.
    LintRow {
        code: Code::MissingChapter,
        name: "missing-chapter",
        category: Category::Ordering,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "this book has verses but no \\c",
        formatter: false,
        fix_label: None,
    },
    // ---- Payload ----------------------------------------------------------
    // Real in the wild (BSB Ecclesiastes). Anchored at token 0, and raised only
    // for a file with markers in it — an empty or prose-only buffer is not a
    // book. `LintReport::book` still reports `None`.
    LintRow {
        code: Code::MissingId,
        name: "missing-id",
        category: Category::Payload,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "this book has no \\id line",
        formatter: false,
        fix_label: None,
    },
    // An `\id` payload that is not one of the spec's 116 identifiers, in any
    // casing (see [`crate::tables::books`]).
    LintRow {
        code: Code::BookCodeUnknown,
        name: "book-code-unknown",
        category: Category::Payload,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is not a book identifier",
        formatter: false,
        fix_label: None,
    },
    // A real identifier, written `gen` or `Gen`. Fires INSTEAD of
    // `book-code-unknown`: the code IS known, only its casing is wrong.
    LintRow {
        code: Code::BookCodeNotUppercase,
        name: "book-code-not-uppercase",
        category: Category::Payload,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "book identifier {anchor} should be uppercase",
        formatter: false,
        fix_label: Some("uppercase the book identifier"),
    },
    // The scanner carves a `Designator` from the first content region after `\c`
    // and abandons the expectation at a newline or marker, so this is exactly
    // "the `\c` line was empty". Attribute lists are stepped over.
    LintRow {
        code: Code::ChapterWithoutDesignator,
        name: "chapter-without-designator",
        category: Category::Payload,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} has no chapter number",
        formatter: false,
        fix_label: None,
    },
    // The verse counterpart, and — since the scanner's designator gate — the
    // whole of "this `\v` names no verse": `\v`, `\v \p` and
    // `\v Then He declared` are one shape, no designator token in any of them.
    LintRow {
        code: Code::VerseWithoutDesignator,
        name: "verse-without-designator",
        category: Category::Payload,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} has no verse number",
        formatter: true,
        fix_label: Some("delete the empty verse marker"),
    },
    // ---- Adjacency --------------------------------------------------------
    // `\ca`/`\cp` may only follow `\c`'s designator, or the other of the pair.
    // Filed under STRUCTURE, not Payload: the fact is a marker sitting where it
    // may not sit, like `content-outside-sidebar-rule`. That it is computed over
    // TOKENS (these open no scope, so the CST cannot see them) is not a category.
    //
    // Whitespace between the parties is fine, newlines included — `\c 1\n\ca
    // 2\ca*\n\cp א` is the spec's own shape — so the rule steps over Newlines
    // and whitespace-only Text, and nothing else.
    LintRow {
        code: Code::CaCpPlacement,
        name: "ca-cp-placement",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} must follow the \\c it re-numbers",
        formatter: false,
        fix_label: None,
    },
    // The same rule for `\va`/`\vp` after `\v`.
    LintRow {
        code: Code::VaVpPlacement,
        name: "va-vp-placement",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} must follow the \\v it re-numbers",
        formatter: false,
        fix_label: None,
    },
    // ---- Payload ----------------------------------------------------------
    // Neither one of the three conventional values (`+`, `-`, `?`) nor three
    // bytes or shorter. The spec pattern is `/[^\\\s]+/`, so a custom caller is
    // legal and this can only be a HINT — the rule exists for the ACCIDENT
    // (`\f +\ft`, a quote glued to the marker). Three bytes keeps it honest:
    // real custom callers are short symbols, and the corpus's one non-`+`
    // caller (`",` — examples.bsb) stays deliberately unreported.
    LintRow {
        code: Code::CallerShape,
        name: "caller-shape",
        category: Category::Payload,
        severity: Some(Severity::Hint),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is an unusual note caller",
        formatter: false,
        fix_label: None,
    },
    // One book spelling a numbered family both ways — `\q` and `\q2`. The bare
    // form is ALWAYS valid, so this is a consistency observation, never an
    // error. ONE finding per family per book, anchored at the token that
    // revealed the mix; `second` is the family's first occurrence, `aux` the
    // row's numbering cap (`liv`, the one uncapped family, reports 0).
    //
    // There is no `numbering-out-of-range` code because it could never fire:
    // `generated::marker_idx` validates the level against the row's cap during
    // resolution (`digits_ok`), so `\q7` is row 0 and `unknown-marker` speaks.
    LintRow {
        code: Code::NumberingMix,
        name: "numbering-mix",
        category: Category::Payload,
        severity: Some(Severity::Info),
        escalation: &[],
        aux: AuxKind::NumberingCap,
        template: "{anchor} mixes numbered and bare spellings with {second} (levels 1-{aux})",
        formatter: false,
        fix_label: None,
    },
    // ---- Form -------------------------------------------------------------
    // `content\s1`. NARROWED to PARAGRAPH rows because character markers
    // legitimately hug and aligned USFM is built out of hugging
    // (`\zaln-s |…\*\w In|…\w*\zaln-e\*`): un-narrowed it reports ~1.5M findings
    // on a corpus with nothing wrong with it.
    //
    // HINT, not Warning: the PARA railroad's two branches into a paragraph
    // marker are `'\n\'` and `/${Ws}\\/`, and `Ws` is `/${anyws}*/` — ZERO or
    // more — so `content\p` is VALID and the newline branch is only the
    // canonical spelling. A formatting preference, not a violation.
    LintRow {
        code: Code::MarkerNotWsPreceded,
        name: "marker-not-ws-preceded",
        category: Category::Form,
        severity: Some(Severity::Hint),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} needs whitespace before it",
        formatter: true,
        fix_label: Some("insert a line break"),
    },
    // NBSP after a marker name, and every other exotic separator with it
    // (`\p\u{00A0}text`, `\v\u{2007}1`).
    //
    // After the lex the delimiter is folded into the marker's own span, so "how
    // was it written" survives only as the marker's LENGTH plus the byte after
    // it. The rule fires on the unambiguous reading: the row requires a
    // delimiter, the span absorbed none (`ws_run_end` folds space/tab only), and
    // the next byte is not whitespace, not a marker or pipe (`TAGEND`'s other
    // alternatives), and not EOF. "Several spaces where one would do" is out of
    // scope — `HS` is `+`, not `?`, in every row's pattern, so it is a
    // FORMATTER's business.
    LintRow {
        code: Code::DelimiterShape,
        name: "delimiter-shape",
        category: Category::Form,
        severity: Some(Severity::Hint),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is not followed by structural whitespace",
        formatter: false,
        fix_label: None,
    },
    // Nothing but Newlines under the node, or nothing at all. `\b` is EXCLUDED
    // — a blank line is empty BY DESIGN — identified by the column that says so,
    // `ws_after_name: SingleNewline` (the table's only such row). `\pb` never
    // reaches here: a Character row opens no scope, so it is never a node.
    LintRow {
        code: Code::EmptyParagraph,
        name: "empty-paragraph",
        category: Category::Form,
        severity: Some(Severity::Info),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} has no content",
        formatter: true,
        fix_label: Some("delete the duplicate paragraph"),
    },
    // ---- Attributes (shape only) ------------------------------------------
    // The 3.1 TRAILING attribute form — `\w grace|lemma="x"\w*` — which 3.2
    // deprecates and 4 removes. Two gates, both load-bearing:
    //
    // - CHARACTER rows only. `\zaln-s |x-strong="G1"\*` is a milestone's NORMAL
    //   syntax and never deprecated; `\fig |src="a.png"…\fig*` is how the spec's
    //   own figure examples are written.
    // - The document must DECLARE `\usfm 3.2` or later — the `None` base below
    //   plus the ladder's first rung. "Deprecated" is simply false in a file
    //   that declares 3.0, where the trailing form is the correct spelling
    //   (en_ult: 3.0 with 792,414 trailing lists, every one of them right).
    //
    // NO FIX. The 3.2 rewrite MOVES the list from back to front —
    // `\w grace|lemma="x"\w*` becomes `\w |lemma="x"|grace\w*` — and a move is
    // not one splice: the content jumped over is arbitrary, and where inside it
    // the boundary falls interprets the author's text.
    LintRow {
        code: Code::AttrTrailingFormDeprecated,
        name: "attr-trailing-form-deprecated",
        category: Category::Attributes,
        severity: None,
        escalation: &[
            (UsfmVersion::V3_2, Severity::Warning),
            (UsfmVersion::V4_0, Severity::Error),
        ],
        aux: AuxKind::Version,
        template: "the trailing attribute form is deprecated in USFM {aux}",
        formatter: false,
        fix_label: None,
    },
    // Two attribute lists on one marker — the spec's own "ridiculous but legal"
    // `\w |Fred|Jésus|Jesus\w*`. Legal back-compat, hence a warning; the winner
    // is the interpreter's merge rule ("later definition wins"), which is why
    // saying it twice is worth reporting. `second` is the first list.
    LintRow {
        code: Code::AttrBothLists,
        name: "attr-both-lists",
        category: Category::Attributes,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "a second attribute list overrides the one at {second}",
        formatter: false,
        fix_label: None,
    },
    // `\w a|k="v"\add*`, or a milestone list closed by a named closer instead of
    // `\*`. The list is well-formed; the scanner checks its terminator only for
    // BEING a closer, never for matching the open frame, and this is that check.
    //
    // Node-initial lists are never reported: their terminator is their own
    // closing pipe, which the scanner found or the bytes would not be a list.
    // That is also why "the list was never closed" has no code — such bytes are
    // not lexed as a list, so the pipe survives as Text and `attr-pipe-hint`
    // speaks instead.
    LintRow {
        code: Code::AttrTerminatorMismatch,
        name: "attr-terminator-mismatch",
        category: Category::Attributes,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "the attribute list on {second} is closed by the wrong marker",
        formatter: false,
        fix_label: None,
    },
    // A raw `|` left in the content of a marker that DEFINES attributes: the
    // list was refuted (it ran past its line, or its closer never came) and the
    // bytes stayed content. Gated on `defined_attributes` being non-empty,
    // because a pipe in ordinary prose is ordinary prose.
    LintRow {
        code: Code::AttrPipeHint,
        name: "attr-pipe-hint",
        category: Category::Attributes,
        severity: Some(Severity::Hint),
        escalation: &[],
        aux: AuxKind::None,
        template: "did you mean an attribute list on {second}?",
        formatter: false,
        fix_label: None,
    },
    // ---- Attributes (the k/v half) ----------------------------------------
    // A name the owning row does not define, judged by `attributes::resolve` —
    // the ONE place naming conventions live. A HINT: `x-`/`z-` names are legal
    // on any marker, so what reaches this rule is genuinely unrecognized, and
    // the spec's extension story makes that "did you mean `x-…`?".
    //
    // TWO shapes, told apart by `aux` (the interpreter reports `Unknown` for
    // both, so the shape is read off `name.is_empty()` — `AttrResolution`):
    //   * aux = 0 — a NAMED attribute nothing matched (`\w a|nope="x"\w*`).
    //   * aux = 1 — the BARE default form on a row with no `default_attribute`
    //     (`\fig |a.png\fig*`).
    //
    // Row 0 owners are silent: its row defines nothing, so every attribute on
    // `\zfoo |k="v"\*` would be a finding, and `unknown-marker` has spoken.
    LintRow {
        code: Code::AttrUnknownName,
        name: "attr-unknown-name",
        category: Category::Attributes,
        severity: Some(Severity::Hint),
        escalation: &[],
        aux: AuxKind::Count,
        template: "{second} defines no such attribute",
        formatter: false,
        fix_label: None,
    },
    // ONE finding per list by construction — `Malformed` ends the interpreter's
    // walk, because a broken tail is one mistake, not one per remaining byte.
    //
    // `aux` is the [`MalformedAttr`](crate::attributes::MalformedAttr)
    // discriminant, in that enum's declaration order:
    //   0 = UnterminatedQuote (`|lemma="grace`)
    //   1 = EmptyName         (`|="x"`)
    //   2 = MissingValue      (`|lemma=`)
    //   3 = BareJunk          (`|lemma="a", strong="G1"` — a comma is no
    //                          separator; usfmtc DROPS the tail, we report it)
    //
    // Warning, not Error: the list lexed as a list, so the document is readable
    // — what is lost is the attributes after the blamed byte.
    LintRow {
        code: Code::AttrMalformed,
        name: "attr-malformed",
        category: Category::Attributes,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::MalformedShape,
        template: "the attribute list on {second} stops making sense",
        formatter: false,
        fix_label: None,
    },
    // The CONDITIONAL cardinality no row can express: `defined_attributes` says
    // Optional because the attribute is optional in general, and whether THIS
    // occurrence owes one is a fact about the document.
    //
    // Two shapes, both anchored at the list (or at the milestone itself when
    // there is no list at all):
    //   * `eid` on a milestone END point whose family was opened with `sid`
    //     (`\qt-s |sid="a"\* … \qt-e\*`); `second` is that earlier sid point.
    //     NOT pairing — which `sid` this `eid` answers is vref territory,
    //     unmodelled in the CST — so the test is per-POINT.
    //   * `\ta`'s "one or more attributes, each beginning with `a-`"
    //     (char/features/ta.html). The row carries the `a-*` wildcard and no
    //     fixed names, so a list with none of that family is the finding.
    LintRow {
        code: Code::AttrRequiredIf,
        name: "attr-required-if",
        category: Category::Attributes,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{second} is missing an attribute this occurrence requires",
        formatter: false,
        fix_label: None,
    },
    // ---- Version ----------------------------------------------------------
    // Membership is [`VERSION_ROWS`] below, because WHICH version said so is
    // owned by no `MarkerRow` column.
    //
    // GATED on a declared `\usfm` via the `None` base: a 2.x-era book full of
    // `\addpn` is correct for its era, and reading an undeclared file as
    // "latest" would report every legacy book in the world.
    //
    // `aux` is the DEPRECATING version, not the declared one — that is what the
    // message needs, and the declared one is in `LintReport::declared_version`.
    LintRow {
        code: Code::DeprecatedMarker,
        name: "deprecated-marker",
        category: Category::Version,
        severity: None,
        escalation: &[
            (UsfmVersion::V3_0, Severity::Warning),
            (UsfmVersion::V4_0, Severity::Error),
        ],
        aux: AuxKind::Version,
        template: "{anchor} is deprecated since USFM {aux}",
        formatter: false,
        fix_label: Some("rename to the replacement marker"),
    },
    // Filed under Version, not Attributes: the fact is a LIFECYCLE one — the
    // spec still recognizes the attribute and tells authors not to use it.
    // Membership is `\xt`'s `link-href` and `\jmp`'s `link-` trio, read off
    // [`AttrStatus::Deprecated`] as the interpreter resolves.
    //
    // INFO and NO GATE, because `AttrStatus` carries no version: a rule that
    // cannot name the version it keys on must not pretend to gate on one.
    //
    // NO FIX: `link-href` → `href` looks mechanical, but the table records only
    // the deprecated name; nothing authored says what replaces it.
    LintRow {
        code: Code::DeprecatedAttribute,
        name: "deprecated-attribute",
        category: Category::Version,
        severity: Some(Severity::Info),
        escalation: &[],
        aux: AuxKind::None,
        template: "the attribute list on {second} uses a deprecated attribute",
        formatter: false,
        fix_label: None,
    },
    // ---- The positional band ----------------------------------------------
    // Every POSITIONAL context of the row is behind the document's position:
    // `\ip` (BookIntroduction) after ChapterContent, `\h` after the titles.
    //
    // The band is the mask's positional half — `Scripture` → … →
    // `ChapterContent`, the axis `SpecContext::is_positional` names — and it is
    // MONOTONIC, which is why this rule needs no data: the document walks the
    // eight bits forward exactly once, so a marker either fits where we are,
    // advances us to the lowest of its contexts above us, or is behind us and is
    // this finding. Two positional contexts (`mt#`, `cl`, `ip`) resolve by
    // lowest-above-current, which is how `\cl`'s two meanings (before the first
    // `\c`, and inside a chapter) fall out of one rule.
    //
    // ABSTAINS on rows with no positional bit — every character marker, row 0,
    // and `\cp`, whose empty mask is deliberate (`ca`/`cp`/`va`/`vp` are an
    // ADJACENCY question over tokens, which this lane says nothing about).
    //
    // Warning, and no fix: the marker is legal, its PLACE is not, and the repair
    // is a MOVE rather than a splice.
    LintRow {
        code: Code::MarkerOutOfBand,
        name: "marker-out-of-band",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} belongs earlier in the book than this",
        formatter: false,
        fix_label: None,
    },
    // A second `\id` line. The FIRST is the book's identification — `second`
    // points at it — and what the second one means is a human question, so no
    // fix. RULED (Will): never legal anywhere.
    LintRow {
        code: Code::DuplicateId,
        name: "duplicate-id",
        category: Category::Payload,
        severity: Some(Severity::Error),
        escalation: &[],
        aux: AuxKind::None,
        template: "this book already has an \\id line",
        formatter: false,
        fix_label: None,
    },
    // A BODY or POETRY paragraph at book level in header/intro territory —
    // the rails put `p`/`q`-class content in ChapterContent, which only `\c`
    // opens. Section paragraphs (`\ms` before `\c 1`) stay silent: live corpus
    // practice, spec-ambiguous. One finding, then the band advances — no
    // cascade onto the paragraphs that follow.
    LintRow {
        code: Code::ParagraphBeforeFirstChapter,
        name: "paragraph-before-first-chapter",
        category: Category::Structure,
        severity: Some(Severity::Warning),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} body paragraph before the book's first \\c",
        formatter: false,
        fix_label: None,
    },
    // A Pad token: delimiter whitespace past the one byte the chrome keeps.
    // The bytes RENDER (no paint stands in for them), so the finding is what
    // explains the ragged look; the fix deletes exactly the surplus. HINT —
    // every delimiter pattern is `HS`+, so several spaces are VALID, only
    // reducible.
    LintRow {
        code: Code::DelimiterSurplus,
        name: "delimiter-surplus",
        category: Category::Form,
        severity: Some(Severity::Hint),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} reducible whitespace after a delimiter",
        formatter: true,
        fix_label: Some("remove the extra whitespace"),
    },
    // ---- The Form channel -------------------------------------------------
    // Eleven rows nobody is ever shown. Their templates exist so a formatter UI
    // can name what it changed ("42 line endings normalized"), never as a
    // diagnostic message.
    //
    // Wholesale removal of a named marker, driven by `FormatOptions`: the row is
    // the rule IDENTITY, its finding set is the caller's list. An empty list
    // (the default) yields nothing at all.
    LintRow {
        code: Code::RemoveMarker,
        name: "remove-marker",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} was removed on request",
        formatter: false,
        fix_label: Some("remove the marker"),
    },
    // A run of EMPTY `\v` markers bridged into the verse where text finally
    // appears. Opt-in: the run's meaning is an editorial fact.
    LintRow {
        code: Code::BridgeEmptyVerses,
        name: "bridge-empty-verses",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} opens a run of empty verses",
        formatter: false,
        fix_label: Some("bridge the empty verses"),
    },
    // `\v 2 2 men went` — the uW-era artifact where the verse number is written
    // twice. Opt-in, and NUMBER-BOUNDED: `\v 2 2000 men` is prose.
    LintRow {
        code: Code::DedupeVerseNumber,
        name: "dedupe-verse-number",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "the text after {anchor} repeats the verse number",
        formatter: false,
        fix_label: Some("delete the repeated verse number"),
    },
    // Every BLOCK-LIKE marker starts its own line, block-likeness read off the
    // marker table's `MarkerKind` and nothing else. `\v` is the configurable
    // citizen (`verse_breaks`), which is also where the verse breaks are REMOVED.
    LintRow {
        code: Code::BlockMarkerOwnLine,
        name: "block-marker-own-line",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} belongs at the start of a line",
        formatter: false,
        fix_label: Some("start a line here"),
    },
    // The aligned-corpus shape: one `\w` per line. Opt-in via
    // `char_marker_breaks`, because the author's breaks are the author's.
    LintRow {
        code: Code::CharMarkerLineJoin,
        name: "char-marker-line-join",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "this line break sits on a character-marker boundary",
        formatter: false,
        fix_label: Some("join the line"),
    },
    // A vertical run collapses to ONE newline — the LAST of the run, so the
    // break that survives is the one touching what follows it.
    LintRow {
        code: Code::CollapseBlankLines,
        name: "collapse-blank-lines",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "blank lines",
        formatter: false,
        fix_label: Some("collapse the blank lines"),
    },
    // Every EXISTING ending rewrites to the configured form. Mixed endings are
    // exactly the "form" this feature exists for.
    LintRow {
        code: Code::NormalizeNewlines,
        name: "normalize-newlines",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "this line ending is not the configured form",
        formatter: false,
        fix_label: Some("normalize the line ending"),
    },
    // The EDGES of a text run only. The interior is content: `In  the
    // beginning` keeps its double space, and NBSP is never structural.
    LintRow {
        code: Code::TrimTextEdges,
        name: "trim-text-edges",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "whitespace at the edge of a text run",
        formatter: false,
        fix_label: Some("collapse the edge whitespace"),
    },
    // The marker's own delimiter — folded into its span by the scanner — reduces
    // to one space, or to nothing when the line ends right after it.
    LintRow {
        code: Code::DelimiterSingle,
        name: "delimiter-single",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is followed by more than one space",
        formatter: false,
        fix_label: Some("reduce the delimiter to one space"),
    },
    // The same fold, on the three PAYLOAD spans that carry one: a designator, a
    // note caller, a book code.
    LintRow {
        code: Code::DesignatorWsSingle,
        name: "designator-ws-single",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is followed by more than one space",
        formatter: false,
        fix_label: Some("reduce the payload delimiter to one space"),
    },
    // No indentation in front of a line-leading marker.
    LintRow {
        code: Code::MarkerWsAtLineStart,
        name: "marker-ws-at-line-start",
        category: Category::Form,
        severity: Some(Severity::Form),
        escalation: &[],
        aux: AuxKind::None,
        template: "{anchor} is indented",
        formatter: false,
        fix_label: Some("delete the indentation"),
    },
];

// ---------------------------------------------------------------------------
// The Version family's authored data
// ---------------------------------------------------------------------------

/// Per-marker version facts lint owns, because no `MarkerRow` column does:
/// `deprecated` is a BOOL, and "deprecated SINCE WHAT" is the fact the rule
/// needs. Keyed by ROW NAME, like the books table — `generated::name` hands the
/// canonical name back, and five string compares beat any index scheme.
pub struct VersionRow {
    pub marker: &'static str,
    /// The first version that says "don't".
    pub deprecated_in: UsfmVersion,
    /// The marker to rename to, when the spec's replacement is a RENAME. `None`
    /// where it is a restructure: a fix is a mechanical splice or nothing.
    pub replacement: Option<&'static str>,
}

/// The five markers `tables::rows` marks `deprecated: true`. All five date from
/// 3.0 (`ph` carries the page evidence: `para/paragraphs/ph.html`,
/// "Deprecated: 3.0"); nothing in 3.1/3.2 re-dates them.
///
/// No `removed_in` column: 3.2 still documents all five, so the column would be
/// five `None`s. `deprecated-marker`'s Error rung carries "this era expects it
/// gone" instead. Add the column with the first marker the spec removes.
pub const VERSION_ROWS: [VersionRow; 5] = [
    // char/addpn.html — the Chinese "added proper name". Its replacement is
    // `\add` WRAPPING `\pn`: two markers where there was one, so no rename.
    VersionRow {
        marker: "addpn",
        deprecated_in: UsfmVersion::V3_0,
        replacement: None,
    },
    // char/fdc.html — footnote text for a deuterocanonical edition only, now
    // handled by publishing a different footnote rather than by any marker.
    VersionRow {
        marker: "fdc",
        deprecated_in: UsfmVersion::V3_0,
        replacement: None,
    },
    // para/paragraphs/ph.html names `\li#` as the form to use, and the level
    // digit carries across (`ph` caps at 3, `li` at 4): `\ph2` → `\li2` is a
    // pure rename of two name bytes.
    VersionRow {
        marker: "ph",
        deprecated_in: UsfmVersion::V3_0,
        replacement: Some("li"),
    },
    // char/pro.html — the Chinese pronunciation gloss, replaced by ruby markup
    // `\rb`, whose only attribute (`gloss`) is Optional: `\pro x\pro*` →
    // `\rb x\rb*` is legal on its own, and `|gloss="…"` is the author's to add.
    VersionRow {
        marker: "pro",
        deprecated_in: UsfmVersion::V3_0,
        replacement: Some("rb"),
    },
    // char/xdc.html — `\fdc`'s cross-reference twin, same restructure.
    VersionRow {
        marker: "xdc",
        deprecated_in: UsfmVersion::V3_0,
        replacement: None,
    },
];

/// The version facts for a marker row's canonical name. A linear scan over five
/// `&'static str`s, guarded by `MarkerRow::deprecated` (false for 148 of 153
/// rows): the BOOL says whether to look, this table says what the answer is.
pub fn version_row(marker: &str) -> Option<&'static VersionRow> {
    VERSION_ROWS.iter().find(|row| row.marker == marker)
}

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
            // An out-of-order rung makes `severity_at` read the wrong severity.
            for pair in row.escalation.windows(2) {
                assert!(pair[0].0 < pair[1].0, "{}'s ladder is unsorted", row.name);
            }
            // A gated row with no ladder is a deleted rule, not a gated one.
            assert!(
                row.severity.is_some() || !row.escalation.is_empty(),
                "{} is silent at every version",
                row.name
            );
        }
        // Every family has at least one rule.
        for family in [
            Category::Structure,
            Category::Ordering,
            Category::Attributes,
            Category::Payload,
            Category::Form,
            Category::Version,
        ] {
            assert!(
                LINT_ROWS.iter().any(|row| row.category == family),
                "{family:?} has no rule"
            );
        }
    }

    /// A `{anchor}` is a TOKEN span, and a marker token's first byte IS its
    /// backslash — so a template that writes one in front of the placeholder
    /// renders `\\ts-s milestone is missing its \*`. Every such template is
    /// pinned here rather than rediscovered one screenshot at a time.
    #[test]
    fn no_template_prefixes_a_placeholder_with_a_backslash() {
        for row in LINT_ROWS {
            for placeholder in ["{anchor}", "{second}"] {
                let mut at = 0;
                while let Some(found) = row.template[at..].find(placeholder) {
                    let start = at + found;
                    assert!(
                        !row.template[..start].ends_with('\\')
                            && !row.template[..start].ends_with("\\+"),
                        "{}: {placeholder} already carries the marker's backslash",
                        row.name
                    );
                    at = start + placeholder.len();
                }
            }
        }
    }

    /// The escalation slice's contract: below the first rung a GATED code says
    /// nothing at all, and each rung takes over at its own version.
    #[test]
    fn the_trailing_form_ladder_is_silent_then_warning_then_error() {
        let row = Code::AttrTrailingFormDeprecated.row();
        assert_eq!(row.severity_at(None), None);
        assert_eq!(row.severity_at(Some(UsfmVersion::V3_0)), None);
        assert_eq!(
            row.severity_at(Some(UsfmVersion::V3_2)),
            Some(Severity::Warning)
        );
        assert_eq!(
            row.severity_at(Some(UsfmVersion::V4_0)),
            Some(Severity::Error)
        );

        // A FLAT row ignores the declaration entirely…
        let flat = Code::UnclosedNote.row();
        for version in [None, Some(UsfmVersion::V3_0), Some(UsfmVersion::V4_0)] {
            assert_eq!(flat.severity_at(version), Some(Severity::Error));
        }
        // …and an ungated one-rung row keeps its base until the rung.
        let container = Code::UnterminatedContainer.row();
        assert_eq!(container.severity_at(None), Some(Severity::Warning));
        assert_eq!(
            container.severity_at(Some(UsfmVersion::V3_2)),
            Some(Severity::Warning)
        );
        assert_eq!(
            container.severity_at(Some(UsfmVersion::V4_0)),
            Some(Severity::Error)
        );
    }

    /// The Version table and `MarkerRow::deprecated` name the same five markers:
    /// the bool is the filter the rule reads first, so a row in one and not the
    /// other is a rule that cannot fire (or a lookup that misses).
    #[test]
    fn the_version_table_matches_the_deprecated_column() {
        use crate::tables::generated;

        let mut from_table: Vec<&str> = (0..generated::ROW_COUNT)
            .map(|idx| idx as generated::MarkerIdx)
            .filter(|idx| generated::deprecated(*idx))
            .map(generated::name)
            .collect();
        from_table.sort_unstable();
        let mut authored: Vec<&str> = VERSION_ROWS.iter().map(|row| row.marker).collect();
        authored.sort_unstable();
        assert_eq!(from_table, authored);
        assert_eq!(authored, ["addpn", "fdc", "ph", "pro", "xdc"]);

        // A replacement must be a real marker row (the fix writes its name).
        for row in &VERSION_ROWS {
            if let Some(replacement) = row.replacement {
                assert!(
                    generated::marker_idx(
                        replacement.as_bytes(),
                        crate::tables::schema::SpellingShape::PlainOnly
                    ) != generated::UNRESOLVED,
                    "{} names an unknown replacement",
                    row.marker
                );
            }
        }
    }
}
