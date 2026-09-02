//! Complete-corpus findings publication.
//!
//! ```text
//! encode_to_corpus_buffer(id, Utf16, [PublicationBook("MRK", "books/mrk.usfm", 50, [finding])])
//!   → 40-byte header · one 20-byte directory row · "books/mrk.usfm" in the id
//!     string table · one 16-byte record
//! ```
//!
//! A fresh publication-ready buffer, not a serialization of the resident
//! analysis model. The producer owns any source/UTF-16 rebasing before calling
//! the writer. Envelope layout: codec/README.md.

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
pub const DIRECTORY_ENTRY_BYTES: usize = 20;
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
pub const DIRECTORY_ID_OFFSET: usize = 16;
/// Bytes of `u16` little-endian length in front of each id's UTF-8 bytes.
pub const ID_PREFIX_BYTES: usize = 2;
/// The id string table is padded to this boundary so record sections stay
/// 4-byte aligned for a typed-array view.
pub const SECTION_ALIGNMENT: usize = 4;

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
    id: &'a str,
    published_len: u32,
    findings: &'a [PackedFinding],
}

impl<'a> PublicationBook<'a> {
    /// `id` is the host's opaque book identity; it must be unique in the
    /// publication, where [`BookKey`] need not be.
    pub const fn new(
        key: BookKey,
        id: &'a str,
        published_len: u32,
        findings: &'a [PackedFinding],
    ) -> Self {
        Self {
            key,
            id,
            published_len,
            findings,
        }
    }

    pub const fn key(&self) -> BookKey {
        self.key
    }

    pub const fn id(&self) -> &'a str {
        self.id
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

    let mut ids = FxHashSet::default();
    ids.reserve(sections.len());
    let mut total_findings = 0u32;
    let mut id_bytes = 0usize;
    for (index, section) in sections.iter().enumerate() {
        validate_key(section.key)?;
        if u16::try_from(section.id.len()).is_err() {
            return Err(CorpusWireError::BookIdTooLong { book: index });
        }
        if !ids.insert(section.id) {
            return Err(CorpusWireError::DuplicateBookId { book: index });
        }
        id_bytes += ID_PREFIX_BYTES + section.id.len();
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
    let id_start = HEADER_BYTES
        .checked_add(directory_bytes)
        .ok_or(CorpusWireError::SizeOverflow)?;
    let data_start = id_start
        .checked_add(padded(id_bytes).ok_or(CorpusWireError::SizeOverflow)?)
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
    let mut id_offset = id_start;
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
        out.extend_from_slice(
            &u32::try_from(id_offset)
                .map_err(|_| CorpusWireError::SizeOverflow)?
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
        id_offset += ID_PREFIX_BYTES + section.id.len();
    }

    for section in sections {
        out.extend_from_slice(&(section.id.len() as u16).to_le_bytes());
        out.extend_from_slice(section.id.as_bytes());
    }
    out.resize(data_start, 0);

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
    books: Vec<BookMeta<'a>>,
}

#[derive(Debug, Clone, Copy)]
struct BookMeta<'a> {
    key: BookKey,
    id: &'a str,
    published_len: u32,
    offset: usize,
    count: usize,
}

pub struct CorpusBook<'a> {
    bytes: &'a [u8],
    index: BookIndex,
    key: BookKey,
    id: &'a str,
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
        let id_start = HEADER_BYTES
            .checked_add(directory_bytes)
            .ok_or(CorpusWireError::SizeOverflow)?;
        if id_start > bytes.len() {
            return Err(CorpusWireError::InvalidLength {
                actual: bytes.len(),
            });
        }
        let (ids, id_end) = read_id_table(bytes, book_count, id_start)?;
        let data_start = id_start
            .checked_add(padded(id_end - id_start).ok_or(CorpusWireError::SizeOverflow)?)
            .ok_or(CorpusWireError::SizeOverflow)?;
        if data_start > bytes.len() {
            return Err(CorpusWireError::InvalidLength {
                actual: bytes.len(),
            });
        }

        let mut books = Vec::with_capacity(book_count);
        let mut cursor = data_start;
        let mut total_seen = 0u32;
        for (index, id) in ids.into_iter().enumerate() {
            let at = HEADER_BYTES + index * DIRECTORY_ENTRY_BYTES;
            let key_bytes: [u8; 3] = bytes[at..at + 3].try_into().expect("directory checked");
            if bytes[at + 3] != 0 {
                return Err(CorpusWireError::InvalidBookKey { bytes: key_bytes });
            }
            let key = BookKey::new(key_bytes);
            validate_key(key)?;
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
                id,
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
            id: meta.id,
            published_len: meta.published_len,
            offset: meta.offset,
            count: meta.count,
        })
    }

    /// The FIRST book carrying this key; two ids may publish the same `\id`.
    pub fn book_by_key(&self, key: BookKey) -> Option<CorpusBook<'a>> {
        self.at(self.books.iter().position(|meta| meta.key == key)?)
    }

    /// The book under this host id, which is unique in a publication.
    pub fn book_by_id(&self, id: &str) -> Option<CorpusBook<'a>> {
        self.at(self.books.iter().position(|meta| meta.id == id)?)
    }

    fn at(&self, index: usize) -> Option<CorpusBook<'a>> {
        self.book(BookIndex::new(index).expect("book count was checked"))
    }
}

impl<'a> CorpusBook<'a> {
    pub const fn index(&self) -> BookIndex {
        self.index
    }

    pub const fn key(&self) -> BookKey {
        self.key
    }

    /// The host id this book was published under.
    pub const fn id(&self) -> &'a str {
        self.id
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

/// The id table read in directory order: every offset must be the running
/// cursor, so the strings cannot overlap, reorder, or hide bytes.
fn read_id_table(
    bytes: &[u8],
    book_count: usize,
    id_start: usize,
) -> Result<(Vec<&str>, usize), CorpusWireError> {
    let mut ids = Vec::with_capacity(book_count);
    let mut seen = FxHashSet::default();
    seen.reserve(book_count);
    let mut cursor = id_start;
    for index in 0..book_count {
        let at = HEADER_BYTES + index * DIRECTORY_ENTRY_BYTES + DIRECTORY_ID_OFFSET;
        let offset =
            usize::try_from(read_u32(bytes, at)).map_err(|_| CorpusWireError::SizeOverflow)?;
        if offset != cursor {
            return Err(CorpusWireError::BookIdOutOfOrder {
                book: index,
                expected: cursor,
                actual: offset,
            });
        }
        let end = offset
            .checked_add(ID_PREFIX_BYTES)
            .ok_or(CorpusWireError::SizeOverflow)?;
        if end > bytes.len() {
            return Err(CorpusWireError::InvalidBookId { book: index });
        }
        let len = usize::from(u16::from_le_bytes(
            bytes[offset..end].try_into().expect("two bytes"),
        ));
        let text_end = end.checked_add(len).ok_or(CorpusWireError::SizeOverflow)?;
        if text_end > bytes.len() {
            return Err(CorpusWireError::InvalidBookId { book: index });
        }
        let id = core::str::from_utf8(&bytes[end..text_end])
            .map_err(|_| CorpusWireError::InvalidBookId { book: index })?;
        if !seen.insert(id) {
            return Err(CorpusWireError::DuplicateBookId { book: index });
        }
        ids.push(id);
        cursor = text_end;
    }
    Ok((ids, cursor))
}

/// `len` rounded up to [`SECTION_ALIGNMENT`].
fn padded(len: usize) -> Option<usize> {
    len.checked_add(SECTION_ALIGNMENT - 1)
        .map(|rounded| rounded & !(SECTION_ALIGNMENT - 1))
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
        }
    }
}

impl std::error::Error for CorpusWireError {}

/// Render the checked-in TypeScript reader from the Rust-owned wire schema.
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
        .replace("@@DIRECTORY_ID_OFFSET@@", &DIRECTORY_ID_OFFSET.to_string())
        .replace("@@ID_PREFIX_BYTES@@", &ID_PREFIX_BYTES.to_string())
        .replace("@@SECTION_ALIGNMENT@@", &SECTION_ALIGNMENT.to_string())
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
    /// One book under `books/mrk.usfm`: header, one directory row, a padded
    /// 16-byte id table, then the records.
    const FIRST_RECORD: usize = HEADER_BYTES + DIRECTORY_ENTRY_BYTES + 16;

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
        let section =
            PublicationBook::new(BookKey::new(*b"MRK"), "books/mrk.usfm", 0x0200, &findings);
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
        assert_eq!(book.id(), "books/mrk.usfm");
        assert_eq!(book.published_len(), 0x0200);
        assert_eq!(book.at(0).unwrap(), finding);
        assert_eq!(
            snapshot.book_by_id("books/mrk.usfm").unwrap().index().get(),
            0
        );
        assert!(snapshot.book_by_id("books/gen.usfm").is_none());
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
        let section =
            PublicationBook::new(BookKey::new(*b"MRK"), "books/mrk.usfm", 0x0100, &findings);
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
            PublicationBook::new(BookKey::new(*b"GEN"), "a/gen.usfm", 0, &[]),
            PublicationBook::new(BookKey::new(*b"MRK"), "b/mrk.usfm", 0, &[]),
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

    /// The string table is what makes two files of one `\id` addressable.
    #[test]
    fn duplicate_book_keys_are_legal_and_the_ids_tell_them_apart() {
        let books = [
            PublicationBook::new(BookKey::new(*b"GEN"), "a/gen-copy.usfm", 4, &[]),
            PublicationBook::new(BookKey::new(*b"GEN"), "a/gen.usfm", 8, &[]),
        ];
        let encoded =
            encode_to_corpus_buffer(SnapshotId::new([2; 16]), CoordinateSpace::Utf8, &books)
                .unwrap();
        let snapshot = CorpusSnapshot::open(&encoded).unwrap();

        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot.book_by_key(BookKey::new(*b"GEN")).unwrap().id(),
            "a/gen-copy.usfm",
            "a key seeks the first of its rows"
        );
        assert_eq!(snapshot.book_by_id("a/gen.usfm").unwrap().index().get(), 1);
        assert_eq!(
            snapshot.book_by_id("a/gen.usfm").unwrap().published_len(),
            8
        );
    }

    #[test]
    fn a_repeated_id_is_refused_on_the_way_in_and_out() {
        let books = [
            PublicationBook::new(BookKey::new(*b"GEN"), "same.usfm", 0, &[]),
            PublicationBook::new(BookKey::new(*b"MRK"), "same.usfm", 0, &[]),
        ];
        assert_eq!(
            encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf8, &books),
            Err(CorpusWireError::DuplicateBookId { book: 1 })
        );
    }

    #[test]
    fn a_moved_or_malformed_id_offset_fails_closed() {
        let books = [PublicationBook::new(
            BookKey::new(*b"MRK"),
            "books/mrk.usfm",
            0,
            &[],
        )];
        let encoded =
            encode_to_corpus_buffer(SnapshotId::new([0; 16]), CoordinateSpace::Utf8, &books)
                .unwrap();

        let mut moved = encoded.clone();
        moved[HEADER_BYTES + DIRECTORY_ID_OFFSET] = 0xff;
        assert!(matches!(
            CorpusSnapshot::open(&moved),
            Err(CorpusWireError::BookIdOutOfOrder { book: 0, .. })
        ));

        let mut torn = encoded;
        // The id's second UTF-8 byte, replaced by a continuation byte.
        torn[HEADER_BYTES + DIRECTORY_ENTRY_BYTES + ID_PREFIX_BYTES] = 0x80;
        assert_eq!(
            CorpusSnapshot::open(&torn).err(),
            Some(CorpusWireError::InvalidBookId { book: 0 })
        );
    }

    #[test]
    fn malformed_directory_and_record_fail_closed() {
        let finding = fixture_finding();
        let findings = [finding];
        let section =
            PublicationBook::new(BookKey::new(*b"MRK"), "books/mrk.usfm", 0x0200, &findings);
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
        bad_code[FIRST_RECORD + 10] = 2;
        assert!(matches!(
            CorpusSnapshot::open(&bad_code),
            Err(CorpusWireError::Record {
                error: CodecError::UnknownRuleCode(2),
                ..
            })
        ));
    }
}
