//! The mask map's declaration. Both ends are generated from this file: the
//! Rust writer (`mask/generated.rs`) and the TypeScript reader
//! (`galley/mask-reader.ts`). Neither is typed by hand, so they cannot
//! disagree.
//!
//! ```text
//! schema::RECORDS  ->  emit::generated_rs()  ->  galley/src/mask/generated.rs
//! schema's header  ->  emit::reader_ts()     ->  galley/mask-reader.ts
//! ```
//!
//! The vocabulary and the emitters are `ticket`'s. What is this format's own
//! is the ENVELOPE below: seven header words and one counted row block, with
//! no directory, because one buffer describes one book.
//!
//! The rows obey five laws, which `mask/mod.rs` and `galley/tests/mask.rs`
//! assert:
//!
//! 1. `ranges` ascending, disjoint, each non-empty.
//! 2. Maximal: `ranges[i].sourceTo < ranges[i + 1].sourceFrom`, strictly.
//! 3. `starts[0] == 0`, `starts[i + 1] == starts[i] + len(ranges[i])` — the
//!    prefix sum the reader rebuilds at open rather than the wire carrying it.
//! 4. The source slices, concatenated, ARE the projection: nothing is
//!    inserted between two ranges.
//! 5. `projectedLen` is that concatenation's length, or 0 when empty.

use ticket::schema::{Field, Layout, Record, Region, Sample, Shape, Slot};

pub use ticket::schema::{Space, Width};

use Space::Offset;
use Width::U32;

/// `MASK`, little-endian — read out of the buffer's first four bytes in order.
pub const MAGIC: u32 = u32::from_le_bytes(*b"MASK");

/// Bumped when this envelope's or the record's shape changes. A reader that
/// does not know this number stops rather than misreading a field.
pub const FORMAT_VERSION: u32 = 1;

/// Header `flags`: every offset in the buffer is UTF-16 rather than a byte.
pub const FLAG_UTF16: u32 = 1 << 0;

/// Header `recipe`: which cut the ranges came from, so a consumer caching both
/// cannot confuse them.
pub const RECIPE_VERSE_TEXT: u32 = 0;
pub const RECIPE_STRUCTURE: u32 = 1;
pub const RECIPE_TEXT: u32 = 2;

/// The header, then the rows. Nothing else.
pub const HEADER_BYTES: usize = 28;
pub const HEADER_MAGIC_OFFSET: usize = 0;
pub const HEADER_VERSION_OFFSET: usize = 4;
pub const HEADER_FLAGS_OFFSET: usize = 8;
pub const HEADER_RECIPE_OFFSET: usize = 12;
pub const HEADER_RANGE_COUNT_OFFSET: usize = 16;
/// The length of the text the ranges were cut FROM, in the flags' unit. A
/// consumer holding a different revision of that text learns so here, before
/// it joins slices out of the wrong string.
pub const HEADER_SOURCE_LEN_OFFSET: usize = 20;
/// The length of the projection those ranges concatenate to — the consumer's
/// checksum after the join.
pub const HEADER_PROJECTED_LEN_OFFSET: usize = 24;

/// One kept source span.
pub const RANGE: Record = Record {
    name: "Range",
    plural: "ranges",
    rust_ty: "Range<u32>",
    binding: "r",
    context: &[],
    tail: None,
    doc: "One kept source span. Sorted, disjoint, non-empty, maximal.",
    fields: &[
        Field {
            name: "sourceFrom",
            width: U32,
            space: Offset,
            rust: "r.start",
            doc: "First kept unit.",
        },
        Field {
            name: "sourceTo",
            width: U32,
            space: Offset,
            rust: "r.end",
            doc: "One past the last.",
        },
    ],
};

/// The one row record this format has.
pub const RECORDS: &[&Record] = &[&RANGE];

pub const HEADER: &[Slot] = &[
    Slot {
        name: "magic",
        at: HEADER_MAGIC_OFFSET,
        bytes: HEADER_VERSION_OFFSET - HEADER_MAGIC_OFFSET,
        doc: "`MASK`, little-endian.",
    },
    Slot {
        name: "version",
        at: HEADER_VERSION_OFFSET,
        bytes: HEADER_FLAGS_OFFSET - HEADER_VERSION_OFFSET,
        doc: "FORMAT_VERSION. A reader that does not know it stops.",
    },
    Slot {
        name: "flags",
        at: HEADER_FLAGS_OFFSET,
        bytes: HEADER_RECIPE_OFFSET - HEADER_FLAGS_OFFSET,
        doc: "Bit 0: every offset is UTF-16 rather than a byte.",
    },
    Slot {
        name: "recipe",
        at: HEADER_RECIPE_OFFSET,
        bytes: HEADER_RANGE_COUNT_OFFSET - HEADER_RECIPE_OFFSET,
        doc: "Which cut the ranges came from: 0 verse text, 1 structure, 2 text.",
    },
    Slot {
        name: "rangeCount",
        at: HEADER_RANGE_COUNT_OFFSET,
        bytes: HEADER_SOURCE_LEN_OFFSET - HEADER_RANGE_COUNT_OFFSET,
        doc: "Range rows after the header.",
    },
    Slot {
        name: "sourceLen",
        at: HEADER_SOURCE_LEN_OFFSET,
        bytes: HEADER_PROJECTED_LEN_OFFSET - HEADER_SOURCE_LEN_OFFSET,
        doc: "The length of the text the ranges were cut FROM, in the flags' unit.",
    },
    Slot {
        name: "projectedLen",
        at: HEADER_PROJECTED_LEN_OFFSET,
        bytes: HEADER_BYTES - HEADER_PROJECTED_LEN_OFFSET,
        doc: "The length of the projection those ranges concatenate to.",
    },
];

/// The whole mask map, region by region, for `docs/layouts/mask.json`.
pub fn layout() -> Layout {
    Layout {
        format: "mask",
        magic: MAGIC,
        version: FORMAT_VERSION,
        declared_in: "galley/src/mask/schema.rs",
        reader: "galley/mask-reader.ts",
        generated: true,
        doc: "One book's projection as the source spans it keeps.",
        envelope: vec![
            Region {
                name: "header",
                at: "0".into(),
                shape: Shape::Fixed {
                    bytes: HEADER_BYTES,
                    slots: HEADER,
                },
                doc: "The header, then the rows. Nothing else.",
            },
            Region {
                name: "ranges",
                at: format!("{HEADER_BYTES}"),
                shape: Shape::Rows {
                    of: &RANGE,
                    count: "header.rangeCount".into(),
                },
                doc: RANGE.doc,
            },
        ],
        records: RECORDS,
        sample: Some(Sample {
            usfm: "samples/sample.usfm",
            bin: "samples/mask.bin",
            options: &[("recipe", "\"verseText\""), ("utf16", "false")],
        }),
    }
}
