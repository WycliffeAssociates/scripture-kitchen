//! The presence rule's own cases, one test per sentence of
//! `sous-chef/rules/presence-shear.md`.
//!
//! Instrument: SYNTHETIC. Every book here is built in this file, so a case is
//! readable as the rule that names it. Corpus behaviour is the ledger's job.
//!
//! The length lane is off in most cases: a presence row is a statement about
//! keys, and nothing here should need a distribution to make it.

use sous_core::{
    BookKey, Chapter, Corpus, FindingKind, LengthConfig, PresenceKind, ProjectedBook,
    SourceLengths, SourceVerse, Substrate, TextRange, Verse, VerseKey, analyze_paired,
};

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

fn verse(chapter: u16, first: u16, last: u16, graphemes: u32) -> SourceVerse {
    SourceVerse::new(
        VerseKey::new(chapter, first, last).expect("a well-formed key"),
        graphemes,
    )
}

/// The presence lane alone: the ratio lane judges nothing here, so every row
/// a case sees is the one the rule under test pushed.
fn presence_only() -> LengthConfig {
    LengthConfig {
        enabled: false,
        presence: true,
        ..LengthConfig::default()
    }
}

/// Every presence row as `(book index, from, to, kind, keys)`.
type Rows = Vec<(u16, u32, u32, PresenceKind, u32)>;

fn judge(target: &[Book], source: &[(BookKey, Vec<SourceVerse>)]) -> Rows {
    judge_with(target, source, presence_only()).0
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
            FindingKind::Presence(digest) => Some((
                finding.book_idx().get(),
                finding.from(),
                finding.to(),
                digest.kind(),
                digest.keys(),
            )),
            _ => None,
        })
        .collect();
    (rows, paired)
}

/// One target verse's projected span.
fn span_of(book: &Book, chapter: u16, number: u16) -> (u32, u32) {
    let text = book
        .verses
        .iter()
        .find(|verse| verse.key() == VerseKey::new(chapter, number, number).unwrap())
        .expect("the book holds that verse")
        .text();
    (text.from(), text.to())
}

/// The end of the target verse a key would be inserted after.
fn ends_after(book: &Book, chapter: u16, number: u16) -> u32 {
    span_of(book, chapter, number).1
}

// ------------------------------------------------------------------- cases

#[test]
fn one_missing_verse_lands_at_the_preceding_verses_end_with_one_key() {
    let target = [book(
        *b"MRK",
        &[
            row(1, 1, 1, "aaaa"),
            row(1, 2, 2, "bbbb"),
            row(1, 4, 4, "dddd"),
        ],
    )];
    let source = [(
        BookKey::new(*b"MRK"),
        vec![
            verse(1, 1, 1, 4),
            verse(1, 2, 2, 4),
            verse(1, 3, 3, 4),
            verse(1, 4, 4, 4),
        ],
    )];
    let at = ends_after(&target[0], 1, 2);
    assert_eq!(
        judge(&target, &source),
        vec![(0, at, at, PresenceKind::Missing, 1)]
    );
}

#[test]
fn a_whole_missing_chapter_is_one_row_of_its_source_keys() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "aaaa"), row(1, 2, 2, "bbbb")])];
    let mut rows = vec![verse(1, 1, 1, 4), verse(1, 2, 2, 4)];
    rows.extend((1..=30).map(|number| verse(2, number, number, 4)));
    let source = [(BookKey::new(*b"MRK"), rows)];
    // Nothing of chapter 2 is in the target, so the insertion point is the end
    // of the target book.
    let at = ends_after(&target[0], 1, 2);
    assert_eq!(
        judge(&target, &source),
        vec![(0, at, at, PresenceKind::Missing, 30)]
    );
}

#[test]
fn consecutive_extra_target_verses_are_one_row_over_both() {
    let target = [book(
        *b"MRK",
        &[
            row(1, 1, 1, "aaaa"),
            row(1, 2, 2, "bbbb"),
            row(1, 3, 3, "cccc"),
        ],
    )];
    let source = [(BookKey::new(*b"MRK"), vec![verse(1, 1, 1, 4)])];
    let from = span_of(&target[0], 1, 2).0;
    let to = span_of(&target[0], 1, 3).1;
    assert_eq!(
        judge(&target, &source),
        vec![(0, from, to, PresenceKind::Extra, 2)]
    );
}

#[test]
fn an_empty_target_verse_beside_a_nonempty_source_is_an_empty_row() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "aaaa"), row(1, 2, 2, "")])];
    let source = [(
        BookKey::new(*b"MRK"),
        vec![verse(1, 1, 1, 4), verse(1, 2, 2, 4)],
    )];
    let at = ends_after(&target[0], 1, 2);
    assert_eq!(
        judge(&target, &source),
        vec![(0, at, at, PresenceKind::Empty, 1)]
    );
}

#[test]
fn an_empty_target_verse_beside_an_empty_source_says_nothing() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "aaaa"), row(1, 2, 2, "")])];
    let source = [(
        BookKey::new(*b"MRK"),
        vec![verse(1, 1, 1, 4), verse(1, 2, 2, 0)],
    )];
    assert_eq!(judge(&target, &source), Rows::new());
}

#[test]
fn a_target_bridge_over_the_sources_constituents_pairs_and_says_nothing() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "aaaa"), row(1, 3, 4, "cccc")])];
    let source = [(
        BookKey::new(*b"MRK"),
        vec![verse(1, 1, 1, 4), verse(1, 3, 3, 2), verse(1, 4, 4, 2)],
    )];
    assert_eq!(judge(&target, &source), Rows::new());
}

#[test]
fn a_target_with_no_source_book_produces_no_rows() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "aaaa"), row(1, 2, 2, "bbbb")])];
    let source = [(BookKey::new(*b"GEN"), vec![verse(1, 1, 1, 4)])];
    assert_eq!(judge(&target, &source), Rows::new());
    assert_eq!(judge(&target, &[]), Rows::new());
}

#[test]
fn an_ambiguous_duplicate_stays_a_fact_and_never_a_row() {
    let target = [book(
        *b"MRK",
        &[
            row(1, 1, 1, "aaaa"),
            row(1, 2, 2, "bbbb"),
            row(1, 2, 2, "cccc"),
        ],
    )];
    let source = [(
        BookKey::new(*b"MRK"),
        vec![verse(1, 1, 1, 4), verse(1, 2, 2, 4)],
    )];
    let (rows, paired) = judge_with(&target, &source, presence_only());
    assert_eq!(rows, Rows::new());
    assert!(
        paired
            .facts
            .iter()
            .any(|fact| matches!(fact, sous_core::AlignmentFact::AmbiguousDuplicate { .. }))
    );
}

#[test]
fn replacing_the_source_moves_the_missing_rows_and_leaves_the_extra_one() {
    let target = [book(
        *b"MRK",
        &[
            row(1, 1, 1, "aaaa"),
            row(1, 2, 2, "bbbb"),
            row(1, 9, 9, "iiii"),
        ],
    )];
    let key = BookKey::new(*b"MRK");
    let first = [(
        key,
        vec![verse(1, 1, 1, 4), verse(1, 2, 2, 4), verse(1, 3, 3, 4)],
    )];
    let second = [(
        key,
        vec![verse(1, 1, 1, 4), verse(1, 2, 2, 4), verse(1, 4, 4, 4)],
    )];
    let (extra_from, extra_to) = span_of(&target[0], 1, 9);
    let at = ends_after(&target[0], 1, 2);
    assert_eq!(
        judge(&target, &first),
        vec![
            (0, at, at, PresenceKind::Missing, 1),
            (0, extra_from, extra_to, PresenceKind::Extra, 1),
        ]
    );
    assert_eq!(
        judge(&target, &second),
        vec![
            (0, at, at, PresenceKind::Missing, 1),
            (0, extra_from, extra_to, PresenceKind::Extra, 1),
        ]
    );
    // The same span, a different absent key: the row says how many, and the
    // consumer reads which from its own table of contents.
    let (rows, paired) = judge_with(&target, &second, presence_only());
    assert_eq!(rows.len(), 2);
    assert!(paired.facts.iter().any(|fact| matches!(
        fact,
        sous_core::AlignmentFact::SourceOnly { key, .. }
            if *key == VerseKey::new(1, 4, 4).unwrap()
    )));
}

#[test]
fn the_channel_switches_off_without_touching_the_ratio_lane() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "aaaa"), row(1, 2, 2, "bbbb")])];
    let source = [(BookKey::new(*b"MRK"), vec![verse(1, 1, 1, 4)])];
    assert_eq!(judge(&target, &source).len(), 1);
    let off = LengthConfig {
        presence: false,
        ..presence_only()
    };
    assert_eq!(judge_with(&target, &source, off).0, Rows::new());
    assert!(LengthConfig::default().presence);
}
