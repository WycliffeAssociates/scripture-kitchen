//! An address: a place in scripture that belongs to no particular text.
//!
//! ```text
//! Address::Book                                    GEN            the book
//! Address::Intro                                   GEN 0          before its first chapter (SRS chapter 0)
//! Address::Chapters { from: 2, to: 4 }             MAT 2-4        whole chapters
//! Address::Verses { from: 1:1, to: 1:2 }           LUK 1:1-2      a single verse has equal ends
//! Address::Verses { from: 1:20, to: 2:3 }          MAT 1:20-2:3   a range may cross a chapter
//! Point { chapter: 1, verse: 3, segment: a }       LUK 1:3a       a place inside verse 3
//! Point { chapter: 7, verse: 0, segment: None }    PSA 7:0        a chapter's title
//! ```
//!
//! A segment is ONE lowercase ASCII letter, as the Scripture Referencing
//! Standard says ("Only a simple ascii lowercase letter may be used for a
//! subverse identifier"). So an `Address` holds no heap and is `Copy`.
//!
//! No book, no offsets, no text: whether a text has the place is a question
//! for that text's `Toc`, and the book is the caller's to match. Sefer's
//! `Address`, minus its book.

use core::cmp::Ordering;
use core::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Address {
    Book,
    Intro,
    Chapters { from: u16, to: u16 },
    Verses { from: Point, to: Point },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub chapter: u16,
    /// 0 is the chapter's title (a Psalm's superscription).
    pub verse: u16,
    /// `a` in `3a`; `None` for the whole verse.
    pub segment: Option<Segment>,
}

/// A verse part: one lowercase ASCII letter, `a` in `3a`. Made only through
/// [`Segment::new`] or [`Segment::parse`], so it is never anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Segment(u8);

impl Segment {
    /// `None` unless `letter` is `a`..=`z`.
    pub const fn new(letter: u8) -> Option<Segment> {
        if letter.is_ascii_lowercase() {
            Some(Segment(letter))
        } else {
            None
        }
    }

    /// One letter as written: `None` for `""`, `ab`, `A` or `1`.
    pub fn parse(text: &[u8]) -> Option<Segment> {
        match text {
            [letter] => Segment::new(*letter),
            _ => None,
        }
    }

    pub const fn byte(self) -> u8 {
        self.0
    }
}

impl core::fmt::Display for Segment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_char(self.0 as char)
    }
}

impl Point {
    pub fn verse(chapter: u16, verse: u16) -> Point {
        Point {
            chapter,
            verse,
            segment: None,
        }
    }

    /// Where two places END, in order: a whole verse runs past each of its
    /// segments, so `3` ends after `3b`, where as starts `3` sorts first.
    pub fn cmp_ends(&self, other: &Point) -> Ordering {
        (self.chapter, self.verse)
            .cmp(&(other.chapter, other.verse))
            .then_with(|| match (self.segment, other.segment) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(&b),
            })
    }
}

/// Start order: chapter, verse, then segment, a whole verse before its
/// segments (`3 < 3a < 3b`).
impl Ord for Point {
    fn cmp(&self, other: &Point) -> Ordering {
        (self.chapter, self.verse, self.segment).cmp(&(other.chapter, other.verse, other.segment))
    }
}

impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Point) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(chapter: u16, verse: u16, segment: &str) -> Point {
        Point {
            chapter,
            verse,
            segment: Segment::parse(segment.as_bytes()),
        }
    }

    #[test]
    fn a_whole_verse_starts_before_and_ends_after_its_segments() {
        let whole = Point::verse(1, 3);
        assert!(whole < seg(1, 3, "a"));
        assert!(seg(1, 3, "a") < seg(1, 3, "b"));
        assert_eq!(whole.cmp_ends(&seg(1, 3, "b")), Ordering::Greater);
        assert_eq!(seg(1, 3, "a").cmp_ends(&seg(1, 3, "b")), Ordering::Less);
        assert!(seg(1, 3, "b") < Point::verse(1, 4));
    }

    #[test]
    fn a_segment_is_one_lowercase_letter() {
        assert_eq!(Segment::parse(b"a").map(Segment::byte), Some(b'a'));
        assert_eq!(
            Segment::parse(b"z").map(|s| s.to_string()),
            Some("z".to_string())
        );
        for not in [&b""[..], b"ab", b"A", b"1", "\u{e9}".as_bytes()] {
            assert_eq!(Segment::parse(not), None, "{not:?}");
        }
    }
}
