//! Find's declaration. The Rust writer (`find/wire/generated.rs`) and the
//! TypeScript reader (`galley/find-reader.ts`) are both generated from this
//! file, so neither is typed by hand and the two cannot disagree.
//!
//! The vocabulary and the emitters are `ticket`'s. What is find's own is the
//! envelope in `mod.rs` — a header, two length arrays and two blobs.
//!
//! Every offset here is ALREADY UTF-16 when the writer sees it: find converts
//! per book, against two different tables (the projection's and the source's),
//! before a row is built. [`Space::Offset`] marks a position in text; whether
//! and when it converts is the encoder's business, and this encoder has done
//! it already.

use ticket::schema::{Field, Layout, Record, Region, Repeat, Sample, Shape, Slot};

pub use ticket::schema::{Space, Width};

use Space::{Offset, Plain};
use Width::U32;

/// `FIND` in ASCII, read out of the buffer's first four bytes in order.
pub const MAGIC: u32 = 0x444E_4946;

/// The layout below. A reader that does not know this number stops.
///
/// UNCHANGED by the move to a generated writer: the bytes are the same bytes,
/// which `find/wire/mod.rs`'s byte gate holds, so no consumer needed a release.
pub const VERSION: u32 = 1;

/// The header, ahead of the first hit.
pub const HEADER_BYTES: usize = 16;
pub const HEADER_MAGIC_OFFSET: usize = 0;
pub const HEADER_VERSION_OFFSET: usize = 4;
pub const HEADER_HIT_COUNT_OFFSET: usize = 8;
pub const HEADER_BOOK_COUNT_OFFSET: usize = 12;

/// One contiguous source piece of a hit.
pub const PIECE: Record = Record {
    name: "Piece",
    plural: "pieces",
    rust_ty: "(u32, u32)",
    binding: "p",
    context: &[],
    tail: None,
    doc: "One contiguous source piece of a hit, in the raw book's UTF-16.",
    fields: &[
        Field {
            name: "sourceFrom",
            width: U32,
            space: Offset,
            rust: "p.0",
            doc: "First unit of the piece in the SOURCE — where an edit lands.",
        },
        Field {
            name: "sourceTo",
            width: U32,
            space: Offset,
            rust: "p.1",
            doc: "One past its last unit.",
        },
    ],
};

/// One hit, and the pieces it landed on.
pub const HIT: Record = Record {
    name: "Hit",
    plural: "hits",
    rust_ty: "WireHit",
    binding: "h",
    context: &[],
    doc: "One hit: where a reader sees it, and where an editor would change it.",
    fields: &[
        Field {
            name: "bookIndex",
            width: U32,
            space: Plain,
            rust: "h.book",
            doc: "Into the buffer's own id table, which names every book SEARCHED.",
        },
        Field {
            name: "projectedFrom",
            width: U32,
            space: Offset,
            rust: "h.from",
            doc: "First unit of the hit in the PROJECTION — what a highlighter wants.",
        },
        Field {
            name: "projectedTo",
            width: U32,
            space: Offset,
            rust: "h.to",
            doc: "One past its last unit.",
        },
        Field {
            name: "pieceCount",
            width: U32,
            space: Plain,
            rust: "h.pieces.len()",
            doc: "Source pieces below; more than one where the hit crosses masked markup.",
        },
    ],
    // The reason the row is tailed rather than two numbers: a hit crossing a
    // masked gap is one range per contiguous piece, in order.
    tail: Some(Repeat {
        count: "pieceCount",
        of: &PIECE,
        rust: "&h.pieces",
    }),
};

/// One book's id length. The array sits before the blob so every `u32` in the
/// buffer stays four-byte aligned.
pub const ID_LEN: Record = Record {
    name: "IdLen",
    plural: "id_lens",
    rust_ty: "BookId",
    binding: "id",
    context: &[],
    tail: None,
    doc: "One searched book's id length in bytes, in id-table order.",
    fields: &[Field {
        name: "idByteLen",
        width: U32,
        space: Plain,
        rust: "id.as_str().len()",
        doc: "Bytes of UTF-8 in the blob at the end of the buffer.",
    }],
};

/// One preview's length, index-aligned with the hits.
pub const PREVIEW_LEN: Record = Record {
    name: "PreviewLen",
    plural: "preview_lens",
    rust_ty: "String",
    binding: "s",
    context: &[],
    tail: None,
    doc: "One hit's preview length in bytes, index-aligned with the hits.",
    fields: &[Field {
        name: "previewByteLen",
        width: U32,
        space: Plain,
        rust: "s.len()",
        doc: "Bytes of UTF-8 in the blob at the end of the buffer.",
    }],
};

/// Every record, in the order the buffer writes them.
pub const RECORDS: &[&Record] = &[&HIT, &PIECE, &ID_LEN, &PREVIEW_LEN];

pub const HEADER: &[Slot] = &[
    Slot {
        name: "magic",
        at: HEADER_MAGIC_OFFSET,
        bytes: HEADER_VERSION_OFFSET - HEADER_MAGIC_OFFSET,
        doc: "`FIND` in ASCII.",
    },
    Slot {
        name: "version",
        at: HEADER_VERSION_OFFSET,
        bytes: HEADER_HIT_COUNT_OFFSET - HEADER_VERSION_OFFSET,
        doc: "VERSION. A reader that does not know it stops.",
    },
    Slot {
        name: "hitCount",
        at: HEADER_HIT_COUNT_OFFSET,
        bytes: HEADER_BOOK_COUNT_OFFSET - HEADER_HIT_COUNT_OFFSET,
        doc: "Hit rows, and preview lengths.",
    },
    Slot {
        name: "bookCount",
        at: HEADER_BOOK_COUNT_OFFSET,
        bytes: HEADER_BYTES - HEADER_BOOK_COUNT_OFFSET,
        doc: "Books SEARCHED, whether or not they matched: the id table's length.",
    },
];

/// The whole find buffer, region by region, for `docs/layouts/find.json`.
pub fn layout() -> Layout {
    Layout {
        format: "find",
        magic: MAGIC,
        version: VERSION,
        declared_in: "galley/src/find/wire/schema.rs",
        reader: "galley/find-reader.ts",
        generated: true,
        doc: "The hits of one search: where a reader sees each, and where an editor would change it.",
        envelope: vec![
            Region {
                name: "header",
                at: "0".into(),
                shape: Shape::Fixed {
                    bytes: HEADER_BYTES,
                    slots: HEADER,
                },
                doc: "The header, ahead of the first hit.",
            },
            Region {
                name: "hits",
                at: format!("{HEADER_BYTES}"),
                shape: Shape::Rows {
                    of: &HIT,
                    count: "header.hitCount".into(),
                },
                doc: HIT.doc,
            },
            Region {
                name: "idLens",
                at: "after the last hit".into(),
                shape: Shape::Rows {
                    of: &ID_LEN,
                    count: "header.bookCount".into(),
                },
                doc: ID_LEN.doc,
            },
            Region {
                name: "previewLens",
                at: "after idLens".into(),
                shape: Shape::Rows {
                    of: &PREVIEW_LEN,
                    count: "header.hitCount".into(),
                },
                doc: PREVIEW_LEN.doc,
            },
            Region {
                name: "strings",
                at: "after previewLens".into(),
                shape: Shape::Blob {
                    bytes: "sum of idByteLen, then sum of previewByteLen".into(),
                    holds: None,
                },
                doc: "Every id's UTF-8 in order, then every preview's; after the lengths so every u32 stays aligned.",
            },
        ],
        records: RECORDS,
        sample: Some(Sample {
            usfm: "samples/sample.usfm",
            bin: "samples/find.bin",
            options: SAMPLE_OPTIONS,
        }),
    }
}

/// The search the checked-in sample ran: one hit crossing a `\\w` marker, so
/// it lands on two source pieces, and one that does not.
pub const SAMPLE_NEEDLE: &str = "the light";
pub const SAMPLE_OPTIONS: &[(&str, &str)] = &[
    ("needle", "\"the light\""),
    ("caseSensitive", "false"),
    ("wholeWord", "false"),
    ("limit", "0"),
];
