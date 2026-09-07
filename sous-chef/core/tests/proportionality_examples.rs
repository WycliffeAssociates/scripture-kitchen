//! The length-proportionality rule's own cases, one test per sentence of
//! `sous-chef/rules/length-proportionality.md`.
//!
//! Instrument: SYNTHETIC. Every book here is built in this file, so a case is
//! readable as the rule that names it. Corpus behaviour is the ledger's job,
//! not this file's.
//!
//! Each book carries `FILLER` verses whose ratios vary gently, because a
//! degenerate sample judges nothing: with every ratio identical the pooled MAD
//! is zero and the rule abstains by design. The verse under test is appended
//! after them.

use sous_core::{
    BookKey, Chapter, Corpus, FindingKind, LengthConfig, ProjectedBook, SourceLengths, SourceVerse,
    Substrate, TextRange, Verse, VerseKey, analyze_paired, source_lengths,
};

/// Paired verses behind the one under test: comfortably over the default
/// `min_verses` of 50, and enough distinct ratios for a real MAD.
const FILLER: usize = 60;

/// Source graphemes every filler verse is measured against.
const BASE: usize = 40;

// ---------------------------------------------------------------- fixtures

struct Book {
    key: BookKey,
    text: String,
    chapters: Vec<Chapter>,
    verses: Vec<Verse>,
}

impl ProjectedBook for Book {
    fn key(&self) -> BookKey {
        self.key
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn chapters(&self) -> impl Iterator<Item = Chapter> {
        self.chapters.iter().copied()
    }

    fn verses(&self) -> impl Iterator<Item = Verse> {
        self.verses.iter().copied()
    }
}

/// One projected verse row: its key and the text the projection holds for it.
#[derive(Clone)]
struct Row {
    key: VerseKey,
    text: String,
}

fn row(chapter: u16, first: u16, last: u16, text: &str) -> Row {
    Row {
        key: VerseKey::new(chapter, first, last).expect("a well-formed key"),
        text: text.to_string(),
    }
}

/// Lays the rows out as one projected book, one chapter span per chapter
/// number, each verse followed by a newline exactly as a mask keeps one.
fn book(key: [u8; 3], rows: &[Row]) -> Book {
    let mut text = String::new();
    let mut verses = Vec::new();
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut open: Option<(u16, u32)> = None;
    for row in rows {
        if open.is_none_or(|(number, _)| number != row.key.chapter()) {
            if let Some((number, from)) = open {
                chapters.push(chapter_span(number, from, text.len() as u32));
            }
            open = Some((row.key.chapter(), text.len() as u32));
        }
        let from = text.len() as u32;
        text.push_str(&row.text);
        // A verse with no content keeps no separator either, so its projected
        // span is genuinely empty rather than one newline long.
        if !row.text.is_empty() {
            text.push('\n');
        }
        verses.push(Verse::new(
            row.key,
            TextRange::new(from, text.len() as u32).expect("a verse grows forward"),
        ));
    }
    if let Some((number, from)) = open {
        chapters.push(chapter_span(number, from, text.len() as u32));
    }
    Book {
        key: BookKey::new(key),
        text,
        chapters,
        verses,
    }
}

fn chapter_span(number: u16, from: u32, to: u32) -> Chapter {
    Chapter::new(
        number,
        TextRange::new(from, to).expect("a chapter grows forward"),
    )
    .expect("chapter numbers start at one")
}

/// `FILLER` verses of chapter 1, target lengths cycling 40..=46 against a
/// constant source length of `BASE`.
fn filler_rows() -> Vec<Row> {
    (0..FILLER)
        .map(|at| row(1, at as u16 + 1, at as u16 + 1, &"a".repeat(BASE + at % 7)))
        .collect()
}

fn filler_source() -> Vec<SourceVerse> {
    (0..FILLER)
        .map(|at| {
            SourceVerse::new(
                VerseKey::new(1, at as u16 + 1, at as u16 + 1).unwrap(),
                BASE as u32,
            )
        })
        .collect()
}

fn verse(chapter: u16, first: u16, last: u16, graphemes: u32) -> SourceVerse {
    SourceVerse::new(
        VerseKey::new(chapter, first, last).expect("a well-formed key"),
        graphemes,
    )
}

/// Every length row as `(book index, from, to, book lane, project lane)`;
/// the pass's own hygiene and convention rows are not this file's subject.
type Rows = Vec<(u16, u32, u32, Option<i16>, Option<i16>)>;

fn judge(target: &[Book], source: &[(BookKey, Vec<SourceVerse>)]) -> Rows {
    judge_with(target, source, LengthConfig::default()).0
}

fn judge_with(
    target: &[Book],
    source: &[(BookKey, Vec<SourceVerse>)],
    config: LengthConfig,
) -> (Rows, sous_core::Paired) {
    let corpus = Corpus::try_new(target).expect("distinct book keys");
    let source: Vec<SourceLengths<'_>> = source
        .iter()
        .map(|(key, verses)| SourceLengths {
            book: *key,
            verses,
            words: None,
        })
        .collect();
    let judging = sous_core::JudgingConfig {
        lengths: config,
        ..Default::default()
    };
    let (findings, paired) = analyze_paired(&corpus, &Substrate, &judging, &source);
    let rows = findings
        .rows()
        .iter()
        .filter_map(|finding| match finding.kind() {
            FindingKind::LengthProportionality(digest) => Some((
                finding.book_idx().get(),
                finding.from(),
                finding.to(),
                digest.book_scope().map(|value| value.raw()),
                digest.project_scope().map(|value| value.raw()),
            )),
            _ => None,
        })
        .collect();
    (rows, paired)
}

/// The projected span of one key in one book, so a test names a row by the
/// verse it is about rather than by an offset.
fn span_of(book: &Book, key: VerseKey) -> (u32, u32) {
    let verse = book
        .verses
        .iter()
        .find(|verse| verse.key() == key)
        .expect("the book declares this key");
    (verse.text().from(), verse.text().to())
}

// ------------------------------------------------------------------- cases

#[test]
fn a_target_with_no_source_book_gets_no_rows_and_no_facts() {
    let books = [book(*b"MRK", &filler_rows())];
    let (rows, paired) = judge_with(&books, &[], LengthConfig::default());
    assert!(rows.is_empty(), "no source, no ratios");
    assert!(paired.facts.is_empty(), "an absent source is the contract");
    assert_eq!(paired.total(), 0);

    // And a source for a different book is the same silence.
    let elsewhere = [(BookKey::new(*b"GEN"), filler_source())];
    assert!(judge(&books, &elsewhere).is_empty());
}

/// The denominator a host shows beside the rows: one count per target book,
/// of the units that produced a ratio.
#[test]
fn the_paired_count_is_the_units_that_produced_a_ratio() {
    let mut rows = filler_rows();
    // One empty target and one absent key: neither pairs into a ratio.
    rows.push(row(1, 61, 61, ""));
    rows.push(row(1, 62, 62, &"a".repeat(BASE)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(source_row(61));

    let corpus = Corpus::try_new(&books).unwrap();
    let lengths = [SourceLengths {
        book: BookKey::new(*b"MRK"),
        verses: &source,
        words: None,
    }];
    let (_, paired) = analyze_paired(
        &corpus,
        &Substrate,
        &sous_core::JudgingConfig::default(),
        &lengths,
    );
    assert_eq!(paired.units, vec![FILLER as u32]);
    assert!(
        paired
            .facts
            .contains(&sous_core::AlignmentFact::TargetOnly {
                book: BookKey::new(*b"MRK"),
                key: VerseKey::new(1, 62, 62).unwrap(),
            }),
        "{:?}",
        paired.facts
    );
}

#[test]
fn half_a_verse_fires_short_and_a_doubled_verse_fires_long() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 61, &"a".repeat(BASE / 2)));
    rows.push(row(1, 62, 62, &"a".repeat(BASE * 2)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(source_row(61));
    source.push(source_row(62));

    let judged = judge(&books, &[(BookKey::new(*b"MRK"), source)]);
    let short = span_of(&books[0], VerseKey::new(1, 61, 61).unwrap());
    let long = span_of(&books[0], VerseKey::new(1, 62, 62).unwrap());
    let named: Vec<(u32, u32)> = judged.iter().map(|row| (row.1, row.2)).collect();
    assert_eq!(
        named,
        vec![short, long],
        "the two extremes and nothing else"
    );

    let (_, _, short_book, short_project) = (judged[0].0, judged[0].1, judged[0].3, judged[0].4);
    assert!(short_book.unwrap() < 0, "shorter than typical is negative");
    assert!(short_project.unwrap() < 0);
    assert!(judged[1].3.unwrap() > 0, "longer than typical is positive");
    assert!(judged[1].4.unwrap() > 0);
}

fn source_row(number: u16) -> SourceVerse {
    verse(1, number, number, BASE as u32)
}

#[test]
fn an_empty_unit_on_either_side_produces_no_ratio() {
    let mut rows = filler_rows();
    // An empty target beside a full source is the most extreme ratio there
    // is, and it still produces nothing.
    rows.push(row(1, 61, 61, ""));
    rows.push(row(1, 62, 62, &"a".repeat(BASE * 3)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(source_row(61));
    // An empty source, against a target three times the usual length.
    source.push(verse(1, 62, 62, 0));

    assert!(
        judge(&books, &[(BookKey::new(*b"MRK"), source)]).is_empty(),
        "an empty side has no ratio to be an outlier of"
    );
}

#[test]
fn a_book_under_min_verses_is_judged_by_the_project_alone() {
    // MRK carries the distribution; LUK carries three verses and one outlier.
    let mut short_book = vec![
        row(1, 1, 1, &"a".repeat(BASE)),
        row(1, 2, 2, &"a".repeat(BASE + 3)),
        row(1, 3, 3, &"a".repeat(BASE / 2)),
    ];
    short_book.truncate(3);
    let books = [book(*b"MRK", &filler_rows()), book(*b"LUK", &short_book)];
    let source = vec![
        (BookKey::new(*b"MRK"), filler_source()),
        (
            BookKey::new(*b"LUK"),
            (1..=3).map(|at| verse(1, at, at, BASE as u32)).collect(),
        ),
    ];

    let judged = judge(&books, &source);
    let named: Vec<u16> = judged.iter().map(|row| row.0).collect();
    assert_eq!(named, vec![1], "only LUK's half-length verse fires");
    assert_eq!(
        judged[0].3, None,
        "the book lane is the unavailable sentinel under min_verses"
    );
    assert!(judged[0].4.unwrap() < 0, "the project lane carries it");
}

#[test]
fn a_bridge_pairs_with_the_same_bridge_as_one_ratio() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 62, &"a".repeat(BASE / 2)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(verse(1, 61, 62, BASE as u32));

    let judged = judge(&books, &[(BookKey::new(*b"MRK"), source)]);
    assert_eq!(judged.len(), 1, "one bridge, one ratio, one row");
    assert_eq!(
        (judged[0].1, judged[0].2),
        span_of(&books[0], VerseKey::new(1, 61, 62).unwrap())
    );
}

#[test]
fn a_bridge_pairs_with_its_exact_constituents_coalesced() {
    // The target bridges 61-62; the source spells both out. One ratio over
    // the two range totals, and the row spans the bridge's whole target span.
    let mut rows = filler_rows();
    rows.push(row(1, 61, 62, &"a".repeat(BASE)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(verse(1, 61, 61, BASE as u32));
    source.push(verse(1, 62, 62, BASE as u32));

    let judged = judge(&books, &[(BookKey::new(*b"MRK"), source)]);
    assert_eq!(judged.len(), 1, "target 40 over source 80 is one outlier");
    assert_eq!(
        (judged[0].1, judged[0].2),
        span_of(&books[0], VerseKey::new(1, 61, 62).unwrap())
    );
    assert!(judged[0].3.unwrap() < 0, "half the coalesced source");

    // The other direction: the target spells them out and the source bridges.
    let mut rows = filler_rows();
    rows.push(row(1, 61, 61, &"a".repeat(BASE / 4)));
    rows.push(row(1, 62, 62, &"a".repeat(BASE / 4)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(verse(1, 61, 62, BASE as u32));

    let judged = judge(&books, &[(BookKey::new(*b"MRK"), source)]);
    assert_eq!(judged.len(), 1, "still one row for the coalesced unit");
    let (from, _) = span_of(&books[0], VerseKey::new(1, 61, 61).unwrap());
    let (_, to) = span_of(&books[0], VerseKey::new(1, 62, 62).unwrap());
    assert_eq!(
        (judged[0].1, judged[0].2),
        (from, to),
        "one row over the bounding target range"
    );
}

#[test]
fn a_partial_overlap_abstains_and_says_so() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 62, &"a".repeat(BASE / 8)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(verse(1, 62, 63, BASE as u32));

    let (judged, facts) = judge_with(
        &books,
        &[(BookKey::new(*b"MRK"), source)],
        LengthConfig::default(),
    );
    assert!(judged.is_empty(), "no unit, so no row, however extreme");
    assert!(
        facts
            .facts
            .contains(&sous_core::AlignmentFact::PartialOverlap {
                book: BookKey::new(*b"MRK"),
                target: VerseKey::new(1, 61, 62).unwrap(),
                source: VerseKey::new(1, 62, 63).unwrap(),
            }),
        "{facts:?}"
    );
}

#[test]
fn equal_duplicate_keys_pair_by_occurrence_and_unequal_ones_abstain() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 61, &"a".repeat(BASE)));
    rows.push(row(1, 61, 61, &"a".repeat(BASE / 8)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(source_row(61));
    source.push(source_row(61));

    let judged = judge(&books, &[(BookKey::new(*b"MRK"), source)]);
    assert_eq!(judged.len(), 1, "the second occurrence is the outlier");
    let second = books[0]
        .verses
        .iter()
        .filter(|verse| verse.key() == VerseKey::new(1, 61, 61).unwrap())
        .nth(1)
        .expect("two rows under the key");
    assert_eq!(
        (judged[0].1, judged[0].2),
        (second.text().from(), second.text().to())
    );

    // One source row against two target rows is ambiguous, not a prefix pair.
    let mut source = filler_source();
    source.push(source_row(61));
    let (judged, facts) = judge_with(
        &books,
        &[(BookKey::new(*b"MRK"), source)],
        LengthConfig::default(),
    );
    assert!(judged.is_empty());
    assert!(
        facts
            .facts
            .contains(&sous_core::AlignmentFact::AmbiguousDuplicate {
                book: BookKey::new(*b"MRK"),
                key: VerseKey::new(1, 61, 61).unwrap(),
            }),
        "{facts:?}"
    );
}

#[test]
fn shuffling_the_source_rows_does_not_change_a_row() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 61, &"a".repeat(BASE / 2)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(source_row(61));
    let forward = judge(&books, &[(BookKey::new(*b"MRK"), source.clone())]);

    // Distinct keys, so occurrence ordinals cannot move: order is not
    // alignment, and reversing the producer's rows proves it.
    source.reverse();
    assert_eq!(judge(&books, &[(BookKey::new(*b"MRK"), source)]), forward);
    assert_eq!(forward.len(), 1);
}

#[test]
fn the_thresholds_move_the_verdict_and_disabling_silences_it() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 61, &"a".repeat(BASE / 2)));
    rows.push(row(1, 62, 62, &"a".repeat(BASE * 2)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(source_row(61));
    source.push(source_row(62));
    let source = [(BookKey::new(*b"MRK"), source)];

    assert_eq!(judge(&books, &source).len(), 2);

    let long_only = LengthConfig {
        z_short: 1_000.0,
        ..LengthConfig::default()
    };
    let judged = judge_with(&books, &source, long_only).0;
    assert_eq!(judged.len(), 1, "the short side is out of reach");
    assert!(judged[0].3.unwrap() > 0);

    let loose = LengthConfig {
        z_long: 1_000.0,
        z_short: 1_000.0,
        ..LengthConfig::default()
    };
    assert!(judge_with(&books, &source, loose).0.is_empty());

    let off = LengthConfig {
        enabled: false,
        ..LengthConfig::default()
    };
    assert!(judge_with(&books, &source, off).0.is_empty());
}

#[test]
fn replacing_the_source_legitimately_changes_the_rows() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 61, &"a".repeat(BASE / 2)));
    let books = [book(*b"MRK", &rows)];

    // A source that matches the target verse for verse: nothing is unusual.
    let twin = book(*b"MRK", &rows);
    let matched = [(BookKey::new(*b"MRK"), source_lengths(&twin))];
    assert!(
        judge(&books, &matched).is_empty(),
        "against itself, every ratio is one"
    );

    // A source of even verses: verse 61 is now half of its counterpart.
    let mut even = filler_source();
    even.push(source_row(61));
    let judged = judge(&books, &[(BookKey::new(*b"MRK"), even)]);
    assert_eq!(judged.len(), 1);
    assert_eq!(
        (judged[0].1, judged[0].2),
        span_of(&books[0], VerseKey::new(1, 61, 61).unwrap())
    );
}

#[test]
fn a_lengths_row_carries_both_scopes_and_the_span_it_is_about() {
    let mut rows = filler_rows();
    rows.push(row(1, 61, 61, &"a".repeat(BASE / 2)));
    let books = [book(*b"MRK", &rows)];
    let mut source = filler_source();
    source.push(source_row(61));

    let judged = judge(&books, &[(BookKey::new(*b"MRK"), source)]);
    assert_eq!(judged.len(), 1);
    let (index, from, to, book_lane, project_lane) = judged[0];
    assert_eq!(index, 0);
    assert_eq!(
        (from, to),
        span_of(&books[0], VerseKey::new(1, 61, 61).unwrap())
    );
    // One book is the whole project, so the two scopes see the same sample
    // and read the same value.
    assert!(book_lane.is_some() && book_lane == project_lane);
}
