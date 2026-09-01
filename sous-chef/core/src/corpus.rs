//! Complete-corpus findings publication.
//!
//! The envelope is deliberately separate from the resident analysis model: it
//! is a fresh, publication-ready buffer whose rows use the declared coordinate
//! space. The producer owns any source/UTF-16 rebasing before calling the
//! writer.

use core::fmt;

use rustc_hash::FxHashSet;

use crate::codec::{
    CodecError, HygieneClass, PackedFinding, RECORD_BOOK_INDEX_OFFSET, RECORD_BOOK_SCOPE_OFFSET,
    RECORD_CODE_OFFSET, RECORD_FLAGS_OFFSET, RECORD_FROM_OFFSET, RECORD_LEN,
    RECORD_PROJECT_SCOPE_OFFSET, RECORD_TO_OFFSET,
};
use crate::{BookIndex, BookKey};

pub const MAGIC: u32 = 0x5355_4f53; // ASCII "SOUS", little endian.
pub const FORMAT_VERSION: u32 = 1;
pub const FLAG_UTF16: u32 = 1 << 0;
pub const HEADER_BYTES: usize = 40;
pub const DIRECTORY_ENTRY_BYTES: usize = 16;
pub const HEADER_MAGIC_OFFSET: usize = 0;
pub const HEADER_VERSION_OFFSET: usize = 4;
pub const HEADER_FLAGS_OFFSET: usize = 8;
pub const HEADER_BOOK_COUNT_OFFSET: usize = 12;
pub const HEADER_RECORD_LEN_OFFSET: usize = 16;
pub const HEADER_TOTAL_FINDINGS_OFFSET: usize = 20;
pub const HEADER_SNAPSHOT_ID_OFFSET: usize = 24;
pub const DIRECTORY_KEY_OFFSET: usize = 0;
pub const DIRECTORY_KEY_TERMINATOR_OFFSET: usize = 3;
pub const DIRECTORY_LENGTH_OFFSET: usize = 4;
pub const DIRECTORY_SECTION_OFFSET: usize = 8;
pub const DIRECTORY_FINDING_COUNT_OFFSET: usize = 12;

/// The opaque identity of the immutable snapshot a publication belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotId([u8; 16]);

impl SnapshotId {
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

impl From<[u8; 16]> for SnapshotId {
    fn from(bytes: [u8; 16]) -> Self {
        Self::new(bytes)
    }
}

/// Coordinate space used by every span in a corpus publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateSpace {
    Utf8,
    Utf16,
}

impl CoordinateSpace {
    const fn flags(self) -> u32 {
        match self {
            Self::Utf8 => 0,
            Self::Utf16 => FLAG_UTF16,
        }
    }
}

impl TryFrom<u32> for CoordinateSpace {
    type Error = CorpusWireError;

    fn try_from(flags: u32) -> Result<Self, Self::Error> {
        match flags {
            0 => Ok(Self::Utf8),
            FLAG_UTF16 => Ok(Self::Utf16),
            other => Err(CorpusWireError::UnknownFlags { actual: other }),
        }
    }
}

/// One publication-ready book section in caller order.
pub struct PublicationBook<'a> {
    key: BookKey,
    published_len: u32,
    findings: &'a [PackedFinding],
}

impl<'a> PublicationBook<'a> {
    pub const fn new(key: BookKey, published_len: u32, findings: &'a [PackedFinding]) -> Self {
        Self {
            key,
            published_len,
            findings,
        }
    }

    pub const fn key(&self) -> BookKey {
        self.key
    }

    pub const fn published_len(&self) -> u32 {
        self.published_len
    }

    pub const fn findings(&self) -> &'a [PackedFinding] {
        self.findings
    }
}

/// Encode one complete corpus publication.
pub fn encode_to_corpus_buffer(
    snapshot_id: SnapshotId,
    coordinate_space: CoordinateSpace,
    sections: &[PublicationBook<'_>],
) -> Result<Vec<u8>, CorpusWireError> {
    let book_count =
        u32::try_from(sections.len()).map_err(|_| CorpusWireError::BookCountOverflow {
            count: sections.len(),
        })?;
    if sections.len() > usize::from(u16::MAX) + 1 {
        return Err(CorpusWireError::BookCountOverflow {
            count: sections.len(),
        });
    }

    let mut keys = FxHashSet::default();
    keys.reserve(sections.len());
    let mut total_findings = 0u32;
    for (index, section) in sections.iter().enumerate() {
        validate_key(section.key)?;
        if !keys.insert(section.key) {
            return Err(CorpusWireError::DuplicateBookKey { key: section.key });
        }
        total_findings = total_findings
            .checked_add(u32::try_from(section.findings.len()).map_err(|_| {
                CorpusWireError::FindingCountOverflow {
                    book: index,
                    count: section.findings.len(),
                }
            })?)
            .ok_or(CorpusWireError::TotalFindingCountOverflow)?;
        let book_index = BookIndex::new(index).expect("book count was checked");
        for (row, finding) in section.findings.iter().enumerate() {
            finding
                .validate_for_book(book_index, section.published_len())
                .map_err(|error| CorpusWireError::Record {
                    book: index,
                    row,
                    error,
                })?;
        }
    }

    let directory_bytes = sections
        .len()
        .checked_mul(DIRECTORY_ENTRY_BYTES)
        .ok_or(CorpusWireError::SizeOverflow)?;
    let data_start = HEADER_BYTES
        .checked_add(directory_bytes)
        .ok_or(CorpusWireError::SizeOverflow)?;
    let total_bytes = data_start
        .checked_add(
            usize::try_from(total_findings)
                .ok()
                .and_then(|count| count.checked_mul(RECORD_LEN))
                .ok_or(CorpusWireError::SizeOverflow)?,
        )
        .ok_or(CorpusWireError::SizeOverflow)?;
    let total_bytes_u32 = u32::try_from(total_bytes).map_err(|_| CorpusWireError::SizeOverflow)?;

    let mut out = Vec::with_capacity(total_bytes);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&coordinate_space.flags().to_le_bytes());
    out.extend_from_slice(&book_count.to_le_bytes());
    out.extend_from_slice(&(RECORD_LEN as u32).to_le_bytes());
    out.extend_from_slice(&total_findings.to_le_bytes());
    out.extend_from_slice(&snapshot_id.as_bytes());

    let mut section_offset = data_start;
    for (index, section) in sections.iter().enumerate() {
        out.extend_from_slice(&section.key.as_bytes());
        out.push(0);
        out.extend_from_slice(&section.published_len.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(section_offset)
                .map_err(|_| CorpusWireError::SizeOverflow)?
                .to_le_bytes(),
        );
        out.extend_from_slice(
            &u32::try_from(section.findings.len())
                .map_err(|_| CorpusWireError::FindingCountOverflow {
                    book: index,
                    count: section.findings.len(),
                })?
                .to_le_bytes(),
        );
        section_offset = section_offset
            .checked_add(
                section
                    .findings
                    .len()
                    .checked_mul(RECORD_LEN)
                    .ok_or(CorpusWireError::SizeOverflow)?,
            )
            .ok_or(CorpusWireError::SizeOverflow)?;
    }

    for section in sections {
        for finding in section.findings {
            out.extend_from_slice(&finding.encode());
        }
    }
    debug_assert_eq!(out.len(), total_bytes);
    if out.len() != usize::try_from(total_bytes_u32).expect("u32 fits usize on supported hosts") {
        return Err(CorpusWireError::SizeOverflow);
    }
    Ok(out)
}

/// A checked borrowed view over one complete corpus publication.
pub struct CorpusSnapshot<'a> {
    bytes: &'a [u8],
    snapshot_id: SnapshotId,
    coordinate_space: CoordinateSpace,
    books: Vec<BookMeta>,
}

#[derive(Debug, Clone, Copy)]
struct BookMeta {
    key: BookKey,
    published_len: u32,
    offset: usize,
    count: usize,
}

pub struct CorpusBook<'a> {
    bytes: &'a [u8],
    index: BookIndex,
    key: BookKey,
    published_len: u32,
    offset: usize,
    count: usize,
}

impl<'a> CorpusSnapshot<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, CorpusWireError> {
        if bytes.len() < HEADER_BYTES {
            return Err(CorpusWireError::InvalidLength {
                actual: bytes.len(),
            });
        }
        let magic = read_u32(bytes, 0);
        if magic != MAGIC {
            return Err(CorpusWireError::InvalidMagic { actual: magic });
        }
        let version = read_u32(bytes, 4);
        if version != FORMAT_VERSION {
            return Err(CorpusWireError::UnsupportedVersion { actual: version });
        }
        let coordinate_space = CoordinateSpace::try_from(read_u32(bytes, 8))?;
        let book_count_raw = read_u32(bytes, 12);
        let book_count = usize::try_from(book_count_raw)
            .map_err(|_| CorpusWireError::BookCountOverflow { count: usize::MAX })?;
        if book_count > usize::from(u16::MAX) + 1 {
            return Err(CorpusWireError::BookCountOverflow { count: book_count });
        }
        let record_len = read_u32(bytes, 16);
        if record_len != RECORD_LEN as u32 {
            return Err(CorpusWireError::InvalidRecordLength { actual: record_len });
        }
        let total_findings = read_u32(bytes, 20);
        let snapshot_id = SnapshotId::new(bytes[24..40].try_into().expect("header checked"));
        let directory_bytes = book_count
            .checked_mul(DIRECTORY_ENTRY_BYTES)
            .ok_or(CorpusWireError::SizeOverflow)?;
        let data_start = HEADER_BYTES
            .checked_add(directory_bytes)
            .ok_or(CorpusWireError::SizeOverflow)?;
        if data_start > bytes.len() {
            return Err(CorpusWireError::InvalidLength {
                actual: bytes.len(),
            });
        }

        let mut keys = FxHashSet::default();
        keys.reserve(book_count);
        let mut books = Vec::with_capacity(book_count);
        let mut cursor = data_start;
        let mut total_seen = 0u32;
        for index in 0..book_count {
            let at = HEADER_BYTES + index * DIRECTORY_ENTRY_BYTES;
            let key_bytes: [u8; 3] = bytes[at..at + 3].try_into().expect("directory checked");
            if bytes[at + 3] != 0 {
                return Err(CorpusWireError::InvalidBookKey { bytes: key_bytes });
            }
            let key = BookKey::new(key_bytes);
            validate_key(key)?;
            if !keys.insert(key) {
                return Err(CorpusWireError::DuplicateBookKey { key });
            }
            let published_len = read_u32(bytes, at + 4);
            let offset = usize::try_from(read_u32(bytes, at + 8))
                .map_err(|_| CorpusWireError::SizeOverflow)?;
            let count = usize::try_from(read_u32(bytes, at + 12))
                .map_err(|_| CorpusWireError::SizeOverflow)?;
            if offset != cursor {
                return Err(CorpusWireError::SectionOutOfOrder {
                    book: index,
                    expected: cursor,
                    actual: offset,
                });
            }
            let section_bytes = count
                .checked_mul(RECORD_LEN)
                .ok_or(CorpusWireError::SizeOverflow)?;
            let end = offset
                .checked_add(section_bytes)
                .ok_or(CorpusWireError::SizeOverflow)?;
            if end > bytes.len() {
                return Err(CorpusWireError::SectionOutOfBounds { book: index });
            }
            let book_index = BookIndex::new(index).expect("book count was checked");
            for row in 0..count {
                let row_at = offset + row * RECORD_LEN;
                let finding = PackedFinding::decode_wire(&bytes[row_at..row_at + RECORD_LEN])
                    .map_err(|error| CorpusWireError::Record {
                        book: index,
                        row,
                        error,
                    })?;
                finding
                    .validate_for_book(book_index, published_len)
                    .map_err(|error| CorpusWireError::Record {
                        book: index,
                        row,
                        error,
                    })?;
            }
            total_seen = total_seen
                .checked_add(u32::try_from(count).map_err(|_| CorpusWireError::SizeOverflow)?)
                .ok_or(CorpusWireError::TotalFindingCountOverflow)?;
            books.push(BookMeta {
                key,
                published_len,
                offset,
                count,
            });
            cursor = end;
        }
        if cursor != bytes.len() {
            return Err(CorpusWireError::TrailingBytes {
                expected: cursor,
                actual: bytes.len(),
            });
        }
        if total_seen != total_findings {
            return Err(CorpusWireError::TotalFindingCountMismatch {
                expected: total_findings,
                actual: total_seen,
            });
        }
        Ok(Self {
            bytes,
            snapshot_id,
            coordinate_space,
            books,
        })
    }

    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    pub const fn coordinate_space(&self) -> CoordinateSpace {
        self.coordinate_space
    }

    pub fn len(&self) -> usize {
        self.books.len()
    }

    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    pub fn book(&self, index: BookIndex) -> Option<CorpusBook<'a>> {
        self.books.get(index.get() as usize).map(|meta| CorpusBook {
            bytes: self.bytes,
            index,
            key: meta.key,
            published_len: meta.published_len,
            offset: meta.offset,
            count: meta.count,
        })
    }

    pub fn book_by_key(&self, key: BookKey) -> Option<CorpusBook<'a>> {
        self.books
            .iter()
            .position(|meta| meta.key == key)
            .map(|index| {
                self.book(BookIndex::new(index).expect("book count was checked"))
                    .expect("position came from the same table")
            })
    }
}

impl<'a> CorpusBook<'a> {
    pub const fn index(&self) -> BookIndex {
        self.index
    }

    pub const fn key(&self) -> BookKey {
        self.key
    }

    pub const fn published_len(&self) -> u32 {
        self.published_len
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn at(&self, row: usize) -> Result<PackedFinding, CorpusWireError> {
        if row >= self.count {
            return Err(CorpusWireError::RowOutOfBounds {
                row,
                count: self.count,
            });
        }
        let offset = self
            .offset
            .checked_add(row * RECORD_LEN)
            .ok_or(CorpusWireError::SizeOverflow)?;
        let finding = PackedFinding::decode_wire(&self.bytes[offset..offset + RECORD_LEN])
            .map_err(|error| CorpusWireError::Record {
                book: self.index.get() as usize,
                row,
                error,
            })?;
        finding
            .validate_for_book(self.index, self.published_len)
            .map_err(|error| CorpusWireError::Record {
                book: self.index.get() as usize,
                row,
                error,
            })?;
        Ok(finding)
    }
}

fn validate_key(key: BookKey) -> Result<(), CorpusWireError> {
    let bytes = key.as_bytes();
    if bytes
        .iter()
        .any(|byte| !byte.is_ascii() || *byte < 0x20 || *byte > 0x7e)
    {
        return Err(CorpusWireError::InvalidBookKey { bytes });
    }
    Ok(())
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("validated field"))
}

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
    DuplicateBookKey {
        key: BookKey,
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
            Self::DuplicateBookKey { key } => write!(f, "duplicate corpus book key {key}"),
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
        }
    }
}

impl std::error::Error for CorpusWireError {}

/// Emit the checked-in TypeScript reader from the Rust-owned wire schema.
pub fn generated_reader_ts() -> String {
    include_str!("../../reader.ts.tmpl")
        .replace("@@MAGIC@@", &format!("0x{MAGIC:08x}"))
        .replace("@@FORMAT_VERSION@@", &FORMAT_VERSION.to_string())
        .replace("@@FLAG_UTF16@@", &FLAG_UTF16.to_string())
        .replace(
            "@@HYGIENE_CLASSES@@",
            &HygieneClass::ALL
                .iter()
                .map(|class| format!("\"{}\"", class.name()))
                .collect::<Vec<_>>()
                .join(", "),
        )
        .replace("@@HEADER_BYTES@@", &HEADER_BYTES.to_string())
        .replace(
            "@@DIRECTORY_ENTRY_BYTES@@",
            &DIRECTORY_ENTRY_BYTES.to_string(),
        )
        .replace("@@RECORD_LEN@@", &RECORD_LEN.to_string())
        .replace("@@HEADER_MAGIC_OFFSET@@", &HEADER_MAGIC_OFFSET.to_string())
        .replace(
            "@@HEADER_VERSION_OFFSET@@",
            &HEADER_VERSION_OFFSET.to_string(),
        )
        .replace("@@HEADER_FLAGS_OFFSET@@", &HEADER_FLAGS_OFFSET.to_string())
        .replace(
            "@@HEADER_BOOK_COUNT_OFFSET@@",
            &HEADER_BOOK_COUNT_OFFSET.to_string(),
        )
        .replace(
            "@@HEADER_RECORD_LEN_OFFSET@@",
            &HEADER_RECORD_LEN_OFFSET.to_string(),
        )
        .replace(
            "@@HEADER_TOTAL_FINDINGS_OFFSET@@",
            &HEADER_TOTAL_FINDINGS_OFFSET.to_string(),
        )
        .replace(
            "@@HEADER_SNAPSHOT_ID_OFFSET@@",
            &HEADER_SNAPSHOT_ID_OFFSET.to_string(),
        )
        .replace(
            "@@DIRECTORY_KEY_OFFSET@@",
            &DIRECTORY_KEY_OFFSET.to_string(),
        )
        .replace(
            "@@DIRECTORY_KEY_TERMINATOR_OFFSET@@",
            &DIRECTORY_KEY_TERMINATOR_OFFSET.to_string(),
        )
        .replace(
            "@@DIRECTORY_LENGTH_OFFSET@@",
            &DIRECTORY_LENGTH_OFFSET.to_string(),
        )
        .replace(
            "@@DIRECTORY_SECTION_OFFSET@@",
            &DIRECTORY_SECTION_OFFSET.to_string(),
        )
        .replace(
            "@@DIRECTORY_FINDING_COUNT_OFFSET@@",
            &DIRECTORY_FINDING_COUNT_OFFSET.to_string(),
        )
        .replace("@@RECORD_FROM_OFFSET@@", &RECORD_FROM_OFFSET.to_string())
        .replace("@@RECORD_TO_OFFSET@@", &RECORD_TO_OFFSET.to_string())
        .replace(
            "@@RECORD_BOOK_INDEX_OFFSET@@",
            &RECORD_BOOK_INDEX_OFFSET.to_string(),
        )
        .replace("@@RECORD_CODE_OFFSET@@", &RECORD_CODE_OFFSET.to_string())
        .replace("@@RECORD_FLAGS_OFFSET@@", &RECORD_FLAGS_OFFSET.to_string())
        .replace(
            "@@RECORD_BOOK_SCOPE_OFFSET@@",
            &RECORD_BOOK_SCOPE_OFFSET.to_string(),
        )
        .replace(
            "@@RECORD_PROJECT_SCOPE_OFFSET@@",
            &RECORD_PROJECT_SCOPE_OFFSET.to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FindingKind, HygieneDigest, ProportionalityDigest, QuantizedDeviation};

    const GENERATED: &str = include_str!("../../reader.ts");

    fn fixture_bytes() -> Vec<u8> {
        include_str!("../../testdata/corpus_v1.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect()
    }

    fn fixture_finding() -> PackedFinding {
        PackedFinding::new(
            0x10,
            0x12,
            BookIndex::new(0).unwrap(),
            FindingKind::LengthProportionality(ProportionalityDigest::new(
                Some(QuantizedDeviation::from_raw(0x0180).unwrap()),
                Some(QuantizedDeviation::from_raw(-0x0180).unwrap()),
                true,
            )),
            &[0x0200],
        )
        .unwrap()
    }

    #[test]
    fn checked_in_reader_is_fresh() {
        assert_eq!(generated_reader_ts(), GENERATED);
    }

    #[test]
    fn writer_matches_shared_golden_buffer_and_reader_view() {
        let finding = fixture_finding();
        let findings = [finding];
        let section = PublicationBook::new(BookKey::new(*b"MRK"), 0x0200, &findings);
        let encoded = encode_to_corpus_buffer(
            SnapshotId::new(core::array::from_fn(|index| index as u8)),
            CoordinateSpace::Utf8,
            &[section],
        )
        .unwrap();
        assert_eq!(encoded, fixture_bytes());

        let snapshot = CorpusSnapshot::open(&encoded).unwrap();
        assert_eq!(
            snapshot.snapshot_id().as_bytes(),
            core::array::from_fn(|i| i as u8)
        );
        assert_eq!(snapshot.coordinate_space(), CoordinateSpace::Utf8);
        let book = snapshot.book_by_key(BookKey::new(*b"MRK")).unwrap();
        assert_eq!(book.index().get(), 0);
        assert_eq!(book.published_len(), 0x0200);
        assert_eq!(book.at(0).unwrap(), finding);
    }

    /// Mixed kinds in one book: a proportionality row between an exact and a
    /// saturated hygiene row.
    #[test]
    fn mixed_kind_golden_buffer_decodes_in_both_readers() {
        let hygiene = |from, to, class, run| {
            PackedFinding::new(
                from,
                to,
                BookIndex::new(0).unwrap(),
                FindingKind::Hygiene(HygieneDigest::new(class, run).unwrap()),
                &[0x0100],
            )
            .unwrap()
        };
        let findings = [
            hygiene(3, 6, HygieneClass::C0Control, 3),
            PackedFinding::new(
                0x10,
                0x12,
                BookIndex::new(0).unwrap(),
                FindingKind::LengthProportionality(ProportionalityDigest::new(
                    Some(QuantizedDeviation::from_raw(0x0180).unwrap()),
                    None,
                    false,
                )),
                &[0x0100],
            )
            .unwrap(),
            hygiene(0x40, 0xa0, HygieneClass::Delete, 40_000),
        ];
        let section = PublicationBook::new(BookKey::new(*b"MRK"), 0x0100, &findings);
        let encoded = encode_to_corpus_buffer(
            SnapshotId::new(core::array::from_fn(|index| index as u8)),
            CoordinateSpace::Utf8,
            &[section],
        )
        .unwrap();
        let golden: Vec<u8> = include_str!("../../testdata/corpus_v1_hygiene.hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect();
        assert_eq!(encoded, golden);

        let snapshot = CorpusSnapshot::open(&encoded).unwrap();
        let book = snapshot.book_by_key(BookKey::new(*b"MRK")).unwrap();
        assert_eq!(book.at(0).unwrap(), findings[0]);
        assert_eq!(book.at(1).unwrap(), findings[1]);
        let FindingKind::Hygiene(digest) = book.at(2).unwrap().kind() else {
            panic!("hygiene kind")
        };
        assert_eq!(digest.class(), HygieneClass::Delete);
        assert!(digest.saturated());
        assert_eq!(digest.run(), 0x7fff);
    }

    #[test]
    fn empty_corpus_and_empty_books_are_valid() {
        let empty =
            encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf16, &[]).unwrap();
        assert_eq!(empty.len(), HEADER_BYTES);
        assert!(CorpusSnapshot::open(&empty).unwrap().is_empty());

        let books = [
            PublicationBook::new(BookKey::new(*b"GEN"), 0, &[]),
            PublicationBook::new(BookKey::new(*b"MRK"), 0, &[]),
        ];
        let encoded =
            encode_to_corpus_buffer(SnapshotId::new([1; 16]), CoordinateSpace::Utf16, &books)
                .unwrap();
        let snapshot = CorpusSnapshot::open(&encoded).unwrap();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot
                .book_by_key(BookKey::new(*b"MRK"))
                .unwrap()
                .index()
                .get(),
            1
        );
    }

    #[test]
    fn malformed_directory_and_record_fail_closed() {
        let finding = fixture_finding();
        let findings = [finding];
        let section = PublicationBook::new(BookKey::new(*b"MRK"), 0x0200, &findings);
        let encoded =
            encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf8, &[section])
                .unwrap();

        let mut bad_key = encoded.clone();
        bad_key[40] = 0xff;
        assert!(matches!(
            CorpusSnapshot::open(&bad_key),
            Err(CorpusWireError::InvalidBookKey { .. })
        ));

        let mut bad_code = encoded;
        bad_code[56 + 10] = 2;
        assert!(matches!(
            CorpusSnapshot::open(&bad_code),
            Err(CorpusWireError::Record {
                error: CodecError::UnknownRuleCode(2),
                ..
            })
        ));
    }
}
