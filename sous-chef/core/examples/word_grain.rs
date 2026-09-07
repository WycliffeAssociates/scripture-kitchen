//! W1: chapter grain vs book grain for the word rows — what caching one
//! `WordRow` per chapter costs against caching one per book, and what each
//! buys back on a keystroke.
//!
//!     cargo run -p sous-core --release --example word_grain
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text` per line), the same
//! harness as `observation_size.rs`, and groups lines by book+chapter. For
//! each book: chapter grain maps every chapter and sums `WordRow::resident_bytes`
//! over them, then folds to a `WordAggregate`; book grain concatenates the
//! book's chapters with `"\n"` — as the projected book would lay them out —
//! walks that once as a single chapter, and folds the one row alone.
//! `WordAggregate` carries no `resident_bytes` of its own, so its size is
//! estimated as `entries * size_of::<WordTotal>()`, noted as such below.
//!
//! Keystroke cost is timed as median-of-5: chapter grain re-maps the book's
//! median-sized chapter and re-folds the book from its (unchanged) cached
//! rows; book grain re-walks the whole book's text and folds the lone row.
//! Not a test: it prints tables for the ledger. Fails loudly if any of the 8
//! named tier files is missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sous_core::words::{WordAggregate, WordRow, WordTotal, fold_book};
use sous_core::{BookKey, ChapterInput, ChapterKey, ChapterObs, ChapterPass, Words};

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

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

/// `None` for a front-matter row (chapter `?`, e.g. book title/encoding
/// lines some vref exports carry) — not chapter content, silently skipped.
/// Anything else that doesn't fit `BOOK C:V<TAB>text` is a hard failure.
fn parse_ref(line: &str) -> Option<(&str, u32, &str)> {
    let (refpart, text) = line
        .split_once('\t')
        .unwrap_or_else(|| panic!("no tab in vref line: {line:?}"));
    let mut parts = refpart.split_whitespace();
    let book = parts
        .next()
        .unwrap_or_else(|| panic!("no book code in vref line: {line:?}"));
    let cv = parts
        .next()
        .unwrap_or_else(|| panic!("no chapter:verse in vref line: {line:?}"));
    let (chapter, _verse) = cv
        .split_once(':')
        .unwrap_or_else(|| panic!("no ':' in chapter:verse: {line:?}"));
    let chapter: u32 = chapter.parse().ok()?;
    Some((book, chapter, text))
}

fn book_key(code: &str) -> BookKey {
    let bytes = code.as_bytes();
    let mut arr = [b' '; 3];
    for (slot, &b) in arr.iter_mut().zip(bytes.iter()) {
        *slot = b;
    }
    BookKey::new(arr)
}

fn chapter_key(code: &str, chapter: u32) -> ChapterKey {
    ChapterKey::new(book_key(code), chapter as u16)
}

/// One book's chapters in order, each already joined from its verse lines.
struct Book {
    code: String,
    chapters: Vec<(u32, String)>,
}

fn load_books(path: &Path) -> Vec<Book> {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));

    let mut chapters: BTreeMap<(String, u32), String> = BTreeMap::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((book, chapter, text)) = parse_ref(line) else {
            continue;
        };
        let entry = chapters.entry((book.to_string(), chapter)).or_default();
        if !entry.is_empty() {
            entry.push(' ');
        }
        entry.push_str(text);
    }
    assert!(!chapters.is_empty(), "{} has no chapters", path.display());

    // BTreeMap orders by (book, chapter): each book's chapters arrive
    // contiguous and ascending, which is the order a real projection joins.
    let mut books: Vec<Book> = Vec::new();
    for ((book, chapter), text) in chapters {
        match books.last_mut() {
            Some(b) if b.code == book => b.chapters.push((chapter, text)),
            _ => books.push(Book {
                code: book,
                chapters: vec![(chapter, text)],
            }),
        }
    }
    books
}

fn map_chapter(code: &str, chapter: u32, text: &str) -> WordRow {
    Words.map(ChapterInput {
        text,
        verses: &[],
        key: chapter_key(code, chapter),
    })
}

fn fold_rows(rows: &[WordRow]) -> WordAggregate {
    let obs: Vec<ChapterObs<&WordRow>> = rows
        .iter()
        .map(|row| ChapterObs { start: 0, obs: row })
        .collect();
    fold_book(&obs)
}

fn agg_bytes_estimate(agg: &WordAggregate) -> usize {
    size_of_val(agg.words())
}

struct BookGrainReport {
    chapter_mem: u64,
    book_mem: u64,
    chapter_keystroke: Duration,
    book_keystroke: Duration,
    agg_entries: usize,
    agg_bytes: usize,
}

fn measure_book(book: &Book) -> BookGrainReport {
    let rows: Vec<WordRow> = book
        .chapters
        .iter()
        .map(|(c, text)| map_chapter(&book.code, *c, text))
        .collect();
    let chapter_mem: u64 = rows.iter().map(|r| r.resident_bytes() as u64).sum();

    let book_text = book
        .chapters
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let book_row = map_chapter(&book.code, book.chapters[0].0, &book_text);
    let book_mem = book_row.resident_bytes() as u64;
    let book_agg = fold_rows(std::slice::from_ref(&book_row));

    // The median-sized chapter by text length, the one a keystroke lands in.
    let mut by_len: Vec<&(u32, String)> = book.chapters.iter().collect();
    by_len.sort_by_key(|(_, t)| t.len());
    let mid = by_len[by_len.len() / 2];
    let (median_c, median_text) = (mid.0, mid.1.as_str());

    let mut chapter_ks: Vec<Duration> = Vec::with_capacity(5);
    for _ in 0..5 {
        let start = Instant::now();
        let refreshed = map_chapter(&book.code, median_c, median_text);
        let mut refreshed_rows = rows.clone();
        let at = book
            .chapters
            .iter()
            .position(|(c, _)| *c == median_c)
            .unwrap();
        refreshed_rows[at] = refreshed;
        let _agg = fold_rows(&refreshed_rows);
        chapter_ks.push(start.elapsed());
    }

    let mut book_ks: Vec<Duration> = Vec::with_capacity(5);
    for _ in 0..5 {
        let start = Instant::now();
        let row = map_chapter(&book.code, book.chapters[0].0, &book_text);
        let _agg = fold_rows(std::slice::from_ref(&row));
        book_ks.push(start.elapsed());
    }

    BookGrainReport {
        chapter_mem,
        book_mem,
        chapter_keystroke: median_duration(chapter_ks),
        book_keystroke: median_duration(book_ks),
        agg_entries: book_agg.words().len(),
        agg_bytes: agg_bytes_estimate(&book_agg),
    }
}

fn median_duration(mut d: Vec<Duration>) -> Duration {
    d.sort_unstable();
    d[d.len() / 2]
}

fn report(name: &str, path: &Path) {
    let books = load_books(path);
    let n_books = books.len();

    let mut chapter_mem_total = 0u64;
    let mut book_mem_total = 0u64;
    let mut chapter_ks: Vec<Duration> = Vec::with_capacity(n_books);
    let mut book_ks: Vec<Duration> = Vec::with_capacity(n_books);
    let mut agg_entries_sample = 0usize;
    let mut agg_bytes_sample = 0usize;

    for book in &books {
        let r = measure_book(book);
        chapter_mem_total += r.chapter_mem;
        book_mem_total += r.book_mem;
        chapter_ks.push(r.chapter_keystroke);
        book_ks.push(r.book_keystroke);
        agg_entries_sample = r.agg_entries;
        agg_bytes_sample = r.agg_bytes;
    }

    let chapter_mb = chapter_mem_total as f64 / 1e6;
    let book_mb = book_mem_total as f64 / 1e6;
    let chapter_ks_med = median_duration(chapter_ks);
    let book_ks_med = median_duration(book_ks);

    println!("=== {name} ({n_books} books) ===");
    println!(
        "  chapter grain: {chapter_mb:.3} MB/Bible, {:.1} us/keystroke (median over books)",
        chapter_ks_med.as_secs_f64() * 1e6
    );
    println!(
        "  book grain:    {book_mb:.3} MB/Bible, {:.1} us/keystroke (median over books)",
        book_ks_med.as_secs_f64() * 1e6
    );
    println!(
        "  WordAggregate estimate (entries * size_of::<WordTotal>(), one book sampled): {agg_entries_sample} entries, {agg_bytes_sample} B"
    );
    let extra_mb = chapter_mb - book_mb;
    let saved_us = book_ks_med.as_secs_f64() * 1e6 - chapter_ks_med.as_secs_f64() * 1e6;
    println!(
        "  chapter grain costs {extra_mb:.3} MB more and saves {saved_us:.1} us per keystroke"
    );
}

fn main() {
    println!(
        "size_of::<WordTotal>() = {} B (WordAggregate carries no resident_bytes; estimated as entries * this)\n",
        size_of::<WordTotal>()
    );
    let dir = corpora_dir();
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        assert!(
            path.is_file(),
            "test-tier corpus {} must be present at {}",
            name,
            path.display()
        );
        report(name, &path);
        println!();
    }
}
