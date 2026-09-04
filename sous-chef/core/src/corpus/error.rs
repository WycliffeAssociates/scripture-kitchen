//! The corpus wire's error type: what every decode-time check refuses.

use core::fmt;

use crate::codec::{CodecError, RECORD_LEN};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusWireError {
    InvalidLength {
        actual: usize,
    },
    InvalidMagic {
        actual: u32,
    },
    UnsupportedVersion {
        actual: u32,
    },
    UnknownFlags {
        actual: u32,
    },
    InvalidRecordLength {
        actual: u32,
    },
    BookCountOverflow {
        count: usize,
    },
    InvalidBookKey {
        bytes: [u8; 3],
    },
    /// Two books published under one host id, the identity a repeated `\id`
    /// cannot collide with.
    DuplicateBookId {
        book: usize,
    },
    BookIdTooLong {
        book: usize,
    },
    InvalidBookId {
        book: usize,
    },
    BookIdOutOfOrder {
        book: usize,
        expected: usize,
        actual: usize,
    },
    FindingCountOverflow {
        book: usize,
        count: usize,
    },
    TotalFindingCountOverflow,
    TotalFindingCountMismatch {
        expected: u32,
        actual: u32,
    },
    SizeOverflow,
    SectionOutOfOrder {
        book: usize,
        expected: usize,
        actual: usize,
    },
    SectionOutOfBounds {
        book: usize,
    },
    TrailingBytes {
        expected: usize,
        actual: usize,
    },
    Record {
        book: usize,
        row: usize,
        error: CodecError,
    },
    RowOutOfBounds {
        row: usize,
        count: usize,
    },
    PatternCountOverflow {
        count: usize,
    },
    PatternSectionOutOfOrder {
        expected: usize,
        actual: usize,
    },
    /// The pattern row's named field is not a value the encoder can write.
    InvalidPattern {
        row: usize,
        field: &'static str,
    },
    /// A pattern index outside the table: `at` names the record that pointed
    /// to it, or is `None` for a reader asking `pattern(index)` directly.
    PatternIndexPastTable {
        index: usize,
        count: usize,
        at: Option<(usize, usize)>,
    },
}

impl fmt::Display for CorpusWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => write!(f, "corpus buffer has {actual} bytes"),
            Self::InvalidMagic { actual } => write!(f, "unknown corpus magic 0x{actual:08x}"),
            Self::UnsupportedVersion { actual } => write!(f, "unsupported corpus version {actual}"),
            Self::UnknownFlags { actual } => write!(f, "unknown corpus flags 0x{actual:08x}"),
            Self::InvalidRecordLength { actual } => {
                write!(f, "record stride is {actual}, expected {RECORD_LEN}")
            }
            Self::BookCountOverflow { count } => {
                write!(f, "corpus contains {count} books; maximum is 65536")
            }
            Self::InvalidBookKey { bytes } => write!(f, "invalid ASCII book key {bytes:02x?}"),
            Self::DuplicateBookId { book } => {
                write!(f, "book {book} repeats a host id already published")
            }
            Self::BookIdTooLong { book } => write!(f, "book {book} has an id longer than 65535"),
            Self::InvalidBookId { book } => {
                write!(
                    f,
                    "book {book} has no readable UTF-8 id in the string table"
                )
            }
            Self::BookIdOutOfOrder {
                book,
                expected,
                actual,
            } => write!(f, "book {book} id starts at {actual}, expected {expected}"),
            Self::FindingCountOverflow { book, count } => {
                write!(f, "book {book} has too many findings: {count}")
            }
            Self::TotalFindingCountOverflow => f.write_str("total finding count overflows u32"),
            Self::TotalFindingCountMismatch { expected, actual } => write!(
                f,
                "header declares {expected} findings, directory contains {actual}"
            ),
            Self::SizeOverflow => f.write_str("corpus buffer size overflows its wire integer"),
            Self::SectionOutOfOrder {
                book,
                expected,
                actual,
            } => write!(
                f,
                "book {book} section starts at {actual}, expected {expected}"
            ),
            Self::SectionOutOfBounds { book } => {
                write!(f, "book {book} section exceeds the corpus buffer")
            }
            Self::TrailingBytes { expected, actual } => write!(
                f,
                "corpus has trailing bytes: expected {expected}, actual {actual}"
            ),
            Self::Record { book, row, error } => write!(f, "book {book} row {row}: {error}"),
            Self::RowOutOfBounds { row, count } => {
                write!(f, "finding row {row} is outside book length {count}")
            }
            Self::PatternCountOverflow { count } => {
                write!(f, "corpus declares {count} patterns; maximum is 65535")
            }
            Self::PatternSectionOutOfOrder { expected, actual } => {
                write!(f, "pattern table starts at {actual}, expected {expected}")
            }
            Self::InvalidPattern { row, field } => {
                write!(f, "pattern row {row} has an invalid {field}")
            }
            Self::PatternIndexPastTable { index, count, at } => match at {
                Some((book, row)) => write!(
                    f,
                    "book {book} row {row} names pattern {index} in a table of {count}"
                ),
                None => write!(f, "pattern {index} is outside a table of {count}"),
            },
        }
    }
}

impl std::error::Error for CorpusWireError {}
