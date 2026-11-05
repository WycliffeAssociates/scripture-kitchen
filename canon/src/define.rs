//! Schemes a project defines: a reviewed `.vrs` import, or any numbering
//! canon doesn't ship.
//!
//! ```text
//! let mut b = SchemeBuilder::new("my-project")?;
//! b.book(GEN, vec![31, 25, …])?;            // maxVerses
//! b.map("GEN 31:55", "GEN 32:1")?;          // a mappedVerses row, toward org
//! b.exclude("MAT 17:21")?;                  // an excludedVerses entry
//! let scheme = b.build()?;                  // a Scheme like any built-in one
//! ```
//!
//! canon reads no JSON: a caller with a parser (galley) walks the Copenhagen
//! format's three sections and hands each entry here, as the strings the
//! format writes. Everything is checked as strictly as canon's own data, and
//! without its errata: a row that pairs ranges of different lengths, a part
//! on the scheme's side, a book outside the USFM table or a verse past the
//! scheme's own counts is refused, naming the entry.

use core::fmt;

use mise::books::{BookKey, canonical_rank};

use crate::map::{Row, Shape, Verse};
use crate::scheme::{Defined, Scheme};

/// Why a defined scheme was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineError(String);

impl fmt::Display for DefineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DefineError {}

fn refuse<T>(message: impl Into<String>) -> Result<T, DefineError> {
    Err(DefineError(message.into()))
}

/// Collects a scheme's entries, then checks them together in [`build`].
///
/// [`build`]: SchemeBuilder::build
#[derive(Debug)]
pub struct SchemeBuilder {
    name: String,
    books: Vec<(BookKey, Vec<u16>)>,
    excluded: Vec<(String, BookKey, u16, u16)>,
    rows: Vec<(String, String, Row)>,
}

impl SchemeBuilder {
    /// A new scheme. The name may not be a built-in one (`"ulb"`), so a
    /// name always means one thing.
    pub fn new(name: &str) -> Result<SchemeBuilder, DefineError> {
        if name.trim().is_empty() {
            return refuse("a scheme needs a name");
        }
        if crate::scheme(name).is_some() {
            return refuse(format!("{name:?} is a built-in scheme's name"));
        }
        Ok(SchemeBuilder {
            name: name.to_owned(),
            books: Vec::new(),
            excluded: Vec::new(),
            rows: Vec::new(),
        })
    }

    /// One book's last verse per chapter, chapter 1 first (`maxVerses`).
    pub fn book(&mut self, book: BookKey, counts: Vec<u16>) -> Result<&mut Self, DefineError> {
        if crate::book(book).is_none() {
            return refuse(format!("{book} is not a USFM book identifier"));
        }
        if self.books.iter().any(|(seen, _)| *seen == book) {
            return refuse(format!("{book} is given twice"));
        }
        if counts.is_empty() {
            return refuse(format!("{book} has no chapters"));
        }
        if let Some(at) = counts.iter().position(|&n| n == 0) {
            return refuse(format!("{book} {} has no verses", at + 1));
        }
        self.books.push((book, counts));
        Ok(self)
    }

    /// A verse the scheme omits on purpose (`excludedVerses`), as `"MAT 17:21"`.
    pub fn exclude(&mut self, reference: &str) -> Result<&mut Self, DefineError> {
        let (book, chapter, first, last, part) = range(reference)
            .ok_or_else(|| DefineError(format!("{reference:?} is not \"BBB c:v\"")))?;
        if first != last || part.is_some() {
            return refuse(format!("{reference:?}: exclude one whole verse at a time"));
        }
        self.excluded
            .push((reference.to_owned(), book, chapter, first));
        Ok(self)
    }

    /// One `mappedVerses` row, from this scheme toward `org`:
    /// `("GEN 31:55", "GEN 32:1")`, `("PSA 3:0-8", "PSA 3:1-9")`,
    /// `("PSA 89:0-1", "PSA 90:0")` (a merge) or `("PSA 89:6", "PSA 90:5-6")`
    /// (a split). A part letter is allowed on a single `org` verse only.
    pub fn map(&mut self, from: &str, to: &str) -> Result<&mut Self, DefineError> {
        let bad = |what: &str| DefineError(format!("{from:?} → {to:?}: {what}"));
        let (fb, fc, ff, fl, fp) = range(from).ok_or_else(|| bad("not \"BBB c:v[-v]\""))?;
        let (tb, tc, tf, tl, tp) = range(to).ok_or_else(|| bad("not \"BBB c:v[-v]\""))?;
        if fp.is_some() {
            return Err(bad("a part on the scheme's side"));
        }
        let (from_len, to_len) = (fl - ff + 1, tl - tf + 1);
        let (len, shape) = match (from_len, to_len) {
            (a, b) if a == b => (a, Shape::Parallel),
            (a, 1) => (a, Shape::Merge),
            (1, b) => (b, Shape::Split),
            (a, b) => return Err(bad(&format!("{a} verses onto {b}"))),
        };
        if tp.is_some() && to_len != 1 {
            return Err(bad("a part on a range"));
        }
        let row = Row {
            from: Verse::new(fb, fc, ff),
            to: Verse {
                part: tp,
                ..Verse::new(tb, tc, tf)
            },
            len,
            shape,
        };
        self.rows.push((from.to_owned(), to.to_owned(), row));
        Ok(self)
    }

    /// Checks the entries against each other and returns the scheme.
    pub fn build(self) -> Result<Scheme, DefineError> {
        let SchemeBuilder {
            name,
            mut books,
            excluded,
            rows,
        } = self;
        if books.is_empty() {
            return refuse(format!("{name} numbers no books"));
        }
        books.sort_by_key(|(book, _)| canonical_rank(*book));
        let last = |book: BookKey, chapter: u16| -> Option<u16> {
            let (_, counts) = books.iter().find(|(key, _)| *key == book)?;
            counts.get(usize::from(chapter).checked_sub(1)?).copied()
        };

        let mut table = Vec::with_capacity(excluded.len());
        for (text, book, chapter, verse) in excluded {
            if last(book, chapter).is_none_or(|n| verse == 0 || verse > n) {
                return refuse(format!("excluded {text:?} is outside the scheme's counts"));
            }
            table.push((book.as_bytes(), chapter, verse));
        }
        table.sort_unstable();
        table.dedup();

        let mut kept = Vec::with_capacity(rows.len());
        for (from, to, row) in rows {
            let end = match row.shape {
                Shape::Split => row.from.verse,
                Shape::Parallel | Shape::Merge => row.from.verse + row.len - 1,
            };
            if last(row.from.book, row.from.chapter).is_none_or(|n| end > n) {
                return refuse(format!(
                    "{from:?} → {to:?}: {from} is outside the scheme's counts"
                ));
            }
            kept.push(row);
        }
        kept.sort_by_key(|r| (r.from.book.as_bytes(), r.from.chapter, r.from.verse));
        if kept.len() > usize::from(u16::MAX) {
            return refuse(format!("{} mapping rows; at most 65535", kept.len()));
        }
        let mut by_target: Vec<u16> = (0..kept.len() as u16).collect();
        by_target.sort_by_key(|&at| {
            let r = &kept[usize::from(at)];
            (r.to.book.as_bytes(), r.to.chapter, r.to.verse, r.to.part)
        });

        Ok(Scheme::defined(Defined {
            name,
            books,
            excluded: table,
            rows: kept,
            by_target,
        }))
    }
}

/// `"PSA 3:0-8"` → `(PSA, 3, 0, 8, None)`; `"ESG 4:17c"` → `(ESG, 4, 17, 17, Some('c'))`.
/// The book must be in canon's table. A backwards range is refused.
pub(crate) fn range(text: &str) -> Option<(BookKey, u16, u16, u16, Option<char>)> {
    let (book, rest) = text.trim().split_once(' ')?;
    let book = BookKey::new(book.as_bytes().try_into().ok()?);
    crate::book(book)?;
    let (chapter, verses) = rest.split_once(':')?;
    let chapter: u16 = chapter.parse().ok()?;
    if chapter == 0 {
        return None;
    }
    let (first, last) = match verses.split_once('-') {
        Some((first, last)) => (first, Some(last)),
        None => (verses, None),
    };
    let (first, part) = match first.strip_suffix(|c: char| c.is_ascii_lowercase()) {
        Some(digits) => (digits, first.chars().last()),
        None => (first, None),
    };
    let first: u16 = first.parse().ok()?;
    let last: u16 = match last {
        Some(last) if part.is_none() => last.parse().ok()?,
        Some(_) => return None,
        None => first,
    };
    (last >= first).then_some((book, chapter, first, last, part))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mapped, map, scheme};

    const GEN: BookKey = BookKey::new(*b"GEN");
    const PSA: BookKey = BookKey::new(*b"PSA");

    /// eng's Genesis and a few Psalms, defined by hand: it must map exactly
    /// as the built-in eng does.
    fn like_eng() -> Scheme {
        let eng = scheme("eng").unwrap();
        let mut b = SchemeBuilder::new("eng-copy").unwrap();
        b.book(GEN, eng.verse_counts(GEN).unwrap().to_vec())
            .unwrap();
        b.book(PSA, eng.verse_counts(PSA).unwrap().to_vec())
            .unwrap();
        b.map("GEN 31:55", "GEN 32:1").unwrap();
        b.map("GEN 32:1-32", "GEN 32:2-33").unwrap();
        b.map("PSA 3:0", "PSA 3:1").unwrap();
        b.map("PSA 3:1-8", "PSA 3:2-9").unwrap();
        b.build().unwrap()
    }

    #[test]
    fn a_defined_scheme_maps_like_the_built_in_it_copies() {
        let copy = like_eng();
        let org = scheme("org").unwrap();
        assert!(!copy.is_builtin());
        assert_eq!(copy.name(), "eng-copy");
        for (v, want) in [
            (
                Verse::new(GEN, 31, 55),
                Mapped::Moved(Verse::new(GEN, 32, 1)),
            ),
            (
                Verse::new(GEN, 32, 1),
                Mapped::Moved(Verse::new(GEN, 32, 2)),
            ),
            (Verse::new(PSA, 3, 0), Mapped::Moved(Verse::new(PSA, 3, 1))),
            (Verse::new(GEN, 1, 1), Mapped::Same),
        ] {
            assert_eq!(map(&copy, org, v), want, "{v}");
        }
        assert_eq!(
            map(org, &copy, Verse::new(GEN, 32, 1)),
            Mapped::Moved(Verse::new(GEN, 31, 55))
        );
        assert_eq!(copy.chapters(GEN), Some(50));
        assert_eq!(copy.chapters(BookKey::new(*b"MAT")), None);
    }

    #[test]
    fn exclusions_are_answered() {
        let mut b = SchemeBuilder::new("no-17-21").unwrap();
        b.book(BookKey::new(*b"MAT"), vec![25; 28]).unwrap();
        b.exclude("MAT 17:21").unwrap();
        let s = b.build().unwrap();
        assert!(s.excluded(BookKey::new(*b"MAT"), 17, 21));
        assert!(!s.excluded(BookKey::new(*b"MAT"), 17, 20));
    }

    #[test]
    fn bad_entries_are_refused_by_name() {
        assert!(SchemeBuilder::new("ulb").is_err(), "a built-in's name");
        assert!(SchemeBuilder::new(" ").is_err());
        let mut b = SchemeBuilder::new("x").unwrap();
        assert!(b.book(BookKey::new(*b"ZZZ"), vec![1]).is_err());
        assert!(b.book(GEN, vec![]).is_err());
        assert!(b.book(GEN, vec![3, 0]).is_err());
        b.book(GEN, vec![31, 25]).unwrap();
        assert!(b.book(GEN, vec![1]).is_err(), "twice");
        let err = b.map("GEN 2:1-5", "GEN 2:1-4").unwrap_err();
        assert!(err.to_string().contains("5 verses onto 4"), "{err}");
        assert!(
            b.map("GEN 2:1a", "GEN 2:1").is_err(),
            "a part on the scheme's side"
        );
        assert!(b.map("GEN 2:3-1", "GEN 2:1-3").is_err(), "backwards");
        assert!(b.exclude("GEN 2").is_err());

        let mut past = SchemeBuilder::new("x").unwrap();
        past.book(GEN, vec![31]).unwrap();
        past.map("GEN 1:30-32", "GEN 1:31-33").unwrap();
        assert!(past.build().is_err(), "1:32 is past the scheme's 31");
    }
}
