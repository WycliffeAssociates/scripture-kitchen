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
//! `data/schemes/README.md`). This module reads only their counts and
//! exclusions. The `mappedVerses` the same files carry are what `map` will
//! read (plan step 3).

use mise::books::BookKey;

use crate::scheme_tables::SCHEMES;

/// One numbering tradition's chapter and verse counts.
///
/// The fields are private so that a scheme a project declares (a reviewed
/// `.vrs` import, plan step 7) can later be held the same way without
/// changing a caller.
#[derive(Debug)]
pub struct Scheme {
    name: &'static str,
    /// In book-table order. Chapter `n` is at index `n - 1`.
    books: &'static [(BookKey, &'static [u16])],
    /// `(book's bytes, chapter, verse)`, sorted: verses the scheme omits on
    /// purpose. Bytes rather than `BookKey`, which has no order.
    excluded: &'static [([u8; 3], u16, u16)],
}

impl Scheme {
    pub(crate) const fn builtin(
        name: &'static str,
        books: &'static [(BookKey, &'static [u16])],
        excluded: &'static [([u8; 3], u16, u16)],
    ) -> Scheme {
        Scheme {
            name,
            books,
            excluded,
        }
    }

    /// `"org"`, `"eng"`, `"ulb"`, …
    pub fn name(&self) -> &str {
        self.name
    }

    /// The books this scheme numbers, in book-table order.
    pub fn books(&self) -> impl Iterator<Item = BookKey> + '_ {
        self.books.iter().map(|(book, _)| *book)
    }

    /// The last verse of every chapter, chapter 1 first. `None` when the
    /// scheme does not number the book at all.
    pub fn verse_counts(&self, book: BookKey) -> Option<&[u16]> {
        self.books
            .iter()
            .find(|(key, _)| *key == book)
            .map(|(_, counts)| *counts)
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
    /// text to footnote it. The question is here for declared schemes.
    pub fn excluded(&self, book: BookKey, chapter: u16, verse: u16) -> bool {
        self.excluded
            .binary_search(&(book.as_bytes(), chapter, verse))
            .is_ok()
    }
}

/// A built-in scheme by name, exactly (`"org"`, not `"ORG"`).
pub fn scheme(name: &str) -> Option<&'static Scheme> {
    SCHEMES.iter().find(|scheme| scheme.name == name)
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
            assert!(scheme.excluded.is_sorted(), "{}", scheme.name());
        }
    }
}
