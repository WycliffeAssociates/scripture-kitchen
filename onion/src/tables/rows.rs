//! The authored marker table — one [`MarkerRow`] per canonical marker.
//!
//! **Referee: USFM 3.2** (docs.usfm.bible/usfm/3.2/). There is no version
//! column and no version tracking: where 3.2 deprecates something, that is what
//! `deprecated` / `AttrStatus::Deprecated` mean. The 3.1 tcdocs/usx.rng is
//! fallback grammar evidence only and never outvotes a 3.2 page. Where a cell
//! needed a judgement call the spec would not settle, the row carries a
//! one-line `// curation:` note naming the fact and the evidence for it.
//!
//! Conventions worth knowing when reading rows: families are collapsed to their
//! canonical stem (`qt3-s` → `qt`; strip `-s`/`-e` first, then digits);
//! overloaded names split by [`SpellingShape`] (`qt` has two rows); every row
//! carries a `// ws:` comment saying whether the value was curated or derived;
//! `priority` counts are MEASURED.
//!

use mise::extensions::ExtensionCategory;

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
    // this index. Its name is "" so no lexeme can ever match it by name.
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
        // Every unknown marker resolves here, so it must carry a wrapper: the
        // class is built off the TOKEN's lexeme (this row's `marker` is `""`),
        // which is how `\s5` reaches the page as `class="usfm-s5"`.
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // ---- Character markers inside notes: the class-wide curation ---------
    //
    // Every scope-opening Character row carries `Footnote` and `CrossReference`,
    // whether or not its page's "Valid In" list says so. Two strands of
    // evidence: usfmtc NESTS them (bsb GEN 2:4's `\f + \fr 2:4 \fq \+nd
    // Lord\+nd*…\f*` reads note:f → char:fq → char:nd with the footnote intact),
    // and the spec contradicts its own "Valid In" lists in its examples (`\jmp`
    // inside `\ef`, `\dc` inside `\x`). Without the two contexts the walker
    // DISPLACES an open note at `\+nd`, which on real scripture yields a wrong
    // tree, a spurious unclosed-note, an orphan `\f*`, and a lint fix that would
    // truncate a good footnote.
    //
    // The CLOSING column is deliberately NOT touched: character markers still
    // require explicit closure everywhere, inside notes included.
    //
    // Deliberately outside the class: `pb` (CharBreaks) opens no scope, so its
    // mask is not load-bearing, and `fig` is its own kind.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // measured 5.9k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "b",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::SingleNewline, // ws: curated — OVERRIDES the ParaPoetry default (TagEndDelimiter)
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        // `\b` is "blank line… always empty" — the same shape `sd` gets `Div`
        // for, and an empty inline span cannot be a stanza break.
        html_element: Some(HtmlElement::Div),
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // <b> is the OUTER element; the <i> is a fixed export template inside it,
    // the same convention `\fig` uses. One marker, one class.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // measured 2.4k occurrences (en_ulb+bsb) — ~4% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "c",
        shape: SpellingShape::Any,
        kind: MarkerKind::Chapter,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // "Valid In: [Scripture] > [ChapterContent]" is a BREADCRUMB PATH, not a
        // list: the IMMEDIATE parent is ChapterContent and Scripture is only the
        // grandparent, so this mask is ChapterContent alone.
        //
        // The self-reference (`c` contributes ChapterContent AND is valid in it)
        // is the general pattern for a REGION rather than a container: a
        // region's members both live in it and initialize it, exactly as `\h`
        // and `\toc1` do for BookHeaders.
        allowed_contexts: &[SpecContext::ChapterContent],
        // `\c` is a POINT — it takes no children and paragraphs are its
        // SIBLINGS, so it pushes NO frame. It still displaces (an unclosed `\f`
        // may not eat the next chapter), but that needs no rank: the walker pops
        // while the top frame's context is absent from this row's mask, and
        // `[ChapterContent]` forbids Para and Footnote alike.
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        // Not a heading: 3.2 files `c` under "Chapters and Verses", not "Titles
        // and Sections", with TextType ChapterNumber — structural, not display.
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "ca",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // ---- `ca`/`va`/`vp` open Character scopes -----------------------
        //
        // These three are `kind: Character` with `closing: RequiredExplicit`, so
        // without a scope they would demand a closer while pushing no frame for
        // it to close: `\ca 2\ca*` drew a spurious `orphan-closer` and an
        // unclosed `\ca` drew nothing. Opening the scope makes the closer close
        // something and the omission an `unclosed-char`.
        //
        // THE MASK IS THEREFORE LOAD-BEARING. The walker pops while the top
        // frame's stamped context is NOT in this row's mask, so an EMPTY mask
        // pops EVERYTHING. These rows take the same slice every scope-opening
        // character row carries, which is what makes their two real placements
        // nest instead of displace: `\ca` after `\c` (a POINT, so the stack is
        // usually just the never-popped root) and `\va`/`\vp` after `\v` inside
        // a paragraph (`Para`/`List`/`Table` keep the `\p` open).
        //
        // These are CONTAINER contexts only. "Right after `\c`/`\v`" is not a
        // context — chapters and verses repeat, so no monotonic encoding works —
        // and the adjacency of `ca`/`cp`/`va`/`vp` stays a lint rule of shape
        // (lastMarker, token) reading TOKENS, not the CST.
        //
        // `cp` is deliberately NOT part of this: it is a Paragraph row with
        // `closing: None` — a published chapter LABEL and nothing else — so it
        // has no closer to orphan and keeps its empty, non-load-bearing slice.
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
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: curated
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::Sidebar,
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        // `\cat category\cat*` is CHAR-SHAPED — it takes content and requires its
        // closer, exactly like `\nd`. USX expressing the category as an attribute
        // on the enclosing note/sidebar is a PROJECTION artifact, the same class
        // of thing as `cp`→`pubnumber`, and does not change what the marker is in
        // USFM.
        //
        // Kind/category stay `Meta` — that is TAXONOMY, and taxonomy does not
        // have to match behavior. Same precedent as `\fig` (Figure kind,
        // Character scope) and `\pb` (Character kind, no scope), which is why no
        // `ScopeKind::Meta` exists.
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        // `\cat` is publishable vernacular content and char-shaped, so it gets
        // the class every char-shaped sibling gets.
        html_element: Some(HtmlElement::Span),
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
    // dual semantics: BEFORE chapter 1 (BookChapterLabel) this is the
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
        // Its own page: "classified as a paragraph-level element, not a heading
        // or title marker" — so no `HEADING_BASE_LEVEL` entry either.
        html_element: Some(HtmlElement::Para),
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
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // Valid in [Chapter] — inside the chapter it labels, not alongside it in
        // the book's content. Adjacency (it must immediately follow `\c`) is a
        // lint rule of shape (lastMarker, token), not a context, so the empty
        // mask means the context machine abstains for this row.
        allowed_contexts: &[],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        // `cp` is a display OVERRIDE for `c`, exactly the relationship `vp` has
        // to `v` — so it matches `vp`'s `Sup`, and is no heading.
        html_element: Some(HtmlElement::Sup),
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Para/List/Table — the containers grant note content, so a
            // note inside its paragraph nests instead of displacing it (para railroad)
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            // curation: +Section — notes belong in headings too (testData doo43-4)
            SpecContext::Section,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // CharTextFeatures, not CharFormatting: 3.2 char/index.html lists `em` under
    // "Text Features", and "Text Formatting" is exactly bd/it/bdit/no/sc/sup.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
        ws_after_name: Ws::TagEndDelimiter, // ws: curated
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
            // curation: +Para/List/Table — the containers grant note content, so a
            // note inside its paragraph nests instead of displacing it (para railroad)
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            // curation: +Section — notes belong in headings too (testData doo43-4)
            SpecContext::Section,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // Aside is the note BODY container; the `<sup>` caller is rendered by the
    // NoteCaller TOKEN KIND, not by this row.
    // measured 10.5k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "f",
        shape: SpellingShape::Any,
        kind: MarkerKind::Note,
        category: Category::NoteFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            // curation: +Para/List/Table — the containers grant note content, so a
            // note inside its paragraph nests instead of displacing it (para railroad)
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            // curation: +Section — notes belong in headings too (testData doo43-4)
            SpecContext::Section,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        // A note sits MID-SENTENCE inside a `<p>`, whose content model is
        // PHRASING only: `<aside>` is flow content, so it would be invalid there
        // and the parser would close the `<p>` early, silently breaking the
        // paragraph. `esb`/`esbe` keep `Aside` — a sidebar interrupts BETWEEN
        // paragraphs, which is exactly where `<aside>` is legal.
        html_element: Some(HtmlElement::Span),
        priority: Some(4),
    },
    MarkerRow {
        marker: "fdc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
            // curation: +Para/List/Table — the containers grant note content, so a
            // note inside its paragraph nests instead of displacing it (para railroad)
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            // curation: +Section — notes belong in headings too (testData doo43-4)
            SpecContext::Section,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // src/size/ref REQUIRED, alt/loc/copy optional, and NO default attribute.
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
            // curation: +Para/Footnote — `\fig` opens a scope, and with a mask
            // no container grants a mid-verse `\fig` displaced its own
            // paragraph. Every fixture nests it where it sits, in a paragraph
            // (paratextTests/FigureAttributesAreValid) or in a footnote's `\ft`
            // (advanced/figureInNote); only those two demonstrated contexts.
            SpecContext::Para,
            SpecContext::Footnote,
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
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesFootnote,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        // `\fm` is CHAR-LIKE, not a note peer: it marks the reference point in
        // the SCRIPTURE TEXT, and every example on its page sits outside a `\f`.
        // So its closer is required, like any character marker's.
        //
        // TODO: strictly the behaviour is "required outside a note, optional
        // (peer-closed) inside one" — a CONDITIONAL `ClosingBehavior` cannot
        // express. The outside-a-note case is the row value because that is
        // where the spec shows it used; a `\fm` inside a note is lint's problem.
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "fp",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 1.4k occurrences (en_ulb+bsb) — ~2% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "fqa",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 4.9k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "fr",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: Some(7),
    },
    // measured 5.8k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "ft",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // cap: 3.2 para/identification/h.html states NO range and marks
    // the `h#` syntax deprecated; the cap of 3 comes from usx.rng
    // The numbered `h1`..`h3` SPELLINGS are deprecated; bare `\h` is
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
        // Publishable but CHROME, not reading flow — `Span` gives a consumer the
        // class it needs to relocate this into real page chrome.
        html_element: Some(HtmlElement::Span),
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
        // The intro-context twin of `\b`, same empty-spacer reasoning.
        html_element: Some(HtmlElement::Div),
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
        // Both marker pages state `Valid In:: [BookHeaders]` while the doc index's
        // Scripture production says BookIdentification. The marker page wins.
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Header),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        // `Span`, not transparent: unwrapped, a reader sees raw book codes and
        // internal remarks as bare text with no class to suppress them by.
        html_element: Some(HtmlElement::Span),
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
        html_element: Some(HtmlElement::Span),
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
    // cap: 3.2 para/introductions/ili.html "The variable # (1-2)"
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
    // cap: 3.2 para/introductions/imt.html "The variable # (1-4)"
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
    // cap: 3.2 para/introductions/imte.html "The variable # (1-2)"
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
    // cap: 3.2 para/introductions/io.html "The variable # (1-4)
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
        allowed_contexts: &[
            SpecContext::BookIntroduction,
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/introductions/iq.html "The variable # (1-3)"
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
        allowed_contexts: &[
            SpecContext::BookIntroduction,
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/introductions/is.html "The variable # (1-2)"
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +List — items are the \list-s container's content (U25003);
            // same-kind eviction is what keeps siblings from nesting
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
            // curation: +List — items are the \list-s container's content (U25003);
            // same-kind eviction is what keeps siblings from nesting
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
    // cap: 3.2 para/lists/li.html "The variable # (1-4)"
    // measured 1.5k occurrences (en_ulb+bsb) — ~2% of `\v`,
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
            // curation: +List — items are the \list-s container's content (U25003);
            // same-kind eviction is what keeps siblings from nesting
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
        allowed_contexts: &[
            SpecContext::List,
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/lists/lim.html "The variable # (1-4)"
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
            // curation: +List — items are the \list-s container's content (U25003);
            // same-kind eviction is what keeps siblings from nesting
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
    // A PAIRED milestone (`\list-s\*` … `\list-e\*`), no attributes.
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
        allowed_contexts: &[
            SpecContext::List,
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 char/lists/liv.html states NO range at all; usx.rng
    // enumerates liv1..liv5
    // Numbered with NO stated maximum, hence Unbounded (char/lists/liv.html).
    MarkerRow {
        marker: "liv",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharLists,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unbounded,
        allowed_contexts: &[
            SpecContext::List,
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // measured 0.9k occurrences (en_ulb+bsb) — ~1% of `\v`,
    // which does NOT pay for a common_marker_checks arm.
    MarkerRow {
        marker: "m",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated
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
    // cap: 3.2 para/paragraphs/mi.html "The variable # (1-3)" —
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
    // cap: 3.2 para/titles-sections/ms.html "The variable # (1-3)"
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
    // cap: 3.2 para/titles-sections/mt.html "The variable # (1-4)"
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
    // cap: 3.2 para/titles-sections/mte.html "The variable # (1-2)"
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // stays Span: HTML has no "normal text" element
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // measured 18.0k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "p",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // curation: +PeripheralContent — usx.rng's `PeripheralDivision`
            // contains `PeripheralContent`, so a paragraph inside a peripheral
            // division nests instead of displacing it (testData
            // advanced/periph). The grammar grants the whole
            // Para/Section/List/Table class this; only `p` is demonstrated, so
            // only `p` takes it.
            SpecContext::PeripheralContent,
        ],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: Some(2),
    },
    // Character/CharBreaks, which is why it opens no scope.
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
        // curation: +id — usx.rng gives `\periph` two attributes, `alt` (the
        // TITLE TEXT, never a k/v pair) and `id` behind the pipe (testData
        // advanced/periph). Optional, because a bare `\periph Title` is legal.
        defined_attributes: &[("id", AttrStatus::Optional)],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Section),
        priority: None,
    },
    // cap: 3.2 para/paragraphs/ph.html "The variable # (1-3)",
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
    // cap: 3.2 para/paragraphs/pi.html "The variable # (1-3)"
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/poetry/q.html "The variable # (1-4) represents
    // the level of indent"
    // measured 47.0k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "q",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaPoetry,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated
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
        allowed_contexts: &[
            SpecContext::Para,
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/poetry/qm.html "The variable # (1-3)"
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
        allowed_contexts: &[
            SpecContext::Para,
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 ms/qt.html — # is the nesting level (1-5)
    // Overloaded name: two shape-keyed rows, this one and the MilestoneOnly.
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
    // cap: 3.2 ms/qt.html — # is the nesting level (1-5)
    // Overloaded name: two shape-keyed rows, this one and the MilestoneOnly.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // measured 1.3k occurrences (en_ulb+bsb) — ~2% of `\v`,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
        // rem's page states `Valid In:: [BookHeaders]` while its own prose says
        // "anywhere within a text" and the doc index adds BookTitles and
        // BookIntroduction. The page wins; the tension is real.
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/titles-sections/s.html "The variable # (1-4)"
    // measured 17.0k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "s",
        shape: SpellingShape::Any,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated — OVERRIDES the ParaTitlesSections default (AtLeastOneHorizontalWhitespace)
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
    // stays Span: HTML has no smallcaps element
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/titles-sections/sd.html "The variable # (1-4)"
    // An EMPTY spacer <div>, not a heading and not <hr>: the spec's layout
    // example renders \sd# as vertical blank space between text blocks. No
    // heading base level, no clamp; the digit survives in data-marker.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
        html_element: Some(HtmlElement::Span),
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // `\ta content|@a-<identifier>\ta*` — "each attribute should begin with
    // `a-`". No fixed names exist, so this row carries the PREFIX WILDCARD
    // spelling; the "one or more" is family-level cardinality, so lint's.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // Paired `\table-s\*` … `\table-e\*`, no attributes documented.
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
    // `\tl content|@lang\tl*` — `lang` is the DEFAULT attribute, an ISO639-1
    // source language for the transliterated text.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // cap: 3.2 para/identification/toc.html "The variable # (1-3)
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
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // cap: 3.2 para/identification/toca.html "The variable # (1-3)
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
        html_element: Some(HtmlElement::Span),
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
            // curation: +Table — rows are the \table-s container's content (U25003);
            // same-kind eviction is what keeps siblings from nesting
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
    MarkerRow {
        marker: "ts",
        shape: SpellingShape::Any,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneTs,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: curated
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::ChapterContent,
            // curation: +Para/List/Table — usfmtc keeps `\ts-s` INSIDE the open
            // paragraph, list item and cell, so ChapterContent alone would
            // displace its host. `list`/`table` deliberately do NOT get these:
            // usfmtc renders their content BESIDE the paragraph.
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
        // Both marker pages state `Valid In:: [BookHeaders]` while the doc index's
        // Scripture production says BookIdentification. The marker page wins.
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Header),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // measured 62.0k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "v",
        shape: SpellingShape::Any,
        kind: MarkerKind::Verse,
        category: Category::ChapterVerse,
        ws_after_name: Ws::AtLeastOneWhitespace, // ws: curated — OVERRIDES the ChapterVerse default (AtLeastOneHorizontalWhitespace)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::Scripture,
            SpecContext::ChapterContent,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        // `\v` is a POINT — it takes no children and paragraphs are its
        // SIBLINGS, so it pushes NO frame. It still displaces an unclosed note
        // (a note spells verse numbers `\fv`, so a bare `\v` means the note is
        // definitively unclosed), but that needs no rank: Footnote is absent
        // from this row's mask.
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
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // Opens a Character scope and carries the character class's mask, so the
        // pop predicate has something to test (see the `ca` row). Adjacency to
        // `\v` stays a lint rule over tokens, not a context.
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
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Sup),
        priority: None,
    },
    // STANDALONE, not paired: docs.usfm.bible/usfm/3.2/ms/vid.html gives
    // `\vid|@h @ref\*` — `ref` required and default, `h` optional, no sid/eid
    MarkerRow {
        marker: "vid",
        shape: SpellingShape::Any,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneVid,
        ws_after_name: Ws::OptionalHorizontalWhitespace, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        // ChapterContent ONLY, on purpose: `\vid` is a fragment header standing
        // BETWEEN structures (usfm-grammar's new-vid-milestone fixture puts it
        // on its own line between paragraphs), so displacing an open paragraph
        // is correct here — unlike `ts`.
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
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace, // ws: curated — OVERRIDES the ChapterVerse default (TagEndDelimiter)
        payload: Payload::Designator,
        numbered_max: Numbering::Unnumbered,
        // Opens a Character scope and carries the character class's mask, so the
        // pop predicate has something to test (see the `ca` row). Adjacency to
        // `\v` stays a lint rule over tokens, not a context.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // Overloaded name. The MilestoneOnly row rests on tcdocs/usx.rng alone —
    // 3.2 documents `wj` only as a character marker.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // `\wl content|@lang\wl*` — same shape as `tl`, `lang` is the default.
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
            // curation: +Footnote/CrossReference — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    MarkerRow {
        marker: "x",
        shape: SpellingShape::Any,
        kind: MarkerKind::Note,
        category: Category::NoteCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: curated
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            // curation: +Para/List/Table — the containers grant note content, so a
            // note inside its paragraph nests instead of displacing it (para railroad)
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            // curation: +Section — notes belong in headings too, class-wide
            // because the grammar reason is the container's: specExamples/
            // cross-ref nests `\x` in an `\s1`, and samples-from-wild/doo43-4
            // writes `\f …\f*` inside `\cl`.
            SpecContext::Section,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    MarkerRow {
        marker: "xdc",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
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
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // No live attributes; `link-href` is deprecated, and so is using `\xt`
    // outside a cross-reference — the marker itself is not.
    // measured 3.2k occurrences (en_ulb+bsb)
    MarkerRow {
        marker: "xt",
        shape: SpellingShape::Any,
        kind: MarkerKind::Character,
        category: Category::CharNotesCrossReference,
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
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter, // ws: derived from category default
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            // curation: +Footnote — chars nest in notes (bsb GEN 2:4)
            SpecContext::Footnote,
            SpecContext::CrossReference,
        ],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // ---- Extension templates: `\z` markers, behaving as their category ----
    //
    // One row per behaviour-bearing `\category` word the spec defines,
    // appended AFTER every spec row so no existing index moves. Each COPIES a
    // spec row — the row IS the behaviour, so a registered `\zfoot` is a
    // footnote with a different name — and owns exactly four columns:
    //
    //   marker         a template name, never a marker a document can spell
    //   shape          which spelling the category admits — PlainOnly for
    //                  everything but `milestone` (MilestoneOnly), because
    //                  the token KIND is decided by the spelling before any
    //                  row is consulted
    //   numbered_max   Unnumbered everywhere but `cell`; the spec shows no
    //                  other numbered extension
    //   priority       None: the hot-marker ranks are MEASURED, and no
    //                  document byte reaches a template by name
    //
    // `every_template_copies_its_source_row` diffs each template against its
    // source, so curating `\p` reaches `zpara` or the build fails.
    //
    // The leading `z` is load-bearing: every `z` lexeme short-circuits before
    // `by_name`, and `emit::by_name_arms` skips these rows, so a template is
    // UNREACHABLE from source text. `\zpara` in a document resolves through
    // the registry like any other extension, or to row 0.

    // `header` — every column of `\h` but the four the template owns.
    MarkerRow {
        marker: "zheader",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIdentification,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::BookHeaders],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // `title` — every column of `\mt` but the four the template owns.
    MarkerRow {
        marker: "ztitle",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::AtLeastOneHorizontalWhitespace,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
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
    // `introduction` — every column of `\ip` but the four the template owns.
    MarkerRow {
        marker: "zintro",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Paragraph,
        category: Category::ParaIntroductions,
        ws_after_name: Ws::TagEndDelimiter,
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
    // `sectionpara` — every column of `\s` but the four the template owns.
    MarkerRow {
        marker: "zsect",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Paragraph,
        category: Category::ParaTitlesSections,
        ws_after_name: Ws::TagEndDelimiter,
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
    // `versepara` — every column of `\p` but the four the template owns.
    MarkerRow {
        marker: "zpara",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent, SpecContext::PeripheralContent],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::Para),
        priority: None,
    },
    // `list` — every column of `\li` but the four the template owns.
    MarkerRow {
        marker: "zlist",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Paragraph,
        category: Category::ParaLists,
        ws_after_name: Ws::TagEndDelimiter,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::ChapterContent, SpecContext::List],
        opens_scope: Some(ScopeKind::Para),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::ListItem),
        priority: None,
    },
    // `otherpara` — every column of `\lit` but the four the template owns.
    MarkerRow {
        marker: "zother",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Paragraph,
        category: Category::ParaBody,
        ws_after_name: Ws::TagEndDelimiter,
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
    // `footnote` — every column of `\f` but the four the template owns.
    MarkerRow {
        marker: "zfoot",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Note,
        category: Category::NoteFootnote,
        ws_after_name: Ws::TagEndDelimiter,
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Section,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // `crossreference` — every column of `\x` but the four the template owns.
    MarkerRow {
        marker: "zxref",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Note,
        category: Category::NoteCrossReference,
        ws_after_name: Ws::TagEndDelimiter,
        payload: Payload::NoteCaller,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookTitles,
            SpecContext::BookIntroduction,
            SpecContext::BookIntroductionEndTitles,
            SpecContext::ChapterContent,
            SpecContext::PeripheralContent,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
            SpecContext::Section,
        ],
        opens_scope: Some(ScopeKind::Note),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // `char` — every column of `\add` but the four the template owns.
    MarkerRow {
        marker: "zchar",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Character,
        category: Category::CharTextFeatures,
        ws_after_name: Ws::TagEndDelimiter,
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
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::RequiredExplicit,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // `introchar` — every column of `\ior` but the four the template owns.
    MarkerRow {
        marker: "zichar",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Character,
        category: Category::CharIntroductions,
        ws_after_name: Ws::TagEndDelimiter,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::BookIntroduction,
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // `listchar` — every column of `\lik` but the four the template owns.
    MarkerRow {
        marker: "zlchar",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Character,
        category: Category::CharLists,
        ws_after_name: Ws::TagEndDelimiter,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::List,
            SpecContext::Footnote,
            SpecContext::CrossReference,
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
    // `footnotechar` — every column of `\ft` but the four the template owns.
    MarkerRow {
        marker: "zfchar",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Character,
        category: Category::CharNotesFootnote,
        ws_after_name: Ws::TagEndDelimiter,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[SpecContext::Footnote, SpecContext::CrossReference],
        opens_scope: Some(ScopeKind::Character),
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::OptionalExplicitUntilNoteEnd,
        deprecated: false,
        html_element: Some(HtmlElement::Span),
        priority: None,
    },
    // `crossreferencechar` — every column of `\xt` but the four the template owns.
    MarkerRow {
        marker: "zxchar",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Character,
        category: Category::CharNotesCrossReference,
        ws_after_name: Ws::TagEndDelimiter,
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
        priority: None,
    },
    // `milestone` — every column of `\qt-s` but the four the template owns.
    MarkerRow {
        marker: "zms",
        shape: SpellingShape::MilestoneOnly,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneQt,
        ws_after_name: Ws::OptionalHorizontalWhitespace,
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
    // `standalone` — "a bare milestone with no attributes or delimiter"
    // (docs.usfm.bible/usfm/3.2/extensions.html). `\ts`'s kind, category and
    // contexts, but no spec row is bare, so it also owns three more columns:
    // no scope, no closer, no attributes. It opens nothing, closes nothing,
    // carries no text and is never closed.
    MarkerRow {
        marker: "zmsbare",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::Milestone,
        category: Category::MilestoneTs,
        ws_after_name: Ws::OptionalHorizontalWhitespace,
        payload: Payload::None,
        numbered_max: Numbering::Unnumbered,
        allowed_contexts: &[
            SpecContext::ChapterContent,
            SpecContext::Para,
            SpecContext::List,
            SpecContext::Table,
        ],
        opens_scope: None,
        closes_scope: None,
        defined_attributes: &[],
        default_attribute: None,
        closing: ClosingBehavior::None,
        deprecated: false,
        html_element: Some(HtmlElement::SelfClosingSpan),
        priority: None,
    },
    // `cell` — every column of `\tc` but the four the template owns.
    MarkerRow {
        marker: "zcell",
        shape: SpellingShape::PlainOnly,
        kind: MarkerKind::TableCell,
        category: Category::CharTables,
        ws_after_name: Ws::TagEndDelimiter,
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
];

/// Which template row each spec `\category` word BEHAVES AS — the whole
/// mapping from the spec's vocabulary to this table's.
///
/// `None` is the two USX-internal words. `attribute` names `cp`/`vp`/`ca`/`va`
/// and `internal` names `usfm`/`cat`: neither describes a marker a USFM
/// document writes, so an extension declaring one parses (a valid
/// `markers.ext` must never error) and registers nothing, leaving the name at
/// row 0 exactly as today.
///
/// Codegen turns the names into indices — `generated::template_for`.
pub static EXTENSION_TEMPLATES: &[(ExtensionCategory, Option<&str>)] = &[
    (ExtensionCategory::Header, Some("zheader")),
    (ExtensionCategory::Title, Some("ztitle")),
    (ExtensionCategory::Introduction, Some("zintro")),
    (ExtensionCategory::SectionPara, Some("zsect")),
    (ExtensionCategory::VersePara, Some("zpara")),
    (ExtensionCategory::List, Some("zlist")),
    (ExtensionCategory::OtherPara, Some("zother")),
    (ExtensionCategory::CrossReference, Some("zxref")),
    (ExtensionCategory::Footnote, Some("zfoot")),
    (ExtensionCategory::Char, Some("zchar")),
    (ExtensionCategory::IntroChar, Some("zichar")),
    (ExtensionCategory::ListChar, Some("zlchar")),
    (ExtensionCategory::FootnoteChar, Some("zfchar")),
    (ExtensionCategory::CrossReferenceChar, Some("zxchar")),
    (ExtensionCategory::Milestone, Some("zms")),
    (ExtensionCategory::Attribute, None),
    (ExtensionCategory::Cell, Some("zcell")),
    (ExtensionCategory::Standalone, Some("zmsbare")),
    (ExtensionCategory::Internal, None),
];

/// How many numbered/paired families collapse into one row. Reported by
/// `cargo run --bin codegen`.
pub const COLLAPSED_FAMILIES: usize = 23;

/// Rows whose fine [`Category`] needed a ruling because the spec group was
/// ambiguous, contradictory, or absent.
///
/// Each entry names the fact and the evidence that settled it.
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

    /// Every row's fine category must be legal for its coarse kind.
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

    /// The lookup key is (name, shape): two rows may share a name only
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

    /// The behaviour the fine category is load-bearing for: a marker
    /// that takes a payload but opens no scope, and `\pb`.
    ///
    /// `\c`/`\v` are Points and `cp` is a bare published label, so none of them
    /// pushes a frame; `ca`/`va`/`vp` DO open Character scopes, so the split
    /// inside the group is pinned by name rather than by category.
    ///
    /// Displacement is deliberately not asserted here: it is not a per-row
    /// value at all — the walker pops while the top frame's context is absent
    /// from the row's mask.
    #[test]
    fn category_drives_scope_not_kind() {
        for row in ROWS {
            let opens = row.opens_scope.is_some();
            match row.category {
                Category::CharBreaks => assert!(!opens, "{} must open no scope", row.marker),
                // The ChapterVerse group splits by name: the three char-shaped
                // annotations open a Character scope, the rest push no frame.
                Category::ChapterVerse => match row.marker {
                    "ca" | "va" | "vp" => assert_eq!(
                        row.opens_scope,
                        Some(ScopeKind::Character),
                        "{}: a closer-requiring character annotation must push a frame",
                        row.marker
                    ),
                    _ => assert!(
                        !opens,
                        "{}: nothing else in the ChapterVerse group pushes a frame (Q16)",
                        row.marker
                    ),
                },
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

    /// Every marker named in the v-forbidden set resolves to a row. No
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
    // ---- Extension templates --------------------------------------------

    /// Every template names a row, every category is mapped exactly once, and
    /// the two USX-internal words map to nothing.
    #[test]
    fn the_category_map_is_total_and_one_to_one() {
        for category in ExtensionCategory::ALL {
            let hits: Vec<_> = EXTENSION_TEMPLATES
                .iter()
                .filter(|(c, _)| *c == category)
                .collect();
            assert_eq!(hits.len(), 1, "{category} is mapped {} times", hits.len());
        }
        assert_eq!(EXTENSION_TEMPLATES.len(), ExtensionCategory::ALL.len());
        for (category, name) in EXTENSION_TEMPLATES {
            let Some(name) = name else {
                assert!(
                    matches!(
                        category,
                        ExtensionCategory::Attribute | ExtensionCategory::Internal
                    ),
                    "{category} has no template"
                );
                continue;
            };
            assert!(
                ROWS.iter().any(|row| row.marker == *name),
                "{category} names `{name}`, which is not a row"
            );
        }
    }

    /// Every template copies its source row. THE test the templates rest on:
    /// curating `\p` has to reach `zpara`, and a column that silently stopped
    /// matching would give a registered extension behaviour the spec marker no
    /// longer has.
    ///
    /// The four columns a template owns are excluded by name; everything else
    /// is compared field for field.
    #[test]
    fn every_template_copies_its_source_row() {
        // (template, source name, source shape) — the source is the row §1 of
        // the plan says the category behaves as.
        const SOURCES: &[(&str, &str, SpellingShape)] = &[
            ("zheader", "h", SpellingShape::Any),
            ("ztitle", "mt", SpellingShape::Any),
            ("zintro", "ip", SpellingShape::Any),
            ("zsect", "s", SpellingShape::Any),
            ("zpara", "p", SpellingShape::Any),
            ("zlist", "li", SpellingShape::Any),
            ("zother", "lit", SpellingShape::Any),
            ("zfoot", "f", SpellingShape::Any),
            ("zxref", "x", SpellingShape::Any),
            ("zchar", "add", SpellingShape::Any),
            ("zichar", "ior", SpellingShape::Any),
            ("zlchar", "lik", SpellingShape::Any),
            ("zfchar", "ft", SpellingShape::Any),
            ("zxchar", "xt", SpellingShape::Any),
            ("zms", "qt", SpellingShape::MilestoneOnly),
            ("zmsbare", "ts", SpellingShape::Any),
            ("zcell", "tc", SpellingShape::Any),
        ];
        // `standalone` is bare where its source is a delimited milestone.
        let bare = |template: &str| template == "zmsbare";
        let row = |name: &str, shape: SpellingShape| {
            ROWS.iter()
                .find(|row| row.marker == name && row.shape == shape)
                .unwrap_or_else(|| panic!("no row `{name}` with shape {shape:?}"))
        };
        assert_eq!(SOURCES.len(), 17, "one source per behavioural category");
        for (template, source, shape) in SOURCES {
            let s = row(source, *shape);
            let t = ROWS
                .iter()
                .find(|row| row.marker == *template)
                .unwrap_or_else(|| panic!("no template `{template}`"));
            assert_eq!(t.kind, s.kind, "{template}: kind");
            assert_eq!(t.category, s.category, "{template}: category");
            assert_eq!(
                t.ws_after_name, s.ws_after_name,
                "{template}: ws_after_name"
            );
            assert_eq!(t.payload, s.payload, "{template}: payload");
            assert_eq!(
                t.allowed_contexts, s.allowed_contexts,
                "{template}: allowed_contexts"
            );
            if bare(template) {
                assert_eq!(t.opens_scope, None, "{template}: opens nothing");
                assert_eq!(t.closing, ClosingBehavior::None, "{template}: never closed");
                assert!(t.defined_attributes.is_empty(), "{template}: no attributes");
            } else {
                assert_eq!(t.opens_scope, s.opens_scope, "{template}: opens_scope");
                assert_eq!(
                    t.defined_attributes, s.defined_attributes,
                    "{template}: defined_attributes"
                );
                assert_eq!(t.closing, s.closing, "{template}: closing");
            }
            assert_eq!(t.closes_scope, s.closes_scope, "{template}: closes_scope");
            assert_eq!(
                t.default_attribute, s.default_attribute,
                "{template}: default_attribute"
            );
            assert_eq!(t.deprecated, s.deprecated, "{template}: deprecated");
            assert_eq!(t.html_element, s.html_element, "{template}: html_element");
            // The four the template owns.
            assert!(t.marker.starts_with('z'), "{template}: name starts with z");
            assert!(t.marker.len() <= 8, "{template}: name fits the u64 key");
            assert!(t.priority.is_none(), "{template}: a template is never hot");
        }
    }

    /// A template is unreachable from source text: `marker_idx` bails on the
    /// leading `z` before any name match, so no document byte lands on one.
    #[test]
    fn no_template_name_resolves_through_the_table() {
        for (_, name) in EXTENSION_TEMPLATES {
            let Some(name) = name else { continue };
            for shape in [
                SpellingShape::Any,
                SpellingShape::PlainOnly,
                SpellingShape::MilestoneOnly,
            ] {
                assert_eq!(
                    crate::tables::generated::marker_idx(name.as_bytes(), shape),
                    crate::tables::generated::UNRESOLVED,
                    "`{name}` resolved by name"
                );
            }
        }
    }

    /// The templates are contiguous at the END of the table, which is what
    /// makes `is_extension` a comparison and what keeps every spec index put.
    #[test]
    fn templates_sit_after_every_spec_row() {
        use crate::tables::generated::{FIRST_EXTENSION_ROW, ROW_COUNT, is_extension};
        let first = FIRST_EXTENSION_ROW as usize;
        assert_eq!(ROW_COUNT - first, 17, "17 templates, all at the end");
        for (i, row) in ROWS.iter().enumerate() {
            assert_eq!(
                is_extension(i as u8),
                i >= first,
                "row {i} (`{}`)",
                row.marker
            );
            assert_eq!(
                i >= first,
                EXTENSION_TEMPLATES
                    .iter()
                    .any(|(_, name)| *name == Some(row.marker)),
                "row {i} (`{}`) is on the wrong side of the line",
                row.marker
            );
        }
    }
}
