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

use mise::unicode::class_of;
use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::substrate::{ChapterRow, ScalarKey, Substrate, is_nonletter as is_nonletter_class};
use sous_core::words::{
    DoubleCount, DoubleTotal, LETTER_RUN_LANES, WordCount, WordRow, WordTotal, fold_book,
};
use sous_core::{
    BookKey, ChapterInput, ChapterKey, ChapterObs, ChapterPass, TextRange, Verse, VerseKey, Words,
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

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

fn main() {
    println!(
        "size_of::<WordCount>() = {} B   size_of::<DoubleCount>() = {} B   \
         size_of::<DoubleTotal>() = {} B   letter-run row = {} B (chapter) / {} B (book)\n",
        size_of::<WordCount>(),
        size_of::<DoubleCount>(),
        size_of::<DoubleTotal>(),
        size_of::<(ScalarKey, [u16; LETTER_RUN_LANES])>(),
        size_of::<(ScalarKey, [u32; LETTER_RUN_LANES])>()
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

// --- classification helpers -------------------------------------------

/// Not alphabetic, not glue, not whitespace — the charter's "everything
/// else" bucket. Delegates to the real classifier so word-boundary
/// stripping agrees with the engine's own `is_nonletter`; unlike a run
/// atom, a word boundary still strips digits.
fn is_nonletter(c: char) -> bool {
    is_nonletter_class(class_of(c))
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

/// Glyph sizing reads the real [`Substrate`] row (`row`), not a local
/// re-derivation: `runs()` already excludes digits by the engine's own
/// `is_run_atom`, so a mixed digit/punctuation stretch is not a run here
/// either. Word sizing (below) is unaffected and stays its own estimate.
fn analyze_chapter(text: &str, row: &ChapterRow) -> ChapterMetrics {
    let distinct_scalars = row.scalars().len() as u64;
    let distinct_nonletter = row
        .scalars()
        .iter()
        .filter(|(key, _)| key.is_digits() || key.scalar().is_some_and(is_nonletter))
        .count() as u64;
    let distinct_pairs = row.pairs().len() as u64;

    let mut distinct_runs = 0u64;
    let mut total_runs = 0u64;
    let mut run_atoms = 0u64;
    for (atoms, count) in row.runs() {
        distinct_runs += 1;
        total_runs += u64::from(count);
        run_atoms += atoms.len() as u64;
    }
    // `runs` is `(offset, len, count)` triples (three u32s); `run_atoms`
    // holds one `ScalarKey` (u32) per atom of each distinct shape, once.
    let run_bytes = 12 * distinct_runs + 4 * run_atoms;

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

    let scalar_lane_bytes = 8 * distinct_scalars + 12 * distinct_pairs + 8 + run_bytes;
    // Distinct folded forms plus a 5% margin for case/inflection variants
    // sharing a folded key, each variant needing its own count+flags entry.
    let lane_a_bytes = distinct_folded as f64 * 1.05 * 24.0;
    let lane_b_bytes = distinct_folded * 12;

    ChapterMetrics {
        scalar_tokens: u64::from(row.scalar_count()),
        distinct_scalars,
        distinct_nonletter,
        distinct_pairs,
        distinct_runs,
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
fn parse_ref(line: &str) -> Option<(&str, u32, u32, &str)> {
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
    let (chapter, verse) = cv
        .split_once(':')
        .unwrap_or_else(|| panic!("no ':' in chapter:verse: {line:?}"));
    let chapter: u32 = chapter.parse().ok()?;
    Some((book, chapter, verse.parse().ok()?, text))
}

/// One chapter through the shipped word walk, as a resident cache would hold
/// it. Verse rows belong here: a verse start is one of the `Before`s the row
/// keys on, so it moves the row count and not only the split.
fn word_row(text: &str, verses: &[Verse]) -> WordRow {
    Words.map(ChapterInput {
        text,
        verses,
        key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
    })
}

/// The real Level 1b row `analyze_chapter`'s glyph sizing reads.
fn substrate_row(text: &str, verses: &[Verse]) -> ChapterRow {
    Substrate.map(ChapterInput {
        text,
        verses,
        key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
    })
}

fn report(name: &str, path: &Path) {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));

    // Group verse text into per-chapter strings with their verse spans,
    // keyed by (book, chapter).
    let mut chapters: BTreeMap<(String, u32), (String, Vec<Verse>)> = BTreeMap::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((book, chapter, verse, text)) = parse_ref(line) else {
            continue;
        };
        let entry = chapters.entry((book.to_string(), chapter)).or_default();
        if !entry.0.is_empty() {
            entry.0.push(' ');
        }
        let from = entry.0.len() as u32;
        entry.0.push_str(text);
        let span = TextRange::new(from, entry.0.len() as u32).expect("a verse grows forward");
        if let Ok(key) = VerseKey::new(chapter as u16, verse as u16, verse as u16) {
            entry.1.push(Verse::new(key, span));
        }
    }
    assert!(!chapters.is_empty(), "{} has no chapters", path.display());

    // The lanes below are estimates from distinct-word counts; this is the
    // shipped row, so the two can be read against each other.
    let mut word_row_bytes: Vec<u64> = Vec::with_capacity(chapters.len());
    // Book grain is what a host keeps: `Words::RETAIN_CHAPTERS` is false, so
    // the chapter rows above are shed and only these aggregates survive.
    let mut aggregate_bytes = 0u64;
    // What the merged rows actually hold, so the gap to the line above is
    // `fold_book`'s one-shot `with_capacity` over the pre-merge rows.
    let mut aggregate_rows = 0u64;
    // The same rows keyed by hash alone, which is what the aggregate held
    // before `Before` joined the key.
    let mut aggregate_words = 0u64;
    // The W2 doubles lane, separated out: it is one row per doubled word in a
    // cased script and one per distinct word in an uncased one.
    let mut aggregate_doubles = 0u64;
    // The W4 letter-run lane: one row per letter the corpus ever repeated, so
    // it is bounded by the script's alphabet and not by the vocabulary.
    let mut aggregate_letter_runs = 0u64;
    let mut aggregate_letters = 0usize;
    let mut fold = |rows: &mut Vec<WordRow>, resident: &mut u64, merged: &mut u64| {
        if rows.is_empty() {
            return;
        }
        let view: Vec<ChapterObs<&WordRow>> = rows
            .iter()
            .map(|obs| ChapterObs { start: 0, obs })
            .collect();
        let folded = fold_book(&view);
        *resident += folded.resident_bytes() as u64;
        *merged += size_of_val(folded.words()) as u64;
        let hashes = folded
            .words()
            .chunk_by(|left, right| left.hash == right.hash)
            .count();
        aggregate_words += (hashes * size_of::<WordTotal>()) as u64;
        aggregate_doubles += size_of_val(folded.doubles()) as u64;
        aggregate_letter_runs += size_of_val(folded.letter_runs()) as u64;
        aggregate_letters += folded.letter_runs().len();
        rows.clear();
    };
    let mut doubles_row_bytes: Vec<u64> = Vec::new();
    let mut letter_run_row_bytes: Vec<u64> = Vec::new();
    let mut book_rows: Vec<WordRow> = Vec::new();
    let mut current = String::new();
    let metrics: Vec<((String, u32), ChapterMetrics)> = chapters
        .into_iter()
        .map(|(key, (text, verses))| {
            if key.0 != current {
                fold(&mut book_rows, &mut aggregate_bytes, &mut aggregate_rows);
                current = key.0.clone();
            }
            let row = word_row(&text, &verses);
            word_row_bytes.push(row.resident_bytes() as u64);
            doubles_row_bytes.push(size_of_val(row.doubles()) as u64);
            letter_run_row_bytes.push(size_of_val(row.letter_runs()) as u64);
            book_rows.push(row);
            let scalar_row = substrate_row(&text, &verses);
            (key, analyze_chapter(&text, &scalar_row))
        })
        .collect();
    fold(&mut book_rows, &mut aggregate_bytes, &mut aggregate_rows);

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
    row_bytes_u64("  of which doubles lane", &doubles_row_bytes);
    row_bytes_u64("  of which letter runs", &letter_run_row_bytes);
    println!(
        "{:<28}{:>12}{:>12}{aggregate_words:>14}{:>10.3}",
        "word aggregate, hash alone",
        "",
        "",
        aggregate_words as f64 / 1e6
    );
    println!(
        "{:<28}{:>12}{:>12}{aggregate_rows:>14}{:>10.3}",
        "word aggregate rows",
        "",
        "",
        aggregate_rows as f64 / 1e6
    );
    println!(
        "{:<28}{:>12}{:>12}{aggregate_doubles:>14}{:>10.3}",
        "  of which doubles lane",
        "",
        "",
        aggregate_doubles as f64 / 1e6
    );
    println!(
        "{:<28}{:>12}{:>12}{aggregate_letter_runs:>14}{:>10.3}   {aggregate_letters} rows",
        "  of which letter runs",
        "",
        "",
        aggregate_letter_runs as f64 / 1e6
    );
    println!(
        "{:<28}{:>12}{:>12}{aggregate_bytes:>14}{:>10.3}",
        "word aggregate (resident)",
        "",
        "",
        aggregate_bytes as f64 / 1e6
    );
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
