//! The producer-neutral text boundary consumed by Sous passes.
//!
//! Onion and vref retain their own source maps and addressing. This module
//! names only the projected UTF-8 ranges and scripture units the analysis
//! engine needs, so `sous-core` does not learn either producer's storage.

use core::{fmt, ops::Range};
use rustc_hash::FxHashSet;

/// The stable scripture identity used to pair books across producer inputs.
///
/// This is deliberately distinct from [`BookIndex`]: a caller may present
/// the same books in any order, while findings still refer to that caller's
/// exact snapshot position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BookKey([u8; 3]);

impl BookKey {
    pub const fn new(bytes: [u8; 3]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; 3] {
        self.0
    }
}

impl From<[u8; 3]> for BookKey {
    fn from(bytes: [u8; 3]) -> Self {
        Self::new(bytes)
    }
}

impl fmt::Display for BookKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match core::str::from_utf8(&self.0) {
            Ok(code) => f.write_str(code),
            Err(_) => write!(f, "{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2]),
        }
    }
}

/// A checked position in the caller-owned corpus book table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BookIndex(u16);

impl BookIndex {
    pub fn new(index: usize) -> Result<Self, InputError> {
        u16::try_from(index)
            .map(Self)
            .map_err(|_| InputError::BookIndexOverflow { index })
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<usize> for BookIndex {
    type Error = InputError;

    fn try_from(index: usize) -> Result<Self, Self::Error> {
        Self::new(index)
    }
}

/// A checked half-open byte range in one projected book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextRange {
    from: u32,
    to: u32,
}

impl TextRange {
    pub fn new(from: u32, to: u32) -> Result<Self, InputError> {
        if from > to {
            return Err(InputError::ReversedRange { from, to });
        }
        Ok(Self { from, to })
    }

    pub const fn from(self) -> u32 {
        self.from
    }

    pub const fn to(self) -> u32 {
        self.to
    }

    pub const fn len(self) -> u32 {
        self.to - self.from
    }

    pub const fn is_empty(self) -> bool {
        self.from == self.to
    }

    pub fn as_range(self) -> Range<u32> {
        self.from..self.to
    }

    fn contains(self, other: Self) -> bool {
        self.from <= other.from && other.to <= self.to
    }
}

/// One independently mappable chapter in projected-book coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chapter {
    number: u16,
    text: TextRange,
}

impl Chapter {
    pub fn new(number: u16, text: TextRange) -> Result<Self, InputError> {
        if number == 0 {
            return Err(InputError::ZeroChapter);
        }
        Ok(Self { number, text })
    }

    pub const fn number(self) -> u16 {
        self.number
    }

    pub const fn text(self) -> TextRange {
        self.text
    }
}

/// The numeric identity shared by an ordinary verse and a verse bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerseKey {
    chapter: u16,
    first: u16,
    last: u16,
}

impl VerseKey {
    pub fn new(chapter: u16, first: u16, last: u16) -> Result<Self, InputError> {
        if chapter == 0 || first == 0 || first > last {
            return Err(InputError::MalformedVerseKey {
                chapter,
                first,
                last,
            });
        }
        Ok(Self {
            chapter,
            first,
            last,
        })
    }

    pub const fn chapter(self) -> u16 {
        self.chapter
    }

    pub const fn first(self) -> u16 {
        self.first
    }

    pub const fn last(self) -> u16 {
        self.last
    }
}

/// One aligned unit. Duplicate keys remain duplicate rows in producer order;
/// pairing derives their occurrence ordinals later instead of collapsing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verse {
    key: VerseKey,
    text: TextRange,
}

impl Verse {
    pub const fn new(key: VerseKey, text: TextRange) -> Self {
        Self { key, text }
    }

    pub const fn key(self) -> VerseKey {
        self.key
    }

    pub const fn text(self) -> TextRange {
        self.text
    }
}

/// A valid producer can compute these rows on demand; the contract does not
/// require a second stored chapter/verse index beside the producer's own one.
pub trait ProjectedBook {
    fn key(&self) -> BookKey;
    fn text(&self) -> &str;
    fn chapters(&self) -> impl Iterator<Item = Chapter>;
    fn verses(&self) -> impl Iterator<Item = Verse>;
}

/// A caller-owned, immutable book table for one analysis snapshot.
///
/// Book order is accepted as supplied. The index is navigation identity for
/// that snapshot, while [`BookKey`] is the identity used when comparing
/// corresponding books from another corpus.
pub struct Corpus<'a, B> {
    books: &'a [B],
}

impl<'a, B: ProjectedBook> Corpus<'a, B> {
    pub fn try_new(books: &'a [B]) -> Result<Self, InputError> {
        if books.len() > usize::from(u16::MAX) + 1 {
            return Err(InputError::BookCountOverflow { count: books.len() });
        }

        let mut keys = FxHashSet::default();
        keys.reserve(books.len());
        for book in books {
            if !keys.insert(book.key()) {
                return Err(InputError::DuplicateBookKey { key: book.key() });
            }
            validate(book)?;
        }

        Ok(Self { books })
    }

    pub fn books(&self) -> &'a [B] {
        self.books
    }

    pub fn len(&self) -> usize {
        self.books.len()
    }

    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    pub fn get(&self, index: BookIndex) -> Option<&'a B> {
        self.books.get(index.get() as usize)
    }

    pub fn index_of(&self, key: BookKey) -> Option<BookIndex> {
        self.books
            .iter()
            .position(|book| book.key() == key)
            .map(|index| BookIndex::new(index).expect("corpus count was checked"))
    }

    pub fn iter(&self) -> impl Iterator<Item = (BookIndex, &'a B)> {
        self.books.iter().enumerate().map(|(index, book)| {
            (
                BookIndex::new(index).expect("corpus count was checked"),
                book,
            )
        })
    }
}

/// Checks the neutral boundary once so analysis passes may trust its ranges,
/// ordering, and UTF-8 edges without repeating producer validation.
pub fn validate(book: &impl ProjectedBook) -> Result<(), InputError> {
    let text = book.text();
    let text_len = u32::try_from(text.len()).map_err(|_| InputError::BookTooLong)?;

    let mut previous_chapter = None;
    let mut previous_end = 0;
    for chapter in book.chapters() {
        validate_range(text, text_len, chapter.text)?;
        if let Some(previous) = previous_chapter
            && chapter.number <= previous
        {
            return Err(InputError::ChapterOrder {
                previous,
                next: chapter.number,
            });
        }
        if chapter.text.from < previous_end {
            return Err(InputError::ChapterOverlap {
                chapter: chapter.number,
            });
        }
        previous_chapter = Some(chapter.number);
        previous_end = chapter.text.to;
    }

    let mut previous_key = None;
    let mut chapters = book.chapters().peekable();
    for verse in book.verses() {
        validate_range(text, text_len, verse.text)?;
        if let Some(previous) = previous_key
            && verse.key < previous
        {
            return Err(InputError::VerseOrder {
                previous,
                next: verse.key,
            });
        }
        previous_key = Some(verse.key);

        while chapters
            .peek()
            .is_some_and(|chapter| chapter.number < verse.key.chapter)
        {
            chapters.next();
        }
        let Some(chapter) = chapters.peek() else {
            return Err(InputError::VerseWithoutChapter { key: verse.key });
        };
        if chapter.number != verse.key.chapter {
            return Err(InputError::VerseWithoutChapter { key: verse.key });
        }
        if !chapter.text.contains(verse.text) {
            return Err(InputError::VerseOutsideChapter { key: verse.key });
        }
    }
    Ok(())
}

fn validate_range(text: &str, text_len: u32, range: TextRange) -> Result<(), InputError> {
    if range.to > text_len {
        return Err(InputError::RangeOutOfBounds { range, text_len });
    }
    if !text.is_char_boundary(range.from as usize) || !text.is_char_boundary(range.to as usize) {
        return Err(InputError::NotUtf8Boundary { range });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    BookCountOverflow { count: usize },
    BookIndexOverflow { index: usize },
    DuplicateBookKey { key: BookKey },
    BookTooLong,
    ReversedRange { from: u32, to: u32 },
    RangeOutOfBounds { range: TextRange, text_len: u32 },
    NotUtf8Boundary { range: TextRange },
    ZeroChapter,
    ChapterOrder { previous: u16, next: u16 },
    ChapterOverlap { chapter: u16 },
    MalformedVerseKey { chapter: u16, first: u16, last: u16 },
    VerseOrder { previous: VerseKey, next: VerseKey },
    VerseWithoutChapter { key: VerseKey },
    VerseOutsideChapter { key: VerseKey },
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BookCountOverflow { count } => {
                write!(f, "corpus contains {count} books; maximum is 65536")
            }
            Self::BookIndexOverflow { index } => {
                write!(f, "book index {index} does not fit in u16")
            }
            Self::DuplicateBookKey { key } => write!(f, "duplicate book key {key}"),
            Self::BookTooLong => f.write_str("projected book exceeds u32 byte coordinates"),
            Self::ReversedRange { from, to } => write!(f, "reversed projected range {from}..{to}"),
            Self::RangeOutOfBounds { range, text_len } => write!(
                f,
                "projected range {}..{} exceeds book length {text_len}",
                range.from, range.to
            ),
            Self::NotUtf8Boundary { range } => write!(
                f,
                "projected range {}..{} splits a UTF-8 scalar",
                range.from, range.to
            ),
            Self::ZeroChapter => f.write_str("chapter zero is front matter, not an analysis unit"),
            Self::ChapterOrder { previous, next } => {
                write!(f, "chapter {next} does not follow chapter {previous}")
            }
            Self::ChapterOverlap { chapter } => {
                write!(f, "chapter {chapter} overlaps the preceding chapter")
            }
            Self::MalformedVerseKey {
                chapter,
                first,
                last,
            } => write!(f, "malformed verse key {chapter}:{first}-{last}"),
            Self::VerseOrder { previous, next } => {
                write!(f, "verse {next:?} sorts before {previous:?}")
            }
            Self::VerseWithoutChapter { key } => {
                write!(f, "verse {key:?} has no chapter row")
            }
            Self::VerseOutsideChapter { key } => {
                write!(f, "verse {key:?} lies outside its chapter range")
            }
        }
    }
}

impl std::error::Error for InputError {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Book {
        key: BookKey,
        text: &'static str,
        chapters: Vec<Chapter>,
        verses: Vec<Verse>,
    }

    impl ProjectedBook for Book {
        fn key(&self) -> BookKey {
            self.key
        }

        fn text(&self) -> &str {
            self.text
        }

        fn chapters(&self) -> impl Iterator<Item = Chapter> {
            self.chapters.iter().copied()
        }

        fn verses(&self) -> impl Iterator<Item = Verse> {
            self.verses.iter().copied()
        }
    }

    fn range(from: u32, to: u32) -> TextRange {
        TextRange::new(from, to).unwrap()
    }

    fn chapter(number: u16, from: u32, to: u32) -> Chapter {
        Chapter::new(number, range(from, to)).unwrap()
    }

    fn verse(chapter: u16, first: u16, last: u16, from: u32, to: u32) -> Verse {
        Verse::new(
            VerseKey::new(chapter, first, last).unwrap(),
            range(from, to),
        )
    }

    #[test]
    fn valid_book_keeps_bridges_and_duplicate_rows() {
        let book = Book {
            key: BookKey::new(*b"TST"),
            text: "one two three",
            chapters: vec![chapter(1, 0, 13)],
            verses: vec![
                verse(1, 1, 1, 0, 3),
                verse(1, 2, 3, 4, 7),
                verse(1, 2, 3, 8, 13),
            ],
        };

        assert_eq!(validate(&book), Ok(()));
    }

    #[test]
    fn rejects_a_range_that_splits_utf8() {
        let book = Book {
            key: BookKey::new(*b"TST"),
            text: "a🧅b",
            chapters: vec![chapter(1, 0, 6)],
            verses: vec![verse(1, 1, 1, 1, 5)],
        };
        assert_eq!(validate(&book), Ok(()));

        let broken = Book {
            key: BookKey::new(*b"TST"),
            text: "a🧅b",
            chapters: vec![chapter(1, 0, 2)],
            verses: vec![],
        };
        assert_eq!(
            validate(&broken),
            Err(InputError::NotUtf8Boundary { range: range(0, 2) })
        );
    }

    #[test]
    fn rejects_reordered_chapters_and_orphaned_verses() {
        let reordered = Book {
            key: BookKey::new(*b"TST"),
            text: "one two",
            chapters: vec![chapter(2, 0, 3), chapter(1, 4, 7)],
            verses: vec![],
        };
        assert_eq!(
            validate(&reordered),
            Err(InputError::ChapterOrder {
                previous: 2,
                next: 1
            })
        );

        let orphaned = Book {
            key: BookKey::new(*b"TST"),
            text: "one",
            chapters: vec![chapter(1, 0, 3)],
            verses: vec![verse(2, 1, 1, 0, 3)],
        };
        assert!(matches!(
            validate(&orphaned),
            Err(InputError::VerseWithoutChapter { .. })
        ));
    }

    fn valid_book_with_key(key: [u8; 3]) -> Book {
        Book {
            key: BookKey::new(key),
            text: "one",
            chapters: vec![chapter(1, 0, 3)],
            verses: vec![verse(1, 1, 1, 0, 3)],
        }
    }

    #[test]
    fn corpus_keeps_caller_order_as_book_index_and_looks_up_by_key() {
        let books = vec![valid_book_with_key(*b"MRK"), valid_book_with_key(*b"GEN")];
        let corpus = Corpus::try_new(&books).unwrap();
        let entries: Vec<_> = corpus
            .iter()
            .map(|(index, book)| (index.get(), book.key()))
            .collect();

        assert_eq!(
            entries,
            vec![(0, BookKey::new(*b"MRK")), (1, BookKey::new(*b"GEN"))]
        );
        assert_eq!(corpus.index_of(BookKey::new(*b"GEN")).unwrap().get(), 1);
        assert_eq!(
            corpus.get(BookIndex::new(0).unwrap()).unwrap().key(),
            BookKey::new(*b"MRK")
        );

        let reordered = vec![valid_book_with_key(*b"GEN"), valid_book_with_key(*b"MRK")];
        let reordered = Corpus::try_new(&reordered).unwrap();
        assert_eq!(reordered.index_of(BookKey::new(*b"GEN")).unwrap().get(), 0);
        assert_eq!(reordered.index_of(BookKey::new(*b"MRK")).unwrap().get(), 1);
    }

    #[test]
    fn corpus_rejects_duplicate_keys_but_allows_same_content_under_distinct_keys() {
        let duplicate = vec![valid_book_with_key(*b"MRK"), valid_book_with_key(*b"MRK")];
        match Corpus::try_new(&duplicate) {
            Err(error) => assert_eq!(
                error,
                InputError::DuplicateBookKey {
                    key: BookKey::new(*b"MRK")
                }
            ),
            Ok(_) => panic!("duplicate book key was accepted"),
        }

        let distinct = vec![valid_book_with_key(*b"MRK"), valid_book_with_key(*b"GEN")];
        assert_eq!(Corpus::try_new(&distinct).unwrap().len(), 2);
    }

    #[test]
    fn book_index_rejects_positions_beyond_u16_capacity() {
        assert_eq!(BookIndex::new(65_535).unwrap().get(), u16::MAX);
        assert_eq!(
            BookIndex::new(65_536),
            Err(InputError::BookIndexOverflow { index: 65_536 })
        );
    }
}
