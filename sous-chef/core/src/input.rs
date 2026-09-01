//! The producer-neutral text boundary consumed by Sous passes.
//!
//! Onion and vref retain their own source maps and addressing. This module
//! names only the projected UTF-8 ranges and scripture units the analysis
//! engine needs, so `sous-core` does not learn either producer's storage.

use core::{fmt, ops::Range};

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
    fn text(&self) -> &str;
    fn chapters(&self) -> impl Iterator<Item = Chapter>;
    fn verses(&self) -> impl Iterator<Item = Verse>;
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
        text: &'static str,
        chapters: Vec<Chapter>,
        verses: Vec<Verse>,
    }

    impl ProjectedBook for Book {
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
            text: "a🧅b",
            chapters: vec![chapter(1, 0, 6)],
            verses: vec![verse(1, 1, 1, 1, 5)],
        };
        assert_eq!(validate(&book), Ok(()));

        let broken = Book {
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
            text: "one",
            chapters: vec![chapter(1, 0, 3)],
            verses: vec![verse(2, 1, 1, 0, 3)],
        };
        assert!(matches!(
            validate(&orphaned),
            Err(InputError::VerseWithoutChapter { .. })
        ));
    }
}
