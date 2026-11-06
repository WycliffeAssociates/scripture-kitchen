//! Reading the standard's grammar (its "Conclusion" section). Strict, as the
//! `reference` crate is: anything else is an error with its byte offset.

use core::fmt;

use crate::{BookKey, CharStep, Index, Place, Segment, SrsRange, SrsRef, Step, Translation};

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
    /// A `!`, `@` or `+` with nothing it can name after it.
    ExpectedStep,
    /// A `-` with nothing after it.
    RangeEnd,
    /// A range in one book whose end comes before its start.
    Backwards,
    /// Something the grammar has no place for.
    Unexpected,
}

impl ErrorKind {
    /// A stable kebab-case name, for a wire or a log.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Empty => "empty",
            ErrorKind::ExpectedBook => "expected-book",
            ErrorKind::UnknownBook => "unknown-book",
            ErrorKind::NoReferenceBefore => "no-reference-before",
            ErrorKind::ExpectedVerse => "expected-verse",
            ErrorKind::ExpectedStep => "expected-step",
            ErrorKind::RangeEnd => "range-end",
            ErrorKind::Backwards => "backwards",
            ErrorKind::Unexpected => "unexpected",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.kind.as_str(), self.at)
    }
}

impl std::error::Error for Error {}

/// A reference list in the standard's spelling.
pub fn parse(text: &str) -> Result<Vec<SrsRange>, Error> {
    let mut reader = Reader { text, at: 0 };
    reader.list()
}

/// How deep a reference goes, for reading the next one against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Book,
    Chapter,
    Verse,
    Word,
    Char,
}

fn level(r: &SrsRef) -> Level {
    for step in r.steps.iter().rev() {
        if let Step::Word { char, .. } = step {
            return if char.is_some() {
                Level::Char
            } else {
                Level::Word
            };
        }
    }
    match r.place {
        Place::Book => Level::Book,
        Place::Chapter(_) => Level::Chapter,
        Place::Verse { .. } | Place::Named { .. } => Level::Verse,
    }
}

struct Reader<'t> {
    text: &'t str,
    at: usize,
}

impl Reader<'_> {
    fn list(&mut self) -> Result<Vec<SrsRange>, Error> {
        let mut out: Vec<SrsRange> = Vec::new();
        self.skip_ws();
        loop {
            if self.done() {
                return Err(self.error(ErrorKind::Empty));
            }
            let previous = out
                .last()
                .map(|r| r.last.as_ref().unwrap_or(&r.first).clone());
            out.push(self.range(previous.as_ref())?);
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

    fn range(&mut self, previous: Option<&SrsRef>) -> Result<SrsRange, Error> {
        let first = self.reference(previous)?;
        let save = self.at;
        self.skip_ws();
        if self.peek() != Some('-') {
            self.at = save;
            return Ok(SrsRange { first, last: None });
        }
        self.at += 1;
        self.skip_ws();
        if matches!(self.peek(), None | Some(';' | ',')) {
            return Err(self.error(ErrorKind::RangeEnd));
        }
        let end_at = self.at;
        let last = self.reference(Some(&first))?;
        if runs_back(&first, &last) {
            return Err(Error {
                kind: ErrorKind::Backwards,
                at: end_at,
            });
        }
        Ok(SrsRange {
            first,
            last: Some(last),
        })
    }

    fn reference(&mut self, previous: Option<&SrsRef>) -> Result<SrsRef, Error> {
        let at = self.at;
        let translation = self.translation()?;
        let mut r = if let Some(code) = self.book_code() {
            self.full(code, at, translation)?
        } else if translation.is_some() {
            return Err(self.error(ErrorKind::ExpectedBook));
        } else {
            self.contextual(previous)?
        };
        self.steps(&mut r.steps)?;
        Ok(r)
    }

    /// `Langtag ("+" Transcode ("+" Productid)?)? booksep`, when a book code
    /// follows: `en+ulb.`, `en-t-wsg:`.
    fn translation(&mut self) -> Result<Option<Translation>, Error> {
        let bytes = &self.text.as_bytes()[self.at..];
        let id = bytes
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'_'))
            .count();
        let starts_lang = bytes.first().is_some_and(u8::is_ascii_alphabetic);
        let code_follows = bytes.get(id + 1..id + 4).is_some_and(is_code)
            && !bytes.get(id + 4).is_some_and(u8::is_ascii_alphanumeric);
        if !(starts_lang && matches!(bytes.get(id), Some(b'.' | b':')) && code_follows) {
            return Ok(None);
        }
        let written = &self.text[self.at..self.at + id];
        let mut parts = written.split('+');
        let language = parts.next().unwrap_or_default().to_string();
        let translation = parts.next().map(str::to_string);
        let product = parts.next().map(str::to_string);
        if parts.next().is_some()
            || language.is_empty()
            || translation.as_deref() == Some("")
            || product.as_deref() == Some("")
        {
            return Err(self.error(ErrorKind::Unexpected));
        }
        self.at += id + 1;
        Ok(Some(Translation {
            language,
            translation,
            product,
        }))
    }

    /// `BookCode (ws+ (Chapter (chaptersep Verse)? | Namespace chaptersep Nameval))?`
    fn full(
        &mut self,
        code: [u8; 3],
        at: usize,
        translation: Option<Translation>,
    ) -> Result<SrsRef, Error> {
        let book = BookKey::new(code);
        if canon::book(book).is_none() {
            return Err(Error {
                kind: ErrorKind::UnknownBook,
                at,
            });
        }
        let mut r = SrsRef {
            translation,
            book,
            place: Place::Book,
            steps: Vec::new(),
        };
        let save = self.at;
        self.skip_ws();
        if self.at == save {
            return Ok(r);
        }
        if let Some((namespace, value)) = self.named()? {
            r.place = Place::Named { namespace, value };
            return Ok(r);
        }
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at = save;
            return Ok(r);
        }
        let chapter = self.number()?;
        r.place = if self.chapter_sep() {
            let (verse, segment) = self.verse()?;
            Place::Verse {
                chapter,
                verse,
                segment,
            }
        } else if one_chapter(book) && chapter != 0 && !self.peek().is_some_and(|c| c == '!') {
            Place::Verse {
                chapter: 1,
                verse: Index::At(chapter),
                segment: self.segment(),
            }
        } else {
            Place::Chapter(chapter)
        };
        Ok(r)
    }

    /// `Namespace chaptersep Nameval`: letters, `:` or `.`, then a name.
    fn named(&mut self) -> Result<Option<(String, String)>, Error> {
        let bytes = &self.text.as_bytes()[self.at..];
        let letters = bytes.iter().take_while(|b| b.is_ascii_alphabetic()).count();
        if letters == 0 || !matches!(bytes.get(letters), Some(b':' | b'.')) {
            return Ok(None);
        }
        let namespace = self.text[self.at..self.at + letters].to_string();
        self.at += letters + 1;
        let start = self.at;
        let rest = &self.text[start..];
        let len = rest
            .char_indices()
            .find(|(i, c)| !name_char(*c, *i == 0))
            .map_or(rest.len(), |(i, _)| i);
        if len == 0 {
            return Err(self.error(ErrorKind::ExpectedStep));
        }
        self.at += len;
        Ok(Some((namespace, rest[..len].to_string())))
    }

    /// A reference without a book, read against `previous`.
    fn contextual(&mut self, previous: Option<&SrsRef>) -> Result<SrsRef, Error> {
        let starts_number = self.peek().is_some_and(|c| c.is_ascii_digit()) || self.at_end_word();
        let starts_part = self.lone_letter();
        if !starts_number && !starts_part {
            return Err(self.error(ErrorKind::ExpectedBook));
        }
        let Some(previous) = previous else {
            return Err(self.error(ErrorKind::NoReferenceBefore));
        };
        let mut r = SrsRef {
            translation: previous.translation.clone(),
            book: previous.book,
            place: previous.place.clone(),
            steps: Vec::new(),
        };
        let prev_chapter = match previous.place {
            Place::Chapter(c) | Place::Verse { chapter: c, .. } => Some(c),
            _ => None,
        };
        if starts_part {
            // A bare part is a part of the verse before it: `2a-b`.
            let Place::Verse { chapter, verse, .. } = previous.place else {
                return Err(self.error(ErrorKind::Unexpected));
            };
            r.place = Place::Verse {
                chapter,
                verse,
                segment: self.segment(),
            };
            return Ok(r);
        }
        let number = self.index()?;
        if self.chapter_sep() {
            let Index::At(chapter) = number else {
                return Err(self.error(ErrorKind::Unexpected));
            };
            let (verse, segment) = self.verse()?;
            r.place = Place::Verse {
                chapter,
                verse,
                segment,
            };
            return Ok(r);
        }
        if self.peek() == Some('+') {
            // A word, with its character after.
            r.steps = words_before(previous);
            self.at += 1;
            let char = self.char_step()?;
            r.steps.push(Step::Word {
                word: number,
                char: Some(char),
            });
            return Ok(r);
        }
        let as_verse = |r: &mut SrsRef, segment| -> Result<(), ErrorKind> {
            let chapter = prev_chapter.ok_or(ErrorKind::Unexpected)?;
            r.place = Place::Verse {
                chapter,
                verse: number,
                segment,
            };
            Ok(())
        };
        // A verse part, or a `!` or `@` after the number, makes it a verse:
        // `2!4`, `2a!4`, and after a verse `1b`.
        if prev_chapter.is_some() {
            let save = self.at;
            let segment = if level(previous) >= Level::Verse {
                self.segment()
            } else {
                None
            };
            if segment.is_some() || matches!(self.peek(), Some('!' | '@')) {
                as_verse(&mut r, segment).map_err(|kind| self.error(kind))?;
                return Ok(r);
            }
            self.at = save;
        }
        match level(previous) {
            Level::Book | Level::Chapter => {
                let one = one_chapter(previous.book) && number != Index::At(0);
                if one {
                    r.place = Place::Verse {
                        chapter: 1,
                        verse: number,
                        segment: self.segment(),
                    };
                } else {
                    let Index::At(chapter) = number else {
                        return Err(self.error(ErrorKind::Unexpected));
                    };
                    r.place = Place::Chapter(chapter);
                }
            }
            Level::Verse => {
                let segment = self.segment();
                as_verse(&mut r, segment).map_err(|kind| self.error(kind))?;
            }
            Level::Word => {
                r.steps = words_before(previous);
                r.steps.push(Step::Word {
                    word: number,
                    char: None,
                });
            }
            Level::Char => {
                let mut steps = previous.steps.clone();
                let Some(Step::Word {
                    char: Some(char), ..
                }) = steps
                    .iter_mut()
                    .rev()
                    .find(|s| matches!(s, Step::Word { .. }))
                else {
                    return Err(self.error(ErrorKind::Unexpected));
                };
                let gap = self.peek() == Some('+') && {
                    self.at += 1;
                    true
                };
                *char = CharStep { at: number, gap };
                let cut = steps
                    .iter()
                    .rposition(|s| matches!(s, Step::Word { .. }))
                    .map_or(steps.len(), |at| at + 1);
                steps.truncate(cut);
                r.steps = steps;
            }
        }
        Ok(r)
    }

    /// `(Wordref | Mrkref)*`: `!3`, `!5+6`, `!f[2]`, `@strong`, chained.
    fn steps(&mut self, steps: &mut Vec<Step>) -> Result<(), Error> {
        loop {
            match self.peek() {
                Some('!') => {
                    self.at += 1;
                    if self.peek().is_some_and(|c| c.is_ascii_digit()) || self.at_end_word() {
                        let word = self.index()?;
                        let char = if self.peek() == Some('+') {
                            self.at += 1;
                            Some(self.char_step()?)
                        } else {
                            None
                        };
                        steps.push(Step::Word { word, char });
                    } else {
                        let marker = self.id()?;
                        let index = self.marker_index()?;
                        steps.push(Step::Element { marker, index });
                    }
                }
                Some('@') => {
                    self.at += 1;
                    steps.push(Step::Attribute(self.id()?));
                }
                _ => return Ok(()),
            }
        }
    }

    /// `Charref = (digits | "end") "+"?`
    fn char_step(&mut self) -> Result<CharStep, Error> {
        if !(self.peek().is_some_and(|c| c.is_ascii_digit()) || self.at_end_word()) {
            return Err(self.error(ErrorKind::ExpectedStep));
        }
        let at = self.index()?;
        let gap = self.peek() == Some('+') && {
            self.at += 1;
            true
        };
        Ok(CharStep { at, gap })
    }

    /// `"[" digits "]"`, negative counting back (`s[-1]`).
    fn marker_index(&mut self) -> Result<Option<i32>, Error> {
        if self.peek() != Some('[') {
            return Ok(None);
        }
        let start = self.at;
        let rest = &self.text[start + 1..];
        let Some(close) = rest.find(']') else {
            return Err(self.error(ErrorKind::Unexpected));
        };
        let n: i32 = rest[..close].parse().map_err(|_| Error {
            kind: ErrorKind::Unexpected,
            at: start,
        })?;
        if n == 0 {
            return Err(Error {
                kind: ErrorKind::Unexpected,
                at: start,
            });
        }
        self.at = start + 1 + close + 1;
        Ok(Some(n))
    }

    /// `id = idinit idmid* idfinal?`: a marker or attribute name.
    fn id(&mut self) -> Result<String, Error> {
        let rest = &self.text.as_bytes()[self.at..];
        let starts = rest
            .first()
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_');
        if !starts {
            return Err(self.error(ErrorKind::ExpectedStep));
        }
        let len = rest
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_' || **b == b'-')
            .count();
        // A hyphen ends an id when a range follows: `!s-2`.
        let len = match rest[..len].iter().position(|b| *b == b'-') {
            Some(dash) if !rest[dash + 1..len].iter().any(u8::is_ascii_alphabetic) => dash,
            _ => len,
        };
        let id = self.text[self.at..self.at + len].to_string();
        self.at += len;
        Ok(id)
    }

    /// `Verse = digits Subverse? | Subverse | "end"`
    fn verse(&mut self) -> Result<(Index, Option<Segment>), Error> {
        if !(self.peek().is_some_and(|c| c.is_ascii_digit()) || self.at_end_word()) {
            return Err(self.error(ErrorKind::ExpectedVerse));
        }
        let verse = self.index()?;
        let segment = match verse {
            Index::At(_) => self.segment(),
            Index::End => None,
        };
        Ok((verse, segment))
    }

    fn index(&mut self) -> Result<Index, Error> {
        if self.at_end_word() {
            self.at += 3;
            return Ok(Index::End);
        }
        self.number().map(Index::At)
    }

    fn number(&mut self) -> Result<u32, Error> {
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

    /// One lowercase letter after a verse number, not starting a word.
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

    fn book_code(&mut self) -> Option<[u8; 3]> {
        let bytes = &self.text.as_bytes()[self.at..];
        let code: [u8; 3] = bytes.get(..3)?.try_into().ok()?;
        let ends = !bytes.get(3).is_some_and(u8::is_ascii_alphanumeric);
        (is_code(&code[..]) && ends).then(|| {
            self.at += 3;
            code
        })
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

/// The steps of `r` before its last word: what a word in context replaces.
fn words_before(r: &SrsRef) -> Vec<Step> {
    let cut = r
        .steps
        .iter()
        .rposition(|s| matches!(s, Step::Word { .. }))
        .unwrap_or(r.steps.len());
    r.steps[..cut].to_vec()
}

/// A range in one book that ends before it starts: verses compared where both
/// are numbered, then words where both are the same verse's.
fn runs_back(first: &SrsRef, last: &SrsRef) -> bool {
    if first.book != last.book || first.translation != last.translation {
        return false;
    }
    let at = |p: &Place| match p {
        Place::Chapter(c) => Some((*c, None)),
        Place::Verse {
            chapter,
            verse: Index::At(v),
            ..
        } => Some((*chapter, Some(*v))),
        _ => None,
    };
    let (Some((c1, v1)), Some((c2, v2))) = (at(&first.place), at(&last.place)) else {
        return false;
    };
    if c2 != c1 {
        return c2 < c1;
    }
    match (v1, v2) {
        (Some(v1), Some(v2)) if v2 != v1 => v2 < v1,
        (Some(_), Some(_)) => {
            let word = |r: &SrsRef| match r.steps.as_slice() {
                [
                    Step::Word {
                        word: Index::At(w), ..
                    },
                ] => Some(*w),
                _ => None,
            };
            matches!((word(first), word(last)), (Some(a), Some(b)) if b < a)
        }
        _ => false,
    }
}

fn is_code(code: &[u8]) -> bool {
    code.iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && code.iter().any(u8::is_ascii_uppercase)
}

fn one_chapter(book: BookKey) -> bool {
    canon::scheme("eng").and_then(|scheme| scheme.chapters(book)) == Some(1)
}

/// `NameInit` / `NameChar`: letters, digits, marks and joiners, none of the
/// grammar's own punctuation. `/` is allowed for the standard's own `t:`
/// example (`NEH t:Israel/6/1`).
fn name_char(c: char, first: bool) -> bool {
    if c.is_whitespace() || is_ws(c) || matches!(c, '!' | '@' | ';' | ',' | '-' | '[' | ']') {
        return false;
    }
    if c.is_ascii() {
        return c.is_ascii_alphabetic() || c == '_' || (!first && (c.is_ascii_digit() || c == '/'));
    }
    !first || c.is_alphabetic()
}

/// The standard's `ws`, bidi marks included.
fn is_ws(c: char) -> bool {
    matches!(c,
        ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200B}' | '\u{200E}' | '\u{200F}'
        | '\u{202A}'..='\u{202F}' | '\u{205F}' | '\u{2066}'..='\u{2069}' | '\u{3000}')
}
