//! The dish's generator: [`schema`] in, both ends of the wire out.
//!
//! ```text
//! schema::RECORDS  ->  wire_generated_rs()  ->  onion/src/wire/generated.rs
//! schema::SECTIONS ->  reader_ts()          ->  onion-wasm/reader.ts
//! ```
//!
//! Both artifacts are CHECKED IN and both have a staleness test, so a schema
//! edit that was not regenerated fails the build rather than shipping a reader
//! that disagrees with its writer.
//!
//! The row emitters are `ticket`'s. What stays here is the dish's own: its
//! sections, its marker table, its enums, its lint catalog.

use ticket::emit as shared;

use super::schema::{self, SectionKind};
use crate::attributes::{AttrResolution, MalformedAttr};

const RS_TEMPLATE: &str = include_str!("generated.rs.tmpl");
const TS_TEMPLATE: &str = include_str!("../../../onion-wasm/reader.ts.tmpl");

/// One writer per record: fields in order, little-endian, offsets recorded.
pub fn wire_generated_rs() -> String {
    RS_TEMPLATE.replace(
        "@@WRITERS@@",
        shared::writers_rs(schema::RECORDS).trim_end(),
    )
}

// ---------------------------------------------------------------------------
// The TypeScript reader
// ---------------------------------------------------------------------------

/// Offsets, strides, section indices, and one accessor class per record.
pub fn reader_ts(marker_table: &str, enums: &str, catalog: &str) -> String {
    TS_TEMPLATE
        .replace("@@FORMAT_VERSION@@", &schema::FORMAT_VERSION.to_string())
        .replace("@@SECTIONS@@", &sections_ts())
        .replace("@@ROWS@@", &rows_ts())
        .replace("@@ENUMS@@", enums.trim_end())
        .replace("@@MARKER_TABLE@@", marker_table.trim_end())
        .replace("@@LINT_CATALOG@@", catalog.trim_end())
}

fn sections_ts() -> String {
    let mut out = String::from("export const SECTION = {\n");
    for (at, section) in schema::SECTIONS.iter().enumerate() {
        out.push_str(&format!(
            "  /** {} */\n  {}: {},\n",
            section.doc, section.name, at
        ));
    }
    out.push_str("} as const;\n\nexport const SECTION_NAMES = [\n");
    for section in schema::SECTIONS {
        out.push_str(&format!("  \"{}\",\n", section.name));
    }
    out.push_str("] as const;\n");
    out
}

/// One `class <Record>` per record: a cursor over a row, with a named getter
/// per field. No stride or offset is ever written by hand on the JS side.
fn rows_ts() -> String {
    let mut out = String::new();
    for section in schema::SECTIONS {
        let SectionKind::Rows(record) = &section.kind else {
            continue;
        };
        out.push_str(&shared::row_class_ts(record));
    }
    out
}

// ---------------------------------------------------------------------------
// The tables the reader resolves through
// ---------------------------------------------------------------------------

use crate::tables::emit as tables_emit;
use crate::tables::{generated, rows};

/// Every enum a wire field names, as a TS const object. Variant names come
/// from `tables::emit`'s own tables — the ones that already generate the Rust
/// side — so a rename lands in both languages from one edit.
pub fn enums_ts() -> String {
    let mut out = String::new();

    let block = |out: &mut String, doc: &str, name: &str, pairs: &[(&str, u32)]| {
        out.push_str(&format!("/** {doc} */\nexport const {name} = {{\n"));
        for (variant, code) in pairs {
            out.push_str(&format!("  {variant}: {code},\n"));
        }
        out.push_str("} as const;\n\n");
    };

    let kinds: Vec<(&str, u32)> = tables_emit::KINDS
        .iter()
        .map(|(k, n)| (*n, *k as u32))
        .collect();
    block(
        &mut out,
        "What a marker IS. All fourteen — never coarsened.",
        "MarkerKind",
        &kinds,
    );

    let categories: Vec<(&str, u32)> = tables_emit::CATEGORIES
        .iter()
        .map(|(c, n)| (*n, *c as u32))
        .collect();
    block(
        &mut out,
        "The spec group a marker belongs to. ParaBody is distinct from ParaPoetry.",
        "Category",
        &categories,
    );

    let closings: Vec<(&str, u32)> = tables_emit::CLOSINGS
        .iter()
        .map(|(c, n)| (*n, *c as u32))
        .collect();
    block(
        &mut out,
        "Whether a marker requires, allows or refuses a closer.",
        "ClosingBehavior",
        &closings,
    );

    let shapes: Vec<(&str, u32)> = tables_emit::SHAPES
        .iter()
        .map(|(s, n)| (*n, *s as u32))
        .collect();
    block(
        &mut out,
        "Which spellings a row admits.",
        "SpellingShape",
        &shapes,
    );

    let wsreqs: Vec<(&str, u32)> = tables_emit::WSREQS
        .iter()
        .map(|(w, n)| (*n, *w as u32))
        .collect();
    block(
        &mut out,
        "What must follow a marker's name — `Marker.ws()`.",
        "StructuralWhitespaceRequirement",
        &wsreqs,
    );

    let contexts: Vec<(&str, u32)> = tables_emit::CONTEXTS
        .iter()
        .map(|(c, n)| (*n, *c as u32))
        .collect();
    block(
        &mut out,
        "Where in the document grammar a node sits — `NodeView.context()`.",
        "SpecContext",
        &contexts,
    );

    block(
        &mut out,
        "Why a node closed. `Explicit` is the well-formed answer.",
        "CloseReason",
        &[
            ("Explicit", 0),
            ("Implicit", 1),
            ("Recovery", 2),
            ("Eof", 3),
        ],
    );

    block(
        &mut out,
        "Why an attribute list stopped parsing — `attrs`'s trailing code.",
        "MalformedAttr",
        &[
            ("UnterminatedQuote", MalformedAttr::UnterminatedQuote as u32),
            ("EmptyName", MalformedAttr::EmptyName as u32),
            ("MissingValue", MalformedAttr::MissingValue as u32),
            ("BareJunk", MalformedAttr::BareJunk as u32),
        ],
    );

    block(
        &mut out,
        "What the marker table says about an attribute name — `attrResolve`.",
        "AttrResolution",
        &[
            ("Defined", AttrResolution::DEFINED),
            ("UserNamespace", AttrResolution::USER_NAMESPACE),
            ("Unknown", AttrResolution::UNKNOWN),
        ],
    );

    // TokenKind's discriminants are `TokenKind::to_bits`, which is the one
    // place they are stated; these names pair with that match arm for arm.
    block(
        &mut out,
        "What shape a token is — `TokenView.kind()`, spelling bit stripped.",
        "TokenKind",
        &[
            ("Marker", 0),
            ("ClosingMarker", 1),
            ("Milestone", 2),
            ("MilestoneTerminator", 3),
            ("Newline", 4),
            ("OptBreak", 5),
            ("AttrList", 6),
            ("Text", 7),
            ("Designator", 8),
            ("NoteCaller", 9),
            ("BookCode", 10),
            ("Pad", 11),
        ],
    );

    out.push_str(&format!(
        "/**\n * Bit 0 of a token's flags byte: the last byte of the span is the one\n\
         \u{20}* horizontal delimiter the scanner folded on, so the payload ends at\n\
         \u{20}* `end - 1`. Read it through `TokenView.payloadEnd()`.\n */\n\
         export const TOKEN_DELIMITER_FOLDED = {};\n\n\
         /**\n * Bit 1 of a token's flags byte: a `Text` token of nothing but horizontal\n\
         \u{20}* whitespace. Never set on another kind.\n */\n\
         export const TOKEN_BLANK = {};\n\n",
        schema::TOKEN_DELIMITER_FOLDED,
        schema::TOKEN_BLANK,
    ));

    out.push_str(
        "/**\n * Bit 4 of a token's kind byte, meaning PER SHAPE: `\\+` nesting on the two\n\
         \u{20}* marker shapes, the `-e` half on a milestone. No shape carries both.\n */\n\
         export const TOKEN_SPELLING_BIT = 1 << 4;\n\n\
         /** Do these kind bits name a marker, and so resolve to a table row? */\n\
         export const isMarkerKind = (kind: number): boolean =>\n  \
         kind === TokenKind.Marker ||\n  kind === TokenKind.ClosingMarker ||\n  \
         kind === TokenKind.Milestone;\n",
    );
    out
}

/// The marker table, as the reader's lookup. One row per `MarkerIdx`, in row
/// order, so `MARKERS[idx]` is the row a token names.
pub fn marker_table_ts() -> String {
    let mut out = doc_ts(&[
        &format!(
            "The marker table — {} rows, generated from `tables::rows::ROWS`.",
            rows::ROWS.len()
        ),
        "",
        "Indexed by a token's `marker` field. Row 0 is UNRESOLVED: an unknown",
        "name, an unregistered `\\z` extension, an illegal spelling. It has no",
        "name, so the spelling is only in the document.",
        "",
        "Rows from `FIRST_EXTENSION_ROW` up are extension TEMPLATES: a registered",
        "`\\z` marker resolves to the template its `\\category` behaves as, so",
        "`kind`, `category` and `closing` are the spec marker's. Their `name` is",
        "the TEMPLATE's, never the marker's, so `Marker.name()` answers `null`",
        "there the way it does on row 0 — read the spelling off the document",
        "with `TokenView.spelling(text)`.",
        "",
        "`numbering` is a packed code: 0 unnumbered, 1..=13 the cap, 14 unbounded,",
        "15 table columns.",
        "",
        "`ws` is a `StructuralWhitespaceRequirement`: what must follow the marker's",
        "name. `SingleNewline` is the rule for a marker that takes no content on",
        "its own line.",
    ]);
    out.push_str(
        "export const MARKERS: readonly {\n  \
         readonly name: string;\n  readonly kind: number;\n  readonly category: number;\n  \
         readonly closing: number;\n  readonly shape: number;\n  readonly numbering: number;\n  \
         readonly ws: number;\n\
         }[] = [\n",
    );
    for idx in 0..rows::ROWS.len() {
        let i = idx as generated::MarkerIdx;
        out.push_str(&format!(
            "  {{ name: {:?}, kind: {}, category: {}, closing: {}, shape: {}, numbering: {}, \
             ws: {} }},\n",
            generated::name(i),
            generated::kind(i) as u32,
            generated::category(i) as u32,
            generated::closing(i) as u32,
            generated::shape(i) as u32,
            tables_emit::numbering_code(generated::numbering(i)),
            generated::ws_after_name(i) as u32,
        ));
    }
    out.push_str("];\n\n");

    out.push_str(&doc_ts(&[
        "The first extension TEMPLATE row: every index below it is a marker USFM",
        "3.2 defines, every index at or above it a template a registered `\\z`",
        "marker resolves to.",
    ]));
    out.push_str(&format!(
        "export const FIRST_EXTENSION_ROW = {};\n\n",
        generated::FIRST_EXTENSION_ROW
    ));
    out.push_str(&doc_ts(&[
        "Is this row a template — and so a row whose `name` is NOT the marker's?",
        "",
        "The one place a consumer asks. Everything else reads `kind` and",
        "`category`, which are the spec marker's either way — that is what makes",
        "a registered extension behave, and this the only thing it cannot carry.",
    ]));
    out.push_str(
        "export const isExtension = (idx: number): boolean => idx >= FIRST_EXTENSION_ROW;\n",
    );
    out
}

/// A JSDoc block whose every line keeps its leading ` * ` — which a `\`
/// string continuation in this file would eat.
fn doc_ts(lines: &[impl AsRef<str>]) -> String {
    let mut out = String::from("/**\n");
    for line in lines {
        let line = line.as_ref();
        out.push_str(" *");
        if !line.is_empty() {
            out.push(' ');
            out.push_str(line);
        }
        out.push('\n');
    }
    out.push_str(" */\n");
    out
}

/// The lint catalog, as the reader's `CODES` lookup.
///
/// Embeds `lint::diagnostics_json()` verbatim rather than restating the field
/// mapping: JSON is a valid TypeScript object literal, so the catalog the JS
/// bundle already loads and the one the reader resolves through are the same
/// bytes, produced once.
pub fn catalog_ts() -> String {
    format!(
        "/**\n * The lint catalog — one row per code, from `lint::LINT_ROWS`.\n\
         *\n * `severity` is the base rung and `escalation` the ladder above it, walked\n\
         * with the document's declared `\\usfm` version. A `null` severity is a GATE:\n\
         * the code says nothing until a version at or above its first rung is\n\
         * declared.\n */\nexport const CATALOG = {} as const;\n\n\
         /** The rows, by code. A diagnostic's `code` field indexes this. */\n\
         export const CODES = CATALOG.codes;\n",
        crate::lint::diagnostics_json().trim_end()
    )
}
