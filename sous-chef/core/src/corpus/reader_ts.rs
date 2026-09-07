//! Renders `reader.ts` from the Rust-owned wire schema.
//!
//! ```text
//! generated_reader_ts() → reader.ts.tmpl with every @@PLACEHOLDER@@ filled
//!   from the layout consts, so the two readers cannot drift.
//! ```

use super::*;
use crate::codec::{
    HygieneClass, PresenceKind, RECORD_BOOK_INDEX_OFFSET, RECORD_BOOK_SCOPE_OFFSET,
    RECORD_CODE_OFFSET, RECORD_FLAGS_OFFSET, RECORD_FROM_OFFSET, RECORD_LEN,
    RECORD_PROJECT_SCOPE_OFFSET, RECORD_TO_OFFSET, Reasons,
};
use crate::judge::{Channel, Staircase};
use crate::substrate::{OuterClass, RUN_BUCKETS};
use crate::unicode::Pool;
use crate::words::{Form, LETTER_RUN_MAX, LETTER_RUN_MIN};

/// Render the checked-in TypeScript reader from the Rust-owned wire schema.
pub fn generated_reader_ts() -> String {
    let substitutions: [(&str, String); 55] = [
        ("@@MAGIC@@", format!("0x{MAGIC:08x}")),
        ("@@FORMAT_VERSION@@", FORMAT_VERSION.to_string()),
        ("@@FLAG_UTF16@@", FLAG_UTF16.to_string()),
        (
            "@@HYGIENE_CLASSES@@",
            HygieneClass::ALL
                .iter()
                .map(|class| format!("\"{}\"", class.name()))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        (
            "@@PRESENCE_KINDS@@",
            PresenceKind::ALL
                .iter()
                .map(|kind| format!("\"{}\"", kind.name()))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        ("@@HEADER_BYTES@@", HEADER_BYTES.to_string()),
        (
            "@@DIRECTORY_ENTRY_BYTES@@",
            DIRECTORY_ENTRY_BYTES.to_string(),
        ),
        ("@@RECORD_LEN@@", RECORD_LEN.to_string()),
        ("@@HEADER_MAGIC_OFFSET@@", HEADER_MAGIC_OFFSET.to_string()),
        (
            "@@HEADER_VERSION_OFFSET@@",
            HEADER_VERSION_OFFSET.to_string(),
        ),
        ("@@HEADER_FLAGS_OFFSET@@", HEADER_FLAGS_OFFSET.to_string()),
        (
            "@@HEADER_BOOK_COUNT_OFFSET@@",
            HEADER_BOOK_COUNT_OFFSET.to_string(),
        ),
        (
            "@@HEADER_RECORD_LEN_OFFSET@@",
            HEADER_RECORD_LEN_OFFSET.to_string(),
        ),
        (
            "@@HEADER_TOTAL_FINDINGS_OFFSET@@",
            HEADER_TOTAL_FINDINGS_OFFSET.to_string(),
        ),
        (
            "@@HEADER_PATTERN_COUNT_OFFSET@@",
            HEADER_PATTERN_COUNT_OFFSET.to_string(),
        ),
        (
            "@@HEADER_PATTERN_OFFSET_OFFSET@@",
            HEADER_PATTERN_OFFSET_OFFSET.to_string(),
        ),
        (
            "@@HEADER_SNAPSHOT_ID_OFFSET@@",
            HEADER_SNAPSHOT_ID_OFFSET.to_string(),
        ),
        ("@@PATTERN_ROW_LEN@@", PATTERN_ROW_LEN.to_string()),
        ("@@PATTERN_GLYPH_OFFSET@@", PATTERN_GLYPH_OFFSET.to_string()),
        (
            "@@PATTERN_NEIGHBOR_OFFSET@@",
            PATTERN_NEIGHBOR_OFFSET.to_string(),
        ),
        (
            "@@PATTERN_CHANNEL_OFFSET@@",
            PATTERN_CHANNEL_OFFSET.to_string(),
        ),
        ("@@PATTERN_KEY_OFFSET@@", PATTERN_KEY_OFFSET.to_string()),
        ("@@PATTERN_BAND_OFFSET@@", PATTERN_BAND_OFFSET.to_string()),
        ("@@PATTERN_FLAGS_OFFSET@@", PATTERN_FLAGS_OFFSET.to_string()),
        (
            "@@PATTERN_NUMERATOR_OFFSET@@",
            PATTERN_NUMERATOR_OFFSET.to_string(),
        ),
        (
            "@@PATTERN_DENOMINATOR_OFFSET@@",
            PATTERN_DENOMINATOR_OFFSET.to_string(),
        ),
        ("@@PATTERN_SHARE_OFFSET@@", PATTERN_SHARE_OFFSET.to_string()),
        ("@@PATTERN_BOOKS_OFFSET@@", PATTERN_BOOKS_OFFSET.to_string()),
        (
            "@@PATTERN_RESERVED_OFFSET@@",
            PATTERN_RESERVED_OFFSET.to_string(),
        ),
        ("@@PATTERN_BAND_NONE@@", PATTERN_BAND_NONE.to_string()),
        ("@@PATTERN_DIGIT_GLYPH@@", PATTERN_DIGIT_GLYPH.to_string()),
        ("@@BAND_STEPS@@", Staircase::STEPS.to_string()),
        ("@@LETTER_RUN_MIN@@", LETTER_RUN_MIN.to_string()),
        ("@@LETTER_RUN_MAX@@", LETTER_RUN_MAX.to_string()),
        ("@@RUN_BUCKETS@@", RUN_BUCKETS.to_string()),
        (
            "@@CHANNELS@@",
            Channel::ALL
                .iter()
                .map(|channel| format!("\"{}\"", channel.name()))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        (
            "@@POOLS@@",
            Pool::ALL
                .iter()
                .map(|pool| format!("\"{}\"", pool.name()))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        (
            "@@OUTER_CLASSES@@",
            OuterClass::ALL
                .iter()
                .map(|class| format!("\"{}\"", class.name()))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        (
            "@@CASING_FORMS@@",
            Form::ALL
                .iter()
                .map(|form| format!("\"{}\"", form.name()))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        (
            "@@CONVENTION_REASONS@@",
            Reasons::NAMES
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        ("@@DIRECTORY_KEY_OFFSET@@", DIRECTORY_KEY_OFFSET.to_string()),
        (
            "@@DIRECTORY_KEY_TERMINATOR_OFFSET@@",
            DIRECTORY_KEY_TERMINATOR_OFFSET.to_string(),
        ),
        (
            "@@DIRECTORY_LENGTH_OFFSET@@",
            DIRECTORY_LENGTH_OFFSET.to_string(),
        ),
        (
            "@@DIRECTORY_SECTION_OFFSET@@",
            DIRECTORY_SECTION_OFFSET.to_string(),
        ),
        (
            "@@DIRECTORY_FINDING_COUNT_OFFSET@@",
            DIRECTORY_FINDING_COUNT_OFFSET.to_string(),
        ),
        ("@@DIRECTORY_ID_OFFSET@@", DIRECTORY_ID_OFFSET.to_string()),
        ("@@ID_PREFIX_BYTES@@", ID_PREFIX_BYTES.to_string()),
        ("@@SECTION_ALIGNMENT@@", SECTION_ALIGNMENT.to_string()),
        ("@@RECORD_FROM_OFFSET@@", RECORD_FROM_OFFSET.to_string()),
        ("@@RECORD_TO_OFFSET@@", RECORD_TO_OFFSET.to_string()),
        (
            "@@RECORD_BOOK_INDEX_OFFSET@@",
            RECORD_BOOK_INDEX_OFFSET.to_string(),
        ),
        ("@@RECORD_CODE_OFFSET@@", RECORD_CODE_OFFSET.to_string()),
        ("@@RECORD_FLAGS_OFFSET@@", RECORD_FLAGS_OFFSET.to_string()),
        (
            "@@RECORD_BOOK_SCOPE_OFFSET@@",
            RECORD_BOOK_SCOPE_OFFSET.to_string(),
        ),
        (
            "@@RECORD_PROJECT_SCOPE_OFFSET@@",
            RECORD_PROJECT_SCOPE_OFFSET.to_string(),
        ),
    ];

    let mut text = include_str!("../../../reader.ts.tmpl").to_string();
    for (placeholder, value) in substitutions {
        text = text.replace(placeholder, &value);
    }
    text
}
