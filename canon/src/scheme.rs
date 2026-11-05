//! Versification schemes: how many chapters a book has and how many verses
//! each chapter has, under one numbering tradition.
//!
//! ```text
//! scheme("eng")?.chapters(MAL)          -> Some(4)    Malachi 4 is org's 3:19-24
//! scheme("org")?.chapters(MAL)          -> Some(3)
//! scheme("org")?.last_verse(PSA, 3)     -> Some(9)    org numbers the title as verse 1
//! scheme("ulb")?.last_verse(ACT, 19)    -> Some(40)   eng says 41
//! scheme("ulb")?.chapters(TOB)          -> None       ulb has no Tobit
//! ```
//!
//! These are the facts "what should this book have" rests on. "What does this
//! text have" is the text's own TOC, in onion. Comparing the two is coverage,
//! which comes later (plan step 4).
//!
//! The built-in schemes are generated from `data/schemes/*.json`: the
//! Copenhagen Alliance files (`org`, `eng`, `lxx`, `vul`, `rsc`, `rso`, CC
//! BY-SA 4.0; see `NOTICE`) and `ulb`, which is measured from en_ulb (see
//! `data/schemes/README.md`). This module holds their counts and
//! exclusions, and the `mappedVerses` rows that `crate::map` reads.

use mise::books::BookKey;

use crate::map::Row;
use crate::scheme_tables::SCHEMES;

/// One numbering tradition's chapter and verse counts, and its rows toward
/// `org`.
///
/// Built in (`scheme("ulb")`, generated from `data/schemes/`) or defined by a
/// project ([`SchemeBuilder`](crate::SchemeBuilder), from a reviewed `.vrs`
/// import). Both answer every question the same way; the fields are private
/// so a caller never knows which it holds.
#[derive(Debug)]
pub struct Scheme {
    inner: Inner,
}

#[derive(Debug)]
enum Inner {
    Builtin(Builtin),
    Defined(Box<Defined>),
}

/// The generated tables, borrowed for the life of the program.
#[derive(Debug)]
struct Builtin {
    name: &'static str,
    /// In book-table order. Chapter `n` is at index `n - 1`.
    books: &'static [(BookKey, &'static [u16])],
    /// `(book's bytes, chapter, verse)`, sorted: verses the scheme omits on
    /// purpose. Bytes rather than `BookKey`, which has no order.
    excluded: &'static [([u8; 3], u16, u16)],
    /// The file's `mappedVerses`, toward `org`, sorted by where they start
    /// here. Empty for `org` itself.
    rows: &'static [Row],
    /// Indexes into `rows`, sorted by where each lands in `org`.
    by_target: &'static [u16],
}

/// The same facts, owned: a scheme built at run time.
#[derive(Debug)]
pub(crate) struct Defined {
    pub(crate) name: String,
    pub(crate) books: Vec<(BookKey, Vec<u16>)>,
    pub(crate) excluded: Vec<([u8; 3], u16, u16)>,
    pub(crate) rows: Vec<Row>,
    pub(crate) by_target: Vec<u16>,
}

impl Scheme {
    pub(crate) const fn builtin(
        name: &'static str,
        books: &'static [(BookKey, &'static [u16])],
        excluded: &'static [([u8; 3], u16, u16)],
        rows: &'static [Row],
        by_target: &'static [u16],
    ) -> Scheme {
        Scheme {
            inner: Inner::Builtin(Builtin {
                name,
                books,
                excluded,
                rows,
                by_target,
            }),
        }
    }

    pub(crate) fn defined(defined: Defined) -> Scheme {
        Scheme {
            inner: Inner::Defined(Box::new(defined)),
        }
    }

    pub(crate) fn rows(&self) -> &[Row] {
        match &self.inner {
            Inner::Builtin(b) => b.rows,
            Inner::Defined(d) => &d.rows,
        }
    }

    pub(crate) fn by_target(&self) -> &[u16] {
        match &self.inner {
            Inner::Builtin(b) => b.by_target,
            Inner::Defined(d) => &d.by_target,
        }
    }

    fn excluded_table(&self) -> &[([u8; 3], u16, u16)] {
        match &self.inner {
            Inner::Builtin(b) => b.excluded,
            Inner::Defined(d) => &d.excluded,
        }
    }

    /// `"org"`, `"eng"`, `"ulb"`, or a defined scheme's own name.
    pub fn name(&self) -> &str {
        match &self.inner {
            Inner::Builtin(b) => b.name,
            Inner::Defined(d) => &d.name,
        }
    }

    /// Whether this is one of canon's own schemes rather than one a project
    /// defined.
    pub fn is_builtin(&self) -> bool {
        matches!(self.inner, Inner::Builtin(_))
    }

    /// The books this scheme numbers, in book-table order.
    pub fn books(&self) -> Box<dyn Iterator<Item = BookKey> + '_> {
        match &self.inner {
            Inner::Builtin(b) => Box::new(b.books.iter().map(|(book, _)| *book)),
            Inner::Defined(d) => Box::new(d.books.iter().map(|(book, _)| *book)),
        }
    }

    /// The last verse of every chapter, chapter 1 first. `None` when the
    /// scheme does not number the book at all.
    pub fn verse_counts(&self, book: BookKey) -> Option<&[u16]> {
        match &self.inner {
            Inner::Builtin(b) => b
                .books
                .iter()
                .find(|(key, _)| *key == book)
                .map(|(_, counts)| *counts),
            Inner::Defined(d) => d
                .books
                .iter()
                .find(|(key, _)| *key == book)
                .map(|(_, counts)| counts.as_slice()),
        }
    }

    /// How many chapters the book has. `None` when the scheme does not
    /// number the book.
    pub fn chapters(&self, book: BookKey) -> Option<u16> {
        self.verse_counts(book)
            .map(|counts| u16::try_from(counts.len()).unwrap_or(u16::MAX))
    }

    /// The last verse of a chapter. `None` for a book the scheme does not
    /// number, chapter 0, or a chapter past the book's last.
    pub fn last_verse(&self, book: BookKey, chapter: u16) -> Option<u16> {
        let index = usize::from(chapter).checked_sub(1)?;
        self.verse_counts(book)?.get(index).copied()
    }

    /// Whether the scheme omits this verse on purpose, so that a text
    /// lacking it is not missing anything. No built-in scheme excludes a
    /// verse today: they all number `MAT 17:21` and its kind, and leave the
    /// text to footnote it. The question is here for defined schemes.
    pub fn excluded(&self, book: BookKey, chapter: u16, verse: u16) -> bool {
        self.excluded_table()
            .binary_search(&(book.as_bytes(), chapter, verse))
            .is_ok()
    }
}

/// A built-in scheme by name, exactly (`"org"`, not `"ORG"`).
pub fn scheme(name: &str) -> Option<&'static Scheme> {
    SCHEMES.iter().find(|scheme| scheme.name() == name)
}

/// Every built-in scheme, `org` (the pivot every mapping goes through) first.
pub fn schemes() -> &'static [Scheme] {
    SCHEMES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canon, books};

    #[test]
    fn org_is_first_and_names_are_unique() {
        assert_eq!(schemes()[0].name(), "org");
        for (at, one) in schemes().iter().enumerate() {
            assert!(
                schemes()[..at]
                    .iter()
                    .all(|other| other.name() != one.name()),
                "{} twice",
                one.name()
            );
        }
    }

    /// Every scheme numbers the 66, except where the Greek and Latin
    /// traditions fold a book into another: in `lxx` Nehemiah is the back
    /// half of EZR (2 Esdras, 23 chapters), and Daniel and Esther exist only
    /// as the Greek DAG and ESG; `vul` likewise has only ESG for Esther.
    #[test]
    fn every_scheme_numbers_the_66_with_no_empty_chapter() {
        let lacks: [(&str, &[&[u8; 3]]); 2] =
            [("lxx", &[b"NEH", b"DAN", b"EST"]), ("vul", &[b"EST"])];
        for scheme in schemes() {
            let gaps = lacks
                .iter()
                .find(|(name, _)| *name == scheme.name())
                .map_or(&[][..], |(_, gaps)| *gaps);
            for book in books(Canon::Protestant66) {
                let counts = scheme.verse_counts(book.code);
                if gaps.contains(&&book.code.as_bytes()) {
                    assert!(counts.is_none(), "{} numbers {}", scheme.name(), book.code);
                    continue;
                }
                let counts =
                    counts.unwrap_or_else(|| panic!("{} lacks {}", scheme.name(), book.code));
                assert!(
                    !counts.is_empty(),
                    "{} {} has no chapters",
                    scheme.name(),
                    book.code
                );
                assert!(
                    counts.iter().all(|&n| n > 0),
                    "{} {} has an empty chapter",
                    scheme.name(),
                    book.code
                );
            }
        }
    }

    #[test]
    fn exclusions_are_sorted_for_binary_search() {
        for scheme in schemes() {
            assert!(scheme.excluded_table().is_sorted(), "{}", scheme.name());
        }
    }
}
