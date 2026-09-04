//! The wire's byte offsets and magic numbers. Envelope layout: codec/README.md.

pub const MAGIC: u32 = 0x5355_4f53; // ASCII "SOUS", little endian.
pub const FORMAT_VERSION: u32 = 1;
pub const FLAG_UTF16: u32 = 1 << 0;
pub const HEADER_BYTES: usize = 48;
pub const DIRECTORY_ENTRY_BYTES: usize = 20;
pub const HEADER_MAGIC_OFFSET: usize = 0;
pub const HEADER_VERSION_OFFSET: usize = 4;
pub const HEADER_FLAGS_OFFSET: usize = 8;
pub const HEADER_BOOK_COUNT_OFFSET: usize = 12;
pub const HEADER_RECORD_LEN_OFFSET: usize = 16;
pub const HEADER_TOTAL_FINDINGS_OFFSET: usize = 20;
pub const HEADER_PATTERN_COUNT_OFFSET: usize = 24;
pub const HEADER_PATTERN_OFFSET_OFFSET: usize = 28;
pub const HEADER_SNAPSHOT_ID_OFFSET: usize = 32;
pub const DIRECTORY_KEY_OFFSET: usize = 0;
pub const DIRECTORY_KEY_TERMINATOR_OFFSET: usize = 3;
pub const DIRECTORY_LENGTH_OFFSET: usize = 4;
pub const DIRECTORY_SECTION_OFFSET: usize = 8;
pub const DIRECTORY_FINDING_COUNT_OFFSET: usize = 12;
pub const DIRECTORY_ID_OFFSET: usize = 16;
/// Bytes of `u16` little-endian length in front of each id's UTF-8 bytes.
pub const ID_PREFIX_BYTES: usize = 2;
/// The id string table is padded to this boundary so the pattern table and
/// the record sections stay 4-byte aligned for a typed-array view.
pub const SECTION_ALIGNMENT: usize = 4;
/// One pattern-table row. A multiple of [`SECTION_ALIGNMENT`], so the record
/// sections behind it stay aligned however many patterns fired.
pub const PATTERN_ROW_LEN: usize = 24;
pub const PATTERN_GLYPH_OFFSET: usize = 0;
pub const PATTERN_NEIGHBOR_OFFSET: usize = 4;
pub const PATTERN_CHANNEL_OFFSET: usize = 8;
pub const PATTERN_KEY_OFFSET: usize = 9;
pub const PATTERN_BAND_OFFSET: usize = 10;
pub const PATTERN_FLAGS_OFFSET: usize = 11;
pub const PATTERN_NUMERATOR_OFFSET: usize = 12;
pub const PATTERN_DENOMINATOR_OFFSET: usize = 16;
pub const PATTERN_SHARE_OFFSET: usize = 20;
/// Books whose counts hold part of the numerator; books-possible is the
/// header's `book_count`.
pub const PATTERN_BOOKS_OFFSET: usize = 22;
pub const PATTERN_RESERVED_OFFSET: usize = 23;
/// A band byte naming no staircase step, which is what `Rarity` carries.
pub const PATTERN_BAND_NONE: u8 = 0xFF;
/// The pooled digit lane's glyph value, [`ScalarKey::DIGITS`] on the wire.
pub const PATTERN_DIGIT_GLYPH: u32 = u32::MAX;
