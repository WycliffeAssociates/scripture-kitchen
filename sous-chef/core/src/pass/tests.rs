//! Instrument: SHAPES — synthetic books built here, chosen so the fold and
//! the judge each have exactly one thing to prove.

use super::*;
use crate::{
    Brigade, HygieneClass, HygieneDigest, Reasons, VerseKey, hygiene::HygieneBytes,
    substrate::Substrate, validate, words::Words,
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

/// The hygiene half of a row set; a substrate corpus judges conventions
/// beside them and sites them.
fn hygiene_rows(findings: &Findings) -> Vec<(u16, u32, u32, HygieneClass, u32)> {
    findings
        .rows()
        .iter()
        .filter_map(|row| {
            let FindingKind::Hygiene(digest) = row.kind() else {
                return None;
            };
            Some((
                row.book_idx().get(),
                row.from(),
                row.to(),
                digest.class(),
                digest.run(),
            ))
        })
        .collect()
}

#[test]
fn analyze_equals_map_then_fold_then_judge_composed_by_hand() {
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
    let mark = HygieneBytes.fold(&[
        ChapterObs {
            start: 0,
            obs: &first,
        },
        ChapterObs {
            start: 3,
            obs: &second,
        },
    ]);
    let genesis = HygieneBytes.fold(&[ChapterObs {
        start: 0,
        obs: &third,
    }]);
    HygieneBytes.judge(&[&mark, &genesis], &(), &mut hand);
    hand.finish();

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
fn analyze_judges_books_in_caller_order() {
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
        type Aggregate = ();
        type Config = ();
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

        fn fold(&self, _book: &[ChapterObs<&Self::Observation>]) {}

        fn judge(&self, _corpus: &[&()], _config: &(), _out: &mut Findings) {}
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
fn a_tuple_maps_both_and_judges_in_span_order() {
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
    type Aggregate = ();
    type Config = ();
    const SCHEMA: SchemaStamp = SchemaStamp::new(S);

    fn map(&self, _chapter: ChapterInput<'_>) -> Self::Observation {}

    fn fold(&self, _book: &[ChapterObs<&()>]) {}

    fn judge(&self, _corpus: &[&()], _config: &(), _out: &mut Findings) {}
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
}

#[test]
fn a_triple_schema_is_order_sensitive_and_not_a_member_or_a_pair() {
    type A = Stamped<1>;
    type B = Stamped<2>;
    type C = Stamped<3>;
    let forward = <(A, B, C) as ChapterPass>::SCHEMA;
    assert_ne!(forward, <(A, C, B) as ChapterPass>::SCHEMA);
    assert_ne!(forward, <(C, B, A) as ChapterPass>::SCHEMA);
    assert_ne!(forward, <(A, B) as ChapterPass>::SCHEMA);
    assert_ne!(forward, <(B, C) as ChapterPass>::SCHEMA);
    assert_ne!(forward, A::SCHEMA);
    assert_ne!(forward, C::SCHEMA);
    // The shipped triple, whose members do not share a stamp.
    let brigade = <Brigade as ChapterPass>::SCHEMA;
    assert_ne!(brigade, HygieneBytes::SCHEMA);
    assert_ne!(brigade, Substrate::SCHEMA);
    assert_ne!(brigade, Words::SCHEMA);
    assert_ne!(brigade, <(HygieneBytes, Substrate) as ChapterPass>::SCHEMA);
}

/// The word member is not decoration: a corpus with a casing minority gets
/// its row and its site out of the shipped triple, beside the other two.
///
/// The word ladder is a tenth of the glyph one, so the sample is the size
/// a three-basis-point rung needs.
#[test]
fn the_brigade_judges_and_sites_its_word_member() {
    let text: &'static str = Box::leak(
        (("and David went. ".repeat(20_000)) + "and david went and david went.").into_boxed_str(),
    );
    let books = vec![Book {
        key: BookKey::new(*b"MRK"),
        text,
        chapters: vec![chapter(1, 0, text.len() as u32)],
        verses: vec![verse(1, 1, 0, text.len() as u32)],
    }];
    let corpus = Corpus::try_new(&books).unwrap();
    let findings = analyze(&corpus, &Brigade::default());

    let casing: Vec<_> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == crate::Channel::Casing)
        .collect();
    assert_eq!(casing.len(), 1);
    assert_eq!((casing[0].numerator, casing[0].denominator), (2, 20_002));
    assert!(casing[0].word_hash().is_some());

    let sites: Vec<_> = findings
        .rows()
        .iter()
        .filter(|row| match row.kind() {
            FindingKind::Convention(digest) => digest.reasons().contains(Reasons::CASING),
            _ => false,
        })
        .map(|row| &text[row.from() as usize..row.to() as usize])
        .collect();
    assert_eq!(sites, vec!["david", "david"]);
}

#[test]
fn a_tuple_folds_each_book_from_a_fresh_state_in_both_halves() {
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

/// Over the hygiene-kind rows, which are the two members that emit them;
/// the word member's rows are convention-kind and are tested above.
#[test]
fn analyze_over_the_brigade_equals_its_members_analyzed_separately() {
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

/// Test-only. Records the seam state it carried into each chapter, so the
/// emitted runs spell out fold's chapter order and every book's fresh
/// start.
struct LastByte;

impl ChapterPass for LastByte {
    type Observation = u8;
    /// `(chapter start, the byte the previous chapter ended on)`.
    type Aggregate = Vec<(u32, u8)>;
    type Config = ();
    const SCHEMA: SchemaStamp = SchemaStamp::new(u32::MAX);

    fn map(&self, chapter: ChapterInput<'_>) -> u8 {
        chapter.text.as_bytes().last().copied().unwrap_or(0)
    }

    fn fold(&self, book: &[ChapterObs<&u8>]) -> Vec<(u32, u8)> {
        let mut carry = None;
        book.iter()
            .map(|chapter| {
                let seen = carry.unwrap_or(0);
                carry = Some(*chapter.obs);
                (chapter.start, seen)
            })
            .collect()
    }

    fn judge(&self, corpus: &[&Vec<(u32, u8)>], _config: &(), out: &mut Findings) {
        for (index, book) in corpus.iter().enumerate() {
            out.open_book(BookIndex::new(index).unwrap());
            for &(start, seen) in book.iter() {
                out.push(
                    TextRange::new(start, start).unwrap(),
                    FindingKind::Hygiene(
                        HygieneDigest::new(HygieneClass::C0Control, u32::from(seen) + 1).unwrap(),
                    ),
                )
                .expect("a chapter start lies inside its book");
            }
        }
    }
}

#[test]
fn every_book_folds_from_a_fresh_state() {
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
fn fold_cannot_tell_a_reused_observation_from_a_fresh_one() {
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

    assert_eq!(LastByte.fold(&fresh), LastByte.fold(&reused));

    let mut direct = Findings::new(vec![6]);
    LastByte.judge(&[&LastByte.fold(&fresh)], &(), &mut direct);
    direct.finish();
    assert_eq!(direct.rows(), analyze(&corpus, &LastByte).rows());
}
