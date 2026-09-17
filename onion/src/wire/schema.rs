//! THE DISH'S DECLARATION. Both ends of this wire are generated from this file:
//! the Rust writer (`wire/generated.rs`) and the TypeScript reader
//! (`onion-wasm/reader.ts`). Neither is typed by hand, so they cannot disagree.
//! The vocabulary it is written in, and the emitters that read it, are
//! `ticket`'s — shared with every other declaration in the workspace.
//!
//! ```text
//! schema::RECORDS  ->  emit::wire_generated_rs()  ->  onion/src/wire/generated.rs
//!                  ->  emit::reader_ts()          ->  onion-wasm/reader.ts
//! ```
//!
//! A field declares four things: what it is called on the wire, how wide it is,
//! whether it is a SOURCE OFFSET (and so converts under `utf16: true`), and the
//! expression the writer uses to reach it on the engine type. Nothing here
//! describes layout beyond field order — the wire is little-endian by
//! construction, never a cast of a Rust struct.

/// The vocabulary every declaration in the workspace is built from. Moved to
/// `ticket` so onion, galley and a future sous declaration share one answer to
/// "how does a `u16` reach the buffer"; re-exported here so every path that
/// named `wire::schema::Record` still resolves.
pub use ticket::schema::{Field, Record, Space, Width};

use Space::{Offset, Plain};
use Width::{U8, U16, U32};

/// The scanner's leaves.
///
/// `end`, not `len`: a length is not independently convertible — a UTF-16 start
/// beside a byte length names no span, and deriving one needs its start's
/// converted value in hand. Writing both endpoints makes every convertible
/// field a plain [`Space::Offset`], so one sorted sweep converts the whole dish
/// with no field special-cased. Two bytes a token buys that.
pub const TOKEN: Record = Record {
    name: "Token",
    plural: "tokens",
    rust_ty: "crate::Token",
    binding: "t",
    context: &[("source", "&[u8]")],
    doc: "One lexical token: the leaves of the tree.",
    tail: None,
    fields: &[
        Field {
            name: "start",
            width: U32,
            space: Offset,
            rust: "t.start",
            doc: "Where the token begins.",
        },
        Field {
            name: "end",
            width: U32,
            space: Offset,
            rust: "t.end()",
            doc: "Where it ends. Spans TILE the document, so ends ascend too.",
        },
        Field {
            name: "kind",
            width: U8,
            space: Plain,
            rust: "t.kind_bits",
            doc: "Packed TokenKind; bit 4 is the per-shape spelling bit.",
        },
        Field {
            name: "marker",
            width: U8,
            space: Plain,
            rust: "t.marker_idx",
            doc: "Row in the marker table; 0 is UNRESOLVED.",
        },
        Field {
            name: "level",
            width: U8,
            space: Plain,
            rust: "t.level",
            doc: "The trailing number as spelled: 2 for `\\q2`, 1 for `\\tc1`, 0 \
                  for a bare `\\q`. The row's `numbering` says whether it is a \
                  nesting level or a column index.",
        },
        Field {
            name: "flags",
            width: U8,
            space: Plain,
            rust: "t.wire_flags(source)",
            doc: "`TOKEN_DELIMITER_FOLDED` | `TOKEN_BLANK`. Also rounds the row \
                  to 12 bytes, so every row's `start` and `end` land aligned \
                  for a typed-array view over a 4-aligned section.",
        },
    ],
};

/// One structural node. Every field is an INDEX — nodes never convert.
pub const NODE: Record = Record {
    name: "Node",
    plural: "nodes",
    rust_ty: "crate::cst::Node",
    binding: "n",
    context: &[],
    doc: "One structural node. `token` and the child range are indices, not offsets.",
    tail: None,
    fields: &[
        Field {
            name: "token",
            width: U32,
            space: Plain,
            rust: "n.token",
            doc: "The opening marker's token index; u32::MAX on the root.",
        },
        Field {
            name: "childFrom",
            width: U32,
            space: Plain,
            rust: "n.children.start",
            doc: "Start of this node's range in the child arena.",
        },
        Field {
            name: "childTo",
            width: U32,
            space: Plain,
            rust: "n.children.end",
            doc: "End of that range. Ranges are neither contiguous nor monotonic.",
        },
        Field {
            name: "reason",
            width: U8,
            space: Plain,
            rust: "n.reason",
            doc: "CloseReason: Explicit, Implicit, Recovery, Eof.",
        },
        Field {
            name: "ctx",
            width: U8,
            space: Plain,
            rust: "n.ctx",
            doc: "The stamped SpecContext.",
        },
    ],
};

/// A finding. A WIRE type: `lint::Observation` names tokens, this names spans,
/// and `Observation.code` is an enum which no wire may carry as-is.
///
/// Eight words rather than seven so a reader indexes by shift, with the last
/// reserved.
pub const DIAGNOSTIC: Record = Record {
    name: "Diagnostic",
    plural: "diagnostics",
    rust_ty: "Diagnostic",
    binding: "d",
    context: &[],
    doc: "One finding, with its anchor resolved from a token index to a span.",
    tail: None,
    fields: &[
        Field {
            name: "code",
            width: U32,
            space: Plain,
            rust: "d.code",
            doc: "Index into the lint catalog.",
        },
        Field {
            name: "from",
            width: U32,
            space: Offset,
            rust: "d.from",
            doc: "The anchor's start.",
        },
        Field {
            name: "to",
            width: U32,
            space: Offset,
            rust: "d.to",
            doc: "The anchor's end.",
        },
        Field {
            name: "secondFrom",
            width: U32,
            space: Offset,
            rust: "d.second_from",
            doc: "The other party's start, or NONE.",
        },
        Field {
            name: "secondTo",
            width: U32,
            space: Offset,
            rust: "d.second_to",
            doc: "The other party's end, or NONE.",
        },
        Field {
            name: "aux",
            width: U32,
            space: Plain,
            rust: "d.aux",
            doc: "The code's aux integer; its meaning is the catalog row's.",
        },
        Field {
            name: "fix",
            width: U32,
            space: Plain,
            rust: "d.fix",
            doc: "Index into the fix section, or NONE.",
        },
        Field {
            name: "reserved",
            width: U32,
            space: Plain,
            rust: "0",
            doc: "Rounds the row to 32 bytes. Room for a u64 code or a checksum.",
        },
    ],
};

/// One repair: a range into the edit section.
pub const FIX: Record = Record {
    name: "Fix",
    plural: "fixes",
    rust_ty: "Fix",
    binding: "f",
    context: &[],
    doc: "One offered repair: the edits it owns.",
    tail: None,
    fields: &[
        Field {
            name: "editFrom",
            width: U32,
            space: Plain,
            rust: "f.edit_from",
            doc: "Start of this fix's range in the edit section.",
        },
        Field {
            name: "editTo",
            width: U32,
            space: Plain,
            rust: "f.edit_to",
            doc: "End of that range.",
        },
    ],
};

/// One edit. `textFrom`/`textLen` index the fix TEXT section — they are
/// positions in an inserted-bytes blob, not in the source, so they never
/// convert. Absolute, so no reader computes a prefix sum.
pub const EDIT: Record = Record {
    name: "Edit",
    plural: "edits",
    rust_ty: "Edit",
    binding: "e",
    context: &[],
    doc: "One edit of a repair: replace [from, to) with the named text.",
    tail: None,
    fields: &[
        Field {
            name: "from",
            width: U32,
            space: Offset,
            rust: "e.from",
            doc: "Where the replacement starts.",
        },
        Field {
            name: "to",
            width: U32,
            space: Offset,
            rust: "e.to",
            doc: "Where it ends; equal to `from` for a pure insertion.",
        },
        Field {
            name: "textFrom",
            width: U32,
            space: Plain,
            rust: "e.text_from",
            doc: "Start of the inserted bytes in the fix-text section.",
        },
        Field {
            name: "textLen",
            width: U32,
            space: Plain,
            rust: "e.text_len",
            doc: "Their length. Zero is a pure deletion.",
        },
    ],
};

/// One chapter, tiling the document. Row 0 is the front matter.
pub const CHAPTER: Record = Record {
    name: "Chapter",
    plural: "chapters",
    rust_ty: "Chapter",
    binding: "c",
    context: &[],
    doc: "One chapter's extent and number. The rows tile the document.",
    tail: None,
    fields: &[
        Field {
            name: "start",
            width: U32,
            space: Offset,
            rust: "c.start",
            doc: "Where the chapter begins.",
        },
        Field {
            name: "end",
            width: U32,
            space: Offset,
            rust: "c.end",
            doc: "Where it ends.",
        },
        Field {
            name: "token",
            width: U32,
            space: Plain,
            rust: "c.token",
            doc: "The `\\c` marker's token index; NONE on the front-matter row.",
        },
        Field {
            name: "designator",
            width: U32,
            space: Plain,
            rust: "c.designator",
            doc: "The number's own token index, or NONE. `\\c` opens no node, so \
                  this is the only route from a chapter to its designator.",
        },
        Field {
            name: "number",
            width: U16,
            space: Plain,
            rust: "c.number",
            doc: "The number read, or 0 for absent or malformed.",
        },
    ],
};

/// One `\v`, wherever it sits — `\q1 \v 5` puts one mid-line.
pub const VERSE: Record = Record {
    name: "Verse",
    plural: "verses",
    rust_ty: "Verse",
    binding: "v",
    context: &[],
    doc: "One verse anchor and the range of verse numbers it names.",
    tail: None,
    fields: &[
        Field {
            name: "at",
            width: U32,
            space: Offset,
            rust: "v.at",
            doc: "Where the anchor sits.",
        },
        Field {
            name: "token",
            width: U32,
            space: Plain,
            rust: "v.token",
            doc: "The `\\v` marker's token index.",
        },
        Field {
            name: "designator",
            width: U32,
            space: Plain,
            rust: "v.designator",
            doc: "The number's own token index, or NONE. `\\v` opens no node, so \
                  this is the only route from a verse to its designator.",
        },
        Field {
            name: "chapter",
            width: U16,
            space: Plain,
            rust: "v.chapter",
            doc: "The enclosing chapter's number.",
        },
        Field {
            name: "first",
            width: U16,
            space: Plain,
            rust: "v.first",
            doc: "First verse named; 0 when the designator is absent or malformed.",
        },
        Field {
            name: "last",
            width: U16,
            space: Plain,
            rust: "v.last",
            doc: "Last verse named — a bridge `\\v 5-7` names 5 through 7.",
        },
    ],
};

/// Every ROW record, for codegen. Section order is [`SECTIONS`], not this.
pub const RECORDS: &[&Record] = &[&TOKEN, &NODE, &DIAGNOSTIC, &FIX, &EDIT, &CHAPTER, &VERSE];

/// What backs a section.
pub enum SectionKind {
    /// Rows of a [`Record`].
    Rows(&'static Record),
    /// A flat `u32` array — the child arena. Indices, so never converted.
    Indices,
    /// Opaque bytes — the fix text blob.
    Bytes,
}

pub struct Section {
    /// The key `deserialize` hands back, and the reader's field name.
    pub name: &'static str,
    pub kind: SectionKind,
    pub doc: &'static str,
}

/// THE WIRE ORDER. A section's index here is its slot in the dish's directory;
/// permuting this is a format change and breaks every reader.
pub const SECTIONS: &[Section] = &[
    Section {
        name: "tokens",
        kind: SectionKind::Rows(&TOKEN),
        doc: "The scanner's output, in document order.",
    },
    Section {
        name: "nodes",
        kind: SectionKind::Rows(&NODE),
        doc: "The tree's nodes. Node id is the row index.",
    },
    Section {
        name: "childIds",
        kind: SectionKind::Indices,
        doc: "The shared child arena: token ids, and node ids tagged with bit 31.",
    },
    Section {
        name: "diagnostics",
        kind: SectionKind::Rows(&DIAGNOSTIC),
        doc: "Findings, sorted by anchor then code. Empty unless asked for.",
    },
    Section {
        name: "fixes",
        kind: SectionKind::Rows(&FIX),
        doc: "Offered repairs, reached through a diagnostic's `fix`.",
    },
    Section {
        name: "edits",
        kind: SectionKind::Rows(&EDIT),
        doc: "The shared edit arena every fix's range indexes.",
    },
    Section {
        name: "fixText",
        kind: SectionKind::Bytes,
        doc: "Inserted bytes, concatenated. Edits carry absolute positions into it.",
    },
    Section {
        name: "chapters",
        kind: SectionKind::Rows(&CHAPTER),
        doc: "The chapter table. Empty unless asked for.",
    },
    Section {
        name: "verses",
        kind: SectionKind::Rows(&VERSE),
        doc: "The verse anchors. Empty unless asked for.",
    },
];

/// Bumped whenever [`SECTIONS`] or any [`Record`] changes shape. A reader
/// refuses a dish it does not recognise rather than misreading one.
pub const FORMAT_VERSION: u32 = 4;

/// `"ONWR"`, little-endian — the dish's first four bytes.
pub const MAGIC: u32 = u32::from_le_bytes(*b"ONWR");

/// Set in the header's flag word when every offset is a UTF-16 code unit.
pub const FLAG_UTF16: u32 = 1 << 0;

/// Set on a token whose last byte is the one horizontal delimiter the scanner
/// folded onto it. Its payload therefore ends at `end - 1`, in byte and UTF-16
/// space alike — a space or a tab is one unit in each. Only the shapes that
/// fold carry it: `Marker`, `Milestone`, `Designator`, `NoteCaller`, `BookCode`.
pub const TOKEN_DELIMITER_FOLDED: u8 = 1 << 0;

/// Set on a `Text` token that is nothing but horizontal whitespace. Never set
/// on another kind; `Pad` is whitespace by definition.
pub const TOKEN_BLANK: u8 = 1 << 1;

/// "No such offset / no such index", the one absent-value rule.
pub const NONE: u32 = u32::MAX;
