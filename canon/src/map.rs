//! Where a verse numbered by one scheme is under another.
//!
//! ```text
//! map(eng, org, GEN 31:55)   -> Moved(GEN 32:1)
//! map(eng, org, PSA 3:0)     -> Moved(PSA 3:1)        eng's title is org's verse 1
//! map(org, lxx, NEH 1:1)     -> Moved(EZR 11:1)       the Septuagint's 2 Esdras
//! map(eng, org, PSA 13:5)    -> Merged(PSA 13:6)      eng 13:5 and 13:6 are both org 13:6
//! map(org, eng, PSA 13:6)    -> Split([PSA 13:5, PSA 13:6])
//! map(eng, org, ACT 19:41)   -> Absent                org ends Acts 19 at 40
//! map(eng, vul, MAT 1:1)     -> Same
//! ```
//!
//! Every mapping passes through `org`. A scheme's rows (the `mappedVerses`
//! of its file) say where its verses sit in `org`. A verse no row mentions
//! keeps its number: that is the format's rule, as Paratext applies it. So
//! `map(a, b, v)` is "a's v in org", then "which of b's verses sit there".
//!
//! Verse 0 is the space before verse 1, where a Psalm's title sits in a
//! scheme that doesn't number titles. It exists in every chapter a scheme has.
//!
//! What comes back is exact or says it isn't. It never guesses "nearest".

use core::fmt;

use mise::books::{BookKey, canonical_rank};

use crate::scheme::{Scheme, schemes};

/// One verse, or one part of one, in some scheme's numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Verse {
    pub book: BookKey,
    pub chapter: u16,
    /// 0 is the title slot before verse 1.
    pub verse: u16,
    /// The letter of a part the scheme files name, as in `ESG 4:17c`. `None`
    /// for the whole verse.
    pub part: Option<char>,
}

impl Verse {
    pub const fn new(book: BookKey, chapter: u16, verse: u16) -> Verse {
        Verse {
            book,
            chapter,
            verse,
            part: None,
        }
    }

    /// The same verse without its part letter.
    pub const fn whole(self) -> Verse {
        Verse::new(self.book, self.chapter, self.verse)
    }
}

impl fmt::Display for Verse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}:{}", self.book, self.chapter, self.verse)?;
        match self.part {
            Some(part) => write!(f, "{part}"),
            None => Ok(()),
        }
    }
}

/// Where a verse is under another scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mapped {
    /// Numbered the same, one to one. By far the common case.
    Same,
    /// Exactly one verse (or part of one) there, and no other verse here
    /// maps to it: GEN 31:55 (eng) is GEN 32:1 (org).
    Moved(Verse),
    /// Exactly one verse there, which other verses here map to as well: this
    /// verse is one piece of it. PSA 13:5 and 13:6 (eng) are both PSA 13:6 (org).
    Merged(Verse),
    /// Several verses there, in canonical order: this one verse spans them.
    Split(Vec<Verse>),
    /// The other scheme has no such verse, or this scheme has no such verse
    /// to begin with.
    Absent,
}

/// How one row of a scheme file pairs its verses with `org`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `len` verses here are `len` verses there, in step.
    Parallel,
    /// `len` verses here are the one verse there.
    Merge,
    /// The one verse here is `len` verses there.
    Split,
}

/// One row of a scheme's `mappedVerses`, from the scheme to `org`. A range
/// never crosses a chapter. `from` is always a whole verse; `to` names a part
/// only when the row is a single verse.
#[derive(Debug)]
pub(crate) struct Row {
    pub(crate) from: Verse,
    pub(crate) to: Verse,
    pub(crate) len: u16,
    pub(crate) shape: Shape,
}

impl Row {
    /// The generated tables' constructor, one row per line.
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        from_book: [u8; 3],
        from_chapter: u16,
        from_verse: u16,
        to_book: [u8; 3],
        to_chapter: u16,
        to_verse: u16,
        to_part: Option<char>,
        len: u16,
        shape: Shape,
    ) -> Row {
        Row {
            from: Verse::new(BookKey::new(from_book), from_chapter, from_verse),
            to: Verse {
                book: BookKey::new(to_book),
                chapter: to_chapter,
                verse: to_verse,
                part: to_part,
            },
            len,
            shape,
        }
    }

    /// The last verse number the row covers here.
    fn end_here(&self) -> u16 {
        match self.shape {
            Shape::Split => self.from.verse,
            Shape::Parallel | Shape::Merge => self.from.verse + self.len - 1,
        }
    }

    /// The last verse number the row covers in `org`.
    fn end_in_org(&self) -> u16 {
        match self.shape {
            Shape::Merge => self.to.verse,
            Shape::Parallel | Shape::Split => self.to.verse + self.len - 1,
        }
    }

    fn covers_from(&self, v: Verse) -> bool {
        self.from.book == v.book
            && self.from.chapter == v.chapter
            && (self.from.verse..=self.end_here()).contains(&v.verse)
    }

    /// A whole verse there matches a row targeting any part of it; a part
    /// matches a row targeting that part or the whole verse.
    fn covers_to(&self, o: Verse) -> bool {
        self.to.book == o.book
            && self.to.chapter == o.chapter
            && (self.to.verse..=self.end_in_org()).contains(&o.verse)
            && match (o.part, self.to.part) {
                (Some(want), Some(have)) => want == have,
                _ => true,
            }
    }

    /// Where `v` (which the row covers) goes in `org`.
    fn targets(&self, v: Verse, out: &mut Vec<Verse>) {
        let at = |verse| Verse { verse, ..self.to };
        match self.shape {
            Shape::Parallel => out.push(at(self.to.verse + (v.verse - self.from.verse))),
            Shape::Merge => out.push(self.to),
            Shape::Split => out.extend((self.to.verse..=self.end_in_org()).map(at)),
        }
    }

    /// Which of this scheme's verses the row sends to `o` (which it covers).
    fn sources(&self, o: Verse, out: &mut Vec<Verse>) {
        let at = |verse| Verse::new(self.from.book, self.from.chapter, verse);
        match self.shape {
            Shape::Parallel => out.push(at(self.from.verse + (o.verse - self.to.verse))),
            Shape::Merge => out.extend((self.from.verse..=self.end_here()).map(at)),
            Shape::Split => out.push(self.from),
        }
    }

    /// Every (here, org) pair the row states.
    fn pairs(&self) -> impl Iterator<Item = (Verse, Verse)> + '_ {
        let from = |verse| Verse::new(self.from.book, self.from.chapter, verse);
        let to = |verse| Verse { verse, ..self.to };
        (0..self.len).map(move |i| match self.shape {
            Shape::Parallel => (from(self.from.verse + i), to(self.to.verse + i)),
            Shape::Merge => (from(self.from.verse + i), self.to),
            Shape::Split => (self.from, to(self.to.verse + i)),
        })
    }
}

/// Where verse `v`, numbered by `from`, is under `to`.
///
/// `v` is a whole verse (its `part` is ignored). A verse `from` doesn't have
/// is `Absent`, and so is one that has no counterpart in `to`.
pub fn map(from: &Scheme, to: &Scheme, v: Verse) -> Mapped {
    let v = v.whole();
    if !from.has_verse(v) {
        return Mapped::Absent;
    }
    if from.name() == to.name() {
        return Mapped::Same;
    }
    let mut there = through_org(from, to, v);
    match there.len() {
        0 => Mapped::Absent,
        1 => {
            let t = there.remove(0);
            let back = through_org(to, from, t.whole());
            if back.iter().any(|b| b.whole() != v) {
                Mapped::Merged(t)
            } else if t == v {
                Mapped::Same
            } else {
                Mapped::Moved(t)
            }
        }
        _ => Mapped::Split(there),
    }
}

/// `from`'s `v` in `org`, then the verses of `to` that sit there, deduplicated
/// and in canonical order.
fn through_org(from: &Scheme, to: &Scheme, v: Verse) -> Vec<Verse> {
    let mut in_org = Vec::new();
    from.in_org(v, &mut in_org);
    let mut there = Vec::new();
    for o in in_org {
        to.sources_of(o, &mut there);
    }
    there.sort_by_key(|t| (canonical_rank(t.book), t.chapter, t.verse, t.part));
    there.dedup();
    there
}

impl Scheme {
    /// Where this scheme's verse is in `org`.
    pub fn to_org(&self, v: Verse) -> Mapped {
        map(self, org(), v)
    }

    /// Where `org`'s verse is in this scheme.
    pub fn from_org(&self, v: Verse) -> Mapped {
        map(org(), self, v)
    }

    /// Every verse whose number differs from `org`'s, with where it sits in
    /// `org`, as the scheme file states them: sparse, so `org` itself and a
    /// book no row mentions yield nothing. A verse that spans several `org`
    /// verses appears once per verse. Pairing uses this to rewrite only the
    /// keys that move.
    pub fn moves(&self) -> impl Iterator<Item = (Verse, Verse)> + '_ {
        self.rows().iter().flat_map(Row::pairs)
    }

    /// Whether the scheme numbers this verse. Verse 0 exists in every chapter.
    pub fn has_verse(&self, v: Verse) -> bool {
        self.last_verse(v.book, v.chapter)
            .is_some_and(|last| v.verse <= last)
    }

    /// The rows covering `v`, in this scheme's numbering.
    fn rows_from(&self, v: Verse) -> impl Iterator<Item = &Row> + '_ {
        let rows = self.rows();
        let key = |row: &Row| (row.from.book.as_bytes(), row.from.chapter);
        let want = (v.book.as_bytes(), v.chapter);
        let start = rows.partition_point(|row| key(row) < want);
        rows[start..]
            .iter()
            .take_while(move |row| key(row) == want)
            .filter(move |row| row.covers_from(v))
    }

    /// The rows covering `o`, in `org`'s numbering.
    fn rows_to(&self, o: Verse) -> impl Iterator<Item = &Row> + '_ {
        let rows = self.rows();
        let index = self.by_target();
        let key = |at: &u16| {
            let row = &rows[usize::from(*at)];
            (row.to.book.as_bytes(), row.to.chapter)
        };
        let want = (o.book.as_bytes(), o.chapter);
        let start = index.partition_point(|at| key(at) < want);
        index[start..]
            .iter()
            .take_while(move |at| key(at) == want)
            .map(move |at| &rows[usize::from(*at)])
            .filter(move |row| row.covers_to(o))
    }

    fn in_org(&self, v: Verse, out: &mut Vec<Verse>) {
        let before = out.len();
        for row in self.rows_from(v) {
            row.targets(v, out);
        }
        if out.len() == before {
            out.push(v);
        }
    }

    fn sources_of(&self, o: Verse, out: &mut Vec<Verse>) {
        for row in self.rows_to(o) {
            row.sources(o, out);
        }
        let whole = o.whole();
        if self.has_verse(whole) && self.rows_from(whole).next().is_none() {
            out.push(o);
        }
    }
}

/// `org`, the pivot. It is always the first built-in scheme.
fn org() -> &'static Scheme {
    &schemes()[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme;

    const GEN: BookKey = BookKey::new(*b"GEN");

    #[test]
    fn org_has_no_rows_and_maps_to_itself() {
        let org = scheme("org").unwrap();
        assert_eq!(org.moves().count(), 0);
        assert_eq!(org.to_org(Verse::new(GEN, 1, 1)), Mapped::Same);
    }

    /// Every verse of every scheme, sent to `org` and back, is among what
    /// comes back: nothing is lost through the pivot.
    #[test]
    fn every_verse_survives_a_round_trip_through_org() {
        for scheme in schemes() {
            for book in scheme.books() {
                let counts = scheme.verse_counts(book).unwrap();
                for (at, &last) in counts.iter().enumerate() {
                    let chapter = u16::try_from(at + 1).unwrap();
                    for verse in 0..=last {
                        let v = Verse::new(book, chapter, verse);
                        let mut in_org = Vec::new();
                        scheme.in_org(v, &mut in_org);
                        let mut back = Vec::new();
                        for o in in_org {
                            scheme.sources_of(o, &mut back);
                        }
                        assert!(
                            back.iter().any(|b| b.whole() == v),
                            "{} {v} is lost through org",
                            scheme.name()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rows_are_sorted_and_the_target_index_is_a_sorted_permutation() {
        for scheme in schemes() {
            let rows = scheme.rows();
            assert!(
                rows.is_sorted_by_key(|r| (r.from.book.as_bytes(), r.from.chapter, r.from.verse)),
                "{} rows",
                scheme.name()
            );
            let index = scheme.by_target();
            assert_eq!(index.len(), rows.len(), "{}", scheme.name());
            assert!(
                index.is_sorted_by_key(|at| {
                    let r = &rows[usize::from(*at)];
                    (r.to.book.as_bytes(), r.to.chapter, r.to.verse)
                }),
                "{} target index",
                scheme.name()
            );
        }
    }
}
