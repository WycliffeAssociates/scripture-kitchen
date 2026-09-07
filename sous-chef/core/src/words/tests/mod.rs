//! Coverage for the word walk, the fold, and the channels over them, one
//! lane per module.
//!
//! Instrument: SHAPES — synthetic chapters built here, chosen so one lane
//! fires at a time.

use super::*;
use crate::judge::{
    Channel, Channels, DoublesPolicy, LetterRoster, PatternKey, Staircase, TerminalTable,
};
use crate::pass::analyze_with;
use crate::substrate::{FollowCounts, ScalarKey, Substrate};
use crate::{BookKey, Chapter, Corpus, FindingKind, ProjectedBook, Reasons, TextRange, VerseKey};

/// A terminal table built by hand: `(glyph, upper, lower)` handoffs, judged
/// under the default share.
fn table(rows: &[(char, u32, u32)]) -> TerminalTable {
    let follows: Vec<(ScalarKey, FollowCounts)> = rows
        .iter()
        .map(|&(glyph, upper, lower)| (ScalarKey::of(glyph), FollowCounts::new([upper, lower, 0])))
        .collect();
    TerminalTable::learn(&follows, &JudgingConfig::default())
}

mod channel;
mod doubled;
mod doubles;
mod fold;
mod letter_runs;
mod row;
mod tally;
fn words(text: &str) -> Vec<(&str, Form, Before)> {
    let mut out = Vec::new();
    for_each_word(text, &[], |word| {
        out.push((
            &text[word.from as usize..word.to as usize],
            word.form,
            word.before,
        ))
    });
    out
}

const fn glyph(scalar: char) -> Before {
    Before::Glyph(ScalarKey::of(scalar))
}

fn spans(text: &str) -> Vec<&str> {
    words(text).into_iter().map(|word| word.0).collect()
}

fn hash_of(text: &str) -> u64 {
    let mut found = None;
    for_each_word(text, &[], |word| found = Some(word.hash));
    found.expect("one word")
}

fn fold(texts: &[&str]) -> WordAggregate {
    let rows: Vec<WordRow> = texts.iter().map(|text| row(text)).collect();
    let view: Vec<ChapterObs<&WordRow>> = rows
        .iter()
        .enumerate()
        .map(|(at, obs)| ChapterObs {
            start: at as u32 * 100,
            obs,
        })
        .collect();
    fold_book(&view)
}

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

fn book(key: &[u8; 3], text: impl Into<String>) -> Book {
    let text = text.into();
    let whole = TextRange::new(0, text.len() as u32).unwrap();
    Book {
        key: BookKey::new(*key),
        text,
        chapters: vec![Chapter::new(1, whole).unwrap()],
        verses: vec![Verse::new(VerseKey::new(1, 1, 1).unwrap(), whole)],
    }
}

/// The parts joined by one space, each part its own span; the joining spaces
/// belong to no part, which is what makes a chapter seam a real edge of text.
fn parts(texts: &[&str]) -> (String, Vec<TextRange>) {
    let mut text = String::new();
    let mut spans = Vec::new();
    for part in texts {
        if !text.is_empty() {
            text.push(' ');
        }
        let from = text.len() as u32;
        text.push_str(part);
        spans.push(TextRange::new(from, text.len() as u32).unwrap());
    }
    (text, spans)
}

/// One chapter, one verse per part: verse state crosses the seam, which is
/// exactly what a doubled pair must ride through.
fn versed(key: &[u8; 3], texts: &[&str]) -> Book {
    let (text, spans) = parts(texts);
    let whole = TextRange::new(0, text.len() as u32).unwrap();
    Book {
        key: BookKey::new(*key),
        text,
        chapters: vec![Chapter::new(1, whole).unwrap()],
        verses: spans
            .iter()
            .enumerate()
            .map(|(at, span)| {
                Verse::new(
                    VerseKey::new(1, at as u16 + 1, at as u16 + 1).unwrap(),
                    *span,
                )
            })
            .collect(),
    }
}

/// One chapter per part, each holding one verse: the seam is an edge of text.
fn chaptered(key: &[u8; 3], texts: &[&str]) -> Book {
    let (text, spans) = parts(texts);
    Book {
        key: BookKey::new(*key),
        text,
        chapters: spans
            .iter()
            .enumerate()
            .map(|(at, span)| Chapter::new(at as u16 + 1, *span).unwrap())
            .collect(),
        verses: spans
            .iter()
            .enumerate()
            .map(|(at, span)| Verse::new(VerseKey::new(at as u16 + 1, 1, 1).unwrap(), *span))
            .collect(),
    }
}

/// The word channels judge beside the substrate, because the terminal table
/// is the substrate's follow lane. `(Substrate, Words)` is that seam; the
/// word rows are the ones with a hash.
fn analyzed(books: &[Book], config: &JudgingConfig) -> Findings {
    let corpus = Corpus::try_new(books).expect("a synthetic corpus is valid");
    analyze_with(&corpus, &(Substrate, Words), &(*config, *config))
}

/// The glyph staircase and its floor, for fixtures whose point is the
/// mechanism rather than the shipped volume: the word ladder is a tenth of
/// the glyph one, and a forty-word sample cannot reach three basis points.
fn loose() -> JudgingConfig {
    JudgingConfig {
        word_support_floor: 5,
        word_bands: Staircase::default(),
        ..JudgingConfig::default()
    }
}

fn judged(books: &[Book], config: &JudgingConfig) -> Vec<Pattern> {
    analyzed(books, config)
        .patterns()
        .iter()
        .filter(|row| row.channel.is_word())
        .copied()
        .collect()
}

/// The spans a word channel sited, in book order.
fn sited<'a>(books: &'a [Book], findings: &Findings, reason: Reasons) -> Vec<&'a str> {
    findings
        .rows()
        .iter()
        .filter(|row| match row.kind() {
            FindingKind::Convention(digest) => digest.reasons().contains(reason),
            _ => false,
        })
        .map(|row| {
            let text = &books[usize::from(row.book_idx().get())].text;
            &text[row.from() as usize..row.to() as usize]
        })
        .collect()
}

/// `David` x40 against `david` x2: the two are the row.
fn row(text: &str) -> WordRow {
    super::walk::walk(text, &[])
}

fn totals_of(books: &[&WordAggregate]) -> WordTotals {
    WordTotals::merge(books)
}

mod walk;
