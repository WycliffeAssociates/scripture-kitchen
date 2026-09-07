//! v1's paired length-ratio survey, reproduced on v2's rule.
//!
//!     cargo run -p sous-core --release --example paired_survey
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text`, `<range>` placeholders
//! fused into one interval unit) and reports the four things the comparator
//! row records:
//!
//! 1. clean volume at the defaults, every tier corpus against a fixed source;
//! 2. seeded truncations — 10 / 20 / 50% of a verse's graphemes dropped —
//!    detected or not;
//! 3. the small-book fallback: books under `min_verses` judged by the project;
//! 4. source sensitivity: one target against two different sources.
//!
//! Not a test: it prints tables for the ledger rather than asserting on them.
//! Fails loudly if any of the 8 named tier files is missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashSet;
use sous_core::substrate::VerseLength;
use sous_core::unicode::atoms::count_atoms;
use sous_core::{
    BookKey, Chapter, ChapterInput, ChapterKey, ChapterObs, ChapterPass, ChapterRow, FindingKind,
    Findings, LengthConfig, SourceLengths, SourceVerse, Substrate, TargetLengths, TextRange, Verse,
    VerseKey, judge_lengths,
};

const CORPORA: &[&str] = &[
    "WA-en-ulb",
    "amh",
    "francl",
    "grcsr",
    "hin2017",
    "nya",
    "spaRV1909",
    "swhulb",
];

/// The comparator every clean row is measured against, and the target of the
/// seeded and sensitivity runs.
const TARGET: &str = "WA-en-ulb";
/// The source the clean run pairs every other corpus with.
const SOURCE: &str = "spaRV1909";
/// The second source of the sensitivity run.
const OTHER: &str = "francl";

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora");
    let held: BTreeMap<&str, Corpus> = CORPORA
        .iter()
        .map(|name| {
            let path = dir.join(format!("{name}.txt"));
            assert!(
                path.is_file(),
                "test-tier corpus {name} must be present at {}",
                path.display()
            );
            (*name, Corpus::read(&path))
        })
        .collect();

    println!("### 1. clean volume at the defaults ###\n");
    println!(
        "{:<12} {:>8} {:>7} {:>9} {:>7} {:>7} {:>8}",
        "target", "source", "paired", "rows", "per 1k", "books", "unpaired"
    );
    for name in CORPORA {
        let source = if *name == SOURCE { TARGET } else { SOURCE };
        let judged = judge(&held[name], &held[source], &LengthConfig::default());
        println!(
            "{name:<12} {source:>8} {:>7} {:>9} {:>7.2} {:>7} {:>8}",
            judged.paired,
            judged.rows.len(),
            judged.per_thousand(),
            judged.books,
            judged.facts,
        );
    }

    println!("\n### 2. seeded truncations, {TARGET} against {SOURCE} ###\n");
    println!(
        "{:<8} {:>7} {:>8} {:>9} {:>9}",
        "dropped", "seeded", "detected", "share", "other rows"
    );
    let clean = judge(&held[TARGET], &held[SOURCE], &LengthConfig::default());
    for drop in [10u32, 20, 50] {
        let seeded = held[TARGET].truncated(drop, SEED_EVERY);
        let judged = judge(&seeded.corpus, &held[SOURCE], &LengthConfig::default());
        let fired: FxHashSet<(u16, u32, u32)> = judged.rows.iter().copied().collect();
        let detected = seeded
            .spans
            .iter()
            .filter(|span| fired.contains(span))
            .count();
        println!(
            "{:<8} {:>7} {:>8} {:>8.1}% {:>10}",
            format!("{drop}%"),
            seeded.spans.len(),
            detected,
            detected as f64 / seeded.spans.len() as f64 * 100.0,
            judged.rows.len() - detected,
        );
    }
    println!(
        "clean run over the same pair fires {} rows",
        clean.rows.len()
    );

    println!("\n### 3. the small-book fallback ###\n");
    let judged = judge(&held[TARGET], &held[SOURCE], &LengthConfig::default());
    println!(
        "books under min_verses {}: {} of {}; they carry {} rows, {} of them with an \
         unavailable book lane",
        LengthConfig::default().min_verses,
        judged.small_books,
        judged.books,
        judged.small_rows,
        judged.small_rows_without_book_scope,
    );
    println!(
        "rows whose book lane is unavailable anywhere: {} of {}",
        judged.rows_without_book_scope,
        judged.rows.len()
    );

    println!("\n### 4. source sensitivity ###\n");
    let against_spanish = judge(&held[TARGET], &held[SOURCE], &LengthConfig::default());
    let against_french = judge(&held[TARGET], &held[OTHER], &LengthConfig::default());
    let spanish: FxHashSet<(u16, u32, u32)> = against_spanish.rows.iter().copied().collect();
    let french: FxHashSet<(u16, u32, u32)> = against_french.rows.iter().copied().collect();
    let shared = spanish.intersection(&french).count();
    println!(
        "{TARGET} against {SOURCE}: {} rows over {} paired units",
        spanish.len(),
        against_spanish.paired
    );
    println!(
        "{TARGET} against {OTHER}: {} rows over {} paired units",
        french.len(),
        against_french.paired
    );
    println!(
        "shared {shared}; only-{SOURCE} {}; only-{OTHER} {}; overlap {:.1}% of the union",
        spanish.len() - shared,
        french.len() - shared,
        shared as f64 / (spanish.len() + french.len() - shared) as f64 * 100.0,
    );
}

/// Every `SEED_EVERY`-th paired verse of the target is truncated, so the
/// seeded set is spread over every book rather than clustered.
const SEED_EVERY: usize = 40;

// ── One corpus ──────────────────────────────────────────────────────────

/// One tier corpus as a list of books, each already laid out as projected
/// text with chapter and verse rows over it.
struct Corpus {
    books: Vec<Book>,
}

struct Book {
    key: BookKey,
    text: String,
    chapters: Vec<Chapter>,
    verses: Vec<Verse>,
}

/// A target with some of its verses shortened, and the spans a row over each
/// would name.
struct Seeded {
    corpus: Corpus,
    spans: Vec<(u16, u32, u32)>,
}

impl Corpus {
    fn read(path: &Path) -> Self {
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let mut units: Vec<(String, Vec<Unit>)> = Vec::new();
        for line in raw.lines() {
            let Some((code, chapter, verse, text)) = parse_row(line) else {
                continue;
            };
            if units.last().is_none_or(|(seen, _)| seen != code) {
                units.push((code.to_string(), Vec::new()));
            }
            let rows = &mut units.last_mut().expect("just pushed").1;
            if text == "<range>" {
                if let Some(open) = rows.last_mut()
                    && open.chapter == chapter
                    && open.last + 1 == verse
                {
                    open.last = verse;
                }
                continue;
            }
            rows.push(Unit {
                chapter,
                first: verse,
                last: verse,
                text: text.to_string(),
            });
        }
        Self {
            books: units
                .into_iter()
                .filter_map(|(code, rows)| Book::lay_out(&code, rows))
                .collect(),
        }
    }

    /// Every `every`-th verse cut to `100 - drop` percent of its graphemes.
    fn truncated(&self, drop: u32, every: usize) -> Seeded {
        let mut books = Vec::with_capacity(self.books.len());
        let mut spans = Vec::new();
        let mut at = 0usize;
        for (index, book) in self.books.iter().enumerate() {
            let mut rows: Vec<Unit> = Vec::with_capacity(book.verses.len());
            let mut cut: Vec<usize> = Vec::new();
            for verse in &book.verses {
                let text = &book.text[verse.text().from() as usize..verse.text().to() as usize];
                let text = text.trim_end_matches('\n');
                at += 1;
                let keep = if at.is_multiple_of(every) && count_atoms(text) >= 20 {
                    cut.push(rows.len());
                    let atoms = count_atoms(text);
                    let target = (u64::from(atoms) * u64::from(100 - drop) / 100) as usize;
                    prefix_of(text, target)
                } else {
                    text.to_string()
                };
                rows.push(Unit {
                    chapter: verse.key().chapter(),
                    first: verse.key().first(),
                    last: verse.key().last(),
                    text: keep,
                });
            }
            let laid = Book::lay_out_keyed(book.key, rows).expect("the source book laid out once");
            for row in cut {
                let verse = laid.verses[row];
                spans.push((
                    u16::try_from(index).expect("the tier is under 65k books"),
                    verse.text().from(),
                    verse.text().to(),
                ));
            }
            books.push(laid);
        }
        Seeded {
            corpus: Corpus { books },
            spans,
        }
    }
}

/// The longest prefix of `text` holding at most `atoms` grapheme clusters.
fn prefix_of(text: &str, atoms: usize) -> String {
    let mut out = String::new();
    for (at, _) in text.char_indices() {
        if count_atoms(&text[..at]) > atoms as u32 {
            break;
        }
        out = text[..at].to_string();
    }
    if out.is_empty() {
        text.to_string()
    } else {
        out
    }
}

struct Unit {
    chapter: u16,
    first: u16,
    last: u16,
    text: String,
}

fn parse_row(line: &str) -> Option<(&str, u16, u16, &str)> {
    let (address, text) = line.split_once('\t')?;
    let mut parts = address.split_whitespace();
    let code = parts.next()?;
    let (chapter, verse) = parts.next()?.split_once(':')?;
    Some((code, chapter.parse().ok()?, verse.parse().ok()?, text))
}

impl Book {
    fn lay_out(code: &str, rows: Vec<Unit>) -> Option<Self> {
        let mut bytes = [b' '; 3];
        for (slot, byte) in bytes.iter_mut().zip(code.as_bytes()) {
            *slot = byte.to_ascii_uppercase();
        }
        Self::lay_out_keyed(BookKey::new(bytes), rows)
    }

    /// Key order, not file order: a projection has to be monotone, and a vref
    /// export's line order is not always.
    fn lay_out_keyed(key: BookKey, mut rows: Vec<Unit>) -> Option<Self> {
        rows.sort_by_key(|row| (row.chapter, row.first, row.last));
        let mut text = String::new();
        let mut verses = Vec::new();
        let mut chapters: Vec<Chapter> = Vec::new();
        let mut open: Option<(u16, u32)> = None;
        for row in &rows {
            let Ok(verse) = VerseKey::new(row.chapter, row.first, row.last) else {
                continue;
            };
            if open.is_none_or(|(number, _)| number != row.chapter) {
                if let Some((number, from)) = open.take() {
                    chapters.push(chapter_row(number, from, text.len() as u32));
                }
                open = Some((row.chapter, text.len() as u32));
            }
            let from = text.len() as u32;
            text.push_str(&row.text);
            text.push('\n');
            verses.push(Verse::new(
                verse,
                TextRange::new(from, text.len() as u32).expect("a verse grows forward"),
            ));
        }
        let (number, from) = open?;
        chapters.push(chapter_row(number, from, text.len() as u32));
        Some(Self {
            key,
            text,
            chapters,
            verses,
        })
    }

    /// This book's verse lane, from the shipped substrate walk and fold.
    fn lengths(&self) -> Vec<VerseLength> {
        let mut rows: Vec<(u32, ChapterRow)> = Vec::with_capacity(self.chapters.len());
        let mut at = 0usize;
        let mut chapter_verses: Vec<Verse> = Vec::new();
        for chapter in &self.chapters {
            let span = chapter.text();
            chapter_verses.clear();
            while self
                .verses
                .get(at)
                .is_some_and(|verse| verse.key().chapter() == chapter.number())
            {
                let verse = self.verses[at];
                let text = verse.text();
                chapter_verses.push(Verse::new(
                    verse.key(),
                    TextRange::new(text.from() - span.from(), text.to() - span.from())
                        .expect("a chapter-relative range keeps its order"),
                ));
                at += 1;
            }
            rows.push((
                span.from(),
                Substrate.map(ChapterInput {
                    text: &self.text[span.from() as usize..span.to() as usize],
                    verses: &chapter_verses,
                    key: ChapterKey::new(self.key, chapter.number()),
                }),
            ));
        }
        let borrowed: Vec<ChapterObs<&ChapterRow>> = rows
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();
        Substrate.fold(&borrowed).verses().to_vec()
    }

    fn source_rows(&self) -> Vec<SourceVerse> {
        self.verses
            .iter()
            .map(|verse| {
                let span = verse.text();
                SourceVerse::new(
                    verse.key(),
                    count_atoms(&self.text[span.from() as usize..span.to() as usize]),
                )
            })
            .collect()
    }
}

fn chapter_row(number: u16, from: u32, to: u32) -> Chapter {
    Chapter::new(
        number,
        TextRange::new(from, to).expect("a chapter grows forward"),
    )
    .expect("chapter numbers start at one")
}

// ── One judged pair ─────────────────────────────────────────────────────

struct Judged {
    rows: Vec<(u16, u32, u32)>,
    rows_without_book_scope: usize,
    paired: usize,
    books: usize,
    small_books: usize,
    small_rows: usize,
    small_rows_without_book_scope: usize,
    facts: usize,
}

impl Judged {
    fn per_thousand(&self) -> f64 {
        if self.paired == 0 {
            return 0.0;
        }
        self.rows.len() as f64 * 1_000.0 / self.paired as f64
    }
}

fn judge(target: &Corpus, source: &Corpus, config: &LengthConfig) -> Judged {
    let lengths: Vec<Vec<VerseLength>> = target.books.iter().map(Book::lengths).collect();
    let sources: Vec<(BookKey, Vec<SourceVerse>)> = source
        .books
        .iter()
        .map(|book| (book.key, book.source_rows()))
        .collect();
    let target_rows: Vec<TargetLengths<'_>> = target
        .books
        .iter()
        .zip(&lengths)
        .map(|(book, verses)| TargetLengths {
            book: book.key,
            verses,
            text: &book.text,
        })
        .collect();
    let source_rows: Vec<SourceLengths<'_>> = sources
        .iter()
        .map(|(key, verses)| SourceLengths {
            book: *key,
            verses,
            words: None,
        })
        .collect();

    let mut out = Findings::new(
        target
            .books
            .iter()
            .map(|book| book.text.len() as u32)
            .collect(),
    );
    let paired = judge_lengths(&target_rows, &source_rows, config, &mut out);

    let small: FxHashSet<usize> = paired
        .units
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0 && **count < config.min_verses)
        .map(|(index, _)| index)
        .collect();

    let mut rows = Vec::new();
    let mut rows_without_book_scope = 0;
    let mut small_rows = 0;
    let mut small_rows_without_book_scope = 0;
    for row in out.rows() {
        let FindingKind::LengthProportionality(digest) = row.kind() else {
            continue;
        };
        rows.push((row.book_idx().get(), row.from(), row.to()));
        let unavailable = digest.book_scope().is_none();
        rows_without_book_scope += usize::from(unavailable);
        if small.contains(&usize::from(row.book_idx().get())) {
            small_rows += 1;
            small_rows_without_book_scope += usize::from(unavailable);
        }
    }

    Judged {
        rows,
        rows_without_book_scope,
        paired: paired.total() as usize,
        books: target.books.len(),
        small_books: small.len(),
        small_rows,
        small_rows_without_book_scope,
        facts: paired.facts.len(),
    }
}
