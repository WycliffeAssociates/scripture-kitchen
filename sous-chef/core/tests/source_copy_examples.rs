//! The source-copy rule's own cases, one test per sentence of
//! `sous-chef/rules/source-copy-residue.md`.
//!
//! Instrument: SYNTHETIC. Every book here is built in this file, so a case is
//! readable as the rule that names it. Corpus behaviour is the ledger's job.
//!
//! The ratio and presence lanes are off throughout: a source-copy row is a
//! statement about words, and nothing here should need a distribution or a
//! versification difference to make it.

use sous_core::{
    BookKey, Chapter, Corpus, FindingKind, LengthConfig, ProjectedBook, SourceLengths, SourceVerse,
    SourceWords, Substrate, TextRange, Verse, VerseKey, analyze_paired, source_lengths,
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

/// Lays the rows out as one projected book, each verse followed by a newline
/// exactly as a mask keeps one.
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

/// The source-copy lane alone.
fn copy_only(min_run: u32) -> LengthConfig {
    LengthConfig {
        enabled: false,
        presence: false,
        source_copy: true,
        source_copy_min_run: min_run,
        ..LengthConfig::default()
    }
}

/// One judged row: the shared text itself, the run, and the unit's eligible
/// word count.
type Rows = Vec<(String, u32, u32)>;

fn judge(target: &[Book], source: &[Book], min_run: u32) -> Rows {
    let corpus = Corpus::try_new(target).expect("distinct book keys");
    let lanes: Vec<(BookKey, Vec<SourceVerse>, SourceWords)> = source
        .iter()
        .map(|book| {
            (
                ProjectedBook::key(book),
                source_lengths(book),
                SourceWords::of(book),
            )
        })
        .collect();
    let declared: Vec<SourceLengths<'_>> = lanes
        .iter()
        .map(|(key, verses, words)| SourceLengths {
            book: *key,
            verses,
            words: Some(words),
        })
        .collect();
    let judging = sous_core::JudgingConfig {
        lengths: copy_only(min_run),
        ..Default::default()
    };
    let (findings, _) = analyze_paired(&corpus, &Substrate, &judging, &declared);
    findings
        .rows()
        .iter()
        .filter_map(|finding| {
            let FindingKind::SourceCopy(digest) = finding.kind() else {
                return None;
            };
            let text = target[usize::from(finding.book_idx().get())].text();
            Some((
                text[finding.from() as usize..finding.to() as usize].to_string(),
                digest.run(),
                digest.eligible(),
            ))
        })
        .collect()
}

// ------------------------------------------------------------------- cases

/// The claim itself: a sentence pasted from the source is one row over the
/// run, and the run is what the reviewer reads.
#[test]
fn a_pasted_sentence_fires_one_row_over_the_run() {
    let target = [book(
        *b"MRK",
        &[row(
            1,
            1,
            1,
            "Mwanzo wa injili the beginning of the good news",
        )],
    )];
    let source = [book(
        *b"MRK",
        &[row(1, 1, 1, "The beginning of the good news of Jesus")],
    )];
    assert_eq!(
        judge(&target, &source, 3),
        vec![("the beginning of the good news".to_string(), 6, 9)]
    );
}

/// Two shared words are not a run at the shipped floor, and the same two are
/// one at a floor of two.
#[test]
fn two_shared_words_do_not_fire_at_three() {
    let target = [book(
        *b"MRK",
        &[row(1, 1, 1, "alpha of the beta gamma delta")],
    )];
    let source = [book(*b"MRK", &[row(1, 1, 1, "one of the two three four")])];
    assert_eq!(judge(&target, &source, 3), Vec::new());
    assert_eq!(
        judge(&target, &source, 2),
        vec![("of the".to_string(), 2, 6)]
    );
}

/// A translated word inside the run breaks it: three shared words split two
/// and one are two runs, neither of them three.
#[test]
fn a_translated_word_between_shared_ones_breaks_the_run() {
    let target = [book(
        *b"MRK",
        &[row(1, 1, 1, "of the kubwa good alpha beta")],
    )];
    let source = [book(*b"MRK", &[row(1, 1, 1, "of the very good news here")])];
    assert_eq!(judge(&target, &source, 3), Vec::new());
    assert_eq!(
        judge(&target, &source, 2),
        vec![("of the".to_string(), 2, 6)]
    );
}

/// A verse of names shared with the source IS a row. The rule does not say
/// untranslated; the run is the evidence and the reviewer decides.
#[test]
fn a_run_of_shared_names_is_a_legitimate_row() {
    let target = [book(
        *b"MAT",
        &[row(1, 2, 2, "Ibrahimu alimzaa Isaka Yakobo Yuda")],
    )];
    let source = [book(
        *b"MAT",
        &[row(
            1,
            2,
            2,
            "Abraham begat Isaka Yakobo Yuda and his brothers",
        )],
    )];
    assert_eq!(
        judge(&target, &source, 3),
        vec![("Isaka Yakobo Yuda".to_string(), 3, 5)]
    );
}

/// A run of digits holding no letter is not a word at all, so a year neither
/// lengthens a run nor breaks one, and it is not eligible either.
#[test]
fn digits_do_not_extend_a_run() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "alpha of 1995 the beta")])];
    let source = [book(*b"MRK", &[row(1, 1, 1, "one of the two 1995")])];
    assert_eq!(
        judge(&target, &source, 2),
        vec![("of 1995 the".to_string(), 2, 4)]
    );
}

/// Exact scalar sequences, not case-folded ones: a paste preserves case, and
/// case-insensitive matching is a claim nobody has adjudicated.
#[test]
fn a_word_that_differs_only_in_case_does_not_match() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "THE BEGINNING OF THE NEWS")])];
    let source = [book(*b"MRK", &[row(1, 1, 1, "the beginning of the news")])];
    assert_eq!(judge(&target, &source, 2), Vec::new());
}

/// A bridge pairs against the union of its constituents' sets, and the run
/// carries across the bridge's own target rows.
#[test]
fn a_bridge_pairs_against_the_combined_source_set() {
    let target = [book(*b"MRK", &[row(1, 1, 2, "alpha beta gamma delta")])];
    let source = [book(
        *b"MRK",
        &[
            row(1, 1, 1, "alpha beta zeta"),
            row(1, 2, 2, "gamma delta eta"),
        ],
    )];
    assert_eq!(
        judge(&target, &source, 4),
        vec![("alpha beta gamma delta".to_string(), 4, 4)]
    );
}

/// No declared source is no rows, not an error.
#[test]
fn an_absent_source_fires_nothing() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "alpha beta gamma delta")])];
    assert_eq!(judge(&target, &[], 3), Vec::new());
}

/// Replacing the source moves the source-compared rows and nothing else: the
/// target's own text never moved.
#[test]
fn a_source_replacement_moves_only_the_source_compared_rows() {
    let target = [book(
        *b"MRK",
        &[row(1, 1, 1, "alpha beta gamma delta epsilon")],
    )];
    let first = [book(*b"MRK", &[row(1, 1, 1, "alpha beta gamma zeta eta")])];
    let second = [book(*b"MRK", &[row(1, 1, 1, "theta iota kappa lambda mu")])];
    assert_eq!(
        judge(&target, &first, 3),
        vec![("alpha beta gamma".to_string(), 3, 5)]
    );
    assert_eq!(judge(&target, &second, 3), Vec::new());
}

/// The floor is a judge-time knob over cached runs, and it never drops below
/// two: one shared word is not a run under any reading of the claim.
#[test]
fn the_minimum_run_never_falls_below_two() {
    let target = [book(*b"MRK", &[row(1, 1, 1, "alpha zeta eta theta")])];
    let source = [book(*b"MRK", &[row(1, 1, 1, "alpha one two three")])];
    assert_eq!(judge(&target, &source, 0), Vec::new());
    assert_eq!(judge(&target, &source, 1), Vec::new());
}
