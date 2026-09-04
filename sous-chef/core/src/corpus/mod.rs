//! Complete-corpus findings publication.
//!
//! ```text
//! encode_to_corpus_buffer(id, Utf16, [PublicationBook("MRK", "books/mrk.usfm", 50, [finding])])
//!   → 48-byte header · one 20-byte directory row · "books/mrk.usfm" in the id
//!     string table · one 16-byte record
//! ```
//!
//! A fresh publication-ready buffer, not a serialization of the resident
//! analysis model. The producer owns any source/UTF-16 rebasing before calling
//! the writer. Envelope layout: codec/README.md.

use rustc_hash::FxHashSet;

use crate::codec::{PackedFinding, RECORD_LEN};
use crate::judge::Pattern;
use crate::{BookIndex, BookKey};

mod error;
mod layout;
mod pattern_row;
mod reader_ts;
#[cfg(test)]
mod tests;

pub use error::CorpusWireError;
pub use layout::*;
pub use reader_ts::generated_reader_ts;

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

/// Encode one complete corpus publication: the pattern table the judge
/// produced, then every book's records.
pub fn encode_to_corpus_buffer(
    snapshot_id: SnapshotId,
    coordinate_space: CoordinateSpace,
    sections: &[PublicationBook<'_>],
    patterns: &[Pattern],
) -> Result<Vec<u8>, CorpusWireError> {
    if patterns.len() > usize::from(u16::MAX) {
        return Err(CorpusWireError::PatternCountOverflow {
            count: patterns.len(),
        });
    }
    let pattern_count = patterns.len() as u32;
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
    let pattern_start = id_start
        .checked_add(padded(id_bytes).ok_or(CorpusWireError::SizeOverflow)?)
        .ok_or(CorpusWireError::SizeOverflow)?;
    let data_start = pattern_start
        .checked_add(
            patterns
                .len()
                .checked_mul(PATTERN_ROW_LEN)
                .ok_or(CorpusWireError::SizeOverflow)?,
        )
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
    out.extend_from_slice(&pattern_count.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(pattern_start)
            .map_err(|_| CorpusWireError::SizeOverflow)?
            .to_le_bytes(),
    );
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
    out.resize(pattern_start, 0);

    for (row, pattern) in patterns.iter().enumerate() {
        pattern
            .validate()
            .map_err(|field| CorpusWireError::InvalidPattern { row, field })?;
        // Books-possible is this publication's own book count, which only the
        // envelope knows.
        if usize::from(pattern.books) > sections.len() {
            return Err(CorpusWireError::InvalidPattern {
                row,
                field: "books",
            });
        }
        out.extend_from_slice(&pattern_row::encode_pattern(pattern));
    }
    debug_assert_eq!(out.len(), data_start);

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
    pattern_start: usize,
    pattern_count: usize,
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
    pattern_count: usize,
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
        let total_findings = read_u32(bytes, HEADER_TOTAL_FINDINGS_OFFSET);
        let pattern_count_raw = read_u32(bytes, HEADER_PATTERN_COUNT_OFFSET);
        let pattern_count = usize::try_from(pattern_count_raw)
            .map_err(|_| CorpusWireError::PatternCountOverflow { count: usize::MAX })?;
        if pattern_count > usize::from(u16::MAX) {
            return Err(CorpusWireError::PatternCountOverflow {
                count: pattern_count,
            });
        }
        let snapshot_id = SnapshotId::new(
            bytes[HEADER_SNAPSHOT_ID_OFFSET..HEADER_BYTES]
                .try_into()
                .expect("header checked"),
        );
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
        let pattern_start = id_start
            .checked_add(padded(id_end - id_start).ok_or(CorpusWireError::SizeOverflow)?)
            .ok_or(CorpusWireError::SizeOverflow)?;
        if read_u32(bytes, HEADER_PATTERN_OFFSET_OFFSET)
            != u32::try_from(pattern_start).unwrap_or(u32::MAX)
        {
            return Err(CorpusWireError::PatternSectionOutOfOrder {
                expected: pattern_start,
                actual: usize::try_from(read_u32(bytes, HEADER_PATTERN_OFFSET_OFFSET))
                    .unwrap_or(usize::MAX),
            });
        }
        let data_start = pattern_start
            .checked_add(
                pattern_count
                    .checked_mul(PATTERN_ROW_LEN)
                    .ok_or(CorpusWireError::SizeOverflow)?,
            )
            .ok_or(CorpusWireError::SizeOverflow)?;
        if data_start > bytes.len() {
            return Err(CorpusWireError::InvalidLength {
                actual: bytes.len(),
            });
        }
        for row in 0..pattern_count {
            let at = pattern_start + row * PATTERN_ROW_LEN;
            pattern_row::decode_pattern(&bytes[at..at + PATTERN_ROW_LEN], row, book_count)?;
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
                pattern_row::validate_pattern_ref(finding, pattern_count, index, row)?;
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
            pattern_start,
            pattern_count,
        })
    }

    /// Rows in the publication's pattern table.
    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    /// One pattern table row, decoded.
    pub fn pattern(&self, index: usize) -> Result<Pattern, CorpusWireError> {
        if index >= self.pattern_count {
            return Err(CorpusWireError::PatternIndexPastTable {
                index,
                count: self.pattern_count,
                at: None,
            });
        }
        let at = self.pattern_start + index * PATTERN_ROW_LEN;
        pattern_row::decode_pattern(
            &self.bytes[at..at + PATTERN_ROW_LEN],
            index,
            self.books.len(),
        )
    }

    /// The whole table, in emission order.
    pub fn patterns(&self) -> Result<Vec<Pattern>, CorpusWireError> {
        (0..self.pattern_count)
            .map(|row| self.pattern(row))
            .collect()
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
            pattern_count: self.pattern_count,
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
        pattern_row::validate_pattern_ref(
            finding,
            self.pattern_count,
            self.index.get() as usize,
            row,
        )?;
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
