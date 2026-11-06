//! srs — the USFM Scripture Referencing Standard, in full.
//!
//! ```text
//! parse("MAT 2:1!3")              third word of Matthew 2:1
//! parse("MAT 2:1!3-2!4")          word 3 of 2:1 to word 4 of verse 2
//! parse("en+ulb.ROM 1:1")         in one translation
//! parse("JON 1:13!f!5")           the fifth word of 1:13's first footnote
//! parse("JON 1:4!s1!2")           the second word of the heading 1:4 opens
//! parse("MAT 1:1!5+6")            the sixth character (NFD) of word 5
//! parse("GEN 1:1!8@strong")       the strong attribute of word 8's element
//! parse("NEH t:Israel/6/1")       a named reference: a table cell by @aid
//! ```
//!
//! The standard is usfm-bible/tcdocs `docs/Scripture Referencing Standard.md`,
//! version 1.0. Its basic reference (book, chapter, verse, part, ranges,
//! lists) is the `reference` crate's machine spelling; this crate reads
//! everything else too: a translation prefix, and a chain of REFINEMENTS
//! below the verse, each narrowing the one before ([`Step`]).
//!
//! Grammar only. Where a reference is in one text (which bytes are word 3,
//! which footnote is `!f[2]`) needs the text, and is galley's `srs` module.
//! [`SrsRange::reference`] is the basic reference a range lies within.
//!
//! Context follows the standard: a reference without a book is read against
//! the reference before it. A number with `:` is a chapter and verse; one
//! followed by `+` is a word; one followed by `!` or `@` is a verse; any other
//! is at the level of the reference before (`MAT 2:1!3-5` is words 3 to 5).

use core::fmt;

pub use reference::{Address, BookKey, Point, Reference, Segment};

mod parse;

pub use parse::{Error, ErrorKind, parse};

/// A translation, and optionally one product of it: `en+ulb`, `en+niv+text`,
/// `en-t-wsg`. Kept as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Translation {
    /// A BCP 47 language tag: `en`, `wsg-gong`, `en-t-wsg`.
    pub language: String,
    pub translation: Option<String>,
    pub product: Option<String>,
}

/// A chapter, verse, word or character number, or `end`, the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Index {
    At(u32),
    End,
}

/// Where in a book a reference starts refining.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    /// The whole book: `GEN`.
    Book,
    /// A chapter (`0` is the introduction): `MAT 2`, `MRK 0`.
    Chapter(u32),
    /// A verse, with its one-letter part: `LUK 11:2b`, `PSA 7:0`, `ROM 9:end`.
    Verse {
        chapter: u32,
        verse: Index,
        segment: Option<Segment>,
    },
    /// A named reference: `NEH t:Israel/6/1`, `MRK k:baptism`.
    Named { namespace: String, value: String },
}

/// One refinement below the place, each narrowing the one before.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Step {
    /// `!3`, `!end`, `!5+6`, `!2+0`, `!2+3+` (the gap after character 3).
    Word { word: Index, char: Option<CharStep> },
    /// `!f`, `!s1`, `!q1[2]`, `!s[-1]`: the nth element of a marker.
    Element { marker: String, index: Option<i32> },
    /// `@strong`, `@caller`: an attribute of the element in hand or one
    /// containing it.
    Attribute(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharStep {
    pub at: Index,
    /// A trailing `+`: the gap after the character rather than the character.
    pub gap: bool,
}

/// One reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SrsRef {
    pub translation: Option<Translation>,
    pub book: BookKey,
    pub place: Place,
    pub steps: Vec<Step>,
}

/// A reference, or a range between two. `last` is whole: whatever it left to
/// context when written is filled in from `first`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SrsRange {
    pub first: SrsRef,
    pub last: Option<SrsRef>,
}

impl SrsRange {
    /// The basic reference this range lies within: its translation and
    /// refinements dropped (`MAT 2:1!3-5` is in `MAT 2:1`). `None` for a named
    /// reference, a range across books, or `end` (which only a versification
    /// can place: give one to `reference::parse_with` instead).
    pub fn reference(&self) -> Option<Reference> {
        let first = &self.first;
        let last = self.last.as_ref().unwrap_or(first);
        if last.book != first.book {
            return None;
        }
        let narrow = |n: u32| u16::try_from(n).ok();
        let point = |place: &Place| -> Option<Option<Point>> {
            Some(match place {
                Place::Verse {
                    chapter,
                    verse: Index::At(verse),
                    segment,
                } => Some(Point {
                    chapter: narrow(*chapter)?,
                    verse: narrow(*verse)?,
                    segment: *segment,
                }),
                Place::Verse { .. } | Place::Named { .. } => return None,
                Place::Book | Place::Chapter(_) => None,
            })
        };
        let address = match (&first.place, &last.place) {
            (Place::Book, Place::Book) => Address::Book,
            (Place::Chapter(0), Place::Chapter(0)) => Address::Intro,
            (Place::Chapter(a), Place::Chapter(b)) if *a > 0 => Address::Chapters {
                from: narrow(*a)?,
                to: narrow(*b)?,
            },
            _ => {
                let from = point(&first.place)??;
                let to = point(&last.place)??;
                Address::Verses { from, to }
            }
        };
        Some(Reference {
            book: first.book,
            address,
        })
    }
}

/// A list, `; `-separated: each range's first reference whole, its last as
/// short as context allows (`MAT 2:1!3-5`, `GEN 7:8!2-12!3`).
pub fn format(ranges: &[SrsRange]) -> String {
    let mut out = String::new();
    for (i, range) in ranges.iter().enumerate() {
        if i > 0 {
            out.push_str("; ");
        }
        out.push_str(&range.to_string());
    }
    out
}

impl fmt::Display for SrsRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.first)?;
        let Some(last) = &self.last else {
            return Ok(());
        };
        f.write_str("-")?;
        let first = &self.first;
        if last.book != first.book || last.translation != first.translation {
            return write!(f, "{last}");
        }
        match (&first.place, &last.place) {
            (Place::Chapter(_), Place::Chapter(b)) if first.steps.is_empty() => {
                write!(f, "{b}")?;
            }
            (
                Place::Verse {
                    chapter: c1,
                    verse: v1,
                    segment: s1,
                },
                Place::Verse {
                    chapter: c2,
                    verse: v2,
                    segment: s2,
                },
            ) if c1 == c2 => {
                // Same verse, and the same steps but the last word: only the
                // word, or only the character when it is the same word
                // (`!3-5`, `!f!2-4`, `!5+2-6`).
                if v1 == v2
                    && s1 == s2
                    && let (
                        [head1 @ .., Step::Word { word: w1, char: c1 }],
                        [head2 @ .., Step::Word { word: w2, char: c2 }],
                    ) = (first.steps.as_slice(), last.steps.as_slice())
                    && head1 == head2
                {
                    if w1 == w2
                        && let (Some(_), Some(c2)) = (c1, c2)
                    {
                        return char_step(f, *c2);
                    }
                    write!(f, "{w2}")?;
                    if let Some(c2) = c2 {
                        f.write_str("+")?;
                        char_step(f, *c2)?;
                    }
                    return Ok(());
                }
                // A bare verse after refinements would read as a word: give
                // its chapter.
                if !first.steps.is_empty() && last.steps.is_empty() {
                    write!(f, "{c2}:")?;
                }
                verse(f, *v2, *s2)?;
            }
            (
                Place::Verse { .. },
                Place::Verse {
                    chapter,
                    verse: v,
                    segment,
                },
            ) => {
                write!(f, "{chapter}:")?;
                verse(f, *v, *segment)?;
            }
            _ => return write!(f, "{last}"),
        }
        steps(f, &last.steps)
    }
}

fn char_step(f: &mut fmt::Formatter<'_>, c: CharStep) -> fmt::Result {
    write!(f, "{}", c.at)?;
    if c.gap {
        f.write_str("+")?;
    }
    Ok(())
}

fn steps(f: &mut fmt::Formatter<'_>, list: &[Step]) -> fmt::Result {
    for step in list {
        write!(f, "{step}")?;
    }
    Ok(())
}

impl fmt::Display for SrsRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(t) = &self.translation {
            write!(f, "{t}.")?;
        }
        write!(f, "{}", self.book)?;
        match &self.place {
            Place::Book => {}
            Place::Chapter(c) => write!(f, " {c}")?,
            Place::Verse {
                chapter,
                verse: v,
                segment,
            } => {
                write!(f, " {chapter}:")?;
                verse(f, *v, *segment)?;
            }
            Place::Named { namespace, value } => write!(f, " {namespace}:{value}")?,
        }
        steps(f, &self.steps)
    }
}

impl fmt::Display for Translation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.language)?;
        if let Some(t) = &self.translation {
            write!(f, "+{t}")?;
            if let Some(p) = &self.product {
                write!(f, "+{p}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Index::At(n) => write!(f, "{n}"),
            Index::End => f.write_str("end"),
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Word { word, char } => {
                write!(f, "!{word}")?;
                if let Some(c) = char {
                    f.write_str("+")?;
                    char_step(f, *c)?;
                }
                Ok(())
            }
            Step::Element { marker, index } => {
                write!(f, "!{marker}")?;
                match index {
                    Some(n) => write!(f, "[{n}]"),
                    None => Ok(()),
                }
            }
            Step::Attribute(name) => write!(f, "@{name}"),
        }
    }
}

fn verse(f: &mut fmt::Formatter<'_>, verse: Index, segment: Option<Segment>) -> fmt::Result {
    write!(f, "{verse}")?;
    match segment {
        Some(s) => write!(f, "{s}"),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
