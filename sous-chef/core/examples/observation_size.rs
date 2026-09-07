//! First-draft sizing for a per-chapter Level 1b observation and the Stage 4
//! word lanes, measured on the committed 8-corpus test tier, beside the real
//! `WordRow` the shipped walk builds.
//!
//!     cargo run -p sous-core --release --example observation_size
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text` per line), groups lines
//! by book+chapter, and reports scalar/pair/run/word counts plus derived
//! byte sizes under several candidate encodings. Not a test: it prints
//! tables for the ledger rather than asserting on them. Fails loudly if any
//! of the 8 named tier files is missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::unicode::{class_of, is_glue};
use sous_core::words::WordCount;
use sous_core::{BookKey, ChapterInput, ChapterKey, ChapterPass, Words};

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

fn main() {
    println!("size_of::<WordCount>() = {} B\n", size_of::<WordCount>());
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

// --- classification helpers -------------------------------------------

/// Not alphabetic, not glue, not whitespace — the charter's "everything
/// else" bucket.
fn is_nonletter(c: char) -> bool {
    let cl = class_of(c);
    !cl.is_alphabetic() && !is_glue(c) && !cl.is_whitespace()
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum NClass {
    Letter,
    Space,
    Digit,
    Nonletter,
    Edge,
}

/// Glue rides with its base letter (charter invariant 8), so it counts as
/// `Letter` for pair-triple neighbor purposes rather than its own bucket.
fn neighbor_class(c: Option<char>) -> NClass {
    match c {
        None => NClass::Edge,
        Some(c) => {
            let cl = class_of(c);
            if cl.is_whitespace() {
                NClass::Space
            } else if cl.is_decimal_digit() {
                NClass::Digit
            } else if cl.is_alphabetic() || is_glue(c) {
                NClass::Letter
            } else {
                NClass::Nonletter
            }
        }
    }
}

/// Strip leading/trailing nonletter scalars; glue, letters, and digits
/// inside the word survive.
fn strip_word(token: &str) -> &str {
    let start = token
        .char_indices()
        .find(|&(_, c)| !is_nonletter(c))
        .map(|(i, _)| i);
    let end = token
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_nonletter(c))
        .map(|(i, c)| i + c.len_utf8());
    match (start, end) {
        (Some(s), Some(e)) if s < e => &token[s..e],
        _ => "",
    }
}

// --- per-chapter metrics -------------------------------------------------

struct ChapterMetrics {
    scalar_tokens: u64,
    distinct_scalars: u64,
    distinct_nonletter: u64,
    distinct_pairs: u64,
    distinct_runs: u64,
    total_runs: u64,
    word_tokens: u64,
    distinct_words_exact: u64,
    distinct_folded: u64,
    frac_le15: f64,
    frac_le7: f64,
    scalar_lane_bytes: u64,
    lane_a_bytes: f64,
    lane_b_bytes: u64,
    folded_words: FxHashSet<String>,
    text_bytes: u64,
}

fn analyze_chapter(text: &str) -> ChapterMetrics {
    let chars: Vec<char> = text.chars().collect();

    let mut all_scalars: FxHashSet<char> = FxHashSet::default();
    let mut nonletter_nondigit: FxHashSet<char> = FxHashSet::default();
    let mut has_digit = false;
    for &c in &chars {
        all_scalars.insert(c);
        if is_nonletter(c) {
            if class_of(c).is_decimal_digit() {
                has_digit = true;
            } else {
                nonletter_nondigit.insert(c);
            }
        }
    }
    let distinct_nonletter = nonletter_nondigit.len() as u64 + u64::from(has_digit);

    let mut pairs: FxHashSet<(char, NClass, NClass)> = FxHashSet::default();
    for i in 0..chars.len() {
        if is_nonletter(chars[i]) {
            let prev = neighbor_class(i.checked_sub(1).map(|j| chars[j]));
            let next = neighbor_class(chars.get(i + 1).copied());
            pairs.insert((chars[i], prev, next));
        }
    }

    let mut run_shapes: FxHashSet<String> = FxHashSet::default();
    let mut total_runs = 0u64;
    let mut run = String::new();
    for &c in &chars {
        if is_nonletter(c) {
            run.push(c);
        } else if !run.is_empty() {
            total_runs += 1;
            run_shapes.insert(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        total_runs += 1;
        run_shapes.insert(run);
    }
    let run_bytes: u64 = run_shapes
        .iter()
        .map(|s| 4 + 4 * s.chars().count() as u64)
        .sum();

    let mut word_tokens = 0u64;
    let mut words_exact: FxHashSet<String> = FxHashSet::default();
    let mut folded_words: FxHashSet<String> = FxHashSet::default();
    for token in text.split(|c: char| class_of(c).is_whitespace()) {
        let stripped = strip_word(token);
        if stripped.is_empty() {
            continue;
        }
        word_tokens += 1;
        words_exact.insert(stripped.to_string());
        folded_words.insert(stripped.to_lowercase());
    }

    let distinct_folded = folded_words.len() as u64;
    let le15 = folded_words.iter().filter(|w| w.len() <= 15).count() as f64;
    let le7 = folded_words.iter().filter(|w| w.len() <= 7).count() as f64;
    let (frac_le15, frac_le7) = if distinct_folded == 0 {
        (0.0, 0.0)
    } else {
        (le15 / distinct_folded as f64, le7 / distinct_folded as f64)
    };

    let scalar_lane_bytes = 8 * all_scalars.len() as u64 + 12 * pairs.len() as u64 + 8 + run_bytes;
    // Distinct folded forms plus a 5% margin for case/inflection variants
    // sharing a folded key, each variant needing its own count+flags entry.
    let lane_a_bytes = distinct_folded as f64 * 1.05 * 24.0;
    let lane_b_bytes = distinct_folded * 12;

    ChapterMetrics {
        scalar_tokens: chars.len() as u64,
        distinct_scalars: all_scalars.len() as u64,
        distinct_nonletter,
        distinct_pairs: pairs.len() as u64,
        distinct_runs: run_shapes.len() as u64,
        total_runs,
        word_tokens,
        distinct_words_exact: words_exact.len() as u64,
        distinct_folded,
        frac_le15,
        frac_le7,
        scalar_lane_bytes,
        lane_a_bytes,
        lane_b_bytes,
        folded_words,
        text_bytes: text.len() as u64,
    }
}

// --- percentiles -----------------------------------------------------

fn percentile_u64(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p * (sorted.len() as f64 - 1.0)).round() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn percentile_f64(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p * (sorted.len() as f64 - 1.0)).round() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn stats_u64(values: &[u64]) -> (u64, u64, u64) {
    let mut v = values.to_vec();
    v.sort_unstable();
    (
        percentile_u64(&v, 0.5),
        percentile_u64(&v, 0.9),
        *v.last().unwrap_or(&0),
    )
}

fn stats_f64(values: &[f64]) -> (f64, f64, f64) {
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (
        percentile_f64(&v, 0.5),
        percentile_f64(&v, 0.9),
        v.last().copied().unwrap_or(0.0),
    )
}

// --- report ------------------------------------------------------------

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

/// One chapter through the shipped word walk, as a resident cache would hold
/// it. Verse rows only move the forced/free split, never the row's size.
fn real_word_row_bytes(text: &str) -> u64 {
    Words
        .map(ChapterInput {
            text,
            verses: &[],
            key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
        })
        .resident_bytes() as u64
}

fn report(name: &str, path: &Path) {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));

    // Group verse text into per-chapter strings, keyed by (book, chapter).
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

    // The lanes below are estimates from distinct-word counts; this is the
    // shipped row, so the two can be read against each other.
    let mut word_row_bytes: Vec<u64> = Vec::with_capacity(chapters.len());
    let metrics: Vec<((String, u32), ChapterMetrics)> = chapters
        .into_iter()
        .map(|(key, text)| {
            word_row_bytes.push(real_word_row_bytes(&text));
            (key, analyze_chapter(&text))
        })
        .collect();

    // Lane C: distinct folded words at book grain, divided evenly back
    // across the book's chapters for the per-chapter figure.
    let mut book_words: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();
    let mut book_chapters: FxHashMap<String, u32> = FxHashMap::default();
    for ((book, _), m) in &metrics {
        book_words
            .entry(book.clone())
            .or_default()
            .extend(m.folded_words.iter().cloned());
        *book_chapters.entry(book.clone()).or_insert(0) += 1;
    }
    let book_lane_c_bytes: FxHashMap<String, u64> = book_words
        .iter()
        .map(|(book, words)| (book.clone(), words.len() as u64 * 12))
        .collect();

    let mut lane_c_per_chapter: Vec<f64> = Vec::with_capacity(metrics.len());
    for ((book, _), _) in &metrics {
        let total = book_lane_c_bytes[book] as f64;
        let n = book_chapters[book] as f64;
        lane_c_per_chapter.push(total / n);
    }

    let n_chapters = metrics.len();
    let raw_text_mb = metrics.iter().map(|(_, m)| m.text_bytes).sum::<u64>() as f64 / 1e6;

    let scalar_tokens: Vec<u64> = metrics.iter().map(|(_, m)| m.scalar_tokens).collect();
    let distinct_scalars: Vec<u64> = metrics.iter().map(|(_, m)| m.distinct_scalars).collect();
    let distinct_nonletter: Vec<u64> = metrics.iter().map(|(_, m)| m.distinct_nonletter).collect();
    let distinct_pairs: Vec<u64> = metrics.iter().map(|(_, m)| m.distinct_pairs).collect();
    let distinct_runs: Vec<u64> = metrics.iter().map(|(_, m)| m.distinct_runs).collect();
    let total_runs: Vec<u64> = metrics.iter().map(|(_, m)| m.total_runs).collect();
    let word_tokens: Vec<u64> = metrics.iter().map(|(_, m)| m.word_tokens).collect();
    let distinct_words_exact: Vec<u64> = metrics
        .iter()
        .map(|(_, m)| m.distinct_words_exact)
        .collect();
    let distinct_folded: Vec<u64> = metrics.iter().map(|(_, m)| m.distinct_folded).collect();
    let frac_le15: Vec<f64> = metrics.iter().map(|(_, m)| m.frac_le15).collect();
    let frac_le7: Vec<f64> = metrics.iter().map(|(_, m)| m.frac_le7).collect();
    let scalar_lane_bytes: Vec<u64> = metrics.iter().map(|(_, m)| m.scalar_lane_bytes).collect();
    let lane_a_bytes: Vec<f64> = metrics.iter().map(|(_, m)| m.lane_a_bytes).collect();
    let lane_b_bytes: Vec<u64> = metrics.iter().map(|(_, m)| m.lane_b_bytes).collect();

    println!("=== {name} ({n_chapters} chapters, {raw_text_mb:.2} MB raw text) ===");
    println!(
        "{:<28}{:>10}{:>10}{:>10}",
        "metric (per chapter)", "median", "p90", "max"
    );
    let row_u64 = |label: &str, v: &[u64]| {
        let (med, p90, max) = stats_u64(v);
        println!("{label:<28}{med:>10}{p90:>10}{max:>10}");
    };
    row_u64("scalar tokens", &scalar_tokens);
    row_u64("distinct scalars", &distinct_scalars);
    row_u64("distinct nonletter scalars", &distinct_nonletter);
    row_u64("distinct pair triples", &distinct_pairs);
    row_u64("distinct run shapes", &distinct_runs);
    row_u64("total nonletter runs", &total_runs);
    row_u64("word tokens", &word_tokens);
    row_u64("distinct words (exact)", &distinct_words_exact);
    row_u64("distinct folded words", &distinct_folded);

    let (le15_med, le15_p90, le15_max) = stats_f64(&frac_le15);
    let (le7_med, le7_p90, le7_max) = stats_f64(&frac_le7);
    println!(
        "{:<28}{:>10.3}{:>10.3}{:>10.3}",
        "frac folded <=15B (u128)", le15_med, le15_p90, le15_max
    );
    println!(
        "{:<28}{:>10.3}{:>10.3}{:>10.3}",
        "frac folded <=7B (u64)", le7_med, le7_p90, le7_max
    );

    println!();
    println!(
        "{:<28}{:>12}{:>12}{:>14}{:>10}",
        "bytes/chapter", "median", "p90", "total", "MB"
    );
    let row_bytes_u64 = |label: &str, v: &[u64]| {
        let (med, p90, _max) = stats_u64(v);
        let total: u64 = v.iter().sum();
        println!(
            "{label:<28}{med:>12}{p90:>12}{total:>14}{:>10.3}",
            total as f64 / 1e6
        );
    };
    let row_bytes_f64 = |label: &str, v: &[f64]| {
        let (med, p90, _max) = stats_f64(v);
        let total: f64 = v.iter().sum();
        println!(
            "{label:<28}{med:>12.0}{p90:>12.0}{total:>14.0}{:>10.3}",
            total / 1e6
        );
    };
    row_bytes_u64("scalar lanes (1+2+3)", &scalar_lane_bytes);
    row_bytes_u64("word row (WordRow, real)", &word_row_bytes);
    row_bytes_f64("word lane A (u128+flags)", &lane_a_bytes);
    row_bytes_u64("word lane B (u64 hash)", &lane_b_bytes);
    row_bytes_f64("word lane C (book grain)", &lane_c_per_chapter);

    println!();
    println!("word lane C by book (distinct folded words, bytes, MB):");
    let mut books: Vec<&String> = book_words.keys().collect();
    books.sort();
    for book in books {
        let distinct = book_words[book].len();
        let bytes = book_lane_c_bytes[book];
        println!(
            "  {:<8}{:>10} distinct{:>14} B{:>10.3} MB",
            book,
            distinct,
            bytes,
            bytes as f64 / 1e6
        );
    }
}
