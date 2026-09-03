//! One chapter in, one detached observation out; ordered observations in,
//! findings out.
//!
//! ```text
//! analyze(corpus, &HygieneBytes)
//!   MRK chapter 1 at 0   "wept \\ here.\0\0\0"  → map → [StrandedBackslash 5..7, C0Control 13..16]
//!   MRK chapter 2 at 16  "An \0 more."          → map → [C0Control 3..4]
//!   reduce([obs at 0, obs at 16], &mut (), out)
//!     → target[0] MRK  5..7   StrandedBackslash run 2
//!       target[0] MRK 13..16  C0Control run 3
//!       target[0] MRK 19..20  C0Control run 1
//! ```
//!
//! [`ChapterPass::map`] reads one chapter and nothing else, so a host may run
//! it in any order or retain its result. Reduce cannot tell a cached
//! observation from a fresh one; that is what makes cold and incremental
//! analysis equal. The carry rules and that argument in full: pass.md.

use crate::{
    BookIndex, BookKey, Chapter, CodecError, Corpus, FindingKind, PackedFinding, ProjectedBook,
    TextRange, Verse,
};

/// The observation schema a host folds into every chapter-cache key.
///
/// Bump it whenever a `map` changes the shape or meaning of what it returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemaStamp(u32);

impl SchemaStamp {
    pub const fn new(stamp: u32) -> Self {
        Self(stamp)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The stamp of `self` composed with `other`, in that order.
    ///
    /// Order-sensitive and not either half, so a tuple pass cannot inherit a
    /// member's key.
    pub const fn then(self, other: Self) -> Self {
        Self((self.0.rotate_left(16) ^ other.0).wrapping_mul(0x9E37_79B1))
    }
}

/// Scripture identity for one independently mappable chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChapterKey {
    book: BookKey,
    chapter: u16,
}

impl ChapterKey {
    pub const fn new(book: BookKey, chapter: u16) -> Self {
        Self { book, chapter }
    }

    pub const fn book(self) -> BookKey {
        self.book
    }

    pub const fn chapter(self) -> u16 {
        self.chapter
    }
}

/// Everything one chapter's map may read, borrowed for that call only.
#[derive(Debug, Clone, Copy)]
pub struct ChapterInput<'a> {
    /// The chapter's masked projected text.
    pub text: &'a str,
    /// Verse rows rebased to that text.
    pub verses: &'a [Verse],
    pub key: ChapterKey,
}

/// One observation and the projected book offset reduce rebases it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterObs<O> {
    pub start: u32,
    pub obs: O,
}

/// A rule's chapter map and its ordered book reduction.
pub trait ChapterPass {
    /// Detached, chapter-relative, borrow-free. A host may retain it across
    /// invocations under the chapter's content key.
    type Observation: Send + 'static;
    /// Seam state carried book-forward during reduce, reset at every book.
    type Carry: Default;
    /// Part of every cache key a host builds for this pass.
    const SCHEMA: SchemaStamp;

    /// A pure function of this chapter; it never reads a neighbor.
    fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation;

    /// Folds one book's chapters in order, rebasing each to book coordinates.
    ///
    /// Borrows the observations, so a cache stays their owner.
    fn reduce(
        &self,
        book: &[ChapterObs<&Self::Observation>],
        carry: &mut Self::Carry,
        out: &mut Findings,
    );
}

/// Two passes over the same chapters as one: both maps run, both reduces run,
/// and the rows come out in span order per book.
impl<A: ChapterPass, B: ChapterPass> ChapterPass for (A, B) {
    type Observation = (A::Observation, B::Observation);
    type Carry = (A::Carry, B::Carry);
    const SCHEMA: SchemaStamp = A::SCHEMA.then(B::SCHEMA);

    fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
        (self.0.map(chapter), self.1.map(chapter))
    }

    fn reduce(
        &self,
        book: &[ChapterObs<&Self::Observation>],
        carry: &mut Self::Carry,
        out: &mut Findings,
    ) {
        let mark = out.mark();
        // Two views over the borrowed pairs: a reduce takes one observation
        // type. Two small vectors per book per publication.
        let left: Vec<ChapterObs<&A::Observation>> = book
            .iter()
            .map(|chapter| ChapterObs {
                start: chapter.start,
                obs: &chapter.obs.0,
            })
            .collect();
        let right: Vec<ChapterObs<&B::Observation>> = book
            .iter()
            .map(|chapter| ChapterObs {
                start: chapter.start,
                obs: &chapter.obs.1,
            })
            .collect();
        self.0.reduce(&left, &mut carry.0, out);
        self.1.reduce(&right, &mut carry.1, out);
        out.sort_tail(mark);
    }
}

/// The reduce sink: rows in projected book coordinates, each naming its book.
///
/// A host builds one per invocation, names a book before reducing it, and
/// publishes the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Findings {
    book_lengths: Vec<u32>,
    book: Option<BookIndex>,
    rows: Vec<PackedFinding>,
}

impl Findings {
    /// The lengths are the caller's book table, in book-index order.
    pub fn new(book_lengths: Vec<u32>) -> Self {
        Self {
            book_lengths,
            book: None,
            rows: Vec::new(),
        }
    }

    /// Names the book every later [`push`](Self::push) belongs to.
    pub fn open_book(&mut self, book: BookIndex) {
        self.book = Some(book);
    }

    /// Records one finding; the span is checked against the open book.
    ///
    /// Panics if no book is open: a row must never land in a book by default.
    pub fn push(&mut self, span: TextRange, kind: FindingKind) -> Result<(), CodecError> {
        let book = self.book.expect("open_book before push");
        let row = PackedFinding::new(span.from(), span.to(), book, kind, &self.book_lengths)?;
        self.rows.push(row);
        Ok(())
    }

    pub fn rows(&self) -> &[PackedFinding] {
        &self.rows
    }

    /// The row count now; hand it back to [`sort_tail`](Self::sort_tail).
    pub fn mark(&self) -> usize {
        self.rows.len()
    }

    /// Stable-sorts the rows pushed since `mark` by `(from, to)`.
    pub fn sort_tail(&mut self, mark: usize) {
        self.rows[mark..].sort_by_key(|row| (row.from(), row.to()));
    }

    pub fn into_rows(self) -> Vec<PackedFinding> {
        self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Calls `visit` with each chapter's projected start and its map input.
///
/// The one place a chapter input is assembled, so [`analyze`] and an
/// incremental host cannot build two different ones from the same book.
pub fn for_each_chapter<B: ProjectedBook>(book: &B, mut visit: impl FnMut(u32, ChapterInput<'_>)) {
    let text = book.text();
    let rows: Vec<Verse> = book.verses().collect();
    let mut verses = Vec::new();
    let mut at = 0;
    for chapter in book.chapters() {
        let span = chapter.text();
        at = collect_verses(&rows, at, chapter, &mut verses);
        visit(
            span.from(),
            ChapterInput {
                text: &text[span.from() as usize..span.to() as usize],
                verses: &verses,
                key: ChapterKey::new(book.key(), chapter.number()),
            },
        );
    }
}

/// Maps every chapter and reduces every book in caller order.
///
/// This is the whole-corpus oracle an incremental host is measured against.
pub fn analyze<B: ProjectedBook, P: ChapterPass>(corpus: &Corpus<'_, B>, pass: &P) -> Findings {
    let book_lengths: Vec<u32> = corpus
        .books()
        .iter()
        .map(|book| u32::try_from(book.text().len()).expect("corpus validation bounds book length"))
        .collect();
    let mut out = Findings::new(book_lengths);

    let mut observations: Vec<(u32, P::Observation)> = Vec::new();
    for (index, book) in corpus.iter() {
        observations.clear();
        for_each_chapter(book, |start, input| {
            observations.push((start, pass.map(input)));
        });
        let rows: Vec<ChapterObs<&P::Observation>> = observations
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();
        out.open_book(index);
        pass.reduce(&rows, &mut P::Carry::default(), &mut out);
    }
    out
}

/// Rebases this chapter's verse rows into `verses`, returning where the next
/// chapter resumes. Rows are non-decreasing by key, so each run is contiguous.
fn collect_verses(
    rows: &[Verse],
    mut at: usize,
    chapter: Chapter,
    verses: &mut Vec<Verse>,
) -> usize {
    verses.clear();
    let span = chapter.text();
    while rows
        .get(at)
        .is_some_and(|row| row.key().chapter() < chapter.number())
    {
        at += 1;
    }
    while let Some(row) = rows
        .get(at)
        .filter(|row| row.key().chapter() == chapter.number())
    {
        let text = row.text();
        debug_assert!(
            span.from() <= text.from() && text.to() <= span.to(),
            "corpus validation places every verse inside its chapter"
        );
        let rebased = TextRange::new(text.from() - span.from(), text.to() - span.from())
            .expect("a chapter-relative range keeps its order");
        verses.push(Verse::new(row.key(), rebased));
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Brigade, HygieneClass, HygieneDigest, VerseKey, hygiene::HygieneBytes,
        substrate::Substrate, validate,
    };

    struct Book {
        key: BookKey,
        text: &'static str,
        chapters: Vec<Chapter>,
        verses: Vec<Verse>,
    }

    impl ProjectedBook for Book {
        fn key(&self) -> BookKey {
            self.key
        }

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

    fn verse(chapter: u16, number: u16, from: u32, to: u32) -> Verse {
        Verse::new(
            VerseKey::new(chapter, number, number).unwrap(),
            range(from, to),
        )
    }

    /// `x\0y` then `\0\0z`; one NUL in chapter 1, two in chapter 2.
    fn mrk() -> Book {
        Book {
            key: BookKey::new(*b"MRK"),
            text: "x\0y\0\0z",
            chapters: vec![chapter(1, 0, 3), chapter(2, 3, 6)],
            verses: vec![verse(1, 1, 0, 3), verse(2, 1, 3, 6)],
        }
    }

    fn genesis() -> Book {
        Book {
            key: BookKey::new(*b"GEN"),
            text: "p\0q",
            chapters: vec![chapter(1, 0, 3)],
            verses: vec![verse(1, 1, 0, 3)],
        }
    }

    fn hygiene_rows(findings: &Findings) -> Vec<(u16, u32, u32, HygieneClass, u32)> {
        findings
            .rows()
            .iter()
            .map(|row| {
                let FindingKind::Hygiene(digest) = row.kind() else {
                    panic!("hygiene kind")
                };
                (
                    row.book_idx().get(),
                    row.from(),
                    row.to(),
                    digest.class(),
                    digest.run(),
                )
            })
            .collect()
    }

    #[test]
    fn analyze_equals_map_then_reduce_composed_by_hand() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();

        let mut hand = Findings::new(vec![6, 3]);
        let first = HygieneBytes.map(ChapterInput {
            text: "x\0y",
            verses: &[verse(1, 1, 0, 3)],
            key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
        });
        let second = HygieneBytes.map(ChapterInput {
            text: "\0\0z",
            verses: &[verse(2, 1, 0, 3)],
            key: ChapterKey::new(BookKey::new(*b"MRK"), 2),
        });
        let third = HygieneBytes.map(ChapterInput {
            text: "p\0q",
            verses: &[verse(1, 1, 0, 3)],
            key: ChapterKey::new(BookKey::new(*b"GEN"), 1),
        });
        hand.open_book(BookIndex::new(0).unwrap());
        HygieneBytes.reduce(
            &[
                ChapterObs {
                    start: 0,
                    obs: &first,
                },
                ChapterObs {
                    start: 3,
                    obs: &second,
                },
            ],
            &mut (),
            &mut hand,
        );
        hand.open_book(BookIndex::new(1).unwrap());
        HygieneBytes.reduce(
            &[ChapterObs {
                start: 0,
                obs: &third,
            }],
            &mut (),
            &mut hand,
        );

        let composed = analyze(&corpus, &HygieneBytes);
        assert_eq!(
            hygiene_rows(&composed),
            vec![
                (0, 1, 2, HygieneClass::C0Control, 1),
                (0, 3, 5, HygieneClass::C0Control, 2),
                (1, 1, 2, HygieneClass::C0Control, 1),
            ]
        );
        assert_eq!(composed.rows(), hand.rows());
    }

    #[test]
    fn analyze_reduces_books_in_caller_order() {
        let forward = vec![mrk(), genesis()];
        let reversed = vec![genesis(), mrk()];
        let forward = analyze(&Corpus::try_new(&forward).unwrap(), &HygieneBytes);
        let reversed = analyze(&Corpus::try_new(&reversed).unwrap(), &HygieneBytes);

        let keys: Vec<_> = forward.rows().iter().map(|row| row.book_idx()).collect();
        assert_eq!(
            keys,
            vec![
                BookIndex::new(0).unwrap(),
                BookIndex::new(0).unwrap(),
                BookIndex::new(1).unwrap()
            ]
        );
        assert_eq!(
            hygiene_rows(&reversed),
            vec![
                (0, 1, 2, HygieneClass::C0Control, 1),
                (1, 1, 2, HygieneClass::C0Control, 1),
                (1, 3, 5, HygieneClass::C0Control, 2),
            ]
        );
    }

    #[test]
    fn map_sees_chapter_relative_text_and_verse_rows() {
        struct Echo;

        impl ChapterPass for Echo {
            type Observation = (String, Vec<(u16, u32, u32)>);
            type Carry = ();
            const SCHEMA: SchemaStamp = SchemaStamp::new(0);

            fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
                (
                    chapter.text.to_string(),
                    chapter
                        .verses
                        .iter()
                        .map(|verse| (verse.key().first(), verse.text().from(), verse.text().to()))
                        .collect(),
                )
            }

            fn reduce(
                &self,
                _book: &[ChapterObs<&Self::Observation>],
                _carry: &mut (),
                _out: &mut Findings,
            ) {
            }
        }

        let book = Book {
            key: BookKey::new(*b"MRK"),
            text: "one two three",
            chapters: vec![chapter(1, 0, 7), chapter(2, 8, 13)],
            verses: vec![verse(1, 1, 0, 3), verse(1, 2, 4, 7), verse(2, 1, 8, 13)],
        };
        assert_eq!(validate(&book), Ok(()));
        let books = vec![book];
        let corpus = Corpus::try_new(&books).unwrap();

        let mut seen = Vec::new();
        for (_, book) in corpus.iter() {
            let text = book.text();
            let rows: Vec<Verse> = book.verses().collect();
            let mut verses = Vec::new();
            let mut at = 0;
            for chapter in book.chapters() {
                at = collect_verses(&rows, at, chapter, &mut verses);
                let span = chapter.text();
                seen.push(Echo.map(ChapterInput {
                    text: &text[span.from() as usize..span.to() as usize],
                    verses: &verses,
                    key: ChapterKey::new(book.key(), chapter.number()),
                }));
            }
        }

        assert_eq!(
            seen,
            vec![
                ("one two".to_string(), vec![(1, 0, 3), (2, 4, 7)]),
                ("three".to_string(), vec![(1, 0, 5)]),
            ]
        );
    }

    /// A NUL run abutting a masked `\c 2`: one run to the whole-book scan,
    /// one per chapter to the pass.
    fn seam() -> Book {
        Book {
            key: BookKey::new(*b"MRK"),
            text: "a\0\0\0\0\0\0b",
            chapters: vec![chapter(1, 0, 4), chapter(2, 4, 8)],
            verses: vec![verse(1, 1, 0, 4), verse(2, 1, 4, 8)],
        }
    }

    #[test]
    fn hygiene_through_analyze_equals_a_whole_book_scan_away_from_chapter_seams() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        let whole: Vec<_> = books
            .iter()
            .enumerate()
            .flat_map(|(index, book)| {
                crate::hygiene::scan(book.text())
                    .into_iter()
                    .map(move |finding| {
                        (
                            index as u16,
                            finding.span().from(),
                            finding.span().to(),
                            finding.class(),
                            finding.run(),
                        )
                    })
            })
            .collect();

        assert_eq!(hygiene_rows(&analyze(&corpus, &HygieneBytes)), whole);
    }

    #[test]
    fn a_run_across_a_masked_chapter_seam_is_two_findings() {
        let books = vec![seam()];
        let corpus = Corpus::try_new(&books).unwrap();

        let whole = crate::hygiene::scan(books[0].text());
        assert_eq!(whole.len(), 1);
        assert_eq!(whole[0].run(), 6);
        assert_eq!((whole[0].span().from(), whole[0].span().to()), (1, 7));

        assert_eq!(
            hygiene_rows(&analyze(&corpus, &HygieneBytes)),
            vec![
                (0, 1, 4, HygieneClass::C0Control, 3),
                (0, 4, 7, HygieneClass::C0Control, 3),
            ]
        );
    }

    /// Chapter 0 never reaches a pass, so front matter is out of scope.
    #[test]
    fn text_outside_every_chapter_is_not_mapped() {
        let books = vec![Book {
            key: BookKey::new(*b"MRK"),
            text: "\0abc",
            chapters: vec![chapter(1, 1, 4)],
            verses: vec![verse(1, 1, 1, 4)],
        }];
        let corpus = Corpus::try_new(&books).unwrap();

        assert_eq!(crate::hygiene::scan(books[0].text()).len(), 1);
        assert!(analyze(&corpus, &HygieneBytes).is_empty());
    }

    /// Chapter 1 carries a byte class and a scalar class alternately, so the
    /// two halves of a tuple emit rows that must interleave.
    fn mixed() -> Book {
        // "\u{301}a\0 \u{301}\0": free mark 0..2, NUL 3..4, free mark 4..7
        // (it takes the space it hangs on), NUL 7..8.
        Book {
            key: BookKey::new(*b"LUK"),
            text: "\u{301}a\0 \u{301}\0",
            chapters: vec![chapter(1, 0, 8)],
            verses: vec![verse(1, 1, 0, 8)],
        }
    }

    #[test]
    fn a_tuple_maps_both_and_reduces_in_span_order() {
        let books = vec![mixed()];
        let corpus = Corpus::try_new(&books).unwrap();
        assert_eq!(
            hygiene_rows(&analyze(&corpus, &Brigade::default())),
            vec![
                (0, 0, 2, HygieneClass::FreeCombiningMark, 1),
                (0, 3, 4, HygieneClass::C0Control, 1),
                (0, 4, 7, HygieneClass::FreeCombiningMark, 1),
                (0, 7, 8, HygieneClass::C0Control, 1),
            ]
        );
    }

    /// Test-only stubs: two stamps, no rows.
    struct Stamped<const S: u32>;

    impl<const S: u32> ChapterPass for Stamped<S> {
        type Observation = ();
        type Carry = ();
        const SCHEMA: SchemaStamp = SchemaStamp::new(S);

        fn map(&self, _chapter: ChapterInput<'_>) -> Self::Observation {}

        fn reduce(&self, _book: &[ChapterObs<&()>], _carry: &mut (), _out: &mut Findings) {}
    }

    #[test]
    fn a_tuple_schema_is_order_sensitive_and_not_either_half() {
        type Left = Stamped<1>;
        type Right = Stamped<2>;
        let forward = <(Left, Right) as ChapterPass>::SCHEMA;
        let backward = <(Right, Left) as ChapterPass>::SCHEMA;
        assert_ne!(forward, backward);
        assert_ne!(forward, Left::SCHEMA);
        assert_ne!(forward, Right::SCHEMA);
        // The shipped tuple, whose halves happen to share a stamp.
        let brigade = <Brigade as ChapterPass>::SCHEMA;
        assert_ne!(brigade, HygieneBytes::SCHEMA);
        assert_ne!(brigade, Substrate::SCHEMA);
    }

    #[test]
    fn a_tuple_carry_resets_per_book_for_both_halves() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        // Substrate contributes no row to these books; LastByte's runs spell
        // out the carry it was handed.
        let carried: Vec<_> = hygiene_rows(&analyze(&corpus, &(LastByte, Substrate)))
            .into_iter()
            .map(|(book, from, _, _, run)| (book, from, run - 1))
            .collect();

        assert_eq!(carried, vec![(0, 0, 0), (0, 3, u32::from(b'y')), (1, 0, 0)]);
    }

    #[test]
    fn analyze_over_a_tuple_equals_the_two_passes_analyzed_separately() {
        let books = vec![mixed(), mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        let mut apart = hygiene_rows(&analyze(&corpus, &HygieneBytes));
        apart.extend(hygiene_rows(&analyze(&corpus, &Substrate)));
        // Stable, so the first pass keeps its place on a tie — as the tuple's
        // own tail sort does.
        apart.sort_by_key(|row| (row.0, row.1, row.2));

        assert_eq!(hygiene_rows(&analyze(&corpus, &Brigade::default())), apart);
    }

    const fn detached<O: Send + 'static>() {}

    #[test]
    fn a_hygiene_observation_is_send_and_borrow_free() {
        detached::<<HygieneBytes as ChapterPass>::Observation>();
        detached::<<LastByte as ChapterPass>::Observation>();
    }

    /// Test-only. Records the carry it was handed, so the emitted runs spell
    /// out reduce's chapter order and every book's fresh `Carry::default()`.
    struct LastByte;

    impl ChapterPass for LastByte {
        type Observation = u8;
        type Carry = Option<u8>;
        const SCHEMA: SchemaStamp = SchemaStamp::new(u32::MAX);

        fn map(&self, chapter: ChapterInput<'_>) -> u8 {
            chapter.text.as_bytes().last().copied().unwrap_or(0)
        }

        fn reduce(&self, book: &[ChapterObs<&u8>], carry: &mut Option<u8>, out: &mut Findings) {
            for chapter in book {
                let seen = carry.unwrap_or(0);
                out.push(
                    TextRange::new(chapter.start, chapter.start).unwrap(),
                    FindingKind::Hygiene(
                        HygieneDigest::new(HygieneClass::C0Control, u32::from(seen) + 1).unwrap(),
                    ),
                )
                .expect("a chapter start lies inside its book");
                *carry = Some(*chapter.obs);
            }
        }
    }

    #[test]
    fn every_book_reduces_from_a_fresh_carry() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        let carried: Vec<_> = hygiene_rows(&analyze(&corpus, &LastByte))
            .into_iter()
            .map(|(book, from, _, _, run)| (book, from, run - 1))
            .collect();

        // MRK chapter 1 ends in `y`, chapter 2 in `z`; GEN restarts at 0.
        assert_eq!(carried, vec![(0, 0, 0), (0, 3, u32::from(b'y')), (1, 0, 0)]);
    }

    #[test]
    fn reduce_cannot_tell_a_reused_observation_from_a_fresh_one() {
        let books = vec![mrk()];
        let corpus = Corpus::try_new(&books).unwrap();
        let book = &books[0];
        let text = book.text();
        let rows: Vec<Verse> = book.verses().collect();

        let mut mapped = Vec::new();
        let mut verses = Vec::new();
        let mut at = 0;
        for chapter in book.chapters() {
            at = collect_verses(&rows, at, chapter, &mut verses);
            let span = chapter.text();
            let input = ChapterInput {
                text: &text[span.from() as usize..span.to() as usize],
                verses: &verses,
                key: ChapterKey::new(book.key(), chapter.number()),
            };
            mapped.push((span.from(), LastByte.map(input)));
        }
        let fresh: Vec<ChapterObs<&u8>> = mapped
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();

        let mut reused = fresh.clone();
        reused.reverse();
        reused.sort_by_key(|chapter| chapter.start);

        let first = BookIndex::new(0).unwrap();
        let mut direct = Findings::new(vec![6]);
        direct.open_book(first);
        LastByte.reduce(&fresh, &mut None, &mut direct);
        let mut roundtripped = Findings::new(vec![6]);
        roundtripped.open_book(first);
        LastByte.reduce(&reused, &mut None, &mut roundtripped);

        assert_eq!(direct, roundtripped);
        assert_eq!(direct.rows(), analyze(&corpus, &LastByte).rows());
    }
}
