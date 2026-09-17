//! The generator: [`schema`](super::schema) in, both ends of the wire out.
//!
//! ```text
//! schema::RECORDS  ->  generated_rs()  ->  galley/src/mask/generated.rs
//! schema's header  ->  reader_ts()     ->  galley/mask-reader.ts
//! ```
//!
//! The rows are `ticket`'s work; the envelope — seven header words and one
//! counted block — is this format's own, and its constants reach the reader
//! through the substitutions below rather than being typed twice.

use ticket::emit::{fill, row_classes_ts, writers_rs};

use super::schema;

const RS_TEMPLATE: &str = include_str!("generated.rs.tmpl");
const TS_TEMPLATE: &str = include_str!("../../mask-reader.ts.tmpl");

/// The record's writer, into the checked-in Rust module.
pub fn generated_rs() -> String {
    RS_TEMPLATE.replace("@@WRITERS@@", writers_rs(schema::RECORDS).trim_end())
}

/// The envelope's constants and the row's accessor class, into the checked-in
/// TypeScript reader.
pub fn reader_ts() -> String {
    let word = |value: usize| value.to_string();
    fill(
        TS_TEMPLATE,
        &[
            ("@@MAGIC@@", format!("0x{:08x}", schema::MAGIC)),
            ("@@FORMAT_VERSION@@", schema::FORMAT_VERSION.to_string()),
            ("@@FLAG_UTF16@@", schema::FLAG_UTF16.to_string()),
            (
                "@@RECIPE_VERSE_TEXT@@",
                schema::RECIPE_VERSE_TEXT.to_string(),
            ),
            ("@@RECIPE_STRUCTURE@@", schema::RECIPE_STRUCTURE.to_string()),
            ("@@HEADER_BYTES@@", word(schema::HEADER_BYTES)),
            ("@@HEADER_MAGIC_OFFSET@@", word(schema::HEADER_MAGIC_OFFSET)),
            (
                "@@HEADER_VERSION_OFFSET@@",
                word(schema::HEADER_VERSION_OFFSET),
            ),
            ("@@HEADER_FLAGS_OFFSET@@", word(schema::HEADER_FLAGS_OFFSET)),
            (
                "@@HEADER_RECIPE_OFFSET@@",
                word(schema::HEADER_RECIPE_OFFSET),
            ),
            (
                "@@HEADER_RANGE_COUNT_OFFSET@@",
                word(schema::HEADER_RANGE_COUNT_OFFSET),
            ),
            (
                "@@HEADER_SOURCE_LEN_OFFSET@@",
                word(schema::HEADER_SOURCE_LEN_OFFSET),
            ),
            (
                "@@HEADER_PROJECTED_LEN_OFFSET@@",
                word(schema::HEADER_PROJECTED_LEN_OFFSET),
            ),
            ("@@RANGE_STRIDE@@", word(schema::RANGE.stride())),
            (
                "@@ROWS@@",
                row_classes_ts(schema::RECORDS).trim_end().to_string(),
            ),
        ],
    )
}
