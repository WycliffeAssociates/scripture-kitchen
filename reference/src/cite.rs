//! Citations: what a person types or reads, as opposed to the strict machine
//! spelling [`parse`](crate::parse) reads.
//!
//! ```text
//! cite("Mateo 5:3-7; 6:1", es)     -> MAT 5:3-7, MAT 6:1          a gateway language's name
//! cite("1john 2:3")                -> 1JN 2:3                     case and spacing folded
//! cite("phil")          navigation -> PHP, assumed: prefix "phil"  four letters and Enter
//! cite("luk 3,1")       navigation -> LUK 3:1, assumed: comma      no `:` anywhere
//! cite("Mat 3,1")       prose      -> MAT 3, MAT 1                a comma is a list
//! cite("Luke intro")               -> LUK 0                       the introduction, in words
//! show(MAT 5:3-7, "Mateo", "introducción") -> "Mateo 5:3-7"
//! ```
//!
//! A port of Sefer's `citation.ts` and `names.ts`, whose cases are
//! `fixtures/cite.json`. Its names come from canon (each requested locale's
//! `\toc` names, English names and aliases, codes) after the project's own,
//! which a caller adds. Two departures, both to match the machine spelling:
//! verse 0 is a chapter's title (`Luke 3:0` is a place, where Sefer refused
//! it), and a one-chapter book's bare number is a verse (`Jude 3`).
//!
//! Lenient, and says so: every guess a reading makes is an [`Assumption`].

use canon::Form;

use crate::{Address, BookKey, Point, Reference, Segment};

/// Which books a word can name, and the words for "the introduction".
#[derive(Debug, Clone)]
pub struct Catalogue {
    /// Canon order (the spec's rows), then books added with [`Catalogue::book`].
    books: Vec<(BookKey, Vec<String>)>,
    intro: Vec<String>,
    /// Books [`Catalogue::books_matching`] offers beyond the 66 and the
    /// deuterocanon: the ones a host listed with [`Catalogue::book`].
    offered: Vec<BookKey>,
    /// Offer every book, peripherals and XXA–XXG included.
    offer_all: bool,
}

impl Catalogue {
    /// Canon's names in `locales` (all three `\toc` forms), English names and
    /// aliases, and each book's code. `intro` and `introduction` mean the
    /// introduction.
    pub fn new(locales: &[&str]) -> Catalogue {
        let books = canon::books(canon::Canon::All)
            .map(|book| {
                let mut names = Vec::new();
                for &locale in locales.iter().chain(["en"].iter()) {
                    for form in [Form::Long, Form::Short, Form::Abbrev] {
                        if let Some(named) = canon::name(book.code, &[locale], form)
                            && (named.locale == locale || locale == "en")
                        {
                            names.push(named.text.to_string());
                        }
                    }
                }
                names.push(book.english.to_string());
                names.extend(book.aliases.iter().map(|alias| alias.to_string()));
                names.push(book.code.to_string());
                let mut folded: Vec<String> = Vec::new();
                for name in names {
                    let name = canon::fold(&name);
                    if !name.is_empty() && !folded.contains(&name) {
                        folded.push(name);
                    }
                }
                (book.code, folded)
            })
            .collect();
        Catalogue {
            books,
            intro: vec!["intro".to_string(), "introduction".to_string()],
            offered: Vec::new(),
            offer_all: false,
        }
    }

    /// Only the book table's English names and aliases, and each code: no
    /// locale's names, so a wasm build that never calls [`Catalogue::new`]
    /// links none of canon's name tables. A host adds the names it has (the
    /// project's own, its heart and UI languages') with [`Catalogue::name`].
    pub fn english() -> Catalogue {
        let books = canon::books(canon::Canon::All)
            .map(|book| {
                let mut folded: Vec<String> = Vec::new();
                let names = std::iter::once(book.english)
                    .chain(book.aliases.iter().copied())
                    .map(str::to_string)
                    .chain(std::iter::once(book.code.to_string()));
                for name in names {
                    let name = canon::fold(&name);
                    if !name.is_empty() && !folded.contains(&name) {
                        folded.push(name);
                    }
                }
                (book.code, folded)
            })
            .collect();
        Catalogue {
            books,
            intro: vec!["intro".to_string(), "introduction".to_string()],
            offered: Vec::new(),
            offer_all: false,
        }
    }

    /// A name the project uses for a book, or a registered abbreviation
    /// (`Mt`, `Matt`). Matched like canon's, folded.
    pub fn name(&mut self, book: BookKey, name: &str) -> &mut Catalogue {
        let name = canon::fold(name);
        if name.is_empty() {
            return self;
        }
        let names = self.names_mut(book);
        if !names.contains(&name) {
            names.insert(0, name);
        }
        self
    }

    /// A book the project holds that should be named and offered even
    /// though it is outside the 66 and the deuterocanon: a peripheral (`FRT`)
    /// or a `\z` book.
    pub fn book(&mut self, book: BookKey) -> &mut Catalogue {
        self.names_mut(book);
        if !self.offered.contains(&book) {
            self.offered.push(book);
        }
        self
    }

    /// Offer every book in [`Catalogue::books_matching`], peripherals and
    /// XXA–XXG included.
    pub fn offer_all(&mut self) -> &mut Catalogue {
        self.offer_all = true;
        self
    }

    /// Whether [`Catalogue::books_matching`] lists `book`: the 66 and the
    /// deuterocanon always, anything else only when listed or asked for.
    fn offers(&self, book: BookKey) -> bool {
        self.offer_all
            || self.offered.contains(&book)
            || canon::books(canon::Canon::WithDeuterocanon).any(|row| row.code == book)
    }

    /// A word for the introduction in the project's or the UI's language.
    pub fn intro_word(&mut self, word: &str) -> &mut Catalogue {
        let word = canon::fold(word);
        if !word.is_empty() && !self.intro.contains(&word) {
            self.intro.push(word);
        }
        self
    }

    fn names_mut(&mut self, book: BookKey) -> &mut Vec<String> {
        let at = match self.books.iter().position(|(code, _)| *code == book) {
            Some(at) => at,
            None => {
                let code = canon::fold(&book.to_string());
                self.books.push((book, vec![code]));
                self.books.len() - 1
            }
        };
        &mut self.books[at].1
    }

    /// Every book a partly typed word could mean, in catalogue order: a name
    /// that starts with it, or has a later word that does ("john" is John and
    /// 1, 2 and 3 John). A filter's answer, so it lists only books a person
    /// would go to: no peripheral or XXA–XXG unless [`Catalogue::book`]
    /// listed it or [`Catalogue::offer_all`] was asked. Reading a citation
    /// still knows them all (`FRT 1` reads).
    pub fn books_matching(&self, word: &str) -> Vec<BookKey> {
        let wanted = canon::fold(word);
        if wanted.is_empty() {
            return Vec::new();
        }
        let later = format!(" {wanted}");
        self.books
            .iter()
            .filter(|(book, _)| self.offers(*book))
            .filter(|(_, names)| {
                names
                    .iter()
                    .any(|name| name.starts_with(&wanted) || name.contains(&later))
            })
            .map(|(book, _)| *book)
            .collect()
    }

    /// The book a word names. Each step is tried across every book before
    /// the next, so an exact name never loses to another book's prefix:
    /// a name or code exactly; then (prefix only) a name by prefix, held books
    /// first, each in canon order; then a code by prefix. `phil` is
    /// Philippians before Philemon.
    fn match_book(&self, word: &str, prefix: bool, held: &[BookKey]) -> Option<(BookKey, bool)> {
        let wanted = canon::fold(word);
        if wanted.is_empty() {
            return None;
        }
        let exact = self
            .books
            .iter()
            .find(|(_, names)| names.contains(&wanted))
            .map(|(book, _)| (*book, false));
        if exact.is_some() || !prefix {
            return exact;
        }
        let ordered = self
            .books
            .iter()
            .filter(|(book, _)| held.contains(book))
            .chain(self.books.iter().filter(|(book, _)| !held.contains(book)));
        let by_name = ordered
            .clone()
            .find(|(_, names)| names.iter().any(|name| name.starts_with(&wanted)));
        let by_code = || {
            ordered
                .clone()
                .find(|(book, _)| canon::fold(&book.to_string()).starts_with(&wanted))
        };
        by_name.or_else(by_code).map(|(book, _)| (*book, true))
    }

    fn is_intro(&self, word: &str) -> bool {
        self.intro.contains(&canon::fold(word))
    }
}

/// How strictly to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grammar {
    /// A go-to box: a book by prefix, and with no `:` or `.` anywhere the
    /// first comma separates chapter from verse (`luk 3,1`).
    Navigation,
    /// A citation in running text: a book only by a whole name, code or
    /// registered abbreviation, and a comma is always a list.
    Prose,
}

/// What a reading took for granted, so a UI can show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assumption {
    /// The word named no book outright and was taken as the first book it
    /// begins: `phil` as PHP.
    Prefix { word: String, book: BookKey },
    /// With no `:` or `.`, the first comma was read as one: `3,1` as `3:1`.
    CommaIsColon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cited {
    pub refs: Vec<Reference>,
    pub assumed: Vec<Assumption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Nothing was written.
    Empty,
    /// The words before the numbers name no book.
    UnknownBook,
    /// Numbers with no book before them, in the first group.
    NoBook,
    /// The numbers don't follow the grammar.
    Malformed,
    /// A chapter 0 that isn't the introduction alone: `LUK 0:1`, `Luke 0-3`.
    Zero,
    /// A range whose end comes before its start.
    Backwards,
}

impl Problem {
    /// Sefer's `CitationProblem` names: `unknown-book`, `no-book`.
    pub fn as_str(self) -> &'static str {
        match self {
            Problem::Empty => "empty",
            Problem::UnknownBook => "unknown-book",
            Problem::NoBook => "no-book",
            Problem::Malformed => "malformed",
            Problem::Zero => "zero",
            Problem::Backwards => "backwards",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiteError {
    pub problem: Problem,
    /// The words that named no book, for [`Problem::UnknownBook`].
    pub word: Option<String>,
}

impl CiteError {
    fn of(problem: Problem) -> CiteError {
        CiteError {
            problem,
            word: None,
        }
    }
}

/// Reads what a person wrote. `held` is the project's books, which win a
/// prefix tie in navigation.
pub fn cite(
    text: &str,
    catalogue: &Catalogue,
    grammar: Grammar,
    held: &[BookKey],
) -> Result<Cited, CiteError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(CiteError::of(Problem::Empty));
    }
    let navigation = grammar == Grammar::Navigation;
    let loose = navigation && !text.contains([':', '.']);
    let mut out = Cited {
        refs: Vec::new(),
        assumed: Vec::new(),
    };
    let mut book: Option<BookKey> = None;
    for group in text.split(';') {
        if group.trim().is_empty() {
            return Err(CiteError::of(Problem::Malformed));
        }
        let (words, refs) = split_group(group);
        let mut intro = false;
        if !words.trim().is_empty() {
            let (named, is_intro, prefix) = read_book(words, catalogue, navigation, held)?;
            if prefix {
                out.assumed.push(Assumption::Prefix {
                    word: words.trim().to_string(),
                    book: named,
                });
            }
            book = Some(named);
            intro = is_intro;
        }
        let Some(book) = book else {
            return Err(CiteError::of(Problem::NoBook));
        };
        let mut tokens = tokenize(refs)?;
        if loose && let Some(comma) = tokens.iter().position(|t| *t == Token::Comma) {
            tokens[comma] = Token::Colon;
            out.assumed.push(Assumption::CommaIsColon);
        }
        if intro {
            if !tokens.is_empty() {
                return Err(CiteError::of(Problem::Malformed));
            }
            out.refs.push(Reference {
                book,
                address: Address::Intro,
            });
        } else if tokens.is_empty() {
            out.refs.push(Reference {
                book,
                address: Address::Book,
            });
        } else {
            read_items(book, &tokens, &mut out.refs)?;
        }
    }
    Ok(out)
}

/// The words before the numbers in the first group — "1 John 3:2" gives
/// "1 John" — for a filter of every book they could mean
/// ([`Catalogue::books_matching`]).
pub fn citation_words(text: &str) -> &str {
    split_group(text.split(';').next().unwrap_or("")).0.trim()
}

/// What a person reads: `Mateo 5:3-7`. The book's name is the caller's (the
/// project's own, or [`canon::name`]'s), and so is the word for the
/// introduction, because both are the project's language.
pub fn show(reference: &Reference, book_name: &str, intro: &str) -> String {
    let code = reference.to_string();
    let rest = match reference.address {
        Address::Book => return book_name.to_string(),
        Address::Intro => intro,
        _ => code.split_once(' ').map_or("", |(_, rest)| rest),
    };
    format!("{book_name} {rest}")
}

/// `(words, numbers)`: the words run from the first letter to the first digit
/// after it; with no letter, all of it is numbers.
fn split_group(group: &str) -> (&str, &str) {
    let Some(letter) = group.find(char::is_alphabetic) else {
        return ("", group);
    };
    match group[letter..].find(|c: char| c.is_ascii_digit()) {
        Some(digit) => group.split_at(letter + digit),
        None => (group, ""),
    }
}

fn read_book(
    words: &str,
    catalogue: &Catalogue,
    navigation: bool,
    held: &[BookKey],
) -> Result<(BookKey, bool, bool), CiteError> {
    let whole = catalogue.match_book(words, navigation, held);
    if let Some((book, _)) = whole
        && catalogue.match_book(words, false, held).is_some()
    {
        return Ok((book, false, false));
    }
    let trimmed = words.trim();
    if let Some((head, last)) = trimmed.rsplit_once(char::is_whitespace)
        && catalogue.is_intro(last)
        && let Some((book, prefix)) = catalogue.match_book(head, navigation, held)
    {
        return Ok((book, true, prefix));
    }
    if let Some((book, prefix)) = whole {
        return Ok((book, false, prefix));
    }
    Err(CiteError {
        problem: Problem::UnknownBook,
        word: Some(trimmed.to_string()),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Number(u16, Option<Segment>),
    Colon,
    Comma,
    Dash,
}

fn tokenize(refs: &str) -> Result<Vec<Token>, CiteError> {
    let malformed = || CiteError::of(Problem::Malformed);
    let mut tokens = Vec::new();
    let mut rest = refs;
    while let Some(head) = rest.chars().next() {
        if head.is_whitespace() {
            rest = &rest[head.len_utf8()..];
            continue;
        }
        if head.is_ascii_digit() {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let value = rest[..digits].parse().map_err(|_| malformed())?;
            let letters = rest[digits..]
                .char_indices()
                .find(|(_, c)| !c.is_alphabetic())
                .map_or(rest.len() - digits, |(at, _)| at);
            let written = &rest[digits..digits + letters];
            // A part is one letter, in either case: `16a`, `16A`.
            let segment = match written {
                "" => None,
                _ => Some(Segment::parse(written.to_lowercase().as_bytes()).ok_or_else(malformed)?),
            };
            tokens.push(Token::Number(value, segment));
            rest = &rest[digits + letters..];
            continue;
        }
        tokens.push(match head {
            ':' | '.' => Token::Colon,
            ',' => Token::Comma,
            '-' | '–' | '—' => Token::Dash,
            _ => return Err(malformed()),
        });
        rest = &rest[head.len_utf8()..];
    }
    Ok(tokens)
}

/// A number as read: with a chapter (`3:16`), or alone (a chapter, or a
/// verse once a chapter is in context).
#[derive(Debug, Clone, Copy)]
struct Ref {
    chapter: Option<u16>,
    number: u16,
    segment: Option<Segment>,
}

fn read_ref(
    tokens: &[Token],
    at: usize,
    context: Option<u16>,
    book: BookKey,
) -> Result<(Ref, usize), CiteError> {
    let malformed = || CiteError::of(Problem::Malformed);
    let Some(Token::Number(first, first_segment)) = tokens.get(at).copied() else {
        return Err(malformed());
    };
    if tokens.get(at + 1) == Some(&Token::Colon) {
        let Some(Token::Number(verse, segment)) = tokens.get(at + 2).copied() else {
            return Err(malformed());
        };
        if first_segment.is_some() {
            return Err(malformed());
        }
        let read = Ref {
            chapter: Some(first),
            number: verse,
            segment,
        };
        return Ok((read, at + 3));
    }
    // In context a number is a verse; so is a one-chapter book's bare number.
    let context = context.or_else(|| (one_chapter(book) && first != 0).then_some(1));
    if let Some(chapter) = context {
        let read = Ref {
            chapter: Some(chapter),
            number: first,
            segment: first_segment,
        };
        return Ok((read, at + 1));
    }
    if first_segment.is_some() {
        return Err(malformed());
    }
    let read = Ref {
        chapter: None,
        number: first,
        segment: None,
    };
    Ok((read, at + 1))
}

fn one_chapter(book: BookKey) -> bool {
    canon::scheme("eng").and_then(|scheme| scheme.chapters(book)) == Some(1)
}

fn read_items(book: BookKey, tokens: &[Token], out: &mut Vec<Reference>) -> Result<(), CiteError> {
    let mut at = 0;
    let mut context: Option<u16> = None;
    while at < tokens.len() {
        let (start, next) = read_ref(tokens, at, context, book)?;
        at = next;
        if start.chapter.is_some() {
            context = start.chapter;
        }
        let mut end = None;
        if tokens.get(at) == Some(&Token::Dash) {
            let (read, next) = read_ref(tokens, at + 1, context, book)?;
            at = next;
            if read.chapter.is_some() {
                context = read.chapter;
            }
            end = Some(read);
        }
        out.push(Reference {
            book,
            address: item_address(start, end)?,
        });
        if at == tokens.len() {
            break;
        }
        if tokens.get(at) != Some(&Token::Comma) || at + 1 == tokens.len() {
            return Err(CiteError::of(Problem::Malformed));
        }
        at += 1;
    }
    Ok(())
}

fn item_address(start: Ref, end: Option<Ref>) -> Result<Address, CiteError> {
    if start.chapter.is_none() && start.number == 0 && end.is_none() {
        return Ok(Address::Intro);
    }
    let zero = |r: &Ref| r.chapter == Some(0) || (r.chapter.is_none() && r.number == 0);
    if zero(&start) || end.as_ref().is_some_and(zero) {
        return Err(CiteError::of(Problem::Zero));
    }
    let point = |r: &Ref| match r.chapter {
        Some(chapter) => Point {
            chapter,
            verse: r.number,
            segment: r.segment,
        },
        // A whole chapter, as the start of a range to a verse: `Mat 2-3:4`.
        None => Point::verse(r.number, 1),
    };
    let Some(end) = end else {
        return Ok(match start.chapter {
            None => Address::Chapters {
                from: start.number,
                to: start.number,
            },
            Some(_) => {
                let at = point(&start);
                Address::Verses { from: at, to: at }
            }
        });
    };
    if start.chapter.is_none() && end.chapter.is_none() {
        if end.number < start.number {
            return Err(CiteError::of(Problem::Backwards));
        }
        return Ok(Address::Chapters {
            from: start.number,
            to: end.number,
        });
    }
    let (from, to) = (point(&start), point(&end));
    if from.runs_back_to(&to) {
        return Err(CiteError::of(Problem::Backwards));
    }
    Ok(Address::Verses { from, to })
}

#[cfg(test)]
mod tests;
