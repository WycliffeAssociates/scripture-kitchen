//! The generator: [`schema`](super::schema) in, both ends of the wire out.
//!
//! ```text
//! schema::RECORDS   ->  generated_rs()  ->  galley/src/find/wire/generated.rs
//! schema's envelope ->  reader_ts()     ->  galley/find-reader.ts
//! ```
//!
//! The rows are `ticket`'s work; the envelope — four header words, two length
//! arrays, two blobs — is find's own, and its constants reach the reader
//! through the substitutions below rather than being typed twice.

use ticket::emit::{fill, row_classes_ts, writers_rs};

use super::schema;

const RS_TEMPLATE: &str = include_str!("generated.rs.tmpl");
const TS_TEMPLATE: &str = include_str!("../../../find-reader.ts.tmpl");

/// One writer per record, into the checked-in Rust module.
pub fn generated_rs() -> String {
    RS_TEMPLATE.replace("@@WRITERS@@", writers_rs(schema::RECORDS).trim_end())
}

/// The envelope's constants and one accessor class per record, into the
/// checked-in TypeScript reader.
pub fn reader_ts() -> String {
    let word = |value: usize| value.to_string();
    fill(
        TS_TEMPLATE,
        &[
            ("@@MAGIC@@", format!("0x{:08x}", schema::MAGIC)),
            ("@@FORMAT_VERSION@@", schema::VERSION.to_string()),
            ("@@HEADER_BYTES@@", word(schema::HEADER_BYTES)),
            ("@@HEADER_MAGIC_OFFSET@@", word(schema::HEADER_MAGIC_OFFSET)),
            (
                "@@HEADER_VERSION_OFFSET@@",
                word(schema::HEADER_VERSION_OFFSET),
            ),
            (
                "@@HEADER_HIT_COUNT_OFFSET@@",
                word(schema::HEADER_HIT_COUNT_OFFSET),
            ),
            (
                "@@HEADER_BOOK_COUNT_OFFSET@@",
                word(schema::HEADER_BOOK_COUNT_OFFSET),
            ),
            ("@@HIT_HEAD_STRIDE@@", word(schema::HIT.stride())),
            ("@@PIECE_STRIDE@@", word(schema::PIECE.stride())),
            ("@@ID_LEN_STRIDE@@", word(schema::ID_LEN.stride())),
            ("@@PREVIEW_LEN_STRIDE@@", word(schema::PREVIEW_LEN.stride())),
            (
                "@@ROWS@@",
                row_classes_ts(schema::RECORDS).trim_end().to_string(),
            ),
        ],
    )
}
