//! Row types the authored marker table (`tables::rows`) is written in.
//!
//! - **Referee: USFM 3.2 only** (<https://docs.usfm.bible/usfm/3.2/>). No
//!   version column; `deprecated`/[`AttrStatus::Deprecated`] record 3.2's own
//!   judgement. The 3.1 usx.rng is fallback evidence and never outvotes a
//!   3.2 page; markers only it knows do not exist.
//! - **Named fields only.** Packing is codegen OUTPUT; derived facts are
//!   `const fn`s (never columns), so codegen bakes them and nothing is kept
//!   in sync by hand.
//! - **Rows are keyed by (canonical name, [`SpellingShape`])** — strip
//!   `-s`/`-e` first, then digits (`qt3-s` → `qt`); longest name is 6 bytes,
//!   which is what makes the u64 name load work.
//! - **`\z` extensions and unknown markers are never rows**: they resolve to
//!   index 0, the inert empty row (opens/closes/contributes nothing).
//!   Extension definitions arrive as caller CONFIG, never from the table.

/// Which SPELLINGS of a name a row claims — the second half of the row key.
///
/// The lookup key is (canonical name, shape). Nearly every row is [`Self::Any`];
/// the axis exists for names the spec overloads across the plain and milestone
/// spellings with different facts. `qt` is the only such name today:
/// `\qt …\qt*` is a character marker with no attributes, while `\qt3-s` is a
/// milestone carrying `who`/`sid` — different kind, category, contexts, closing,
/// and delimiter rule, so they cannot share a row.
///
/// Read by: codegen (emits the shape compare only for names that actually
/// collide), the scanner (supplies the shape it already classified).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpellingShape {
    /// Matches the name in any spelling, plain or `-s`/`-e` suffixed. The
    /// normal case.
    Any,
    /// Matches ONLY the bare spelling — `\qt …\qt*`.
    PlainOnly,
    /// Matches ONLY the `-s`/`-e` suffixed spellings — `\qt-s`, `\qt3-e`.
    MilestoneOnly,
}

impl SpellingShape {
    /// Do two rows with the same name claim any spelling in common? Asserted by
    /// the table tests: overlapping rows would make the lookup ambiguous.
    pub const fn overlaps(self, other: Self) -> bool {
        matches!(self, Self::Any) || matches!(other, Self::Any) || (self as u8 == other as u8)
    }
}

/// Marker kind at the DEFINITION level — the spec's coarse taxonomy.
///
/// Definition level means no start/end split: a `qt` milestone is one
/// [`MarkerKind::Milestone`] whichever side the occurrence spells, and `esb` /
/// `esbe` are both [`MarkerKind::Sidebar`]. The side is a property of the
/// occurrence's spelling, read off the token span, never stored here [G].
///
/// Coarse taxonomy only — the behavior-bearing distinctions live in
/// [`Category`]. Read by: scanner (ws fold + open-marker stack), lint (context
/// checks), codegen (packing, USJ type projection), export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MarkerKind {
    /// The generic EMPTY row at index 0 — an unconfigured `\z` extension, an
    /// unknown name, an illegal spelling. Added 2026-08-12 when codegen landed:
    /// the schema always said index 0 is a ROW and not a sentinel, and without
    /// this variant every generated accessor would have had to return an
    /// `Option` purely to describe row 0. Pairs only with [`Category::Unknown`].
    Unknown,
    Paragraph,
    Character,
    Note,
    Chapter,
    Verse,
    Milestone,
    Figure,
    Sidebar,
    Periph,
    Meta,
    TableRow,
    TableCell,
    Header,
}

/// The spec's own FINE category, one flat enum [C].
///
/// The spec's own two-level taxonomy ([`MarkerKind`] × [`Category`]) —
/// it reads like the documentation it came from.
///
/// **Category is load-bearing for behavior, not decoration.** The canonical
/// example: `\pb` is a *character* marker of category [`Category::CharBreaks`],
/// and therefore opens NO scope — where every other character marker opens a
/// [`ScopeKind::Character`]. Reading behavior off `kind` alone would get `\pb`
/// wrong, which is exactly how onion got it wrong (it files `\pb` as a
/// Paragraph, so an incoming `\pb` closes the paragraph it sits inside).
///
/// Variants are PREFIXED by their kind group on purpose: the spec reuses group
/// names across kinds (`Poetry` and `Lists` and `Tables` appear under both
/// Paragraphs and Characters) and `ParaPoetry` (`\q`) is not remotely the same
/// thing as `CharPoetry` (`\qs`). The prefix also makes the kind↔category
/// coherence rule readable at the call site.
///
/// Coherence is asserted by [`Category::is_coherent_with`], which
/// `debug_assert!`s per row.
///
/// Read by: lint (legality rules key off the fine category), scanner (via
/// [`MarkerRow::opens_scope`], authored from it), export, codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// The empty row's category — see [`MarkerKind::Unknown`]. No spec group.
    Unknown,

    // ---- Paragraphs (docs.usfm.bible: Paragraphs > …) --------------------
    /// Paragraphs > Identification — `ide`, `h#`, `toc#`, `toca#`, `rem`, `sts`.
    ParaIdentification,
    /// Paragraphs > Introductions — `imt#`, `is#`, `ip*`, `im*`, `iq#`, `io#`, ….
    ParaIntroductions,
    /// Paragraphs > Titles and Sections — `mt#`, `mte#`, `ms#`, `mr`, `s#`,
    /// `sr`, `r`, `d`, `sp`, `sd#`, `cd`, `cl`.
    ParaTitlesSections,
    /// Paragraphs > Body Paragraphs — `p`, `m`, `po`, `pi#`, `mi#`, `nb`, `b`, ….
    ParaBody,
    /// Paragraphs > Poetry — `q#`, `qr`, `qc`, `qa`, `qm#`, `qd`.
    ParaPoetry,
    /// Paragraphs > Lists — `lh`, `li#`, `lf`, `lim#`.
    ParaLists,
    /// Paragraphs > Tables — the row marker `\tr`.
    ParaTables,
    /// Peripheral paragraphs — `p1`, `p2`. USFM 3.1.1 moved these out of the
    /// ordinary paragraph group ("not used in scripture files"); the USX grammar
    /// calls the group `PeriphPara.para.style.enum` (tcdocs/usx.rng line 1062).
    /// Bare `\p` stays [`Self::ParaBody`].
    ParaPeripheral,

    // ---- Characters (docs.usfm.bible: Characters > …) --------------------
    /// Characters > Special Text + Special Features — `add`, `nd`, `wj`, `w`,
    /// `jmp`, `ref`, `rb`, `pn`, `qt`(char), ….
    CharTextFeatures,
    /// Characters > Character Styling — `bd`, `it`, `bdit`, `em`, `no`, `sc`, `sup`.
    CharFormatting,
    /// Characters > Breaks — `pb`. **Opens no scope**; see the type-level note.
    CharBreaks,
    /// Characters > Introduction Characters — `ior`, `iqt`.
    CharIntroductions,
    /// Characters > Poetry Characters — `qac`, `qs`.
    CharPoetry,
    /// Characters > List Characters — `lik`, `litl`, `liv#`.
    CharLists,
    /// Table cell content markers — `th#`, `thr#`, `thc#`, `tc#`, `tcr#`, `tcc#`.
    CharTables,
    /// Note-internal character markers — `fr`, `ft`, `fq`, `xo`, `xt`, ….
    CharNotes,

    // ---- Notes ----------------------------------------------------------
    /// Footnote containers — `f`, `fe`, `ef`.
    NoteFootnote,
    /// Cross-reference containers — `x`, `ex`.
    NoteCrossReference,

    // ---- Milestones -----------------------------------------------------
    /// The `list` structure milestone.
    MilestoneList,
    /// The `table` structure milestone (and onion's `t-s`/`t-e` spelling).
    MilestoneTable,
    /// Quotation milestones — `qt#-s` / `qt#-e`.
    MilestoneQt,
    /// Translator-section milestones — `ts`, `ts-s`, `ts-e`.
    MilestoneTs,
    /// Verse-id milestones — `vid`. Standalone, not paired
    /// (<https://docs.usfm.bible/usfm/3.2/ms/vid.html>).
    MilestoneVid,

    // ---- Chapters and Verses --------------------------------------------
    /// Chapters and Verses — `c`, `ca`, `cp`, `v`, `va`, `vp`. One spec group
    /// spanning four kinds, so it is one category spanning four kinds.
    ChapterVerse,

    // ---- One-of-a-kind groups -------------------------------------------
    /// Sidebars — `esb`, `esbe`.
    Sidebar,
    /// Sidebars > `cat` (note/sidebar category metadata).
    Meta,
    /// Peripherals — `periph`.
    Peripheral,
    /// Document Structure — `id`, `usfm`.
    DocumentStructure,
    /// Characters > Special Features > `fig`, which the spec files under
    /// Characters but which carries its own [`MarkerKind::Figure`].
    Figure,
}

impl Category {
    /// Is this category legal for `kind`? `debug_assert!`ed per row so a
    /// mis-paired row fails loudly in tests rather than drifting.
    ///
    /// A few categories legitimately span kinds — [`Category::ChapterVerse`] is
    /// one spec group covering `c` (Chapter), `v` (Verse), `ca`/`va`/`vp`
    /// (Character), and `cp` (Paragraph) — so this is a relation, not a
    /// function.
    pub const fn is_coherent_with(self, kind: MarkerKind) -> bool {
        use Category as C;
        use MarkerKind as K;
        match kind {
            K::Unknown => matches!(self, C::Unknown),
            K::Paragraph => matches!(
                self,
                C::ParaIdentification
                    | C::ParaIntroductions
                    | C::ParaTitlesSections
                    | C::ParaBody
                    | C::ParaPoetry
                    | C::ParaLists
                    | C::ParaTables
                    | C::ParaPeripheral
                    | C::ChapterVerse
            ),
            K::Character => matches!(
                self,
                C::CharTextFeatures
                    | C::CharFormatting
                    | C::CharBreaks
                    | C::CharIntroductions
                    | C::CharPoetry
                    | C::CharLists
                    | C::CharNotes
                    | C::ChapterVerse
            ),
            K::Note => matches!(self, C::NoteFootnote | C::NoteCrossReference),
            K::Milestone => matches!(
                self,
                C::MilestoneList
                    | C::MilestoneTable
                    | C::MilestoneQt
                    | C::MilestoneTs
                    | C::MilestoneVid
            ),
            K::Chapter | K::Verse => matches!(self, C::ChapterVerse),
            K::Figure => matches!(self, C::Figure),
            K::Sidebar => matches!(self, C::Sidebar),
            K::Periph => matches!(self, C::Peripheral),
            K::Meta => matches!(self, C::Meta),
            K::TableRow => matches!(self, C::ParaTables),
            K::TableCell => matches!(self, C::CharTables),
            K::Header => matches!(self, C::DocumentStructure),
        }
    }
}

/// The kinds of scope the context machine can have open. Onion's
/// `StructuralScopeKind`, unchanged.
///
/// Distinct from [`MarkerKind`], which says what the SPEC calls a marker;
/// this says what the MACHINE does with it. They diverge wherever behavior
/// and taxonomy disagree (`\pb` is a Character that opens nothing; `\fig` is a
/// Figure that behaves as a Character scope).
///
/// Read by: the walker (frame kind on push, pop-barrier and close-matching
/// checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    /// Reserved for the unresolved/custom marker (index 0): unwind, push
    /// nothing. Unknown/illegal markers: pop all, start fresh.
    Unknown,
    Header,
    Para,
    Note,
    Character,
    Milestone,
    // No `Chapter`/`Verse`: `\c`/`\v` are POINTS (Q16) — no frame, and the
    // walker needs no rank. Deleted 2026-08-12 with zero users.
    /// No marker opens a table — the WALKER synthesizes this frame when `\tr`
    /// arrives with no table already open (`\tr` needs it to know whether to
    /// start one or join one). A peer of [`Self::Para`]. Unreachable from any
    /// row, deliberately — same shape as `HtmlElement::Table`.
    Table,
    TableRow,
    TableCell,
    Sidebar,
    Periph,
    // No `Meta`: `\cat` is char-shaped and opens a `Character` scope. A scope
    // kind nothing can push is dead weight.
}

/// Structural-whitespace requirement immediately AFTER the marker name —
/// ported from onion's `whitespace::StructuralWhitespaceRequirement`, keeping
/// the verbose spec-mapped variant names on purpose (the reader should not have
/// to look up `HS` vs `Hs` vs `WS`). Patterns: tcdocs/def.txt.
///
/// Read by: scanner (this column IS the per-class delimiter-space fold rule —
/// fold when the value is `TagEndDelimiter` | `AtLeastOneHorizontalWhitespace` |
/// `AtLeastOneWhitespace`), lint (missing / illegal delimiter findings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralWhitespaceRequirement {
    /// At least one horizontal whitespace character (space or tab) is required.
    /// Spec: `HS` = `/${hs}+/`.
    AtLeastOneHorizontalWhitespace,
    /// Zero or more horizontal whitespace characters are allowed.
    /// Spec: `Hs` = `/${hs}*/`.
    OptionalHorizontalWhitespace,
    /// At least one whitespace character (horizontal or newline) is required.
    /// Spec: `WS` = `/${anyws}+/`.
    AtLeastOneWhitespace,
    /// Zero or more whitespace characters are allowed. Spec: `Ws` = `/${anyws}*/`.
    OptionalWhitespace,
    /// Exactly one newline (CR, LF, or CRLF) is required.
    /// Spec: `nl` = `/(?:\u{000D}?\u{000A}|\u{000D})/`.
    SingleNewline,
    /// At least one newline is required. Spec: `NL` = `/${nl}+/`.
    AtLeastOneNewline,
    /// "Tag end": whitespace OR end-of-input OR start-of-attributes. Delimits
    /// an open marker's name from what follows.
    /// Spec: `TAGEND` = `/(?:${ws}+|(?=[\\|]|$))/`.
    TagEndDelimiter,
    /// No structural whitespace is required at this position.
    NotRequired,
}

/// The argument a marker's opening form consumes immediately after its
/// delimiter, before any content [E] [K].
///
/// Read by: scanner (drives the pending-payload mode and the payload token
/// kinds — NEXT-STEPS step 4), interpreters (which grammar to run over the
/// payload span), export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// Nothing special follows the delimiter; content is ordinary text.
    None,
    /// `\id` — a book-code slice. Spec `TLC` = `/[0-9A-Z]{3}/`, but the slice
    /// is kept verbatim and never validated, truncated, or repaired: an
    /// out-of-list or overlong code is revision-state DATA (GLOSSARY "Book
    /// code").
    BookCode,
    /// `\c`, `\cp`, `\ca`, `\v`, `\vp`, `\va` — a chapter/verse designator [E].
    ///
    /// Spec `VERSE` pattern:
    /// `/[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*/` — a leading
    /// number, optional letter/mark suffix (`1a`), then any number of
    /// range/sequence parts joined by `-` or `,`, optionally preceded by
    /// U+200F RLM.
    ///
    /// **That grammar belongs to the verse-designator INTERPRETER, not here.**
    /// The scanner emits ONE payload token spanning the designator and never
    /// looks inside it; the fast path only ever recognizes the pure-digit happy
    /// shape and falls back to the general path for suffixes, ranges,
    /// sequences, RLM, and junk.
    Designator,
    /// `\usfm` — a version string, spec shape `\d+\.\d+(\.\d+)?` [E]. Same
    /// discipline as `Designator`: one span, interpreted on demand.
    Version,
    /// `\f`, `\fe`, `\ef`, `\x`, `\ex` — the note caller [K]: `+` (auto), `-`
    /// (no caller), `?`, or a custom caller string, consumed after the
    /// delimiter and before the note's content.
    ///
    /// **Spec pattern, from the Footnote railroad** (3.2 note/index.html, read
    /// 2026-08-12): the caller is `/[^\\\s]+/` — one or more characters that are
    /// neither a backslash nor whitespace. So it is not an enumeration of
    /// `+`/`-`/`?` plus a special case; those are just the conventional values of
    /// one general run, and the interpreter never has to enumerate them.
    ///
    /// The same production shows what FOLLOWS the caller: an optional `category`
    /// (`\cat …\cat*`) BEFORE any content. So `\cat` is positionally constrained
    /// inside a note — `\ef - \cat People\cat*\fr 1.2-6a: …` — which is a lint
    /// rule, not something the caller payload expresses.
    ///
    /// Same pending-payload machinery as `Designator`; it becomes the 10th
    /// `TokenKind` when step 4 lands (kind_bits has room after the nested-bit
    /// slide). The table's job here is only to record the FACT that these
    /// kinds consume a caller.
    NoteCaller,
}

/// Legal numeric suffix range for a marker's spelling (`q` accepts `q1..q4`).
///
/// The number itself is NEVER stored — it lives in the token's span. This
/// column only says which digit runs are legal, so the matcher can validate
/// after stripping.
///
/// **Spec-wide rule [D]: the BARE form is ALWAYS a valid spelling.** `\s` is as
/// legal as `\s1`. The spec's "use the bare form only when a single level
/// exists in the text" is a document-consistency rule and therefore LINT's
/// business — the matcher must accept bare and numbered alike, always.
///
/// Packing (codegen's job, not authored) fits 4 bits: `Unnumbered` = 0,
/// `UpTo(n)` = n for `1..=13`, `Unbounded` = 14, `TableColumns` = 15. Read by:
/// codegen (name→idx validation), lint (out-of-range level findings), export
/// (level derivation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Numbering {
    /// No digit suffix is legal; `\p1` is not a spelling of `\p`.
    Unnumbered,
    /// Digits `1..=N` are legal, and so is the bare form. N must be `1..=13`
    /// to survive packing.
    UpTo(u8),
    /// Any digit run is legal — the marker is numbered but 3.2 states no cap.
    /// `liv` is the only such row: its page shows `\liv1` in the examples and
    /// never bounds `#` (<https://docs.usfm.bible/usfm/3.2/char/lists/liv.html>).
    Unbounded,
    /// Table cell markers [O]: `\tc1`, `\tc1-2` — a column number or a column
    /// SPAN. The matcher matches only the alpha stem (`tc`, `thr`, …) and
    /// hands everything after it to that marker's payload interpreter over the
    /// span; `Numbering` deliberately does not model spans, and no cap is
    /// asserted here.
    ///
    /// This is a distinct variant rather than `Unbounded` because it tells the
    /// matcher something different: not "any digit run", but "stop at the stem,
    /// the rest is payload".
    TableColumns,
}

/// The 18 spec contexts a marker may legally appear in.
///
/// AUTHORED FORM is a `&'static [SpecContext]` slice on the row. The 18-bit
/// mask is CODEGEN output computed from this slice — authored rows never
/// carry a mask, and the mask is EXACTLY the slice: codegen never invents
/// facts (the "effective context" promotion pass was deleted 2026-08-12 for
/// contradicting `fm`'s own page; a test pins the equality).
///
/// Read by: lint, and the walker's recovery predicate (ruling [I]: the row's
/// mask AND-ed against the top frame's stamped context). Codegen emits the mask.
///
/// The enum spans TWO AXES; [`SpecContext::is_positional`] makes every
/// variant answer which one it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpecContext {
    Scripture,
    BookIdentification,
    BookHeaders,
    BookTitles,
    BookIntroduction,
    BookIntroductionEndTitles,
    BookChapterLabel,
    ChapterContent,
    Peripheral,
    PeripheralContent,
    PeripheralDivision,
    Section,
    Para,
    List,
    Table,
    Sidebar,
    Footnote,
    CrossReference,
}

impl SpecContext {
    /// The axis this variant lives on. POSITIONAL contexts are the document
    /// sequence (`Scripture` → … → `ChapterContent`): monotonic, established
    /// by position in the book, and the only band any "advance" rule may
    /// apply to — the document moves forward through them exactly once.
    /// Everything else REPEATS FREELY: scope-established sub-groups of
    /// chapter/peripheral content (`Section`, `Para`, `Footnote`, …).
    /// "Right after `\c`" is deliberately NOT a context (chapters repeat, so
    /// no monotonic encoding works): `ca`/`cp`/`va`/`vp` are checked by an
    /// adjacency lint rule instead, and their rows carry an empty context
    /// slice.
    ///
    /// Exhaustive on purpose — a new variant must declare its axis here
    /// before it compiles.
    pub const fn is_positional(self) -> bool {
        use SpecContext as S;
        match self {
            S::Scripture
            | S::BookIdentification
            | S::BookHeaders
            | S::BookTitles
            | S::BookIntroduction
            | S::BookIntroductionEndTitles
            | S::BookChapterLabel
            | S::ChapterContent => true,
            S::Peripheral
            | S::PeripheralContent
            | S::PeripheralDivision
            | S::Section
            | S::Para
            | S::List
            | S::Table
            | S::Sidebar
            | S::Footnote
            | S::CrossReference => false,
        }
    }
}

/// What context an OPEN scope of this (kind, category) puts its children in —
/// **derived, not a column** (ruling [C] removed the authored
/// `contributes_context` field; ruling [I] still needs the value).
///
/// `None` means the frame is TRANSPARENT: it contributes no context of its own,
/// so a child's legality is judged against whatever frame is below it. Per [I]
/// the walker stamps the resolved value onto each frame at push time
/// (`frame.ctx = contributes_context(row).unwrap_or(parent.ctx)`), so no
/// consumer ever walks the stack.
///
/// Read by: the walker (frame stamping), lint (legality), codegen (bakes this
/// into the packed row so it costs nothing at runtime).
pub const fn contributes_context(kind: MarkerKind, category: Category) -> Option<SpecContext> {
    use Category as C;
    use MarkerKind as K;
    use SpecContext as S;
    match kind {
        // Paragraph frames always contribute; which context depends on the
        // fine category. NOTE: onion lumped every non-inline paragraph to
        // ChapterContent; this is finer and spec-shaped. See the flag list in
        // tables/unaudited.rs.
        // `cp` is the only Paragraph in the ChapterVerse group, and it opens no
        // scope — it is a published chapter LABEL, a payload and nothing else
        // (Will, 2026-08-12). So it contributes nothing: a value here would be
        // dead, since `contributes_context` is only ever read at push time.
        K::Paragraph if matches!(category, C::ChapterVerse) => None,
        K::Paragraph => Some(match category {
            C::ParaBody | C::ParaPoetry => S::Para,
            C::ParaLists => S::List,
            C::ParaTitlesSections => S::Section,
            C::ParaIntroductions => S::BookIntroduction,
            C::ParaIdentification => S::BookHeaders,
            C::ParaPeripheral => S::PeripheralContent,
            C::ParaTables => S::Table,
            _ => S::ChapterContent,
        }),
        K::Note => Some(match category {
            C::NoteCrossReference => S::CrossReference,
            _ => S::Footnote,
        }),
        K::Chapter => Some(S::ChapterContent),
        K::Periph => Some(S::PeripheralContent),
        K::Sidebar => Some(S::Sidebar),
        K::Header => Some(S::Scripture),
        K::TableRow | K::TableCell => Some(S::Table),
        // Transparent: an open `\nd`, `\ft`, `\zaln-s`, `\v`, or `\fig` does
        // not change what is legal inside it — the enclosing block or note
        // still decides.
        // `\cat` is Meta by TAXONOMY but char-shaped in behavior, so it is
        // transparent like any character marker (Will, 2026-08-12).
        K::Character | K::Milestone | K::Verse | K::Figure | K::Meta => None,
        // The empty row contributes nothing, which is also the recovery
        // walker's cue to unwind (pop-all recovery).
        K::Unknown => None,
    }
}

/// Markers whose paragraph may not contain `\v` — a rule 3.2 states that no
/// column here can express.
///
/// **`\v` is not allowed in a paragraph of rail category `otherpara` or
/// `sectionpara`** (USFM 3.2 revisions). This is keyed on the ENCLOSING
/// PARAGRAPH, not on `\v`'s own [`SpecContext`] set — so the 20-bit context mask
/// cannot express it, and no per-row value on `\v` can either: the same `\v` is
/// legal or illegal depending on what is open above it.
///
/// It is a LINT rule, which fits the [L] constraint: the walker's stack has the
/// fact, and no new frame field is needed — a frame already carries the
/// `marker_idx` that opened it, so the enclosing paragraph is one array read.
///
/// **Why a MARKER list and not a `Category` list** (resolved round 6 / [D]): the
/// two rail groups do not align with [`Category`] at all. Extracted from
/// tcdocs/usx.rng:
///
/// - `OtherPara.para.style.enum` (line 1151): `lit`, `cp`, `pb`, `qa`, `sts`,
///   `rem` — which spans FIVE of our categories (ParaBody, ChapterVerse,
///   CharBreaks, ParaPoetry, ParaIdentification).
/// - `SectionPara.para.style.enum` (line 892): `iex`, `ip`, `ms#`, `ms`, `mr`,
///   `mte#`, `mte`, `r`, `s#`, `sr`, `sp`, `sd#`, `sd`, `cl`, `cd` — which
///   includes `iex`/`ip` (we file them ParaIntroductions) while OMITTING `mt`
///   and `d` that our ParaTitlesSections holds.
///
/// So the rail groups are a different partition of the same markers, not a
/// coarsening of ours. A marker-level set is the only honest encoding; codegen
/// turns it into a bitmask over row indices.
///
/// Members are canonical names, so `s` covers `s1`..`s4`. **Every member has a
/// row.** The rails' `k1`, `k2` (usx.rng:1161, 1163) and `restore`
/// (usx.rng:894) are NOT listed: 3.2 documents no such markers, so they do
/// not exist, and a rule about markers that do not exist is cruft.
pub const V_FORBIDDEN_IN_PARAGRAPHS: &[&str] = &[
    // OtherPara (usx.rng:1151).
    "lit", "cp", "pb", "qa", "sts", "rem", // SectionPara (usx.rng:892).
    "iex", "ip", "ms", "mr", "mte", "r", "s", "sr", "sp", "sd", "cl", "cd",
];

/// Lexical facts about CONTENT that the scanner owns and this table only
/// records, so the two cannot drift.
///
/// **Explicit Unicode escapes (U25004, approved for 3.2), ruled round 6 / [A]:
/// `\uXXXX` and `\UXXXXXXXX` are TEXT, folded into the text run by the text
/// arm's escape peek.** Fixed width — exactly 4 or exactly 8 uppercase hex
/// digits — no terminator. No new `TokenKind`. Two consequences recorded here
/// because nothing else would hold them:
///
/// 1. **The exact USV pattern BEATS any marker claim.** `\u0020` is content, not
///    a marker named `u0020`, even though the marker grammar would otherwise
///    accept that name. Precedence is by pattern, not by table lookup.
/// 2. **Lowercase hex is a LINT flag, not a scanner rejection.** The proposal
///    says "uppercase hexadecimal"; `\u00e9` is therefore ill-cased rather than
///    not-a-USV. Same treatment as the sibling lint fact below.
///
/// **Lint fact — "marker cased wrong".** The scanner tolerates uppercase letters
/// in marker names as DATA (they cannot resolve to a row, since every canonical
/// name is lowercase, so they land on index 0). Lint reports the casing; the
/// scanner never rejects it. This is why tightening the name scan to
/// lowercase-only was NOT adopted.
///
/// See NEXT-STEPS §5 (attributes) for the analysis and the outstanding
/// step-4 scanner work item.
pub const USV_ESCAPE_LETTERS: &[(char, usize)] = &[('u', 4), ('U', 8)];

/// How a marker's scope ends.
///
/// Read by: the walker (open-marker stack: what a `\X*` may close), lint
/// (unclosed-marker findings), export (tree building). See
/// NEXT-STEPS §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosingBehavior {
    /// Opens no scope that needs closing (paragraphs, chapter, verse, `\pb`).
    None,
    /// Requires an explicit `\X*` (character markers, note containers).
    RequiredExplicit,
    /// `\X*` is optional; the scope ends at the enclosing note's end if absent
    /// (note-internal character markers like `\ft`, `\fr`).
    OptionalExplicitUntilNoteEnd,
    /// Self-closing `\*` form (milestones).
    SelfClosingMilestone,
}

/// Per-attribute status: what the 3.2 spec says about ONE attribute of ONE
/// marker.
///
/// Ruled in (round 4) to replace a bare `&[&str]` inventory, which could say
/// only "this attribute exists". Cardinality and deprecation are both real spec
/// facts that the docs state per attribute, per marker, and both are needed by
/// lint — `\fig` without `src` is an error, `\xt|link-href="…"` is a warning.
///
/// Deliberately NOT a status: which attribute is the DEFAULT. Default-ness is
/// orthogonal to cardinality (`w`'s `lemma` is Optional AND default; `fig` has
/// three Required attributes and NO default; `xt`'s only attribute is
/// Deprecated and cannot be a default), so folding it in would double the enum
/// into `RequiredDefault`/`OptionalDefault`/… for no gain. It stays the
/// separate [`MarkerRow::default_attribute`] field.
///
/// Read by: lint (missing-required, unknown-attribute, deprecated-attribute
/// findings), interpreters (attribute list), export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrStatus {
    /// The spec requires this attribute on every occurrence of the marker.
    /// Example: `\fig`'s `src`, `size`, `ref`
    /// (<https://docs.usfm.bible/usfm/3.2/fig/fig.html>).
    Required,
    /// The spec permits this attribute. The common case.
    Optional,
    /// The spec still recognizes this attribute but tells authors not to use it.
    /// Example: `\xt`'s `link-href`, and the whole `link-` prefixed family on
    /// `\jmp`, deprecated in 3.1
    /// (<https://docs.usfm.bible/usfm/3.2/char/features/jmp.html>).
    Deprecated,
}

/// Default HTML element class for export. **Ruled in (round 6 / [C])**: the
/// column lives in the main table rather than an export-side one — the row is a
/// u128 either way, so co-location is free and the audit sees one place.
///
/// The element is a **DEFAULT, not a mandate.** The export convention that makes
/// that true is ruled: **always emit verbose data attributes** —
/// `data-marker` (the canonical name), `data-category`, and `data-usfm-type` —
/// so a consumer restyles via CSS without needing a different element, and a
/// consumer-supplied marker↔element map can override the default wholesale
/// later. Onion's export reached the same place from the other direction: it
/// emits `div`/`span` plus `data-usfm-type` and picks almost no semantic
/// elements at all (`src/html.rs::tag_and_type_for_marker`).
///
/// Packing: **u5, 32 slots** (widened from the u4/16 in the NEXT-STEPS sketch to
/// make room for `Ruby` and the milestone form). 21 used, 11 spare.
///
/// ## There are TWO render surfaces, and this column is only one of them
///
/// Recorded because Q-H5 turned on it. Rendering is not "this column plus
/// special cases":
///
/// - **marker-keyed** — THIS column. One class per canonical marker.
/// - **kind-keyed** — a `TokenKind` renders by its kind, exactly as `Newline` and
///   `OptBreak` already do. `NoteCaller` is the case that forced the point: a
///   footnote's `<sup>` caller belongs to the CALLER TOKEN, while `\f` itself maps
///   to [`Self::Aside`], the note body container. Two different things rendering
///   two ways — not one thing with an exception.
///
/// A third, narrower surface: **scope-derived** elements. `ListContainer` and
/// `Table` are **not reachable from any row** and never will be, because they
/// correspond to walker SCOPES rather than to markers — USFM has no marker that
/// opens either. Export synthesizes them around a scope the same way it
/// synthesizes closing tags, which is also why closing tags are not a column
/// (Q-H4).
///
/// Read by: export. Nothing else may read it — it is a rendering policy, the one
/// column in this table that is our choice rather than the spec's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlElement {
    /// No element of its own; children flow into the parent.
    Transparent,
    /// `<p>`
    Para,
    /// `<h1>`…`<h6>`. The level is `HEADING_BASE_LEVEL` for the family plus the
    /// span's digit minus one, clamped to 6 — see [`heading_level`].
    Heading,
    /// `<span>`
    Span,
    /// A self-closing `<span …/>` carrying only its data attributes. Milestones
    /// render this way (round 6 / [C] ii): they mark a point, so there is no
    /// content to wrap.
    SelfClosingSpan,
    /// `<li>`
    ListItem,
    /// `<ul>` — synthesized by export, never mapped from a row.
    ListContainer,
    /// `<table>` — synthesized by export, never mapped from a row.
    Table,
    /// `<tr>`
    TableRow,
    /// `<td>`, or `<th>` when the marker name starts `th` — derivable from the
    /// span, so it does not cost a second class.
    TableCell,
    /// `<aside>`
    Aside,
    /// `<sup>`
    Sup,
    /// `<a>`
    Anchor,
    /// `<figure>` wrapping an [`Self::Image`] and a `<figcaption>`. The column
    /// names the OUTER element; the interior is a fixed export template — the
    /// convention `\bdit` also uses (`<b>` outer, `<i>` templated inside).
    Figure,
    /// `<img>`
    Image,
    /// `<section>`
    Section,
    /// `<ruby>` + `<rt>` — `\rb` only (round 6 / [C] i).
    Ruby,
    /// `<b>` — `\bd`, and the outer element of `\bdit` (round 8 / Q-H2).
    Bold,
    /// `<i>` — `\it` (round 8 / Q-H2).
    Italic,
    /// `<em>` — `\em` (round 9 / 4). Its semantic twin, consistent with
    /// `bd`→`<b>`; the 3.2-index recategorisation to `CharTextFeatures` stands,
    /// this only gives it the right element.
    Em,
    /// An EMPTY `<div>` — a vertical spacer with no content of its own.
    ///
    /// `\sd#` only (round 9 / 5). Ruled from the spec's own layout example: a
    /// semantic division renders as vertical BLANK SPACE between text blocks, so
    /// it is neither a heading nor an `<hr>`. The level digit is not lost — it
    /// rides in `data-marker` like every other marker's, per the always-emit
    /// convention.
    Div,
}

/// Base `<h…>` level for each heading FAMILY — the level its digit-1 (and bare)
/// spelling renders at. Ruled round 6 / [C] iv: an auxiliary side table, the same
/// shape the scope-kind precedence data uses, rather than more bits on every row.
///
/// The reason it has to exist: `mt1`, `ms1`, and `s1` all carry digit `1` and are
/// three different heading levels, so `class + digit` alone is underspecified.
///
/// Levels are `base + digit - 1`, clamped at 6 (see [`heading_level`]).
pub const HEADING_BASE_LEVEL: &[(&str, u8)] = &[
    ("mt", 1),
    ("mte", 1),
    ("imt", 1),
    ("imte", 1),
    ("ms", 2),
    ("c", 2),
    ("cp", 2),
    ("cl", 2),
    ("is", 3),
    ("iot", 3),
    ("s", 3),
    ("qa", 4),
    // `sd` is deliberately ABSENT (round 9 / 5): it is not a heading at all but an
    // empty spacer `<div>`, so it has no level. That removed the only family that
    // needed clamping — see `heading_level`.
];

/// Resolve a heading level from the family base and the occurrence's digit.
/// Unknown families fall back to 3 (a mid-document heading).
///
/// **No clamp.** One existed for `sd#`, which round 9 removed from the heading set
/// entirely; with `s4`→`<h6>` the deepest legal level, every family now fits
/// `<h1>`..`<h6>` exactly. A bad base therefore produces an out-of-range level
/// rather than silently collapsing two levels into `<h6>`. Deliberately NOT
/// covered by a test (round 10): the table is 12 hand-written rows an export
/// bug would surface immediately, and the assertion was not worth its keep.
pub fn heading_level(marker: &str, digit: Option<u8>) -> u8 {
    let base = HEADING_BASE_LEVEL
        .iter()
        .find_map(|(name, base)| (*name == marker).then_some(*base))
        .unwrap_or(3);
    base + digit.unwrap_or(1).saturating_sub(1)
}

/// One authored marker row: everything the table knows about one CANONICAL
/// marker name.
///
/// Every field carries the consumer that reads it. If a field has no consumer,
/// it does not belong here (GLOSSARY: "a derivable fact sneaking into the
/// format is the disease"). Facts derivable from other columns are `const fn`s
/// in this module, not fields — see [`contributes_context`].
#[derive(Debug, Clone, Copy)]
pub struct MarkerRow {
    /// Canonical name: `-s`/`-e` stripped, then digits stripped (`"q"` not
    /// `"q1"`; `"qt"` not `"qt3-s"`) [G]. Longest is 6 bytes (`periph`) — must
    /// fit a u64 load.
    ///
    /// Read by: codegen (name→idx match arms, names side array), everything
    /// that reports a marker to a human.
    pub marker: &'static str,

    /// Which spellings of `marker` this row claims — see [`SpellingShape`].
    /// Together with `marker` this is the lookup KEY.
    pub shape: SpellingShape,

    /// Coarse spec kind. Read by: scanner, lint, codegen, export.
    pub kind: MarkerKind,

    /// Fine spec category [C] — behavior-bearing, see [`Category`].
    /// Read by: lint, export, codegen; the authored source of `opens_scope`.
    pub category: Category,

    /// Structural whitespace required after the marker NAME. This column is
    /// the per-class delimiter-space fold rule (NEXT-STEPS step 4).
    ///
    /// Read by: scanner (fold), lint (delimiter findings).
    pub ws_after_name: StructuralWhitespaceRequirement,

    /// Argument consumed right after the delimiter [E] [K].
    /// Read by: scanner (pending-payload mode), interpreters, export.
    pub payload: Payload,

    /// Which numeric suffixes are legal spellings of this row [D] [O].
    /// Read by: codegen (validation after stripping), lint, export.
    pub numbered_max: Numbering,

    /// Contexts this marker is legal in, AUTHORED as a slice. The bitmask and
    /// the effective-context promotions are codegen output.
    ///
    /// Read by: lint, and the walker's recovery predicate [I]. Codegen reads it
    /// to emit the mask.
    pub allowed_contexts: &'static [SpecContext],

    /// The scope an occurrence of this marker PUSHES A FRAME for, or `None` [A].
    ///
    /// **This column means pushing ONLY** — displacement is the walker's
    /// pop_while over the context mask, never a column (`\c`/`\v` push
    /// nothing yet still pop).
    ///
    /// Authored from `kind` × `category`, which is why `\pb` (CharBreaks) is
    /// `None` while every other character marker is `Some(Character)`. This
    /// column retires onion's ~90-line marker-name-prefix cascade
    /// (`is_list_marker_name` / `is_section_marker_name` / … ), whose
    /// `starts_with` fallbacks are a known bug source.
    ///
    /// The PRECEDENCE data (what an incoming scope displaces) is keyed on
    /// [`ScopeKind`] in a separate ~13-row auxiliary table, NOT here.
    ///
    /// Read by: the walker.
    pub opens_scope: Option<ScopeKind>,

    /// The scope an occurrence of this marker CLOSES [B]. `Some(Sidebar)` for
    /// `esbe` and nothing else today.
    ///
    /// Retires onion's `\esbe` phantom frame: onion files `esbe` as a Sidebar
    /// OPEN, so it pushes a meaningless frame that `export_tree` then has to
    /// special-case and retroactively patch (~40 lines across two files). With
    /// this column `esbe` closes what `esb` opened, by the same mechanism the
    /// milestone `\*` closer uses.
    ///
    /// Read by: the walker.
    pub closes_scope: Option<ScopeKind>,

    /// Attributes USFM 3.2 defines for this marker, in spec order, each with
    /// its [`AttrStatus`]. Every entry is read off that marker's own 3.2 page —
    /// the group index pages do not carry cardinality or defaults:
    ///
    /// | marker | attributes | page |
    /// |---|---|---|
    /// | `w` | lemma\*, strong, srcloc (all optional) | char/features/w.html |
    /// | `rb` | gloss\* (optional) | char/features/rb.html |
    /// | `jmp` | href\*, title, id (optional) + `link-`prefixed trio (deprecated 3.1) | char/features/jmp.html |
    /// | `ref` | loc\* (optional) | char/features/ref.html |
    /// | `fig` | src, size, ref (REQUIRED); alt, loc, copy (optional); **no default** | fig/fig.html |
    /// | `xt` | link-href (DEPRECATED), no default | char/notes/crossref/xt.html |
    /// | `qt` | who\*, sid, eid (optional) | ms/qt.html |
    /// | `ts` | sid, eid (optional) | ms/ts.html |
    /// | `vid` | ref\* (REQUIRED), h (optional) | ms/vid.html |
    /// | `list`, `table` | none documented | ms/list.html, ms/table.html |
    ///
    /// `*` marks the default attribute, carried separately in
    /// [`Self::default_attribute`].
    ///
    /// Two facts this shape still cannot express, recorded here instead:
    ///
    /// 1. **`sid` belongs to the START spelling and `eid` to the END spelling**
    ///    of a paired milestone. Both are listed on the one row; which applies
    ///    is narrowed per occurrence by the `-s`/`-e` the span already carries,
    ///    which is what keeps the milestone side out of the table [G].
    /// 2. **Conditional cardinality.** 3.2 says `eid` is required *if* `sid` was
    ///    used (`qt`), and that `sid`/`eid` are optional standalone but required
    ///    when a milestone is paired (`ts`). [`AttrStatus`] is per-attribute, so
    ///    a dependency between two attributes has nowhere to live. Both rows use
    ///    `Optional`; the conditional is a lint rule. Flagged.
    ///
    /// **User-defined `x-…` and `z-…` attributes are legal on ANY character
    /// marker** and are non-canonical. A KIND-level fact, deliberately not a
    /// column — a per-row boolean would be `true` for all ~60 character rows and
    /// tell the reader nothing.
    ///
    /// Codegen turns the names into side-array indices. Read by: interpreters
    /// (attribute list), lint (missing-required / unknown / deprecated
    /// findings), export.
    pub defined_attributes: &'static [(&'static str, AttrStatus)],

    /// Which of `defined_attributes` an unnamed default value binds to
    /// (`\w word|lemma-value\w*` → `lemma`). Not always the first entry —
    /// `fig`'s default is `src`, its fourth.
    ///
    /// Read by: interpreters (attribute list), lint, export.
    pub default_attribute: Option<&'static str>,

    /// How this marker's scope ends. Read by: the walker, lint (unclosed
    /// findings), export (tree building).
    pub closing: ClosingBehavior,

    /// Marker is deprecated by the spec but still legal to read.
    /// Read by: lint only. Never affects scanning.
    pub deprecated: bool,

    /// Default HTML element class for export — see [`HtmlElement`] for the
    /// inventory, the always-emit-data-attributes convention that makes it a
    /// default rather than a mandate, and the heading-level side table.
    /// Populated round 6 / [C]; `None` means "export decides", which no row uses
    /// today. Read by: export.
    pub html_element: Option<HtmlElement>,

    /// CODEGEN-ONLY: ordering hint for `common_marker_checks`, the hot-marker
    /// fast path (NEXT-STEPS step 4 — built one pattern at a time, measured).
    /// Lower runs earlier; `None` means "not hot, general path".
    ///
    /// **MEASURED** — the counts, method, corpora, and the reasoning behind the
    /// cut all live in planning/marker-frequencies.md. Not restated here so the
    /// two cannot drift; re-measure there and re-rank from there.
    ///
    /// Read by: codegen. The runtime table does not carry it.
    pub priority: Option<u8>,
}

impl MarkerRow {
    /// `debug_assert!`able coherence check for one row [C]. Called over the
    /// whole table by the tests in `tables::unaudited` (and later
    /// `tables::rows`) so a mis-paired kind/category cannot land silently.
    pub const fn is_coherent(&self) -> bool {
        self.category.is_coherent_with(self.kind)
    }

    /// The context an open frame of this row puts its children in — see
    /// [`contributes_context`]. Derived, never stored.
    pub const fn contributes_context(&self) -> Option<SpecContext> {
        contributes_context(self.kind, self.category)
    }

    // Deliberately NO `precedence()`/rank: displacement is
    // `pop_while(top frame's stamped context ∉ row's context mask)` — a rank
    // has no consumer. Don't re-derive one. See NEXT-STEPS §5 (walker design).
}
