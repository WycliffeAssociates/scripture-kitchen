//! The census's declaration. Both ends are generated from this file: the Rust
//! writer (`toc/generated.rs`) and the TypeScript reader
//! (`galley/toc-reader.ts`). Neither is typed by hand, so they cannot disagree.
//!
//! ```text
//! schema::RECORDS  ->  emit::generated_rs()  ->  galley/src/toc/generated.rs
//! schema::ENVELOPE ->  emit::reader_ts()     ->  galley/toc-reader.ts
//! ```
//!
//! The vocabulary and the emitters are `ticket`'s: a second vocabulary would be
//! a second thing to keep in step, which is the failure this design exists to
//! make impossible. The ENVELOPE below — header and directory — is this
//! format's alone, because the dish frames sections and a census frames books.

use ticket::schema::{Field, Record};

use Space::{Offset, Plain};
use Width::{U16, U32};
pub use ticket::schema::{Space, Width};

/// `TOCS`, little-endian — read out of the buffer's first four bytes in order.
pub const MAGIC: u32 = u32::from_le_bytes(*b"TOCS");

/// Bumped when this envelope's or either record's shape changes. A reader that
/// does not know this number stops rather than misreading a field.
pub const FORMAT_VERSION: u32 = 1;

/// Header `flags`: every offset in the buffer is UTF-16 rather than a byte.
pub const FLAG_UTF16: u32 = 1 << 0;

/// The header, then the directory, then each book's rows, then the ids.
pub const HEADER_BYTES: usize = 28;
pub const HEADER_MAGIC_OFFSET: usize = 0;
pub const HEADER_VERSION_OFFSET: usize = 4;
pub const HEADER_FLAGS_OFFSET: usize = 8;
pub const HEADER_BOOK_COUNT_OFFSET: usize = 12;
pub const HEADER_CHAPTER_STRIDE_OFFSET: usize = 16;
pub const HEADER_VERSE_STRIDE_OFFSET: usize = 20;
/// Where the directory starts. Written rather than assumed, so a later header
/// may grow without moving the one read every consumer begins with.
pub const HEADER_DIRECTORY_AT_OFFSET: usize = 24;

/// One book's entry: where its two row blocks are, and how many rows each has.
pub const DIRECTORY_ENTRY_BYTES: usize = 28;
/// The book code's three bytes, then a NUL — the same shape the Sous
/// directory uses, so a reader reads a code the one way it already knows.
pub const DIRECTORY_CODE_OFFSET: usize = 0;
pub const DIRECTORY_CODE_TERMINATOR_OFFSET: usize = 3;
pub const DIRECTORY_CHAPTERS_AT_OFFSET: usize = 4;
pub const DIRECTORY_CHAPTER_ROWS_OFFSET: usize = 8;
pub const DIRECTORY_VERSES_AT_OFFSET: usize = 12;
pub const DIRECTORY_VERSE_ROWS_OFFSET: usize = 16;
pub const DIRECTORY_ID_AT_OFFSET: usize = 20;
pub const DIRECTORY_ID_LEN_OFFSET: usize = 24;

/// Every block starts on a 4-byte boundary, so a reader may take a typed-array
/// view over one book's rows without copying them out first.
pub const SECTION_ALIGNMENT: usize = 4;

/// What the retained `Toc` knows about one chapter, and nothing else.
///
/// No `token` and no `designator`: those index the TOKEN STREAM, which is the
/// rebuildable tier and not resident. A consumer that needs a chapter's raw
/// label (`\c 12b`, which `number` cannot carry) needs the tokens, and that
/// means a parse — see `galley/src/toc.md`.
pub const CHAPTER: Record = Record {
    name: "Chapter",
    plural: "chapters",
    rust_ty: "Chapter",
    binding: "c",
    context: &[],
    doc: "One chapter's extent, its number, and what its verses count to.",
    tail: None,
    fields: &[
        Field {
            name: "start",
            width: U32,
            space: Offset,
            rust: "c.start",
            doc: "First byte of the `\\c` marker; 0 on the front-matter row.",
        },
        Field {
            name: "end",
            width: U32,
            space: Offset,
            rust: "c.end",
            doc: "One past the chapter's last byte. Rows TILE the book.",
        },
        Field {
            name: "number",
            width: U16,
            space: Plain,
            rust: "c.number",
            doc: "The number read, or 0 — absent, malformed, or row 0's front matter.",
        },
        Field {
            name: "anchors",
            width: U16,
            space: Plain,
            rust: "c.anchors",
            doc: "How many `\\v` markers sit inside this chapter's span, saturating.",
        },
        Field {
            name: "lastVerse",
            width: U16,
            space: Plain,
            rust: "c.last_verse",
            doc: "The highest verse number those anchors name; 0 when none does. \
                  A bridge `\\v 5-7` is one anchor reaching 7 — so this is the \
                  count that keys by NUMBER where `anchors` counts MARKERS.",
        },
    ],
};

/// One `\v` anchor, in source order.
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
            doc: "First byte of the `\\v` marker.",
        },
        Field {
            name: "chapter",
            width: U16,
            space: Plain,
            rust: "v.chapter",
            doc: "The number of the chapter row whose span CONTAINS this anchor \
                  — position, never the designator.",
        },
        Field {
            name: "first",
            width: U16,
            space: Plain,
            rust: "v.first",
            doc: "Lowest verse named; 0 when the designator is absent or malformed.",
        },
        Field {
            name: "last",
            width: U16,
            space: Plain,
            rust: "v.last",
            doc: "Highest named — equal to `first` unless this is a bridge.",
        },
    ],
};

/// Every row record, in the order a book's blocks are written.
pub const RECORDS: &[&Record] = &[&CHAPTER, &VERSE];
