//! The authored marker table — one [`MarkerRow`] per canonical marker.
//!
//! **Provenance, stated plainly (2026-08-12).** These rows arrived as a
//! MECHANICAL translation of onion's data into the reworked schema, and were
//! promoted here WITHOUT the planned row-by-row sitting against the 3.2 docs.
//! Will's call: the audit wanted two screens side by side to be worth doing, and
//! everything it would have changed is a table value — cheap to fix later, and
//! every ruling [A]–[P] that shaped the schema was itself reviewed. So this is
//! the table, and a wrong cell here is a bug like any other, not a known-bad
//! staging area. `tables::unaudited` is gone.
//!
//! What that means for trust: the SCHEMA and the rulings are reviewed; the
//! CELLS are machine-derived from onion plus the spec taxonomy in
//! planning/scratch.md, and where the derivation needed a judgement call it is
//! recorded in [`CATEGORY_JUDGEMENT_CALLS`] below (all 20 reviewed by hand).
//!
//! **Referee: USFM 3.2** (docs.usfm.bible/usfm/3.2/), the latest spec. No
//! version column, no version tracking — where 3.2 deprecates something, that is
//! what `deprecated` / `AttrStatus::Deprecated` mean. The 3.1 tcdocs/usx.rng is
//! fallback grammar evidence only and never outvotes a 3.2 page.
//!
//! Conventions worth knowing when reading rows: families are collapsed to
//! their canonical stem (`qt3-s` → `qt`; strip `-s`/`-e` first, then digits);
//! overloaded names split by [`SpellingShape`] (`qt` has two rows); every row
//! carries a `// ws:` comment saying whether the value was curated or
//! derived; `priority` counts are MEASURED (planning/marker-frequencies.md).
//!
//! One APPROVED 3.2 addition is analysed but NOT implemented — it changes
//! the scanner arms, not this table: generalised node-initial attributes
//! (U25001). See NEXT-STEPS §5 (attributes).
//!

use super::schema::{
    AttrStatus, Category, ClosingBehavior, HtmlElement, MarkerKind, MarkerRow, Numbering, Payload,
    ScopeKind, SpecContext, SpellingShape, StructuralWhitespaceRequirement as Ws,
};

/// The table. Sorted by canonical name; `ROWS[0]` is the generic EMPTY row
/// every unresolved marker lands on — index 0 is DATA, not a sentinel (see
/// schema's `\z`/unknown-marker section).
///
/// `tables::generated` is emitted from exactly this slice, so a row's position
/// here IS its marker index.
pub static ROWS: &[MarkerRow] = &[
    // Index 0 — the generic EMPTY row. Every unresolved marker resolves here:
    // an unconfigured `\z` extension (bailed to by a first-byte test, without
    // any name match), an unknown name, an out-of-range digit. It is inert by
    // construction, and the walker keys its unwind-and-start-fresh recovery on
    // this index [F]. Its name is "" so no lexeme can ever match it by name.
    MarkerRow {
        marker: "",
        shape: SpellingShape::Any,
        kind: MarkerKind::Unknown,
        category: Category::Unknown,
        ws_after_name: Ws::NotRequired,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "add",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "addpn",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: true,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 5.9k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "b",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::SingleNewline, // ws: curated (onion MARKER_WHITESPACE row) — OVERRIDES the ParaPoetry default (TagEndDelimiter)
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: Some(5),
    },
    MarkerRow {
        marker: "bd",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharFormatting,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Bold),
        priority: None,
    },
    // [round 8 / Q-H2] <b> is the OUTER element; the <i> is a fixed export
    // template inside it, the same convention `\fig` uses. One marker, one
    // class
    MarkerRow {
        marker: "bdit",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharFormatting,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Bold),
        priority: None,
    },
    MarkerRow {
        marker: "bk",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 2.4k occurrences (en_ulb+bsb, 2026-08-10) — ~4% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "c",
        shape: SpellingShape::Any,
        kind: MarkerKind::Chapter,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // "Valid In: [Scripture] > [ChapterContent]" is a BREADCRUMB PATH, not a
        // list — the IMMEDIATE parent is ChapterContent, and Scripture is only the
        // grandparent. The chaptercontent rail names `c` as one of its own members
        // ("Chapters and Verses > c"), and the railroad diagram puts Chapter inside
        // ChapterContent's repeat group. So: ChapterContent, and not Scripture.
        //
        // The self-reference (`c` contributes ChapterContent AND is valid in it) is
        // correct, and is the general pattern for a REGION rather than a container:
        // a region's members both live in it and initialize it — exactly as `\h`
        // and `\toc1` do for BookHeaders (Will, 2026-08-12).
        allowed_contexts: &[SpecContext::ChapterContent],
        // Q16, ruled 2026-08-12: `\c` is a POINT — it takes no children and
        // paragraphs are its SIBLINGS, so it pushes NO frame. It still displaces
        // (an unclosed `\f` may not eat the next chapter), but that needs no
        // rank: the walker pops while the top frame's stamped context is not in
        // this row's mask, and `[ChapterContent]` forbids Para and Footnote alike.
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    MarkerRow {
        marker: "ca",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated (onion MARKER_WHITESPACE row) — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // Adjacency-checked (must immediately follow `\c`/each other): a lint
        // rule of shape (lastMarker, token), not a context. Empty = the context
        // machine abstains for this row.
        allowed_contexts: &[],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Sup),
        priority: None,
    },
    MarkerRow {
        marker: "cat",
        shape: SpellingShape::Any,
        kind: MarkerKind::Meta,
        category: Category::Meta,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::Sidebar,
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        // Will, 2026-08-12: `\cat category\cat*` is CHAR-SHAPED — it takes content
        // and requires its closer, exactly like `\nd`. USX expressing the category
        // as an attribute on the enclosing note/sidebar is a PROJECTION artifact,
        // the same class of thing as `cp`→`pubnumber`, and does not change what the
        // marker is in USFM. (The 3.1 grammar groups it in `Attribute.style.enum`
        // with ca/cp/va/vp for that USX reason — not evidence about USFM shape.)
        //
        // Kind/category stay `Meta` — that is TAXONOMY (3.2 gives `\cat` its own
        // top-level group) and taxonomy does not have to match behavior. Same
        // precedent as `\fig` (Figure kind, Character scope) and `\pb` (Character
        // kind, no scope). This is why no `ScopeKind::Meta` exists.
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        // Transparent, not Span: the category is metadata, so the term must not
        // render as inline text inside the note.
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "cd",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // [7] dual semantics: BEFORE chapter 1 (BookChapterLabel) this is the
    // book-wide word for "Chapter"; AFTER a `\c` it is that one chapter's own
    // label. Same marker, two jobs, decided by position
    MarkerRow {
        marker: "cl",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookChapterLabel, SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    MarkerRow {
        marker: "cls",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "cp",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated (onion MARKER_WHITESPACE row) — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // Valid in [Chapter] — inside the chapter it labels, not alongside it in
        // the book's content (Will, 2026-08-12).
        // Adjacency-checked (must immediately follow `\c`/each other): a lint
        // rule of shape (lastMarker, token), not a context. Empty = the context
        // machine abstains for this row.
        allowed_contexts: &[],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    MarkerRow {
        marker: "d",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "dc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "ef",
        shape: SpellingShape::Any,
        kind: MarkerKind::Note,
        category: Category::NoteFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            // Scope contexts curated 2026-08-18: the note pages' "Valid In"
            // speak only the positional band, but the CONTAINERS grant these —
            // para/index.html's content railroad lists Footnote and
            // CrossReference as paragraph content, and li/tc type their
            // content as VerseText (the same content model). Mirrors the
            // spec-given scope list on the `v` row. Without these, a note
            // opening inside its paragraph DISPLACES the paragraph.
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Aside),
        priority: None,
    },
    // [round 8] RECATEGORISED CharFormatting -> CharTextFeatures: 3.2
    // char/index.html lists `em` under "Text Features" ("Emphasis text"),
    // while "Text Formatting" is exactly bd/it/bdit/no/sc/sup. Element [round
    // 9 / 4] element is now <em>, its semantic twin
    MarkerRow {
        marker: "em",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Em),
        priority: None,
    },
    MarkerRow {
        marker: "esb",
        shape: SpellingShape::Any,
        kind: MarkerKind::Sidebar,
        category: Category::Sidebar,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent, SpecContext::PeripheralContent],
        opens_scope: Some(ScopeKind::Sidebar),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Aside),
        priority: None,
    },
    // [B] closes what `esb` opened; opens nothing
    MarkerRow {
        marker: "esbe",
        shape: SpellingShape::Any,
        kind: MarkerKind::Sidebar,
        category: Category::Sidebar,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent, SpecContext::PeripheralContent],
        opens_scope: None,
        closes_scope: Some(ScopeKind::Sidebar),
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Aside),
        priority: None,
    },
    MarkerRow {
        marker: "ex",
        shape: SpellingShape::Any,
        kind: MarkerKind::Note,
        category: Category::NoteCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            // Scope contexts curated 2026-08-18: the note pages' "Valid In"
            // speak only the positional band, but the CONTAINERS grant these —
            // para/index.html's content railroad lists Footnote and
            // CrossReference as paragraph content, and li/tc type their
            // content as VerseText (the same content model). Mirrors the
            // spec-given scope list on the `v` row. Without these, a note
            // opening inside its paragraph DISPLACES the paragraph.
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Aside),
        priority: None,
    },
    // [round 8 / Q-H5] Aside is the note BODY container. The `<sup>` caller
    // is rendered by the NoteCaller TOKEN KIND, not by this row — two render
    // surfaces, no special case
    // measured 10.5k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "f",
        shape: SpellingShape::Any,
        kind: MarkerKind::Note,
        category: Category::NoteFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            // Scope contexts curated 2026-08-18: the note pages' "Valid In"
            // speak only the positional band, but the CONTAINERS grant these —
            // para/index.html's content railroad lists Footnote and
            // CrossReference as paragraph content, and li/tc type their
            // content as VerseText (the same content model). Mirrors the
            // spec-given scope list on the `v` row. Without these, a note
            // opening inside its paragraph DISPLACES the paragraph.
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Aside),
        priority: Some(4),
    },
    MarkerRow {
        marker: "fdc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: true,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "fe",
        shape: SpellingShape::Any,
        kind: MarkerKind::Note,
        category: Category::NoteFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            // Scope contexts curated 2026-08-18: the note pages' "Valid In"
            // speak only the positional band, but the CONTAINERS grant these —
            // para/index.html's content railroad lists Footnote and
            // CrossReference as paragraph content, and li/tc type their
            // content as VerseText (the same content model). Mirrors the
            // spec-given scope list on the `v` row. Without these, a note
            // opening inside its paragraph DISPLACES the paragraph.
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Aside),
        priority: None,
    },
    // opens a Character scope, NOT onion's Block — see flags
    // 3.2 fig/fig.html: src/size/ref REQUIRED, alt/loc/copy optional, and NO
    // default attribute (onion said `src`)
    MarkerRow {
        marker: "fig",
        shape: SpellingShape::Any,
        kind: MarkerKind::Figure,
        category: Category::Figure,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[
            ("src", AttrStatus::Required),
            ("size", AttrStatus::Required),
            ("ref", AttrStatus::Required),
            ("alt", AttrStatus::Optional),
            ("loc", AttrStatus::Optional),
            ("copy", AttrStatus::Optional),
        ],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Figure),
        priority: None,
    },
    MarkerRow {
        marker: "fk",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "fl",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "fm",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        // Will, 2026-08-12: `\fm` is CHAR-LIKE, not a note peer. It is the only
        // member of the 20-row OptionalExplicitUntilNoteEnd family that is NOT
        // valid inside a note — it marks the reference point in the SCRIPTURE
        // TEXT ("use where multiple locations in the scripture text refer to a
        // common footnote text"), and every example on its page sits outside a
        // `\f`. So its closer is required, like any character marker's.
        //
        // TODO: INVESTIGATE WHAT TO DO WITH THIS:WILL
        // NOT MODELLED, deliberately: strictly the behaviour is "required outside
        // a note, optional (peer-closed) inside one" — a CONDITIONAL that
        // `ClosingBehavior` cannot express, the same shape as the conditional
        // attribute cardinality parked in ideas/committed/linter.md. The
        // outside-a-note case is taken as the row value because that is where the
        // spec shows it used; a `\fm` nested in a note is lint's problem.
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "fp",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "fq",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 1.4k occurrences (en_ulb+bsb, 2026-08-10) — ~2% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "fqa",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 4.9k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "fr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: Some(7),
    },
    // measured 5.8k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "ft",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: Some(6),
    },
    MarkerRow {
        marker: "fv",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "fw",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // [cap verified] 3.2 para/identification/h.html states NO range and marks
    // the `h#` syntax deprecated; the cap of 3 comes from usx.rng
    // [ruling] the numbered `h1`..`h3` SPELLINGS are deprecated; bare `\h` is
    // not, so the row is not marked deprecated
    MarkerRow {
        marker: "h",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIdentification,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "ib",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "id",
        shape: SpellingShape::Any,
        kind: MarkerKind::Header,
        category: Category::DocumentStructure,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::BookCode,
        numbered_max: Numbering::Unnumbered,
        // Their own marker pages (tcdocs markers/doc/{id,usfm}.adoc, master
        // 2026-08-10) both state `Valid In:: [BookHeaders]`. The doc index's
        // Scripture production instead lists BookIdentification = {id, usfm}; the
        // two spec pages CONTRADICT each other. Marker page wins — that is the
        // standing referee.
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Header),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "ide",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIdentification,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "ie",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "iex",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: ili1, ili2
    // [cap verified] 3.2 para/introductions/ili.html "The variable # (1-2)"
    MarkerRow {
        marker: "ili",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(2),
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "im",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "imi",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "imq",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: imt1, imt2, imt3, imt4
    // [cap verified] 3.2 para/introductions/imt.html "The variable # (1-4)"
    MarkerRow {
        marker: "imt",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
        ],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    // collapses: imte1, imte2
    // [cap verified] 3.2 para/introductions/imte.html "The variable # (1-2)"
    // [round 5 / 2] cap 1-2 confirmed by the 3.2 revisions: "The variable #
    // (1-2)"
    MarkerRow {
        marker: "imte",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(2),
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    // collapses: io1, io2, io3, io4
    // [cap verified] 3.2 para/introductions/io.html "The variable # (1-4)
    // represents the outline level"
    MarkerRow {
        marker: "io",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "ior",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "iot",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    MarkerRow {
        marker: "ip",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction, SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "ipc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "ipi",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "ipq",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "ipr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: iq1, iq2, iq3
    // [cap verified] 3.2 para/introductions/iq.html "The variable # (1-3)"
    MarkerRow {
        marker: "iq",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "iqt",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: is1, is2
    // [cap verified] 3.2 para/introductions/is.html "The variable # (1-2)"
    MarkerRow {
        marker: "is",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(2),
        allowed_contexts: &[SpecContext::BookIntroduction],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    MarkerRow {
        marker: "it",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharFormatting,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Italic),
        priority: None,
    },
    MarkerRow {
        marker: "jmp",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[
            ("href", AttrStatus::Optional),
            ("title", AttrStatus::Optional),
            ("id", AttrStatus::Optional),
            ("link-href", AttrStatus::Deprecated),
            ("link-title", AttrStatus::Deprecated),
            ("link-id", AttrStatus::Deprecated),
        ],
        default_attribute: Some("href"),
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Anchor),
        priority: None,
    },
    MarkerRow {
        marker: "k",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "lf",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // List curated 2026-08-18 (U25003): items are the \list-s
            // container's content, so they must be legal in the List context
            // the container contributes. Same-kind eviction in the walker is
            // what keeps siblings from nesting despite this.
            SpecContext::List,
        ],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "lh",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // List curated 2026-08-18 (U25003): items are the \list-s
            // container's content, so they must be legal in the List context
            // the container contributes. Same-kind eviction in the walker is
            // what keeps siblings from nesting despite this.
            SpecContext::List,
        ],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: li1, li2, li3, li4
    // [cap verified] 3.2 para/lists/li.html "The variable # (1-4)"
    // measured 1.5k occurrences (en_ulb+bsb, 2026-08-10) — ~2% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "li",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // List curated 2026-08-18 (U25003): items are the \list-s
            // container's content, so they must be legal in the List context
            // the container contributes. Same-kind eviction in the walker is
            // what keeps siblings from nesting despite this.
            SpecContext::List,
        ],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::ListItem),
        priority: None,
    },
    MarkerRow {
        marker: "lik",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::List],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: lim1, lim2, lim3, lim4
    // [cap verified] 3.2 para/lists/lim.html "The variable # (1-4)"
    MarkerRow {
        marker: "lim",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // List curated 2026-08-18 (U25003): items are the \list-s
            // container's content, so they must be legal in the List context
            // the container contributes. Same-kind eviction in the walker is
            // what keeps siblings from nesting despite this.
            SpecContext::List,
        ],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::ListItem),
        priority: None,
    },
    // 3.2 documents this as a PAIRED milestone (`\list-s\*` … `\list-e\*`,
    // ms/list.html) with no attributes; onion carried only the bare spelling
    MarkerRow {
        marker: "list",
        shape: SpellingShape::Any,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneList,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Milestone),
        closes_scope: None,
        defined_attributes: &[("sid", AttrStatus::Optional), ("eid", AttrStatus::Optional)],
        default_attribute: None,
        closing: ClosingBehavior::SelfClosingMilestone,
        deprecated: false,
        html_element: Some(HtmlElement::SelfClosingSpan),
        priority: None,
    },
    MarkerRow {
        marker: "lit",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookIntroduction, SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "litl",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::List],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: liv1, liv2, liv3, liv4, liv5
    // [cap verified] 3.2 char/lists/liv.html states NO range at all; usx.rng
    // enumerates liv1..liv5
    // [round 5 / 3] 3.2 char/lists/liv.html: character marker with `\liv
    // content\liv*`, numbered (`\liv1` in the examples) with NO stated
    // maximum — hence Unbounded, not onion's implied cap of 5
    MarkerRow {
        marker: "liv",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unbounded,
        allowed_contexts: &[SpecContext::List],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 0.9k occurrences (en_ulb+bsb, 2026-08-10) — ~1% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "m",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: mi1, mi2, mi3
    // [cap verified] 3.2 para/paragraphs/mi.html "The variable # (1-3)" —
    // note usx.rng enumerates `mi4`; the PAGE wins
    MarkerRow {
        marker: "mi",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "mr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: ms1, ms2, ms3
    // [cap verified] 3.2 para/titles-sections/ms.html "The variable # (1-3)"
    MarkerRow {
        marker: "ms",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    // collapses: mt1, mt2, mt3, mt4
    // [cap verified] 3.2 para/titles-sections/mt.html "The variable # (1-4)"
    MarkerRow {
        marker: "mt",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroductionEndTitles,
        ],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    // collapses: mte1, mte2
    // [cap verified] 3.2 para/titles-sections/mte.html "The variable # (1-2)"
    MarkerRow {
        marker: "mte",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(2),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    MarkerRow {
        marker: "nb",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "nd",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // [round 8 / Q-H2] stays Span: HTML has no "normal text" element
    MarkerRow {
        marker: "no",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharFormatting,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "ord",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 18.0k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "p",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: Some(2),
    },
    // [C] re-kinded Character/CharBreaks (onion: Paragraph/Other) — this is
    // why it opens no scope
    MarkerRow {
        marker: "pb",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharBreaks,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "pc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "periph",
        shape: SpellingShape::Any,
        kind: MarkerKind::Periph,
        category: Category::Peripheral,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Peripheral],
        opens_scope: Some(ScopeKind::Periph),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Section),
        priority: None,
    },
    // [cap verified] 3.2 para/paragraphs/ph.html "The variable # (1-3)",
    // "Deprecated: 3.0"
    MarkerRow {
        marker: "ph",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: true,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: pi1, pi2, pi3
    // [cap verified] 3.2 para/paragraphs/pi.html "The variable # (1-3)"
    MarkerRow {
        marker: "pi",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "pm",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "pmc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "pmo",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "pmr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "pn",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "png",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "po",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "pr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "pro",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: true,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: q1, q2, q3, q4
    // [cap verified] 3.2 para/poetry/q.html "The variable # (1-4) represents
    // the level of indent"
    // measured 47.0k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "q",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: Some(1),
    },
    MarkerRow {
        marker: "qa",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: None,
    },
    MarkerRow {
        marker: "qac",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Para],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "qc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "qd",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // collapses: qm1, qm2, qm3
    // [cap verified] 3.2 para/poetry/qm.html "The variable # (1-3)"
    MarkerRow {
        marker: "qm",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "qr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "qs",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Para],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: qt-e, qt-s, qt1-e, qt1-s, qt2-e, qt2-s, qt3-e, qt3-s, qt4-e, qt4-s, qt5-e, qt5-s
    // [cap verified] 3.2 ms/qt.html — # is the nesting level (1-5)
    // [1] overloaded name — see the MilestoneOnly row above and
    // (qt overload: two shape-keyed rows)
    MarkerRow {
        marker: "qt",
        shape: SpellingShape::MilestoneOnly,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneQt,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(5),
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Milestone),
        closes_scope: None,
        defined_attributes: &[
            ("who", AttrStatus::Optional),
            ("sid", AttrStatus::Optional),
            ("eid", AttrStatus::Optional),
        ],
        default_attribute: Some("who"),
        closing: ClosingBehavior::SelfClosingMilestone,
        deprecated: false,
        html_element: Some(HtmlElement::SelfClosingSpan),
        priority: None,
    },
    // [cap verified] 3.2 ms/qt.html — # is the nesting level (1-5)
    // [1] overloaded name — see the MilestoneOnly row above and
    // (qt overload: two shape-keyed rows)
    MarkerRow {
        marker: "qt",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 1.3k occurrences (en_ulb+bsb, 2026-08-10) — ~2% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "r",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "rb",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[("gloss", AttrStatus::Optional)],
        default_attribute: Some("gloss"),
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Ruby),
        priority: None,
    },
    MarkerRow {
        marker: "ref",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[("loc", AttrStatus::Optional)],
        default_attribute: Some("loc"),
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Anchor),
        priority: None,
    },
    MarkerRow {
        marker: "rem",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIdentification,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        // rem's own page (markers/para/rem.adoc) states `Valid In:: [BookHeaders]`
        // and nothing else, though its Description says rem "can be used for adding
        // non-publishable remarks/comments anywhere within a text" and the doc index
        // lists it under BookTitles and BookIntroduction too. Page wins; the prose
        // and the index are recorded here as the known tension.
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "rq",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: s1, s2, s3, s4
    // [cap verified] 3.2 para/titles-sections/s.html "The variable # (1-4)"
    // measured 17.0k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "s",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated (onion MARKER_WHITESPACE row) — OVERRIDES the ParaTitlesSections default (AtLeastOneHorizontalWhitespace)
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Heading),
        priority: Some(3),
    },
    // [round 8 / Q-H2] stays Span: HTML has no smallcaps element
    MarkerRow {
        marker: "sc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharFormatting,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: sd1, sd2, sd3, sd4
    // [cap verified] 3.2 para/titles-sections/sd.html "The variable # (1-4)"
    // [round 9 / 5] an EMPTY spacer <div>, not a heading and not <hr>: the
    // spec's own layout example renders \sd# as vertical blank space between
    // text blocks. No heading base level, no clamp; the digit is preserved in
    // data-marker
    MarkerRow {
        marker: "sd",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(4),
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Div),
        priority: None,
    },
    MarkerRow {
        marker: "sig",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "sls",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "sp",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "sr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    MarkerRow {
        marker: "sts",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIdentification,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "sup",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharFormatting,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Sup),
        priority: None,
    },
    // 3.2 char/features/ta.html (read 2026-08-14): `\ta content|@a-<identifier>\ta*`
    // — "one or more attributes for providing text alternatives. Each attribute
    // should begin with `a-`". No fixed names exist, so this row carries the
    // PREFIX WILDCARD spelling ruled at `defined_attributes`; the "one or more"
    // is family-level cardinality and therefore lint's. Added by USFM 3.1.2.
    MarkerRow {
        marker: "ta",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[("a-*", AttrStatus::Optional)],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // 3.2: paired `\table-s\*` … `\table-e\*` (ms/table.html), no attributes
    // documented
    MarkerRow {
        marker: "table",
        shape: SpellingShape::Any,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneTable,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Milestone),
        closes_scope: None,
        defined_attributes: &[("sid", AttrStatus::Optional), ("eid", AttrStatus::Optional)],
        default_attribute: None,
        closing: ClosingBehavior::SelfClosingMilestone,
        deprecated: false,
        html_element: Some(HtmlElement::SelfClosingSpan),
        priority: None,
    },
    // [O] all six cell stems use TableColumns
    MarkerRow {
        marker: "tc",
        shape: SpellingShape::Any,
        kind: MarkerKind::TableCell,
        category: Category::CharTables,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::TableColumns,
        allowed_contexts: &[SpecContext::Table],
        opens_scope: Some(ScopeKind::TableCell),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::TableCell),
        priority: None,
    },
    MarkerRow {
        marker: "tcc",
        shape: SpellingShape::Any,
        kind: MarkerKind::TableCell,
        category: Category::CharTables,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::TableColumns,
        allowed_contexts: &[SpecContext::Table],
        opens_scope: Some(ScopeKind::TableCell),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::TableCell),
        priority: None,
    },
    MarkerRow {
        marker: "tcr",
        shape: SpellingShape::Any,
        kind: MarkerKind::TableCell,
        category: Category::CharTables,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::TableColumns,
        allowed_contexts: &[SpecContext::Table],
        opens_scope: Some(ScopeKind::TableCell),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::TableCell),
        priority: None,
    },
    MarkerRow {
        marker: "th",
        shape: SpellingShape::Any,
        kind: MarkerKind::TableCell,
        category: Category::CharTables,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::TableColumns,
        allowed_contexts: &[SpecContext::Table],
        opens_scope: Some(ScopeKind::TableCell),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::TableCell),
        priority: None,
    },
    MarkerRow {
        marker: "thc",
        shape: SpellingShape::Any,
        kind: MarkerKind::TableCell,
        category: Category::CharTables,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::TableColumns,
        allowed_contexts: &[SpecContext::Table],
        opens_scope: Some(ScopeKind::TableCell),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::TableCell),
        priority: None,
    },
    MarkerRow {
        marker: "thr",
        shape: SpellingShape::Any,
        kind: MarkerKind::TableCell,
        category: Category::CharTables,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::TableColumns,
        allowed_contexts: &[SpecContext::Table],
        opens_scope: Some(ScopeKind::TableCell),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::TableCell),
        priority: None,
    },
    // 3.2 char/features/tl.html (read 2026-08-14): `\tl content|@lang\tl*` —
    // `lang` is the DEFAULT attribute, "source language of the transliterated
    // text according to ISO639-1 (2 letter codes)". Added by USFM 3.1.2.
    MarkerRow {
        marker: "tl",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[("lang", AttrStatus::Optional)],
        default_attribute: Some("lang"),
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // collapses: toc1, toc2, toc3
    // [cap verified] 3.2 para/identification/toc.html "The variable # (1-3)
    // represents the book name form"
    MarkerRow {
        marker: "toc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIdentification,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    // collapses: toca1, toca2, toca3
    // [cap verified] 3.2 para/identification/toca.html "The variable # (1-3)
    // represents the book name form"
    MarkerRow {
        marker: "toca",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIdentification,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::UpTo(3),
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    MarkerRow {
        marker: "tr",
        shape: SpellingShape::Any,
        kind: MarkerKind::TableRow,
        category: Category::ParaTables,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // Table curated 2026-08-18 (U25003): rows are the \table-s
            // container's content. Same-kind eviction (a row ends the open
            // cells and row before it) keeps siblings from nesting.
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::TableRow),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::TableRow),
        priority: None,
    },
    // collapses: ts-e, ts-s
    MarkerRow {
        marker: "ts",
        shape: SpellingShape::Any,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneTs,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // Scope contexts curated 2026-08-18: usfmtc's USJ keeps `\ts-s`
            // INSIDE the open paragraph, list item, and table cell (probe in
            // scratchpad, three cases) — with ChapterContent alone this row
            // would DISPLACE its host. Same gap class as the note rows.
            // `list`/`table` deliberately do NOT get these: usfmtc renders
            // their content BESIDE the paragraph, so displacing is correct.
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Milestone),
        closes_scope: None,
        defined_attributes: &[("sid", AttrStatus::Optional), ("eid", AttrStatus::Optional)],
        default_attribute: None,
        closing: ClosingBehavior::SelfClosingMilestone,
        deprecated: false,
        html_element: Some(HtmlElement::SelfClosingSpan),
        priority: None,
    },
    MarkerRow {
        marker: "usfm",
        shape: SpellingShape::Any,
        kind: MarkerKind::Header,
        category: Category::DocumentStructure,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: derived from category default
        payload: Payload::Version,
        numbered_max: Numbering::Unnumbered,
        // Their own marker pages (tcdocs markers/doc/{id,usfm}.adoc, master
        // 2026-08-10) both state `Valid In:: [BookHeaders]`. The doc index's
        // Scripture production instead lists BookIdentification = {id, usfm}; the
        // two spec pages CONTRADICT each other. Marker page wins — that is the
        // standing referee.
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Header),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Transparent),
        priority: None,
    },
    // measured 62.0k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "v",
        shape: SpellingShape::Any,
        kind: MarkerKind::Verse,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneWhitespace, // ws: curated (onion MARKER_WHITESPACE row) — OVERRIDES the ChapterVerse default (AtLeastOneHorizontalWhitespace)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::Scripture,
            SpecContext::ChapterContent,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        // Q16, ruled 2026-08-12: `\v` is a POINT — it takes no children and
        // paragraphs are its SIBLINGS, so it pushes NO frame. It still displaces
        // an unclosed note (a note's content set has `\fv` for verse numbers, so
        // a bare `\v` means the note is definitively unclosed), but that needs
        // no rank: the walker pops while the top frame's stamped context is not
        // in this row's mask, and Footnote is not in it.
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Sup),
        priority: Some(0),
    },
    MarkerRow {
        marker: "va",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated (onion MARKER_WHITESPACE row) — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // Adjacency-checked (must immediately follow `\v`/each other): a lint
        // rule of shape (lastMarker, token), not a context. Empty = the context
        // machine abstains for this row.
        allowed_contexts: &[],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Sup),
        priority: None,
    },
    // collapses: vid-e, vid-s
    // [4] STANDALONE, not paired: docs.usfm.bible/usfm/3.2/ms/vid.html gives
    // `\vid|@h @ref\*` — `ref` required and default, `h` optional, no sid/eid
    MarkerRow {
        marker: "vid",
        shape: SpellingShape::Any,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneVid,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        // ChapterContent ONLY, on purpose (re-confirmed 2026-08-18): `\vid`
        // is a fragment-header milestone that stands BETWEEN structures —
        // usfm-grammar master gives it `_chapterContent` placement only, and
        // its new-vid-milestone fixture shows it on its own line before \s1 /
        // between paragraphs. Displacing an open paragraph is therefore
        // correct, unlike `ts` (see that row).
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Milestone),
        closes_scope: None,
        defined_attributes: &[("ref", AttrStatus::Required), ("h", AttrStatus::Optional)],
        default_attribute: Some("ref"),
        closing: ClosingBehavior::SelfClosingMilestone,
        deprecated: false,
        html_element: Some(HtmlElement::SelfClosingSpan),
        priority: None,
    },
    MarkerRow {
        marker: "vp",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated (onion MARKER_WHITESPACE row) — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // Adjacency-checked (must immediately follow `\v`/each other): a lint
        // rule of shape (lastMarker, token), not a context. Empty = the context
        // machine abstains for this row.
        allowed_contexts: &[],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Sup),
        priority: None,
    },
    MarkerRow {
        marker: "w",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[
            ("lemma", AttrStatus::Optional),
            ("strong", AttrStatus::Optional),
            ("srcloc", AttrStatus::Optional),
        ],
        default_attribute: Some("lemma"),
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "wa",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "wg",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "wh",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // [3] overloaded name, second member of the SpellingShape class. The
    // MilestoneOnly row rests on tcdocs/usx.rng 1883-1884 ALONE — 3.2
    // documents `wj` only as a character marker. See flags
    MarkerRow {
        marker: "wj",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // 3.2 char/features/wl.html (read 2026-08-14): `\wl content|@lang\wl*` —
    // same shape as `tl`, `lang` is the DEFAULT attribute. Added by 3.1.2.
    MarkerRow {
        marker: "wl",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[("lang", AttrStatus::Optional)],
        default_attribute: Some("lang"),
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // [round 8 / Q-H5] see `f`
    MarkerRow {
        marker: "x",
        shape: SpellingShape::Any,
        kind: MarkerKind::Note,
        category: Category::NoteCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated (onion MARKER_WHITESPACE row)
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            // Scope contexts curated 2026-08-18: the note pages' "Valid In"
            // speak only the positional band, but the CONTAINERS grant these —
            // para/index.html's content railroad lists Footnote and
            // CrossReference as paragraph content, and li/tc type their
            // content as VerseText (the same content model). Mirrors the
            // spec-given scope list on the `v` row. Without these, a note
            // opening inside its paragraph DISPLACES the paragraph.
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Aside),
        priority: None,
    },
    MarkerRow {
        marker: "xdc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: true,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "xk",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "xnt",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "xo",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "xop",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "xot",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "xq",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // 3.2 char/notes/crossref/xt.html: no live attributes; `link-href`
    // deprecated, and using `\xt` outside a cross-reference is deprecated too
    // (the marker itself is not)
    // measured 3.2k occurrences (en_ulb+bsb, 2026-08-10)
    MarkerRow {
        marker: "xt",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::Section,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[("link-href", AttrStatus::Deprecated)],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Anchor),
        priority: Some(8),
    },
    MarkerRow {
        marker: "xta",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotes,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
];

/// How many families the translation collapsed. Reported by
/// `cargo run --bin codegen`.
pub const COLLAPSED_FAMILIES: usize = 23;

/// Rows whose fine [`Category`] needed a ruling because the spec group was
/// ambiguous, contradictory, or absent.
///
/// **All 20 rulings reviewed and CONFIRMED CORRECT by Will, 2026-08-10**
/// (round 6 / [G]) — the eyeball item is closed. Kept as the audit record of
/// how each was decided, not as an open list.
pub static CATEGORY_JUDGEMENT_CALLS: &[(&str, &str)] = &[
    (
        "b",
        "CONFIRMED [6] ParaPoetry. Not a doc-reading error on either side: USFM 3.2's own para index lists `b` under BOTH \"Body Paragraphs\" and \"Poetry\" (docs.usfm.bible/usfm/3.2/para/index.html), so either value is spec-faithful. Will: \"poetry or prose is fine, doesn't matter\".",
    ),
    (
        "ca",
        "CONFIRMED [12] as `cp` — 3.2 cv/index.html \"Alternate chapter number\".",
    ),
    (
        "cat",
        "CONFIRMED [13] its own kind=Meta / category=Meta pair. 3.2 gives `\\cat` its own top-level group (cat/index.html), which supports not folding it into Sidebar.",
    ),
    (
        "cl",
        "CONFIRMED [7] ParaTitlesSections — 3.2 files `cl` under Titles and Sections. Contexts now include BookChapterLabel. Dual semantics, on the row: BEFORE chapter 1 it is the book-wide word for \"Chapter\"; AFTER a `\\c` it is that chapter's own label.",
    ),
    (
        "cp",
        "CONFIRMED [12] cross-kind ChapterVerse, and 3.2 corroborates the grouping: ca/cp/v/va/vp all live in docs.usfm.bible/usfm/3.2/cv/, not in para/. Milestone-like: expects a payload, opens no scope.",
    ),
    (
        "fig",
        "CONFIRMED [2] opens_scope Some(Character), contexts BookTitles / BookIntroduction / BookIntroductionEndTitles / ChapterContent / PeripheralContent. 3.2 gives it its own top-level group (fig/fig.html), which is why it keeps a trivial Category::Figure.",
    ),
    (
        "id",
        "CONFIRMED [13] kind Header, category DocumentStructure — 3.2 puts `id`/`usfm` in their own doc/ group, not in para/.",
    ),
    (
        "ipc",
        "CONFIRMED [11] ParaIntroductions. No row needed ADDING: onion already carries `ipc`, and 3.2 para/index.html lists it under \"Introductions\".",
    ),
    (
        "k1 / k2",
        "CONFIRMED [round 9 / 1] — ERRATA, no rows. `k` itself is CONFIRMED as the CHARACTER keyword marker (docs.usfm.bible/usfm/3.1/char/features/k.html, and 3.2 char/index.html \"Keyword/keyterm\"), so the collision fix stands. As for the paragraph forms: tcdocs/usx.rng declares `k1` (line 1161) and `k2` (line 1163) as PARAGRAPH styles in OtherPara.para.style.enum (\"Concordance main entry text or keyword, level 1/2\"), but 3.2's posted para index documents no `k#` paragraph at all — and we go by the posted docs. Same class as `t-s`/`t-e` and `wj-s`/`wj-e`. Our only `k` row stays the CHARACTER marker (3.2 char/index.html, \"Keyword/keyterm\"). Both names remain in `V_FORBIDDEN_IN_PARAGRAPHS` as rail membership, spelled `k1`/`k2` so they cannot be confused with the character `\\k`.",
    ),
    (
        "lit",
        "CONFIRMED [13] ParaBody — 3.2 para/index.html lists `lit` under \"Body Paragraphs\".",
    ),
    (
        "p1",
        "CONFIRMED [10] ParaPeripheral. Corroborated by 3.2: `p1`/`p2` are ABSENT from para/index.html entirely, matching the 3.1.1 move to PeriphPara (\"not used in scripture files\"). Bare `p` stays ParaBody. Name kept per [round 4 / 7].",
    ),
    ("p2", "CONFIRMED [10] as `p1`."),
    (
        "restore",
        "CONFIRMED [round 9 / 1] — ERRATA, no row. tcdocs/usx.rng declares `restore` (line 894) in SectionPara.para.style.enum; 3.2's posted para index does not document it, and onion referenced it in `is_non_inline_paragraph_marker_name` without ever giving it a row. Listed in `V_FORBIDDEN_IN_PARAGRAPHS` as rail membership only.",
    ),
    (
        "rq",
        "CONFIRMED [13] CharTextFeatures — 3.2 char/index.html lists `rq` under \"Text Features\" as \"Inline quotation refs\".",
    ),
    (
        "t",
        "CONFIRMED [round 4 / 2] — ROW DELETED as errata. `t-s`/`t-e` ARE in tcdocs/usx.rng Milestone.style.enum lines 1868-1869 with sid?/eid?, so this is not an oversight: the marker is in the 3.1 grammar. But USFM 3.2 documents exactly five milestones (list, table, qt#, ts, vid — docs.usfm.bible/usfm/3.2/ms/index.html) and there is no ms/t.html. Under the 3.2-only version policy the grammar entry does not survive. Deleted.",
    ),
    (
        "ta",
        "CONFIRMED [8] CharTextFeatures — 3.2 char/index.html lists `ta` under \"Text Features\" as \"Text alternatives\".",
    ),
    (
        "tr",
        "CONFIRMED [13] TableRow + ParaTables — 3.2 para/index.html lists `tr` under \"Tables\". The CELL markers keep CharTables because 3.2 files th#/thr#/thc#/tc#/tcr#/tcc# under Characters > Tables; see the MarkerKind::TableCell flag.",
    ),
    ("usfm", "CONFIRMED [13] as `id`."),
    (
        "va",
        "CONFIRMED [12] as `ca` — 3.2 cv/index.html \"Alternate verse number\".",
    ),
    (
        "vid",
        "FIXED [round 4 / 4] — standalone, NOT paired. docs.usfm.bible/usfm/3.2/ms/vid.html gives `\\vid|@h @ref\\*` with `ref` REQUIRED and the default attribute, `h` optional, and no sid/eid. The sid/eid this row briefly carried are gone.",
    ),
    (
        "vp",
        "CONFIRMED [12] as `ca` — 3.2 cv/index.html \"Published verse number\".",
    ),
    (
        "wj",
        "CONFIRMED [round 5 / 1] — MilestoneOnly row DELETED, `wj` is CHAR-ONLY. Same errata class as `t-s`/`t-e`, and on the same evidence: present in tcdocs/usx.rng 1883-1884, absent from 3.2's milestone index, ms/wj.html 404s, and 3.2 files `wj` under Characters > Text Features. The Category::MilestoneWj variant added for it is gone too. SpellingShape is back to one user (`qt`) — the mechanism stays, it is still right.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::schema;

    /// [C] every row's fine category must be legal for its coarse kind.
    #[test]
    fn kind_and_category_are_coherent() {
        for row in ROWS {
            assert!(
                row.is_coherent(),
                "{}: {:?} is not a legal category for {:?}",
                row.marker,
                row.category,
                row.kind
            );
        }
    }

    /// [G] the lookup key is (name, shape): two rows may share a name only
    /// when their shapes are disjoint. Names must also fit the u64 load.
    #[test]
    fn lookup_keys_are_unambiguous_and_u64_sized() {
        for (i, row) in ROWS.iter().enumerate() {
            assert!(
                row.marker.len() <= 8,
                "name too long for a u64: {}",
                row.marker
            );
            assert!(
                !row.marker.ends_with("-s") && !row.marker.ends_with("-e"),
                "milestone side must be stripped: {}",
                row.marker
            );
            for other in &ROWS[i + 1..] {
                assert!(
                    other.marker != row.marker || !row.shape.overlaps(other.shape),
                    "ambiguous lookup key: {} claimed by {:?} and {:?}",
                    row.marker,
                    row.shape,
                    other.shape
                );
            }
        }
    }

    /// [12]/[C] the behaviour the fine category is load-bearing for: a marker
    /// that takes a payload but opens no scope, and `\pb`.
    ///
    /// Rewritten 2026-08-12 for Q16: the whole ChapterVerse group now pushes NO
    /// frame — `\c`/`\v` are Points, and `ca`/`cp`/`va`/`vp` always were.
    ///
    /// Note what this test can NO LONGER assert, because the fact stopped
    /// existing: which of them "displaces". Displacement is not a per-row value
    /// at all — the walker pops while the top frame's stamped context is absent
    /// from the row's context mask. So `\pb` leaving its paragraph alone (onion's
    /// bug) is now guaranteed by `pb`'s contexts INCLUDING `Para`, which the
    /// spec-diff in planning/spec_contexts_diff.py checks against the docs. There
    /// is nothing left here to pin.
    #[test]
    fn category_drives_scope_not_kind() {
        for row in ROWS {
            let opens = row.opens_scope.is_some();
            match row.category {
                Category::CharBreaks => assert!(!opens, "{} must open no scope", row.marker),
                Category::ChapterVerse => assert!(
                    !opens,
                    "{}: nothing in the ChapterVerse group pushes a frame (Q16)",
                    row.marker
                ),
                _ => {}
            }
        }
    }

    /// A default attribute must be one of the attributes the spec defines,
    /// and a DEPRECATED attribute can never be the default.
    #[test]
    fn default_attribute_is_a_live_defined_attribute() {
        for row in ROWS {
            let Some(default) = row.default_attribute else {
                continue;
            };
            let found = row
                .defined_attributes
                .iter()
                .find(|(name, _)| *name == default);
            let Some((_, status)) = found else {
                panic!(
                    "{}: default `{}` is not in defined_attributes",
                    row.marker, default
                );
            };
            assert_ne!(
                *status,
                AttrStatus::Deprecated,
                "{}: default `{}` is deprecated",
                row.marker,
                default
            );
        }
    }

    /// [D] every marker named in the v-forbidden set resolves to a row. No
    /// exceptions: a rule about a marker that does not exist is cruft, so a name
    /// with no row means the LIST is wrong, not that the test needs an allowance.
    #[test]
    fn v_forbidden_markers_resolve_to_rows() {
        for marker in schema::V_FORBIDDEN_IN_PARAGRAPHS {
            let found = ROWS.iter().any(|row| row.marker == *marker);
            assert!(found, "v-forbidden marker `{}` has no row", marker);
        }
    }

    /// No marker may define the same attribute name twice.
    #[test]
    fn defined_attributes_have_no_duplicate_names() {
        for row in ROWS {
            for (i, (name, _)) in row.defined_attributes.iter().enumerate() {
                assert!(
                    !row.defined_attributes[i + 1..]
                        .iter()
                        .any(|(n, _)| n == name),
                    "{}: duplicate attribute `{}`",
                    row.marker,
                    name
                );
            }
        }
    }
}
