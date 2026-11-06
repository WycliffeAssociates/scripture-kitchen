//! reference — a place in scripture, spelled for machines.
//!
//! ```text
//! parse("MAT 1:20-2:3; MRK 1:1")  -> [MAT Verses 1:20..2:3, MRK Verses 1:1..1:1]
//! parse("PSA 7:0")                -> [PSA Verses 7:0..7:0]       a Psalm's title
//! parse("MRK 0")                  -> [MRK Intro]
//! parse("ROM 3:21, 24-26")        -> [ROM 3:21, ROM 3:24-26]     24 is read in 3:21's chapter
//! parse("JUD 3")                  -> [JUD 1:3]                  a one-chapter book's number is a verse
//! parse_in(MAT, "5:3-12")         -> [MAT 5:3-12]               a book from context
//! format(&refs)                   -> "MAT 1:20-2:3; MRK 1:1"
//! ```
//!
//! The spelling is the Scripture Referencing Standard's BASIC reference
//! (usfm-bible/tcdocs, `docs/Scripture Referencing Standard.md`, version 1.0):
//! a book code, a chapter, a verse with an optional one-letter part, ranges
//! and lists. Everything else the standard adds (translation prefixes, word,
//! character and marker references, named references) is a different
//! address space and is refused here by name ([`ErrorKind::NotBasic`]).
//!
//! STRICT: one spelling, and anything else is an error with its byte offset,
//! never a guess. Book codes are capitals, as the standard writes them; a
//! person's `rom 9:1` or `Romans 9:1` is a citation, a different reader.
//!
//! The rules the grammar leaves to a processor, and what this one does:
//!
//! - **Context.** A reference without a book is read against the PARENT of the
//!   one before it (the standard's rule): after `ROM 3:21`, `24` is a verse of
//!   chapter 3; after `ROM 9`, `10` is a chapter; `4:1` is always a chapter and
//!   verse of the same book. In a list the previous reference is the end of
//!   the previous range.
//! - **One-chapter books** (`OBA`, `PHM`, `2JN`, `3JN`, `JUD`): a bare number
//!   is a verse of chapter 1, so `JUD 1-4` is `JUD 1:1-4`, as the standard
//!   says. `JUD 0` is still the introduction. Written out with the chapter.
//! - **`end`** is the last chapter or verse, which only a versification knows:
//!   [`parse_with`] takes a canon scheme, and without one `end` is refused.
//!   Given a scheme, a chapter or verse past its end is refused too.
//! - **Chapter 0** is the introduction, which has no verses. **Verse 0** is a
//!   chapter's title (`PSA 7:0`).
//! - A range is one book (`ROM 9:1-GEN 1:1` is refused) and runs forwards, at
//!   one level: chapters to chapters, verses to verses.

use core::fmt;

pub mod cite;
pub use cite::{
    Assumption, Catalogue, CiteError, Cited, Grammar, Problem, citation_words, cite, show,
};

use canon::scheme::Scheme;
pub use mise::address::{Address, Point, Segment};
pub use mise::books::BookKey;

/// One range in one book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reference {
    pub book: BookKey,
    pub address: Address,
}

/// What a reader may know beyond the text.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context<'s> {
    /// The book a reference without one is in: `1:3-5` in MAT's notes.
    pub book: Option<BookKey>,
    /// Chapter and verse counts, for `end` and for range checks.
    pub scheme: Option<&'s Scheme>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    /// Byte offset into the text where the problem starts.
    pub at: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Nothing, or nothing after a `;` or `,`.
    Empty,
    /// Not a book code: three capitals or digits, at least one a capital.
    ExpectedBook,
    /// A well-formed code canon has no book for.
    UnknownBook,
    /// A reference with no book and nothing before it to take one from.
    NoReferenceBefore,
    ExpectedVerse,
    /// A `-` with nothing after it.
    RangeEnd,
    /// Something the grammar has no place for.
    Unexpected,
    /// A range whose two ends are in different books.
    DifferentBook,
    /// A range that ends before it starts.
    Backwards,
    /// A range from a chapter to a verse, or from a book to anything.
    MixedRange,
    /// `GEN 0:3`, or chapter 0 in a range.
    IntroHasNoVerses,
    /// `end` without a scheme to say where the end is.
    EndNeedsScheme,
    ChapterOutOfRange,
    VerseOutOfRange,
    /// A part of the standard beyond the basic reference: a translation prefix,
    /// or a word, character or marker reference.
    NotBasic,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.kind {
            ErrorKind::Empty => "expected a reference",
            ErrorKind::ExpectedBook => "expected a book code, such as ROM or 1CO",
            ErrorKind::UnknownBook => "no book has this code",
            ErrorKind::NoReferenceBefore => "a reference without a book needs one before it",
            ErrorKind::ExpectedVerse => "expected a verse",
            ErrorKind::RangeEnd => "expected the end of the range",
            ErrorKind::Unexpected => "unexpected text",
            ErrorKind::DifferentBook => "a range must stay in one book",
            ErrorKind::Backwards => "the range ends before it starts",
            ErrorKind::MixedRange => "a range runs chapter to chapter or verse to verse",
            ErrorKind::IntroHasNoVerses => "chapter 0, the introduction, has no verses",
            ErrorKind::EndNeedsScheme => "`end` needs a versification",
            ErrorKind::ChapterOutOfRange => "the book has no such chapter",
            ErrorKind::VerseOutOfRange => "the chapter has no such verse",
            ErrorKind::NotBasic => {
                "a translation prefix or a word, character or marker reference is SRS beyond the basic reference"
            }
        };
        write!(f, "{what} (at byte {})", self.at)
    }
}

impl std::error::Error for Error {}

/// A reference list in the machine spelling.
pub fn parse(text: &str) -> Result<Vec<Reference>, Error> {
    parse_with(text, Context::default())
}

/// A reference list whose first reference may leave its book out: `1:3-5`, `5`.
pub fn parse_in(book: BookKey, text: &str) -> Result<Vec<Reference>, Error> {
    parse_with(
        text,
        Context {
            book: Some(book),
            ..Context::default()
        },
    )
}

pub fn parse_with(text: &str, context: Context<'_>) -> Result<Vec<Reference>, Error> {
    let mut reader = Reader {
        text,
        at: 0,
        context,
    };
    reader.list()
}

/// The list in the machine spelling, `; `-separated, every reference whole.
pub fn format(refs: &[Reference]) -> String {
    let mut out = String::new();
    for (i, one) in refs.iter().enumerate() {
        if i > 0 {
            out.push_str("; ");
        }
        out.push_str(&one.to_string());
    }
    out
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.book)?;
        match self.address {
            Address::Book => Ok(()),
            Address::Intro => f.write_str(" 0"),
            Address::Chapters { from, to } if from == to => write!(f, " {from}"),
            Address::Chapters { from, to } => write!(f, " {from}-{to}"),
            Address::Verses { from, to } => {
                write!(f, " {}:", from.chapter)?;
                verse(f, &from)?;
                if to == from {
                    return Ok(());
                }
                f.write_str("-")?;
                if to.chapter != from.chapter {
                    write!(f, "{}:", to.chapter)?;
                }
                verse(f, &to)
            }
        }
    }
}

fn verse(f: &mut fmt::Formatter<'_>, point: &Point) -> fmt::Result {
    write!(f, "{}", point.verse)?;
    match point.segment {
        Some(segment) => write!(f, "{segment}"),
        None => Ok(()),
    }
}

/// A number as written, before a scheme says where `end` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Num {
    At(u16),
    End,
}

/// One reference as read: a book, a chapter, a verse.
#[derive(Debug, Clone, Copy)]
struct Raw {
    book: BookKey,
    chapter: Option<Num>,
    verse: Option<(Num, Option<Segment>)>,
    at: usize,
}

struct Reader<'t, 's> {
    text: &'t str,
    at: usize,
    context: Context<'s>,
}

impl Reader<'_, '_> {
    fn list(&mut self) -> Result<Vec<Reference>, Error> {
        let mut out = Vec::new();
        let mut previous = self.context.book.map(|book| Raw {
            book,
            chapter: None,
            verse: None,
            at: 0,
        });
        self.skip_ws();
        loop {
            if self.done() {
                return Err(self.error(ErrorKind::Empty));
            }
            let (one, last) = self.range(previous.as_ref())?;
            out.push(one);
            previous = Some(last);
            self.skip_ws();
            match self.peek() {
                None => return Ok(out),
                Some(';' | ',') => {
                    self.at += 1;
                    self.skip_ws();
                }
                Some(_) => return Err(self.error(ErrorKind::Unexpected)),
            }
        }
    }

    /// `Reference (ws* "-" ws* Reference)?`, and the raw end the next item reads
    /// its context from.
    fn range(&mut self, previous: Option<&Raw>) -> Result<(Reference, Raw), Error> {
        let start = self.reference(previous)?;
        let save = self.at;
        self.skip_ws();
        if self.peek() != Some('-') {
            self.at = save;
            let address = self.address(&start, None)?;
            return Ok((
                Reference {
                    book: start.book,
                    address,
                },
                start,
            ));
        }
        self.at += 1;
        self.skip_ws();
        if matches!(self.peek(), None | Some(';' | ',')) {
            return Err(self.error(ErrorKind::RangeEnd));
        }
        let end = self.reference(Some(&start))?;
        if end.book != start.book {
            return Err(Error {
                kind: ErrorKind::DifferentBook,
                at: end.at,
            });
        }
        let address = self.address(&start, Some(&end))?;
        Ok((
            Reference {
                book: start.book,
                address,
            },
            end,
        ))
    }

    fn reference(&mut self, previous: Option<&Raw>) -> Result<Raw, Error> {
        let at = self.at;
        let raw = if let Some(code) = self.book_code() {
            self.full(code, at)?
        } else if self.translation_prefix() {
            return Err(self.error(ErrorKind::NotBasic));
        } else {
            self.contextual(previous, at)?
        };
        if matches!(self.peek(), Some('!' | '@')) {
            return Err(self.error(ErrorKind::NotBasic));
        }
        Ok(raw)
    }

    /// `BookCode (ws+ Chapter (chaptersep Verse)?)?`
    fn full(&mut self, code: [u8; 3], at: usize) -> Result<Raw, Error> {
        let book = BookKey::new(code);
        if canon::books::book(book).is_none() {
            return Err(Error {
                kind: ErrorKind::UnknownBook,
                at,
            });
        }
        let mut raw = Raw {
            book,
            chapter: None,
            verse: None,
            at,
        };
        let save = self.at;
        self.skip_ws();
        if self.at == save || !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at = save;
            return Ok(raw);
        }
        let number = self.number()?;
        if self.chapter_sep() {
            raw.chapter = Some(Num::At(number));
            raw.verse = Some(self.verse()?);
        } else if self.one_chapter(book) && number != 0 {
            raw.chapter = Some(Num::At(1));
            raw.verse = Some((Num::At(number), self.segment()));
        } else {
            raw.chapter = Some(Num::At(number));
        }
        Ok(raw)
    }

    /// A reference without a book, read against the parent of `previous`.
    fn contextual(&mut self, previous: Option<&Raw>, at: usize) -> Result<Raw, Error> {
        // `1co`: digits run into a word, which is a book code mistyped, not a
        // number with a part after it (`2b`).
        let word = &self.text.as_bytes()[self.at..];
        let run = word
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric())
            .count();
        let digits = word.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0 && run - digits > 1 {
            return Err(self.error(ErrorKind::ExpectedBook));
        }
        let starts_number = digits > 0 || self.at_end_word();
        let starts_part = self.lone_letter();
        if !starts_number && !starts_part {
            return Err(self.error(ErrorKind::ExpectedBook));
        }
        let Some(previous) = previous else {
            return Err(self.error(ErrorKind::NoReferenceBefore));
        };
        let mut raw = Raw {
            book: previous.book,
            chapter: previous.chapter,
            verse: None,
            at,
        };
        if starts_part {
            // A bare part (`b` in `2a-b`) is a part of the verse before it.
            let Some((verse, _)) = previous.verse else {
                return Err(self.error(ErrorKind::Unexpected));
            };
            raw.verse = Some((verse, self.segment()));
            return Ok(raw);
        }
        let number = self.num()?;
        if self.chapter_sep() {
            let Num::At(chapter) = number else {
                return Err(self.error(ErrorKind::Unexpected));
            };
            raw.chapter = Some(Num::At(chapter));
            raw.verse = Some(self.verse()?);
        } else if previous.verse.is_some() {
            raw.verse = Some((number, self.segment()));
        } else if self.one_chapter(previous.book) && number != Num::At(0) {
            raw.chapter = Some(Num::At(1));
            raw.verse = Some((number, self.segment()));
        } else {
            raw.chapter = Some(number);
        }
        Ok(raw)
    }

    /// `Verse = digits Subverse? | "end"`
    fn verse(&mut self) -> Result<(Num, Option<Segment>), Error> {
        if !(self.peek().is_some_and(|c| c.is_ascii_digit()) || self.at_end_word()) {
            return Err(self.error(ErrorKind::ExpectedVerse));
        }
        let number = self.num()?;
        let segment = match number {
            Num::At(_) => self.segment(),
            Num::End => None,
        };
        Ok((number, segment))
    }

    fn num(&mut self) -> Result<Num, Error> {
        if self.at_end_word() {
            self.at += 3;
            return Ok(Num::End);
        }
        self.number().map(Num::At)
    }

    fn number(&mut self) -> Result<u16, Error> {
        let start = self.at;
        let digits = self.text[start..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        self.at += digits;
        self.text[start..self.at].parse().map_err(|_| Error {
            kind: ErrorKind::Unexpected,
            at: start,
        })
    }

    /// One lowercase letter right after a verse number, not starting a word.
    fn segment(&mut self) -> Option<Segment> {
        let bytes = self.text.as_bytes();
        let letter = *bytes.get(self.at)?;
        if bytes
            .get(self.at + 1)
            .is_some_and(|next| next.is_ascii_alphanumeric())
        {
            return None;
        }
        let segment = Segment::new(letter)?;
        self.at += 1;
        Some(segment)
    }

    fn lone_letter(&self) -> bool {
        let bytes = &self.text.as_bytes()[self.at..];
        matches!(bytes, [letter, rest @ ..]
            if letter.is_ascii_lowercase() && !rest.first().is_some_and(u8::is_ascii_alphanumeric))
    }

    fn at_end_word(&self) -> bool {
        let rest = &self.text.as_bytes()[self.at..];
        rest.starts_with(b"end") && !rest.get(3).is_some_and(u8::is_ascii_alphanumeric)
    }

    /// `bidi? (":" | ".")`
    fn chapter_sep(&mut self) -> bool {
        let rest = &self.text[self.at..];
        let bidi = rest
            .chars()
            .next()
            .filter(|c| matches!(c, '\u{200E}' | '\u{200F}'))
            .map_or(0, char::len_utf8);
        if matches!(rest.as_bytes().get(bidi), Some(b':' | b'.')) {
            self.at += bidi + 1;
            true
        } else {
            false
        }
    }

    /// A book code here, followed by no further letter or digit.
    fn book_code(&mut self) -> Option<[u8; 3]> {
        let bytes = &self.text.as_bytes()[self.at..];
        let code: [u8; 3] = bytes.get(..3)?.try_into().ok()?;
        let fits = code
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        let has_capital = code.iter().any(u8::is_ascii_uppercase);
        let ends = !bytes.get(3).is_some_and(u8::is_ascii_alphanumeric);
        (fits && has_capital && ends).then(|| {
            self.at += 3;
            code
        })
    }

    /// `Langtag ("+" id)* booksep` ahead of a book code: `en+ulb.ROM`.
    fn translation_prefix(&self) -> bool {
        let bytes = &self.text.as_bytes()[self.at..];
        let id = bytes
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'_'))
            .count();
        id > 0
            && matches!(bytes.get(id), Some(b'.' | b':'))
            && bytes.get(id + 1..id + 4).is_some_and(|code| {
                code.iter()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            })
    }

    fn one_chapter(&self, book: BookKey) -> bool {
        self.scheme().and_then(|scheme| scheme.chapters(book)) == Some(1)
    }

    /// The caller's scheme, or `eng` for which books have one chapter, which
    /// no scheme disagrees on.
    fn scheme(&self) -> Option<&Scheme> {
        self.context.scheme.or_else(|| canon::scheme::scheme("eng"))
    }

    /// The raw ends as an address, with `end` placed and, given a scheme,
    /// the numbers checked.
    fn address(&self, start: &Raw, end: Option<&Raw>) -> Result<Address, Error> {
        let start_point = self.point(start)?;
        let Some(end) = end else {
            return Ok(match (start.chapter, start_point) {
                (None, _) => Address::Book,
                (Some(_), Pointed::Chapter(0)) => Address::Intro,
                (Some(_), Pointed::Chapter(n)) => Address::Chapters { from: n, to: n },
                (Some(_), Pointed::Verse(point)) => Address::Verses {
                    from: point,
                    to: point,
                },
            });
        };
        let end_point = self.point(end)?;
        let backwards = Error {
            kind: ErrorKind::Backwards,
            at: end.at,
        };
        match (start.chapter, start_point, end_point) {
            (None, _, _) => Err(Error {
                kind: ErrorKind::MixedRange,
                at: end.at,
            }),
            (_, Pointed::Chapter(a), Pointed::Chapter(b)) => {
                if a == 0 || b == 0 {
                    Err(Error {
                        kind: ErrorKind::IntroHasNoVerses,
                        at: if a == 0 { start.at } else { end.at },
                    })
                } else if b < a {
                    Err(backwards)
                } else {
                    Ok(Address::Chapters { from: a, to: b })
                }
            }
            (_, Pointed::Verse(from), Pointed::Verse(to)) => {
                if to < from {
                    Err(backwards)
                } else {
                    Ok(Address::Verses { from, to })
                }
            }
            _ => Err(Error {
                kind: ErrorKind::MixedRange,
                at: end.at,
            }),
        }
    }

    fn point(&self, raw: &Raw) -> Result<Pointed, Error> {
        let Some(chapter) = raw.chapter else {
            return Ok(Pointed::Chapter(0));
        };
        let chapter = match chapter {
            Num::At(n) => n,
            Num::End => self
                .scheme_given()
                .and_then(|scheme| scheme.chapters(raw.book))
                .ok_or(Error {
                    kind: ErrorKind::EndNeedsScheme,
                    at: raw.at,
                })?,
        };
        if let Some(scheme) = self.context.scheme
            && chapter > 0
            && scheme.chapters(raw.book).is_some_and(|last| chapter > last)
        {
            return Err(Error {
                kind: ErrorKind::ChapterOutOfRange,
                at: raw.at,
            });
        }
        let Some((verse, segment)) = raw.verse else {
            return Ok(Pointed::Chapter(chapter));
        };
        if chapter == 0 {
            return Err(Error {
                kind: ErrorKind::IntroHasNoVerses,
                at: raw.at,
            });
        }
        let verse = match verse {
            Num::At(n) => n,
            Num::End => self
                .scheme_given()
                .and_then(|scheme| scheme.last_verse(raw.book, chapter))
                .ok_or(Error {
                    kind: ErrorKind::EndNeedsScheme,
                    at: raw.at,
                })?,
        };
        if let Some(scheme) = self.context.scheme
            && scheme
                .last_verse(raw.book, chapter)
                .is_some_and(|last| verse > last)
        {
            return Err(Error {
                kind: ErrorKind::VerseOutOfRange,
                at: raw.at,
            });
        }
        Ok(Pointed::Verse(Point {
            chapter,
            verse,
            segment,
        }))
    }

    /// Only a scheme the caller gave: `end` is never placed by a default.
    fn scheme_given(&self) -> Option<&Scheme> {
        self.context.scheme
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek()
            && is_ws(c)
        {
            self.at += c.len_utf8();
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }

    fn done(&self) -> bool {
        self.at >= self.text.len()
    }

    fn error(&self, kind: ErrorKind) -> Error {
        Error { kind, at: self.at }
    }
}

/// A raw reference placed: a chapter (0 is the introduction, and also what a
/// book-only reference reports, which `address` tells apart), or a verse.
#[derive(Debug, Clone, Copy)]
enum Pointed {
    Chapter(u16),
    Verse(Point),
}

/// The standard's `ws`, bidi marks included.
fn is_ws(c: char) -> bool {
    matches!(c,
        ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200B}' | '\u{200E}' | '\u{200F}'
        | '\u{202A}'..='\u{202F}' | '\u{205F}' | '\u{2066}'..='\u{2069}' | '\u{3000}')
}

#[cfg(test)]
mod tests;
