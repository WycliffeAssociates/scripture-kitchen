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

use ticket::schema::{Field, Record};

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
