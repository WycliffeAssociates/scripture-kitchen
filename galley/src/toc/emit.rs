//! The generator: [`schema`](super::schema) in, both ends of the wire out.
//!
//! ```text
//! schema::RECORDS   ->  generated_rs()  ->  galley/src/toc/generated.rs
//! schema's envelope ->  reader_ts()     ->  galley/toc-reader.ts
//! ```
//!
//! Both artifacts are CHECKED IN and both have a staleness test, so a schema
//! edit that was not regenerated fails the build rather than shipping a reader
//! that disagrees with its writer.
//!
//! The rows are `ticket`'s work — one answer in the workspace to how a field
//! reaches the buffer, and where a reader looks for it. What is galley's own
//! is the ENVELOPE: a census frames books where a dish frames sections.

use ticket::emit::{fill, row_classes_ts, writers_rs};

use super::schema;

const RS_TEMPLATE: &str = include_str!("generated.rs.tmpl");
const TS_TEMPLATE: &str = include_str!("../../toc-reader.ts.tmpl");

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
            ("@@FORMAT_VERSION@@", schema::FORMAT_VERSION.to_string()),
            ("@@FLAG_UTF16@@", schema::FLAG_UTF16.to_string()),
            ("@@HEADER_BYTES@@", word(schema::HEADER_BYTES)),
            ("@@HEADER_MAGIC_OFFSET@@", word(schema::HEADER_MAGIC_OFFSET)),
            (
                "@@HEADER_VERSION_OFFSET@@",
                word(schema::HEADER_VERSION_OFFSET),
            ),
            ("@@HEADER_FLAGS_OFFSET@@", word(schema::HEADER_FLAGS_OFFSET)),
            (
                "@@HEADER_BOOK_COUNT_OFFSET@@",
                word(schema::HEADER_BOOK_COUNT_OFFSET),
            ),
            (
                "@@HEADER_CHAPTER_STRIDE_OFFSET@@",
                word(schema::HEADER_CHAPTER_STRIDE_OFFSET),
            ),
            (
                "@@HEADER_VERSE_STRIDE_OFFSET@@",
                word(schema::HEADER_VERSE_STRIDE_OFFSET),
            ),
            (
                "@@HEADER_MEMBER_STRIDE_OFFSET@@",
                word(schema::HEADER_MEMBER_STRIDE_OFFSET),
            ),
            (
                "@@HEADER_DIRECTORY_AT_OFFSET@@",
                word(schema::HEADER_DIRECTORY_AT_OFFSET),
            ),
            (
                "@@DIRECTORY_ENTRY_BYTES@@",
                word(schema::DIRECTORY_ENTRY_BYTES),
            ),
            (
                "@@DIRECTORY_CODE_OFFSET@@",
                word(schema::DIRECTORY_CODE_OFFSET),
            ),
            (
                "@@DIRECTORY_CHAPTERS_AT_OFFSET@@",
                word(schema::DIRECTORY_CHAPTERS_AT_OFFSET),
            ),
            (
                "@@DIRECTORY_CHAPTER_ROWS_OFFSET@@",
                word(schema::DIRECTORY_CHAPTER_ROWS_OFFSET),
            ),
            (
                "@@DIRECTORY_VERSES_AT_OFFSET@@",
                word(schema::DIRECTORY_VERSES_AT_OFFSET),
            ),
            (
                "@@DIRECTORY_VERSE_ROWS_OFFSET@@",
                word(schema::DIRECTORY_VERSE_ROWS_OFFSET),
            ),
            (
                "@@DIRECTORY_ID_AT_OFFSET@@",
                word(schema::DIRECTORY_ID_AT_OFFSET),
            ),
            (
                "@@DIRECTORY_ID_LEN_OFFSET@@",
                word(schema::DIRECTORY_ID_LEN_OFFSET),
            ),
            (
                "@@DIRECTORY_MEMBERS_AT_OFFSET@@",
                word(schema::DIRECTORY_MEMBERS_AT_OFFSET),
            ),
            (
                "@@DIRECTORY_MEMBER_ROWS_OFFSET@@",
                word(schema::DIRECTORY_MEMBER_ROWS_OFFSET),
            ),
            ("@@CHAPTER_STRIDE@@", word(schema::CHAPTER.stride())),
            ("@@VERSE_STRIDE@@", word(schema::VERSE.stride())),
            ("@@MEMBER_STRIDE@@", word(schema::MEMBER.stride())),
            (
                "@@ROWS@@",
                row_classes_ts(schema::RECORDS).trim_end().to_string(),
            ),
        ],
    )
}
