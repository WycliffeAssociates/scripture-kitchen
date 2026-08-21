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
/// Nearly every row is [`Self::Any`]; the axis exists for names the spec
/// overloads across plain and milestone spellings with different facts. `qt` is
/// the only such name: `\qt …\qt*` is a character marker with no attributes,
/// `\qt3-s` a milestone carrying `who`/`sid` — different kind, category,
/// contexts, closing and delimiter rule, so they cannot share a row.
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
/// occurrence's spelling, read off the token span, never stored here.
///
/// Behavior-bearing distinctions live in [`Category`]. Read by: scanner (ws
/// fold + open-marker stack), lint (context checks), codegen (packing, USJ type
/// projection), export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MarkerKind {
    /// The generic EMPTY row at index 0 — an unconfigured `\z` extension, an
    /// unknown name, an illegal spelling. Index 0 is a ROW and not a sentinel,
    /// so every generated accessor stays total instead of returning `Option`
    /// purely to describe row 0. Pairs only with [`Category::Unknown`].
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

/// The spec's own FINE category, one flat enum completing the two-level
/// taxonomy [`MarkerKind`] × [`Category`].
///
/// **Category is load-bearing for behavior, not decoration.** `\pb` is a
/// *character* marker of category [`Category::CharBreaks`] and therefore opens
/// NO scope, where every other character marker opens a
/// [`ScopeKind::Character`] — so reading behavior off `kind` alone gets `\pb`
/// wrong (file it as a Paragraph and an incoming `\pb` closes the paragraph it
/// sits inside).
///
/// Variants are PREFIXED by their kind group because the spec reuses group
/// names across kinds: `ParaPoetry` (`\q`) is not `CharPoetry` (`\qs`). The
/// prefix also makes [`Category::is_coherent_with`] readable at the call site.
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
    /// Peripheral paragraphs — `p1`, `p2`: not used in scripture files, so they
    /// sit outside the ordinary paragraph group (usx.rng
    /// `PeriphPara.para.style.enum`). Bare `\p` stays [`Self::ParaBody`].
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
    /// The `table` structure milestone, and the `t-s`/`t-e` spelling of it.
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
    /// mis-paired row fails loudly in tests rather than drifting. A relation,
    /// not a function: [`Category::ChapterVerse`] is one spec group covering `c`
    /// (Chapter), `v` (Verse), `ca`/`va`/`vp` (Character) and `cp` (Paragraph).
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

/// The kinds of scope the context machine can have open.
///
/// Distinct from [`MarkerKind`], which says what the SPEC calls a marker; this
/// says what the MACHINE does with it. They diverge wherever behavior and
/// taxonomy disagree (`\pb` is a Character that opens nothing; `\fig` is a
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
    // No `Chapter`/`Verse`: `\c`/`\v` are POINTS — no frame, and the walker
    // needs no rank.
    /// The container `\tr` rows and `\tc` cells hang off. A peer of
    /// [`Self::Para`], unreachable from any row.
    ///
    /// Two paths reach it: the WALKER synthesizes the frame when `\tr` arrives
    /// with no table open, and `\table-s`/`\table-e` delimit one explicitly —
    /// `\table-s` followed by `\tr` must not nest two frames. The explicit
    /// markers keep `opens_scope: Some(Milestone)`, honest about what they are,
    /// so the walker keys the container off `category` (`MilestoneTable` here,
    /// `MilestoneList` for [`Self::List`]) rather than off `opens_scope` or the
    /// `-s`/`-e` spelling.
    Table,
    /// The list container, [`Self::Table`]'s peer: same unreachable-from-any-row
    /// status, same `category`-keyed entry (`MilestoneList`), same export twin
    /// (`HtmlElement::ListContainer`).
    ///
    /// **It exists for `\list-s`/`\list-e` alone.** `\li` is a PARAGRAPH: it
    /// opens [`Self::Para`] and contributes `SpecContext::List`, so the context
    /// lane already carries "inside a list", and a list is FLAT where a table
    /// NESTS (table > row > cell) and `\tc` must find its row. Only the explicit
    /// begin/end needs a frame, its closure rule being a POP BARRIER and a
    /// barrier needing a frame to be a barrier on.
    List,
    TableRow,
    TableCell,
    Sidebar,
    Periph,
    // No `Meta`: `\cat` is char-shaped and opens a `Character` scope. A scope
    // kind nothing can push is dead weight.
}

/// Structural-whitespace requirement immediately AFTER the marker name. The
/// variant names are verbose on purpose — the reader should not have to look up
/// `HS` vs `Hs` vs `WS`.
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
/// delimiter, before any content.
///
/// Read by: scanner (drives the pending-payload mode and the payload token
/// kinds), interpreters (which grammar to run over the payload span), export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// Nothing special follows the delimiter; content is ordinary text.
    None,
    /// `\id` — a book-code slice. Spec `TLC` = `/[0-9A-Z]{3}/`, but the slice is
    /// kept verbatim and never validated, truncated or repaired: an out-of-list
    /// or overlong code is revision-state DATA.
    BookCode,
    /// `\c`, `\cp`, `\ca`, `\v`, `\vp`, `\va` — a chapter/verse designator.
    ///
    /// Spec `VERSE` is
    /// `/[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*/` — a number,
    /// optional letter/mark suffix (`1a`), then range/sequence parts joined by
    /// `-` or `,`, optionally preceded by U+200F RLM.
    ///
    /// **That grammar belongs to the verse-designator INTERPRETER, not here.**
    /// The scanner emits ONE payload token spanning the designator and never
    /// looks inside it.
    Designator,
    /// `\usfm` — a version string, spec shape `\d+\.\d+(\.\d+)?`. Same
    /// discipline as `Designator`: one span, interpreted on demand.
    Version,
    /// `\f`, `\fe`, `\ef`, `\x`, `\ex` — the note caller, consumed after the
    /// delimiter and before the note's content.
    ///
    /// The Footnote railroad's pattern is `/[^\\\s]+/`: one or more characters
    /// that are neither backslash nor whitespace. `+` (auto), `-` (no caller)
    /// and `?` are just the conventional values of that one general run, so the
    /// interpreter never enumerates them.
    ///
    /// The same production shows what FOLLOWS the caller: an optional `category`
    /// (`\ef - \cat People\cat*\fr 1.2-6a: …`) BEFORE any content. That
    /// positional constraint is a lint rule; this column records only that these
    /// kinds consume a caller.
    NoteCaller,
}

/// Legal numeric suffix range for a marker's spelling (`q` accepts `q1..q4`).
///
/// The number itself is NEVER stored — it lives in the token's span. This
/// column only says which digit runs are legal, so the matcher can validate
/// after stripping.
///
/// **Spec-wide rule: the BARE form is ALWAYS a valid spelling.** `\s` is as
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
    /// Table cell markers: `\tc1`, `\tc1-2` — a column number or a column SPAN.
    /// Distinct from `Unbounded` because it tells the matcher something else: not
    /// "any digit run" but "stop at the stem, the rest is payload", handed to the
    /// marker's payload interpreter. Spans are deliberately not modelled here and
    /// no cap is asserted.
    TableColumns,
}

/// The 18 spec contexts a marker may legally appear in.
///
/// AUTHORED FORM is a `&'static [SpecContext]` slice on the row. The 18-bit
/// mask is CODEGEN output computed from this slice, and it is EXACTLY the slice:
/// codegen never promotes or invents contexts (a test pins the equality).
///
/// Read by: lint, and the walker's recovery predicate (the row's mask AND-ed
/// against the top frame's stamped context). Codegen emits the mask.
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
    /// sequence (`Scripture` → … → `ChapterContent`): monotonic, established by
    /// position in the book, and the only band an "advance" rule may apply to.
    /// Everything else REPEATS FREELY — scope-established sub-groups of
    /// chapter/peripheral content (`Section`, `Para`, `Footnote`, …).
    ///
    /// "Right after `\c`" is deliberately NOT a context, since chapters repeat
    /// and no monotonic encoding works: `ca`/`cp`/`va`/`vp` are checked by an
    /// adjacency lint rule over TOKENS. Those rows still open Character scopes,
    /// so they carry the character class's CONTAINER contexts (`Para`, `List`,
    /// `Table`, `Footnote`, …) — an empty mask would pop every frame. Only `cp`
    /// (a Paragraph opening nothing) carries the empty slice.
    ///
    /// Exhaustive on purpose: a new variant must declare its axis to compile.
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

    /// Every variant, IN DECLARATION ORDER — `ALL[ctx as usize] == ctx` is the
    /// contract [`Self::from_u8`] rests on. Kept beside `is_positional`, whose
    /// exhaustive match already stops compilation when a variant is added, so
    /// both get extended together.
    pub const ALL: [Self; 18] = [
        Self::Scripture,
        Self::BookIdentification,
        Self::BookHeaders,
        Self::BookTitles,
        Self::BookIntroduction,
        Self::BookIntroductionEndTitles,
        Self::BookChapterLabel,
        Self::ChapterContent,
        Self::Peripheral,
        Self::PeripheralContent,
        Self::PeripheralDivision,
        Self::Section,
        Self::Para,
        Self::List,
        Self::Table,
        Self::Sidebar,
        Self::Footnote,
        Self::CrossReference,
    ];

    /// Decodes a stamped discriminant (e.g. `Node.ctx`). A value outside the
    /// enum is a builder bug, refused loudly by the index panic.
    pub fn from_u8(value: u8) -> Self {
        Self::ALL[value as usize]
    }
}

#[cfg(test)]
mod spec_context_tests {
    use super::SpecContext;

    #[test]
    fn all_is_in_declaration_order() {
        for (index, ctx) in SpecContext::ALL.iter().enumerate() {
            assert_eq!(*ctx as usize, index);
            assert_eq!(SpecContext::from_u8(index as u8), *ctx);
        }
    }
}

/// What context an OPEN scope of this (kind, category) puts its children in —
/// **derived, not a column.**
///
/// `None` means the frame is TRANSPARENT: it contributes no context of its own,
/// so a child's legality is judged against whatever frame is below it. The
/// walker stamps the resolved value onto each frame at push time
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
        // Paragraph frames always contribute; which context depends on the fine
        // category. `cp` is the only Paragraph in the ChapterVerse group and it
        // opens no scope — a published chapter LABEL, payload and nothing else —
        // so a value here would be dead, this being read only at push time.
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
        // still decides. `\cat` is Meta by taxonomy but char-shaped in
        // behavior, so it is transparent like any character marker.
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
/// `sectionpara`.** Keyed on the ENCLOSING PARAGRAPH, not on `\v`'s own
/// [`SpecContext`] set, so neither the context mask nor any per-row value on
/// `\v` can express it: the same `\v` is legal or illegal depending on what is
/// open above it. A lint rule needing no new frame field — a frame already
/// carries the `marker_idx` that opened it, so the enclosing paragraph is one
/// array read.
///
/// A MARKER list rather than a `Category` list because the rail groups are a
/// different partition of the same markers, not a coarsening of ours:
/// `OtherPara` spans five of our categories, and `SectionPara` includes
/// `iex`/`ip` (we file those ParaIntroductions) while omitting `mt` and `d` that
/// our ParaTitlesSections holds. Codegen turns the set into a bitmask over row
/// indices.
///
/// Members are canonical names, so `s` covers `s1`..`s4`, and every member has a
/// row. The rails' `k1`, `k2` and `restore` are NOT listed: 3.2 documents no
/// such markers, so a rule about them would be cruft.
pub const V_FORBIDDEN_IN_PARAGRAPHS: &[&str] = &[
    // OtherPara (usx.rng:1151).
    "lit", "cp", "pb", "qa", "sts", "rem", // SectionPara (usx.rng:892).
    "iex", "ip", "ms", "mr", "mte", "r", "s", "sr", "sp", "sd", "cl", "cd",
];

/// Lexical facts about CONTENT that the scanner owns and this table only
/// records, so the two cannot drift.
///
/// **Explicit Unicode escapes (U25004): `\uXXXX` and `\UXXXXXXXX` are TEXT**,
/// folded into the text run by the text arm's escape peek. Fixed width — exactly
/// 4 or exactly 8 hex digits, no terminator, no new `TokenKind`.
///
/// ```text
/// \u0020      ->  text " "       NOT a marker named `u0020`: the
///                                USV pattern beats any marker claim,
///                                matched by pattern, never by lookup
/// \U0001F600  ->  text (1 emoji) 8 digits for anything above U+FFFF
/// \u00e9      ->  text + lint    the proposal says UPPERCASE hex, so
///                                lowercase is ill-cased, never
///                                not-a-USV
/// \ND         ->  marker + lint  an uppercase NAME cannot resolve to
///                                a row (canonical names are all
///                                lowercase), so it lands on index 0
///                                as DATA, and is never rejected
/// ```
///
/// Read by the scanner's `escape_len` (letter + fixed hex width), so the escape
/// set is defined exactly once.
///
/// <https://github.com/usfm-bible/tcdocs/blob/main/proposals/2025/U25004%20Explicit%20Unicode.md>
pub const USV_ESCAPE_LETTERS: &[(char, usize)] = &[('u', 4), ('U', 8)];

/// How a marker's scope ends.
///
/// Read by: the walker (open-marker stack: what a `\X*` may close), lint
/// (unclosed-marker findings), export (tree building).
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
/// Cardinality and deprecation are both spec facts stated per attribute, per
/// marker, and lint needs both — `\fig` without `src` is an error,
/// `\xt|link-href="…"` is a warning.
///
/// Deliberately NOT a status: which attribute is the DEFAULT. Default-ness is
/// orthogonal to cardinality (`w`'s `lemma` is Optional AND default; `fig` has
/// three Required attributes and NO default; `xt`'s only attribute is Deprecated
/// and cannot be a default), so folding it in would double the enum for no gain.
/// It stays the separate [`MarkerRow::default_attribute`] field.
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

/// Default HTML element class for export. The column lives in the main table
/// rather than an export-side one: the row is a u128 either way, so co-location
/// is free and a reader checks one place.
///
/// The element is a **DEFAULT, not a mandate**, because export always emits
/// verbose data attributes — `data-marker` (the canonical name),
/// `data-category`, `data-usfm-type` — so a consumer restyles via CSS without
/// needing a different element, and a consumer-supplied marker↔element map can
/// override the default wholesale.
///
/// Packing: u5, 32 slots (`Ruby` and the milestone form did not fit u4). 21
/// used, 11 spare.
///
/// ## There are THREE render surfaces, and this column is only one
///
/// - **marker-keyed** — THIS column. One class per canonical marker.
/// - **kind-keyed** — a `TokenKind` renders by its kind, as `Newline` and
///   `OptBreak` do. A footnote's `<sup>` caller belongs to the CALLER TOKEN,
///   while `\f` itself maps to [`Self::Aside`], the note body container: two
///   different things rendering two ways, not one thing with an exception.
/// - **scope-derived** — `ListContainer` and `Table` are not reachable from any
///   row and never will be: they correspond to walker SCOPES, and USFM has no
///   marker that opens either. Export synthesizes them around a scope the way it
///   synthesizes closing tags, which is also why closing tags are not a column.
///
/// Read by: export. Nothing else may read it — it is a rendering policy, the one
/// column in this table that is our choice rather than the spec's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlElement {
    /// No element of its own; children flow into the parent.
    ///
    /// **No row uses this.** `Transparent` still EMITS the text while giving a
    /// consumer no class to style, hide, or relocate it by, so metadata rows
    /// (`cat`, `h`, `toc`, `id`, `rem`, row 0, …) use [`Self::Span`] instead.
    /// Kept as a rendering POLICY export honours (children emit, no wrapper) so
    /// a future row has somewhere to point.
    Transparent,
    /// `<p>`
    Para,
    /// `<h1>`…`<h6>`. The level is `HEADING_BASE_LEVEL` for the family plus the
    /// span's digit minus one — see [`heading_level`].
    Heading,
    /// `<span>`
    Span,
    /// A self-closing `<span …/>` carrying only its data attributes. Milestones
    /// render this way: they mark a point, so there is no content to wrap.
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
    /// `<ruby>` + `<rt>` — `\rb` only.
    Ruby,
    /// `<b>` — `\bd`, and the outer element of `\bdit`.
    Bold,
    /// `<i>` — `\it`.
    Italic,
    /// `<em>` — `\em`, the semantic twin of `bd`→`<b>`.
    Em,
    /// An EMPTY `<div>` — a vertical spacer with no content of its own.
    ///
    /// `\sd#` only: the spec's layout example renders a semantic division as
    /// vertical BLANK SPACE between text blocks, so it is neither a heading nor
    /// an `<hr>`. The level digit rides in `data-marker` like every other
    /// marker's.
    Div,
}

/// Base `<h…>` level for each heading FAMILY — the level its digit-1 (and bare)
/// spelling renders at. An auxiliary side table rather than more bits on every
/// row, and it has to exist because `mt1`, `ms1` and `s1` all carry digit `1`
/// and are three different heading levels: `class + digit` alone is
/// underspecified. Levels are `base + digit - 1` (see [`heading_level`]).
///
/// `c`, `cp` and `cl` are ABSENT: none is a heading by its own 3.2 page's words
/// (`c` is TextType ChapterNumber; `cp` is a display override for `c`, as `vp`
/// is for `v`; `cl` is "a paragraph-level element, not a heading or title
/// marker").
///
/// Two properties recorded rather than smoothed over: `s4` lands on `<h6>`
/// exactly, the deepest legal level; and the intro branch skips a level (`imt1`
/// h1 → `is1` h3) because USFM has no marker between them — a heading-order
/// smell owned by the spec's two-tier intro structure, not by this table.
/// Sibling `<h1>`s from `imt1` and `mt1` are likewise accepted as two genuinely
/// separate top-level sections.
pub const HEADING_BASE_LEVEL: &[(&str, u8)] = &[
    ("mt", 1),
    ("mte", 1),
    ("imt", 1),
    ("imte", 1),
    ("ms", 2),
    ("is", 3),
    ("iot", 3),
    ("s", 3),
    ("qa", 4),
    // `sd` is deliberately ABSENT: not a heading at all but an empty spacer
    // `<div>`, so it has no level.
];

/// Resolve a heading level from the family base and the occurrence's digit.
/// Unknown families fall back to 3 (a mid-document heading).
///
/// **No clamp**: every family fits `<h1>`..`<h6>` exactly (`s4`→`<h6>` is the
/// deepest), so a bad base produces an out-of-range level rather than silently
/// collapsing two levels into `<h6>`.
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
/// Every field carries the consumer that reads it; a field with no consumer does
/// not belong here. Facts derivable from other columns are `const fn`s in this
/// module, not fields — see [`contributes_context`].
#[derive(Debug, Clone, Copy)]
pub struct MarkerRow {
    /// Canonical name: `-s`/`-e` stripped, then digits stripped (`"q"` not
    /// `"q1"`; `"qt"` not `"qt3-s"`). Longest is 6 bytes (`periph`) — must
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

    /// Fine spec category — behavior-bearing, see [`Category`].
    /// Read by: lint, export, codegen; the authored source of `opens_scope`.
    pub category: Category,

    /// Structural whitespace required after the marker NAME. This column IS the
    /// per-class delimiter-space fold rule.
    ///
    /// Read by: scanner (fold), lint (delimiter findings).
    pub ws_after_name: StructuralWhitespaceRequirement,

    /// Argument consumed right after the delimiter.
    /// Read by: scanner (pending-payload mode), interpreters, export.
    pub payload: Payload,

    /// Which numeric suffixes are legal spellings of this row.
    /// Read by: codegen (validation after stripping), lint, export.
    pub numbered_max: Numbering,

    /// Contexts this marker is legal in, AUTHORED as a slice; the bitmask is
    /// codegen output.
    ///
    /// Read by: lint, and the walker's recovery predicate. Codegen reads it to
    /// emit the mask.
    pub allowed_contexts: &'static [SpecContext],

    /// The scope an occurrence of this marker PUSHES A FRAME for, or `None`.
    ///
    /// **Pushing ONLY** — displacement is the walker's pop_while over the
    /// context mask, never a column (`\c`/`\v` push nothing yet still pop), and
    /// the PRECEDENCE data is keyed on [`ScopeKind`] in a separate auxiliary
    /// table.
    ///
    /// Authored from `kind` × `category`, which is why `\pb` (CharBreaks) is
    /// `None` while every other character marker is `Some(Character)` — a
    /// column instead of a marker-name-prefix cascade whose `starts_with`
    /// fallbacks are a known bug source.
    ///
    /// Read by: the walker.
    pub opens_scope: Option<ScopeKind>,

    /// The scope an occurrence of this marker CLOSES. `Some(Sidebar)` for `esbe`
    /// and nothing else today: `esbe` closes what `esb` opened, by the same
    /// mechanism the milestone `\*` closer uses, instead of pushing a phantom
    /// frame that export then has to patch away.
    ///
    /// Read by: the walker.
    pub closes_scope: Option<ScopeKind>,

    /// Attributes USFM 3.2 defines for this marker, in spec order, each with its
    /// [`AttrStatus`]. Every entry is read off that marker's OWN 3.2 page: the
    /// group index pages carry neither cardinality nor defaults. The default
    /// attribute is carried separately in [`Self::default_attribute`].
    ///
    /// Two facts this shape cannot express, recorded here instead:
    ///
    /// 1. **`sid` belongs to the START spelling and `eid` to the END spelling**
    ///    of a paired milestone. Both are listed on the one row; which applies
    ///    is narrowed per occurrence by the `-s`/`-e` the span already carries,
    ///    which is what keeps the milestone side out of the table.
    /// 2. **Conditional cardinality.** 3.2 says `eid` is required *if* `sid` was
    ///    used (`qt`), and that `sid`/`eid` are optional standalone but required
    ///    when a milestone is paired (`ts`). [`AttrStatus`] is per-attribute, so
    ///    a dependency between two attributes has nowhere to live: both rows use
    ///    `Optional` and the conditional is a lint rule.
    ///
    /// **User-defined `x-…` and `z-…` attributes are legal on ANY character
    /// marker** and are non-canonical. A KIND-level fact, deliberately not a
    /// column — a per-row boolean would be `true` for all ~60 character rows and
    /// tell the reader nothing.
    ///
    /// **A name ending in `*` is a PREFIX WILDCARD**: `("a-*", Optional)` means
    /// "any attribute whose name begins with `a-`". `\ta` needs it — its spec
    /// defines no fixed names, only "each attribute should begin with `a-`" — so
    /// an exact-name list can express neither the positive rule (every legal
    /// `a-…` would lint as unknown) nor the negative one (an attribute that does
    /// NOT start with `a-` should be flagged). A sentinel rather than new schema
    /// because `*` is not legal in an attribute name so it cannot collide, and
    /// prefix-ness is a property of the NAME, not of the status. Whatever matches
    /// names is the ONE place that learns the convention; the scanner never reads
    /// this column. Cardinality is not expressed here either — `\ta`'s "one or
    /// more" is a lint rule, like point 2 above.
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
    /// default rather than a mandate, and the heading-level side table. `None`
    /// means "export decides", which no row uses today. Read by: export.
    pub html_element: Option<HtmlElement>,

    /// CODEGEN-ONLY: ordering hint for `common_marker_checks`, the hot-marker
    /// fast path. Lower runs earlier; `None` means "not hot, general path".
    /// Ranked from corpus MEASUREMENT, never intuition — re-measure before
    /// re-ranking.
    ///
    /// Read by: codegen. The runtime table does not carry it.
    pub priority: Option<u8>,
}

impl MarkerRow {
    /// `debug_assert!`able coherence check for one row. Called over the whole
    /// table by the `tables::rows` tests so a mis-paired kind/category cannot
    /// land silently.
    pub const fn is_coherent(&self) -> bool {
        self.category.is_coherent_with(self.kind)
    }

    /// The context an open frame of this row puts its children in — see
    /// [`contributes_context`]. Derived, never stored.
    pub const fn contributes_context(&self) -> Option<SpecContext> {
        contributes_context(self.kind, self.category)
    }

    // Deliberately NO `precedence()`/rank: displacement is
    // `pop_while(top frame's stamped context ∉ row's context mask)`, so a rank
    // has no consumer. Don't re-derive one.
}
