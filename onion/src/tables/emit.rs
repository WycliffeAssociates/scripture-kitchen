//! The generator: `rows::ROWS` in, the TEXT of `generated.rs` out.
//!
//! Lives in the library rather than in `src/bin/codegen.rs` so the freshness
//! test can call it (regenerate to a `String`, compare against the checked-in
//! file). The bin is a thin main over [`generated_rs`].
// @TODO: THIS FEATURE GATING WHEN WE GET THERE.
//! Wants feature-gating off for the wasm build — pure build-time machinery with
//! no business in the bundle — but gating it means `cargo test` silently skips
//! the freshness check.
//!
//! ## Shape of the emission
//!
//! Everything row-independent lives in `generated.rs.tmpl` as plain, readable
//! Rust; the data-driven pieces are built here and spliced over the template's
//! `@@TOKEN@@` lines by [`splice`]. Read the template first — it IS the
//! generated file, minus the data.
//!
//! ## The one rule for editing this file
//!
//! **Position is the code.** The `&[(Variant, "path")]` tables below assign each
//! enum variant its bit pattern BY POSITION, and the same tables emit the
//! generated decoder — so encode and decode cannot drift, but reordering a table
//! silently changes the bit layout. Appending is always safe; anything else,
//! rerun codegen and read the diff (which is what the checked-in `generated.rs`
//! is for).
//!
//! Bit layout: one `u128` per row, 82 bits used, 46 spare. Widths are asserted
//! at emit time — a value that outgrows its field panics the generator instead
//! of wrapping into its neighbour.

use std::fmt::Write as _;

use super::rows::ROWS;
use super::schema::{
    self, AttrStatus, Category, ClosingBehavior, HtmlElement, MarkerKind, MarkerRow, Numbering,
    Payload, ScopeKind, SpecContext, SpellingShape, StructuralWhitespaceRequirement as Ws,
};

// ---------------------------------------------------------------------------
// Variant code tables. POSITION IS THE CODE — see the module doc.
// ---------------------------------------------------------------------------

pub(crate) const KINDS: &[(MarkerKind, &str)] = &[
    (MarkerKind::Unknown, "Unknown"),
    (MarkerKind::Paragraph, "Paragraph"),
    (MarkerKind::Character, "Character"),
    (MarkerKind::Note, "Note"),
    (MarkerKind::Chapter, "Chapter"),
    (MarkerKind::Verse, "Verse"),
    (MarkerKind::Milestone, "Milestone"),
    (MarkerKind::Figure, "Figure"),
    (MarkerKind::Sidebar, "Sidebar"),
    (MarkerKind::Periph, "Periph"),
    (MarkerKind::Meta, "Meta"),
    (MarkerKind::TableRow, "TableRow"),
    (MarkerKind::TableCell, "TableCell"),
    (MarkerKind::Header, "Header"),
];

pub(crate) const CATEGORIES: &[(Category, &str)] = &[
    (Category::Unknown, "Unknown"),
    (Category::ParaIdentification, "ParaIdentification"),
    (Category::ParaIntroductions, "ParaIntroductions"),
    (Category::ParaTitlesSections, "ParaTitlesSections"),
    (Category::ParaBody, "ParaBody"),
    (Category::ParaPoetry, "ParaPoetry"),
    (Category::ParaLists, "ParaLists"),
    (Category::ParaTables, "ParaTables"),
    (Category::ParaPeripheral, "ParaPeripheral"),
    (Category::CharTextFeatures, "CharTextFeatures"),
    (Category::CharFormatting, "CharFormatting"),
    (Category::CharBreaks, "CharBreaks"),
    (Category::CharIntroductions, "CharIntroductions"),
    (Category::CharPoetry, "CharPoetry"),
    (Category::CharLists, "CharLists"),
    (Category::CharTables, "CharTables"),
    (Category::CharNotesFootnote, "CharNotesFootnote"),
    (Category::CharNotesCrossReference, "CharNotesCrossReference"),
    (Category::NoteFootnote, "NoteFootnote"),
    (Category::NoteCrossReference, "NoteCrossReference"),
    (Category::MilestoneList, "MilestoneList"),
    (Category::MilestoneTable, "MilestoneTable"),
    (Category::MilestoneQt, "MilestoneQt"),
    (Category::MilestoneTs, "MilestoneTs"),
    (Category::MilestoneVid, "MilestoneVid"),
    (Category::ChapterVerse, "ChapterVerse"),
    (Category::Sidebar, "Sidebar"),
    (Category::Meta, "Meta"),
    (Category::Peripheral, "Peripheral"),
    (Category::DocumentStructure, "DocumentStructure"),
    (Category::Figure, "Figure"),
];

const SCOPES: &[(ScopeKind, &str)] = &[
    (ScopeKind::Unknown, "Unknown"),
    (ScopeKind::Header, "Header"),
    (ScopeKind::Para, "Para"),
    (ScopeKind::Note, "Note"),
    (ScopeKind::Character, "Character"),
    (ScopeKind::Milestone, "Milestone"),
    (ScopeKind::TableRow, "TableRow"),
    (ScopeKind::TableCell, "TableCell"),
    (ScopeKind::Sidebar, "Sidebar"),
    (ScopeKind::Periph, "Periph"),
    // `Table` reuses a freed code and `List` is appended, so no existing
    // variant's bit pattern moved; 12 scopes still fit the same 4 bits.
    (ScopeKind::Table, "Table"),
    (ScopeKind::List, "List"),
];

pub(crate) const WSREQS: &[(Ws, &str)] = &[
    (Ws::NotRequired, "NotRequired"),
    (Ws::TagEndDelimiter, "TagEndDelimiter"),
    (
        Ws::AtLeastOneHorizontalWhitespace,
        "AtLeastOneHorizontalWhitespace",
    ),
    (
        Ws::OptionalHorizontalWhitespace,
        "OptionalHorizontalWhitespace",
    ),
    (Ws::AtLeastOneWhitespace, "AtLeastOneWhitespace"),
    (Ws::OptionalWhitespace, "OptionalWhitespace"),
    (Ws::SingleNewline, "SingleNewline"),
    (Ws::AtLeastOneNewline, "AtLeastOneNewline"),
];

const PAYLOADS: &[(Payload, &str)] = &[
    (Payload::None, "None"),
    (Payload::BookCode, "BookCode"),
    (Payload::Designator, "Designator"),
    (Payload::Version, "Version"),
    (Payload::NoteCaller, "NoteCaller"),
];

pub(crate) const CLOSINGS: &[(ClosingBehavior, &str)] = &[
    (ClosingBehavior::None, "None"),
    (ClosingBehavior::RequiredExplicit, "RequiredExplicit"),
    (
        ClosingBehavior::OptionalExplicitUntilNoteEnd,
        "OptionalExplicitUntilNoteEnd",
    ),
    (
        ClosingBehavior::SelfClosingMilestone,
        "SelfClosingMilestone",
    ),
];

const HTML: &[(HtmlElement, &str)] = &[
    (HtmlElement::Transparent, "Transparent"),
    (HtmlElement::Para, "Para"),
    (HtmlElement::Heading, "Heading"),
    (HtmlElement::Span, "Span"),
    (HtmlElement::SelfClosingSpan, "SelfClosingSpan"),
    (HtmlElement::ListItem, "ListItem"),
    (HtmlElement::ListContainer, "ListContainer"),
    (HtmlElement::Table, "Table"),
    (HtmlElement::TableRow, "TableRow"),
    (HtmlElement::TableCell, "TableCell"),
    (HtmlElement::Aside, "Aside"),
    (HtmlElement::Sup, "Sup"),
    (HtmlElement::Anchor, "Anchor"),
    (HtmlElement::Figure, "Figure"),
    (HtmlElement::Image, "Image"),
    (HtmlElement::Section, "Section"),
    (HtmlElement::Ruby, "Ruby"),
    (HtmlElement::Bold, "Bold"),
    (HtmlElement::Italic, "Italic"),
    (HtmlElement::Em, "Em"),
    (HtmlElement::Div, "Div"),
];

/// Position is the code AND the bit position in the context mask.
pub(crate) const CONTEXTS: &[(SpecContext, &str)] = &[
    (SpecContext::Scripture, "Scripture"),
    (SpecContext::BookIdentification, "BookIdentification"),
    (SpecContext::BookHeaders, "BookHeaders"),
    (SpecContext::BookTitles, "BookTitles"),
    (SpecContext::BookIntroduction, "BookIntroduction"),
    (
        SpecContext::BookIntroductionEndTitles,
        "BookIntroductionEndTitles",
    ),
    (SpecContext::BookChapterLabel, "BookChapterLabel"),
    (SpecContext::ChapterContent, "ChapterContent"),
    (SpecContext::Peripheral, "Peripheral"),
    (SpecContext::PeripheralContent, "PeripheralContent"),
    (SpecContext::PeripheralDivision, "PeripheralDivision"),
    (SpecContext::Section, "Section"),
    (SpecContext::Para, "Para"),
    (SpecContext::List, "List"),
    (SpecContext::Table, "Table"),
    (SpecContext::Sidebar, "Sidebar"),
    (SpecContext::Footnote, "Footnote"),
    (SpecContext::CrossReference, "CrossReference"),
];

pub(crate) const SHAPES: &[(SpellingShape, &str)] = &[
    (SpellingShape::Any, "Any"),
    (SpellingShape::PlainOnly, "PlainOnly"),
    (SpellingShape::MilestoneOnly, "MilestoneOnly"),
];

const ATTR_STATUSES: &[(AttrStatus, &str)] = &[
    (AttrStatus::Required, "Required"),
    (AttrStatus::Optional, "Optional"),
    (AttrStatus::Deprecated, "Deprecated"),
];

/// The code for `v` — its position in `table`. Panics if the variant is missing:
/// forgetting to list a new enum variant fails the generator rather than
/// emitting a wrong bit pattern.
fn code_of<T: PartialEq + Copy + std::fmt::Debug>(table: &[(T, &str)], v: T) -> u32 {
    table
        .iter()
        .position(|(candidate, _)| *candidate == v)
        .unwrap_or_else(|| panic!("emit.rs code table is missing {v:?} — add it at the END"))
        as u32
}

/// `Option<Variant>` → `0` for `None`, `1 + code` otherwise.
fn opt_code<T: PartialEq + Copy + std::fmt::Debug>(table: &[(T, &str)], v: Option<T>) -> u32 {
    v.map_or(0, |inner| 1 + code_of(table, inner))
}

// ---------------------------------------------------------------------------
// Bit layout
// ---------------------------------------------------------------------------

/// One packed field: `(shift, width, rust name)`. The name is emitted as a
/// `const` in `generated.rs` so the decoder reads the same numbers.
struct Field(u32, u32, &'static str);

const F_KIND: Field = Field(0, 4, "KIND");
const F_CATEGORY: Field = Field(4, 5, "CATEGORY");
const F_WS: Field = Field(9, 3, "WS");
const F_PAYLOAD: Field = Field(12, 3, "PAYLOAD");
const F_NUMBERING: Field = Field(15, 4, "NUMBERING");
const F_OPENS: Field = Field(19, 4, "OPENS");
const F_CLOSES: Field = Field(23, 4, "CLOSES");
const F_CLOSING: Field = Field(27, 2, "CLOSING");
const F_DEPRECATED: Field = Field(29, 1, "DEPRECATED");
const F_HTML: Field = Field(30, 5, "HTML");
const F_SHAPE: Field = Field(35, 2, "SHAPE");
const F_CONTEXTS: Field = Field(37, 20, "CONTEXTS");
const F_CONTRIBUTES: Field = Field(57, 5, "CONTRIBUTES");
const F_ATTR_OFF: Field = Field(62, 12, "ATTR_OFF");
const F_ATTR_LEN: Field = Field(74, 4, "ATTR_LEN");
const F_DEFAULT_ATTR: Field = Field(78, 4, "DEFAULT_ATTR");
// Deliberately no PRECEDENCE/rank field — see the note in schema.rs.

const FIELDS: &[&Field] = &[
    &F_KIND,
    &F_CATEGORY,
    &F_WS,
    &F_PAYLOAD,
    &F_NUMBERING,
    &F_OPENS,
    &F_CLOSES,
    &F_CLOSING,
    &F_DEPRECATED,
    &F_HTML,
    &F_SHAPE,
    &F_CONTEXTS,
    &F_CONTRIBUTES,
    &F_ATTR_OFF,
    &F_ATTR_LEN,
    &F_DEFAULT_ATTR,
];

/// Bits of the 128 the layout above occupies. Asserted against the field table
/// by the tests, and reported by the bin.
pub const BITS_USED: u32 = 82;

/// Place `value` into `field`, asserting it fits. An overflowing value is a
/// design event — widen the field and re-lay-out, never truncate.
fn put(word: &mut u128, field: &Field, value: u32, row: &MarkerRow) {
    let max = (1u64 << field.1) - 1;
    assert!(
        u64::from(value) <= max,
        "{}: value {} does not fit {} bits of field {}",
        row.marker,
        value,
        field.1,
        field.2
    );
    *word |= u128::from(value) << field.0;
}

/// `Numbering` has no variant table because its code carries a PAYLOAD (the
/// cap): 0 = Unnumbered, 1..=13 = `UpTo(n)`, 14 = Unbounded, 15 = TableColumns.
pub(crate) fn numbering_code(n: Numbering) -> u32 {
    match n {
        Numbering::Unnumbered => 0,
        Numbering::UpTo(cap) => {
            assert!(
                (1..=13).contains(&cap),
                "Numbering::UpTo({cap}) is outside the 1..=13 the packing allows"
            );
            u32::from(cap)
        }
        // 14/15 = the two codes above any packable cap, in declaration order.
        Numbering::Unbounded => 14,
        Numbering::TableColumns => 15,
    }
}

// ---------------------------------------------------------------------------
// Derived facts, computed here so no consumer recomputes them
// ---------------------------------------------------------------------------

fn context_mask(contexts: &[SpecContext]) -> u32 {
    contexts
        .iter()
        .fold(0, |mask, c| mask | 1 << code_of(CONTEXTS, *c))
}

/// The name a lexeme is matched by, as the u64 the generated matcher compares.
/// Little-endian byte load of a name padded with zeros — so `"q"` and `"q\0..."`
/// are the same key and no length compare is needed.
fn name_key(name: &str) -> u64 {
    assert!(
        name.len() <= 8,
        "canonical name `{name}` does not fit a u64 load"
    );
    // The matcher is ONE compare because the alpha stem is the whole name. A
    // canonical name carrying a digit would silently make `\p1` resolve to `p`
    // + level 1 instead of to itself, so the invariant is asserted.
    assert!(
        name.bytes().all(|b| b.is_ascii_lowercase()),
        "canonical name `{name}` is not pure lowercase ASCII — the single-compare \
         matcher in the generated `by_name` assumes the alpha stem IS the name. \
         Either drop the row or restore the exact-name-first pass."
    );
    let mut bytes = [0u8; 8];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    u64::from_le_bytes(bytes)
}

/// Every row's attribute list concatenated into one flat array, reusing an
/// identical run where one already exists (`(sid, eid)` is shared by several
/// milestones). Returns the flat entries plus each row's `(offset, len)`.
type AttrEntry = (&'static str, AttrStatus);
struct FlatAttrs {
    entries: Vec<AttrEntry>,
    spans: Vec<(usize, usize)>,
}

fn flatten_attributes() -> FlatAttrs {
    let mut entries: Vec<AttrEntry> = Vec::new();
    let mut spans = Vec::with_capacity(ROWS.len());

    for row in ROWS {
        let wanted = row.defined_attributes;
        let offset = if wanted.is_empty() {
            0
        } else if let Some(at) = entries.windows(wanted.len()).position(|w| w == wanted) {
            at
        } else {
            let at = entries.len();
            entries.extend_from_slice(wanted);
            at
        };
        spans.push((offset, wanted.len()));
    }

    FlatAttrs { entries, spans }
}

// ---------------------------------------------------------------------------
// The emission: template + data chunks
// ---------------------------------------------------------------------------

/// Everything row-independent, as plain Rust. The chunks below replace its
/// `@@TOKEN@@` lines.
const TEMPLATE: &str = include_str!("generated.rs.tmpl");

/// Generate the full text of `src/tables/generated.rs`. Pure: same `ROWS`, same
/// bytes, which is what makes the freshness test possible.
pub fn generated_rs() -> String {
    let attrs = flatten_attributes();
    splice(
        TEMPLATE,
        &[
            ("LAYOUT_CONSTS", layout_consts()),
            ("PACKED_ROWS", packed_rows(&attrs)),
            ("NAMES", names_array()),
            ("ATTRS", attrs_array(&attrs)),
            ("DECODERS", decoders()),
            ("CONTEXT_BIT_ARMS", context_bit_arms()),
            ("BY_NAME_ARMS", by_name_arms()),
            ("V_FORBIDDEN", v_forbidden()),
        ],
    )
}

/// Replace each `@@NAME@@` line with its chunk. Asserts every token is used
/// exactly once and none are left over — a template/chunk mismatch is a
/// generator bug, not a formatting choice.
fn splice(template: &str, chunks: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (name, chunk) in chunks {
        let token = format!("@@{name}@@\n");
        assert!(
            out.contains(&token),
            "template is missing the @@{name}@@ line"
        );
        out = out.replacen(&token, chunk, 1);
    }
    assert!(!out.contains("@@"), "template has an unreplaced @@token@@");
    out
}

fn layout_consts() -> String {
    let mut out = String::new();
    out.push_str("// Bit layout — emitted from `tables::emit`'s field table.\n");
    for f in FIELDS {
        let _ = writeln!(out, "const S_{}: u32 = {};", f.2, f.0);
        let _ = writeln!(out, "const M_{}: u128 = {};", f.2, (1u128 << f.1) - 1);
    }
    let _ = writeln!(out, "\n/// Bits used of the 128 available.");
    let _ = writeln!(out, "pub const BITS_USED: u32 = {BITS_USED};");
    out
}

fn packed_rows(attrs: &FlatAttrs) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "\n/// The packed table. Index IS the marker index.\n\
         #[rustfmt::skip]\nstatic PACKED: [u128; {}] = [",
        ROWS.len()
    );

    for (i, row) in ROWS.iter().enumerate() {
        let mut word: u128 = 0;
        put(&mut word, &F_KIND, code_of(KINDS, row.kind), row);
        put(
            &mut word,
            &F_CATEGORY,
            code_of(CATEGORIES, row.category),
            row,
        );
        put(&mut word, &F_WS, code_of(WSREQS, row.ws_after_name), row);
        put(&mut word, &F_PAYLOAD, code_of(PAYLOADS, row.payload), row);
        put(
            &mut word,
            &F_NUMBERING,
            numbering_code(row.numbered_max),
            row,
        );
        put(&mut word, &F_OPENS, opt_code(SCOPES, row.opens_scope), row);
        put(
            &mut word,
            &F_CLOSES,
            opt_code(SCOPES, row.closes_scope),
            row,
        );
        put(&mut word, &F_CLOSING, code_of(CLOSINGS, row.closing), row);
        put(&mut word, &F_DEPRECATED, u32::from(row.deprecated), row);
        put(&mut word, &F_HTML, opt_code(HTML, row.html_element), row);
        put(&mut word, &F_SHAPE, code_of(SHAPES, row.shape), row);
        put(
            &mut word,
            &F_CONTEXTS,
            context_mask(row.allowed_contexts),
            row,
        );
        put(
            &mut word,
            &F_CONTRIBUTES,
            opt_code(CONTEXTS, row.contributes_context()),
            row,
        );

        let (offset, len) = attrs.spans[i];
        put(&mut word, &F_ATTR_OFF, offset as u32, row);
        put(&mut word, &F_ATTR_LEN, len as u32, row);
        let default = row.default_attribute.map(|name| {
            1 + row
                .defined_attributes
                .iter()
                .position(|(candidate, _)| *candidate == name)
                .unwrap_or_else(|| {
                    panic!(
                        "{}: default `{name}` is not in defined_attributes",
                        row.marker
                    )
                }) as u32
        });
        put(&mut word, &F_DEFAULT_ATTR, default.unwrap_or(0), row);

        let label = if row.marker.is_empty() {
            "<unresolved>"
        } else {
            row.marker
        };
        let _ = writeln!(out, "    0x{word:022x}, // {i:>3} {label}");
    }
    out.push_str("];\n");
    out
}

fn names_array() -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "\n/// idx → canonical name. This array is what makes the table double as the\n\
         /// MARKER CATALOG: iterate it and you have every marker USFM 3.2 defines.\n\
         #[rustfmt::skip]\nstatic NAMES: [&str; {}] = [",
        ROWS.len()
    );
    for row in ROWS {
        let _ = writeln!(out, "    {:?},", row.marker);
    }
    out.push_str("];\n");
    out
}

fn attrs_array(attrs: &FlatAttrs) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "\n/// Every row's `defined_attributes`, concatenated; a row's slice is\n\
         /// `[ATTR_OFF .. ATTR_OFF + ATTR_LEN]`. Identical runs are shared.\n\
         #[rustfmt::skip]\nstatic ATTRS: [(&str, AttrStatus); {}] = [",
        attrs.entries.len()
    );
    for (name, status) in &attrs.entries {
        let status_name = ATTR_STATUSES
            .iter()
            .find(|(candidate, _)| candidate == status)
            .map(|(_, name)| *name)
            .expect("AttrStatus code table is incomplete");
        let _ = writeln!(out, "    ({name:?}, AttrStatus::{status_name}),");
    }
    out.push_str("];\n");
    out
}

/// The field decoders — the only accessor fns whose bodies are data-driven
/// (their match arms come from the variant code tables).
fn decoders() -> String {
    let mut out = String::new();

    decoder(
        &mut out,
        &F_KIND,
        "kind",
        "MarkerKind",
        KINDS,
        "Coarse spec kind.",
    );
    decoder(
        &mut out,
        &F_CATEGORY,
        "category",
        "Category",
        CATEGORIES,
        "Fine spec category — behavior-bearing, see the schema's note on `\\pb`.",
    );
    decoder(
        &mut out,
        &F_WS,
        "ws_after_name",
        "Ws",
        WSREQS,
        "Structural whitespace required after the marker name — the scanner's\nper-class delimiter fold rule.",
    );
    decoder(
        &mut out,
        &F_PAYLOAD,
        "payload",
        "Payload",
        PAYLOADS,
        "The argument consumed right after the delimiter.",
    );
    decoder(
        &mut out,
        &F_CLOSING,
        "closing",
        "ClosingBehavior",
        CLOSINGS,
        "How this marker's scope ends.",
    );
    decoder(
        &mut out,
        &F_SHAPE,
        "shape",
        "SpellingShape",
        SHAPES,
        "Which spellings of the name this row claims.",
    );

    opt_decoder(
        &mut out,
        &F_OPENS,
        "opens_scope",
        "ScopeKind",
        SCOPES,
        "The scope an occurrence PUSHES A FRAME for, or `None`. Pushing only —\ndisplacement is the walker's pop over `context_mask`, never a column.",
    );
    opt_decoder(
        &mut out,
        &F_CLOSES,
        "closes_scope",
        "ScopeKind",
        SCOPES,
        "The scope an occurrence CLOSES — `esbe` and nothing else today.",
    );
    opt_decoder(
        &mut out,
        &F_HTML,
        "html_element",
        "HtmlElement",
        HTML,
        "Default HTML element class for export. A default, not a mandate.",
    );
    opt_decoder(
        &mut out,
        &F_CONTRIBUTES,
        "contributes_context",
        "SpecContext",
        CONTEXTS,
        "What context an open frame of this row puts its children in — BAKED from\n`schema::contributes_context(kind, category)`, so the walker's push-time\nframe stamp is one array read [I]. `None` means the frame is transparent.",
    );

    out
}

/// Emit a total accessor: `pub fn <fn_name>(idx) -> <ty>`. `doc` is plain
/// text; each line gets the `///` prefix here.
fn decoder<T: Copy>(
    out: &mut String,
    field: &Field,
    fn_name: &str,
    ty: &str,
    table: &[(T, &str)],
    doc: &str,
) {
    let docs = doc_lines(doc);
    let arms: String = table
        .iter()
        .enumerate()
        .map(|(i, (_, name))| format!("        {i} => {ty}::{name},\n"))
        .collect();
    let f = field.2;
    let _ = write!(
        out,
        "{docs}\
#[inline]
pub fn {fn_name}(idx: MarkerIdx) -> {ty} {{
    match field(idx, S_{f}, M_{f}) {{
{arms}        _ => unreachable!(),
    }}
}}

"
    );
}

/// Emit an `Option`-returning accessor for a `0 = None, 1 + code` field.
fn opt_decoder<T: Copy>(
    out: &mut String,
    field: &Field,
    fn_name: &str,
    ty: &str,
    table: &[(T, &str)],
    doc: &str,
) {
    let docs = doc_lines(doc);
    let arms: String = table
        .iter()
        .enumerate()
        .map(|(i, (_, name))| format!("        {} => Some({ty}::{name}),\n", i + 1))
        .collect();
    let f = field.2;
    let _ = write!(
        out,
        "{docs}\
#[inline]
pub fn {fn_name}(idx: MarkerIdx) -> Option<{ty}> {{
    match field(idx, S_{f}, M_{f}) {{
        0 => None,
{arms}        _ => unreachable!(),
    }}
}}

"
    );
}

/// Plain text → `///`-prefixed doc lines.
fn doc_lines(doc: &str) -> String {
    doc.lines().map(|l| format!("/// {l}\n")).collect()
}

fn context_bit_arms() -> String {
    let mut out = String::new();
    for (i, (_, name)) in CONTEXTS.iter().enumerate() {
        let _ = writeln!(out, "        SpecContext::{name} => {i},");
    }
    out
}

fn by_name_arms() -> String {
    // Group rows by canonical name, skipping the nameless index-0 row.
    let mut by_name: Vec<(&str, Vec<usize>)> = Vec::new();
    for (i, row) in ROWS.iter().enumerate() {
        if row.marker.is_empty() {
            continue;
        }
        match by_name.iter_mut().find(|(name, _)| *name == row.marker) {
            Some((_, idxs)) => idxs.push(i),
            None => by_name.push((row.marker, vec![i])),
        }
    }
    by_name.sort_by_key(|(name, _)| name_key(name));

    let mut out = String::new();
    for (name, idxs) in &by_name {
        let key = name_key(name);
        if idxs.len() == 1 {
            let _ = writeln!(out, "        0x{key:016x} => {}, // {name}", idxs[0]);
            continue;
        }
        // An overloaded name: one extra compare against the shape the lexer saw.
        let _ = writeln!(out, "        0x{key:016x} => match shape {{ // {name}");
        for (query, shape_name) in SHAPES {
            let mut candidates = idxs.iter().filter(|i| ROWS[**i].shape.overlaps(*query));
            let resolved = match (candidates.next(), candidates.next()) {
                (Some(only), None) => only.to_string(),
                // `Any` against two disjoint rows is ambiguous by construction;
                // the lexer always knows which spelling it saw, so reaching
                // this arm is a caller bug, answered with the inert row.
                _ => "UNRESOLVED".to_string(),
            };
            let _ = writeln!(
                out,
                "            SpellingShape::{shape_name} => {resolved},"
            );
        }
        out.push_str("        },\n");
    }
    out
}

fn v_forbidden() -> String {
    let words = ROWS.len().div_ceil(64);
    let mut mask = vec![0u64; words];
    let mut listed: Vec<&str> = Vec::new();
    let mut errata: Vec<&str> = Vec::new();

    for name in schema::V_FORBIDDEN_IN_PARAGRAPHS {
        match ROWS.iter().position(|row| row.marker == *name) {
            Some(idx) => {
                mask[idx / 64] |= 1 << (idx % 64);
                listed.push(name);
            }
            // Rail members 3.2 does not document (`k1`, `k2`, `restore`): the
            // authored list records rail membership, so they stay there and
            // simply have no bit to set.
            None => errata.push(name),
        }
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "/// `\\v` is illegal inside a paragraph opened by one of these markers — a\n\
         /// 3.2 rule keyed on the ENCLOSING paragraph, which no context mask can\n\
         /// express. A frame carries the `marker_idx` that opened it, so this is one\n\
         /// bit test on that index.\n\
         ///\n\
         /// {} markers set a bit. {} rail members are ruled ERRATA with no row and no\n\
         /// bit ({}) — see `schema::V_FORBIDDEN_IN_PARAGRAPHS`.\n\
         #[rustfmt::skip]\nstatic V_FORBIDDEN: [u64; {}] = [",
        listed.len(),
        errata.len(),
        errata.join(", "),
        words
    );
    for word in &mask {
        let _ = writeln!(out, "    0x{word:016x},");
    }
    out.push_str("];\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one invariant the whole packing rests on: no two fields overlap, and
    /// `BITS_USED` is honest. Overlap would corrupt a neighbour silently — the
    /// `put` width assert only catches a value that is too big for its OWN field.
    #[test]
    fn fields_do_not_overlap_and_bits_used_is_honest() {
        let mut claimed: u128 = 0;
        for f in FIELDS {
            let bits = ((1u128 << f.1) - 1) << f.0;
            assert_eq!(
                claimed & bits,
                0,
                "field {} at shift {} overlaps an earlier field",
                f.2,
                f.0
            );
            claimed |= bits;
        }
        assert_eq!(
            claimed.count_ones(),
            BITS_USED,
            "BITS_USED disagrees with the field table"
        );
        assert_eq!(
            claimed,
            (1u128 << BITS_USED) - 1,
            "the field table leaves a hole below BITS_USED — fine, but say so here"
        );
    }

    /// The generated text must be valid enough to have the pieces every consumer
    /// imports. Cheap smoke check; the real proof is that the crate compiles.
    #[test]
    fn emission_has_its_public_surface() {
        let text = generated_rs();
        for expected in [
            "pub fn marker_idx(",
            "pub const UNRESOLVED",
            "pub fn contributes_context(",
            "pub fn forbids_verse(",
            "static PACKED",
            "static NAMES",
        ] {
            assert!(text.contains(expected), "emission is missing `{expected}`");
        }
    }
}
