//! `docs/markers.json`: the marker table as language-neutral JSON, for a port
//! that generates its own table instead of reading Rust.
//!
//! ```text
//! {"schema": "onion-markers-v1", "usfmVersion": "3.2", …,
//!  "enums": {"MarkerKind": [{"name": "Unknown"}, {"name": "Paragraph"}, …], …},
//!  "rows": [
//!    {"index": 0, "marker": "", "shape": "Any", "kind": "Unknown", …, "derived": {…}},
//!    {"index": 3, "marker": "p", …},
//!  ]}
//! ```
//!
//! Enum variants are listed in DECLARATION order, so a variant's position is
//! its Rust discriminant (`Node.ctx`, the context-mask bit). Rows are in index
//! order and every authored column is present; `derived` holds the facts the
//! engine computes from those columns, read through the same functions the
//! scanner and walker call, so a port never re-derives them.

use std::fmt::Write as _;

use mise::extensions::ExtensionCategory;

use super::{
    ATTR_STATUSES, CATEGORIES, CLOSINGS, CONTEXTS, HTML, KINDS, PAYLOADS, SCOPES, SHAPES, WSREQS,
    first_extension_row,
};
use crate::cst::{CloseReason, container_kind, displaces};
use crate::scanner::{designator_gated, folds_delimiter, opens_attrs_frame};
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::rows::{EXTENSION_TEMPLATES, ROWS};
use crate::tables::schema::{AttrStatus, MarkerRow, Numbering, USV_ESCAPE_LETTERS};
use crate::token::TokenKind;

const SCHEMA: &str = "onion-markers-v1";

/// How a marker token reaches its row. Kept as one string so the JSON is
/// self-contained; `generated::marker_idx` and `extensions::marker_idx` are
/// the code it describes.
const LOOKUP: &[&str] = &[
    "Input: one marker token's bytes, from its backslash to its end.",
    "1. Spelling. Skip the backslash and an optional '+' (nested). name = the longest run of ASCII [A-Za-z0-9]. Then: name empty and next byte '*' -> MilestoneTerminator ('\\*'); next bytes '-s' or '-e' -> Milestone (end = suffix is 'e'), the suffix is part of the token; next byte '*' -> ClosingMarker; otherwise Marker.",
    "2. spelled = name plus any '-s'/'-e' (no backslash, '+' or '*'). shape = MilestoneOnly for Milestone and MilestoneTerminator tokens, PlainOnly for everything else.",
    "3. A spelled name starting with 'z' goes to the extension registry only (no registry -> 0). Every other name goes to the spec lookup below.",
    "4. Spec lookup: empty -> 0. Strip one trailing '-s' or '-e'. stem = the leading run of ASCII a-z; rest = what follows it. stem empty or longer than 8 bytes -> 0. Candidates = rows with marker == stem, index < firstExtensionRow, and shape overlapping the query shape (Any overlaps everything; PlainOnly and MilestoneOnly overlap themselves and Any). Exactly one candidate, else 0 ('qt' is the only name with two rows).",
    "5. The rest must be a legal suffix for the candidate's numbering, else 0: Unnumbered -> rest empty; UpTo(n) -> rest empty, or all ASCII digits whose value (saturating) is in 1..=n; Unbounded -> rest all digits (empty allowed); TableColumns -> anything.",
    "6. A spec miss (0) asks the registry too when it holds a legacy name (registered without the 'z'). Registry: strip '-s'/'-e', exact name; on a miss strip trailing digits and accept only a TableColumns template. The template's shape must overlap the query shape, else 0.",
    "Index 0 is the inert empty row: unknown names, wrong case ('\\ND'), illegal levels ('\\s5', '\\p1'), unregistered '\\z' names.",
    "Token level: 0 when the resolved row's numbering is Unnumbered (so always 0 on row 0); otherwise the trailing ASCII digits of spelled minus '-s'/'-e', folded as u8 with saturating *10 + d ('\\liv999' -> 255).",
];

/// What each `derived` key means. Emitted beside the rows.
const DERIVED: &[(&str, &str)] = &[
    (
        "contributesContext",
        "SpecContext an open frame of this row stamps on its children, or null: the frame inherits its parent's.",
    ),
    (
        "contextMask",
        "allowedContexts as bits: bit i set for SpecContext ordinal i. Exactly the authored list.",
    ),
    (
        "isExtension",
        "index >= firstExtensionRow: a template a registered \\z marker resolves to, never reached by name.",
    ),
    (
        "displaces",
        "A Marker token of this row runs the walker's pop loop: opensScope != null, or kind is Chapter/Verse with a non-zero contextMask.",
    ),
    (
        "displacementCloseReason",
        "CloseReason stamped on a frame of this row when something else pops it: Implicit for closing None/OptionalExplicitUntilNoteEnd, Recovery otherwise.",
    ),
    (
        "foldsDelimiter",
        "A Marker or Milestone token of this row folds one following space/tab into its own span. Closers never fold. Row 0 folds (the shape default).",
    ),
    (
        "opensAttrsFrame",
        "An opener of this row can own a trailing attribute list (kind Character, Figure or Periph): the scanner arms its '|' search.",
    ),
    (
        "designatorGated",
        "This row's Designator payload must start with an ASCII digit, else the payload is Text (kind Chapter or Verse).",
    ),
    (
        "containerKind",
        "List or Table for the U25003 container milestones (category MilestoneList/MilestoneTable), else null.",
    ),
    (
        "forbidsVerse",
        "A paragraph opened by this row may not contain \\v (a lint rule).",
    ),
];

/// Generate the full text of `docs/markers.json`. Pure, like every emitter.
pub fn markers_json() -> String {
    let mut out = String::with_capacity(96 * 1024);
    out.push_str("{\n");
    field(&mut out, "schema", &quoted(SCHEMA));
    field(&mut out, "usfmVersion", &quoted("3.2"));
    field(
        &mut out,
        "source",
        &quoted(
            "onion/src/tables/rows.rs (tables::rows::ROWS), projected by tables::emit::markers_json",
        ),
    );
    field(
        &mut out,
        "regenerate",
        &quoted("cargo run -p usfm_onion --bin codegen"),
    );
    field(&mut out, "rowCount", &ROWS.len().to_string());
    field(&mut out, "unresolved", &generated::UNRESOLVED.to_string());
    field(
        &mut out,
        "firstExtensionRow",
        &first_extension_row().to_string(),
    );

    out.push_str("  \"lookup\": [\n");
    list(
        &mut out,
        LOOKUP.iter().map(|line| format!("    {}", quoted(line))),
    );
    out.push_str("  ],\n");

    out.push_str("  \"usvEscapes\": [");
    let escapes: Vec<String> = USV_ESCAPE_LETTERS
        .iter()
        .map(|(letter, digits)| {
            format!(
                "{{\"letter\": {}, \"hexDigits\": {digits}}}",
                quoted(&letter.to_string())
            )
        })
        .collect();
    out.push_str(&escapes.join(", "));
    out.push_str("],\n");

    out.push_str("  \"enums\": {\n");
    let enums = enums();
    list(
        &mut out,
        enums.iter().map(|(name, variants)| {
            format!(
                "    {}: [\n{}\n    ]",
                quoted(name),
                variants
                    .iter()
                    .map(|v| format!("      {v}"))
                    .collect::<Vec<_>>()
                    .join(",\n")
            )
        }),
    );
    out.push_str("  },\n");

    out.push_str("  \"extensionTemplates\": [\n");
    list(
        &mut out,
        EXTENSION_TEMPLATES.iter().map(|(category, name)| {
            let index = name.map_or(0, |name| {
                ROWS.iter()
                    .position(|row| row.marker == name)
                    .expect("template is a row")
            });
            format!(
                "    {{\"category\": {}, \"word\": {}, \"template\": {}, \"index\": {index}}}",
                quoted(&format!("{category:?}")),
                quoted(category.as_str()),
                name.map_or("null".to_string(), quoted)
            )
        }),
    );
    out.push_str("  ],\n");

    out.push_str("  \"derived\": {\n");
    list(
        &mut out,
        DERIVED
            .iter()
            .map(|(key, doc)| format!("    {}: {}", quoted(key), quoted(doc))),
    );
    out.push_str("  },\n");

    out.push_str("  \"rows\": [\n");
    list(
        &mut out,
        ROWS.iter()
            .enumerate()
            .map(|(index, row)| row_json(index, row)),
    );
    out.push_str("  ]\n}\n");
    out
}

fn row_json(index: usize, row: &MarkerRow) -> String {
    let idx = index as MarkerIdx;
    let contexts: Vec<String> = row
        .allowed_contexts
        .iter()
        .map(|c| quoted(name_of(CONTEXTS, *c)))
        .collect();
    let attributes: Vec<String> = row
        .defined_attributes
        .iter()
        .map(|(name, status)| {
            format!(
                "{{\"name\": {}, \"status\": {}}}",
                quoted(name),
                quoted(name_of(ATTR_STATUSES, *status))
            )
        })
        .collect();
    let opt = |name: Option<&str>| name.map_or("null".to_string(), quoted);

    let mut out = String::new();
    let _ = write!(
        out,
        "    {{\"index\": {index}, \"marker\": {}, \"shape\": {}, \"kind\": {}, \"category\": {}, \
         \"wsAfterName\": {}, \"payload\": {}, \"numbering\": {}, \"allowedContexts\": [{}], \
         \"opensScope\": {}, \"closesScope\": {}, \"attributes\": [{}], \"defaultAttribute\": {}, \
         \"closing\": {}, \"deprecated\": {}, \"htmlElement\": {}, \"priority\": {}",
        quoted(row.marker),
        quoted(name_of(SHAPES, row.shape)),
        quoted(name_of(KINDS, row.kind)),
        quoted(name_of(CATEGORIES, row.category)),
        quoted(name_of(WSREQS, row.ws_after_name)),
        quoted(name_of(PAYLOADS, row.payload)),
        numbering(row.numbered_max),
        contexts.join(", "),
        opt(row.opens_scope.map(|s| name_of(SCOPES, s))),
        opt(row.closes_scope.map(|s| name_of(SCOPES, s))),
        attributes.join(", "),
        opt(row.default_attribute),
        quoted(name_of(CLOSINGS, row.closing)),
        row.deprecated,
        opt(row.html_element.map(|h| name_of(HTML, h))),
        row.priority.map_or("null".to_string(), |p| p.to_string()),
    );

    let reason = match CloseReason::for_displacement(idx) {
        CloseReason::Implicit => "Implicit",
        CloseReason::Recovery => "Recovery",
        other => unreachable!("displacement never stamps {other:?}"),
    };
    let _ = write!(
        out,
        ", \"derived\": {{\"contributesContext\": {}, \"contextMask\": {}, \"isExtension\": {}, \
         \"displaces\": {}, \"displacementCloseReason\": {}, \"foldsDelimiter\": {}, \
         \"opensAttrsFrame\": {}, \"designatorGated\": {}, \"containerKind\": {}, \
         \"forbidsVerse\": {}}}}}",
        opt(row.contributes_context().map(|c| name_of(CONTEXTS, c))),
        generated::context_mask(idx),
        generated::is_extension(idx),
        displaces(row.kind, row.opens_scope, generated::context_mask(idx)),
        quoted(reason),
        folds_delimiter(TokenKind::Marker { nested: false }, idx),
        opens_attrs_frame(idx),
        designator_gated(idx),
        opt(container_kind(idx).map(|s| name_of(SCOPES, s))),
        generated::forbids_verse(idx),
    );
    out
}

/// `{"kind": "UpTo", "max": 4}`; the other three carry no payload.
fn numbering(n: Numbering) -> String {
    match n {
        Numbering::UpTo(max) => format!("{{\"kind\": \"UpTo\", \"max\": {max}}}"),
        Numbering::Unnumbered => "{\"kind\": \"Unnumbered\"}".to_string(),
        Numbering::Unbounded => "{\"kind\": \"Unbounded\"}".to_string(),
        Numbering::TableColumns => "{\"kind\": \"TableColumns\"}".to_string(),
    }
}

/// Every enum a row column uses, each variant a JSON object, in declaration
/// order. The emit code tables are in PACKING order (which differs for
/// `StructuralWhitespaceRequirement` and `ScopeKind`), so they are re-sorted by
/// discriminant here and checked dense.
fn enums() -> Vec<(&'static str, Vec<String>)> {
    fn plain<T: Copy>(table: &[(T, &str)], ordinal: impl Fn(T) -> usize) -> Vec<String> {
        let mut named: Vec<(usize, &str)> = table.iter().map(|(v, n)| (ordinal(*v), *n)).collect();
        named.sort_unstable();
        for (at, (ordinal, name)) in named.iter().enumerate() {
            assert_eq!(at, *ordinal, "{name} is not at its discriminant");
        }
        named
            .into_iter()
            .map(|(_, name)| format!("{{\"name\": {}}}", quoted(name)))
            .collect()
    }

    // CONTEXTS' position is already the mask bit; the assert pins it to the
    // discriminant too, which `Node.ctx` stores.
    let contexts = CONTEXTS
        .iter()
        .enumerate()
        .map(|(bit, (ctx, name))| {
            assert_eq!(*ctx as usize, bit, "CONTEXTS is not in declaration order");
            format!(
                "{{\"name\": {}, \"isPositional\": {}}}",
                quoted(name),
                ctx.is_positional()
            )
        })
        .collect();

    let numbering = vec![
        "{\"name\": \"Unnumbered\"}".to_string(),
        "{\"name\": \"UpTo\", \"payload\": \"max\", \"payloadRange\": [1, 13]}".to_string(),
        "{\"name\": \"Unbounded\"}".to_string(),
        "{\"name\": \"TableColumns\"}".to_string(),
    ];

    let extension_categories = ExtensionCategory::ALL
        .iter()
        .map(|c| {
            format!(
                "{{\"name\": {}, \"word\": {}}}",
                quoted(&format!("{c:?}")),
                quoted(c.as_str())
            )
        })
        .collect();

    vec![
        ("SpellingShape", plain(SHAPES, |v| v as usize)),
        ("MarkerKind", plain(KINDS, |v| v as usize)),
        ("Category", plain(CATEGORIES, |v| v as usize)),
        ("ScopeKind", plain(SCOPES, |v| v as usize)),
        (
            "StructuralWhitespaceRequirement",
            plain(WSREQS, |v| v as usize),
        ),
        ("Payload", plain(PAYLOADS, |v| v as usize)),
        ("Numbering", numbering),
        ("SpecContext", contexts),
        ("ClosingBehavior", plain(CLOSINGS, |v| v as usize)),
        (
            "AttrStatus",
            plain(ATTR_STATUSES, |v: AttrStatus| v as usize),
        ),
        ("HtmlElement", plain(HTML, |v| v as usize)),
        (
            "CloseReason",
            plain(
                &[
                    (CloseReason::Explicit, "Explicit"),
                    (CloseReason::Implicit, "Implicit"),
                    (CloseReason::Recovery, "Recovery"),
                    (CloseReason::Eof, "Eof"),
                ],
                |v| v as usize,
            ),
        ),
        ("ExtensionCategory", extension_categories),
    ]
}

fn name_of<T: PartialEq + Copy + std::fmt::Debug>(
    table: &[(T, &'static str)],
    v: T,
) -> &'static str {
    table
        .iter()
        .find(|(candidate, _)| *candidate == v)
        .map(|(_, name)| *name)
        .unwrap_or_else(|| panic!("emit.rs code table is missing {v:?}"))
}

fn field(out: &mut String, key: &str, value: &str) {
    let _ = writeln!(out, "  {}: {value},", quoted(key));
}

/// Items joined `,\n`, with a final newline.
fn list(out: &mut String, items: impl Iterator<Item = String>) {
    let items: Vec<String> = items.collect();
    out.push_str(&items.join(",\n"));
    out.push('\n');
}

fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
