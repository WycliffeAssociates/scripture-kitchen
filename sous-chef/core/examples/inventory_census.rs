//! Sizing for a "rarity roster" (a fixed-size table of the least-frequent
//! scalars, letters and nonletters kept separate) and for the point where a
//! letter-count bound would exclude logographic scripts from that roster.
//!
//! ```text
//! cargo run -p sous-core --release --example inventory_census
//! ```
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text`), groups lines by
//! book+chapter exactly as `examples/observation_size.rs` does, and sums
//! each chapter's `Substrate::map(..).scalars()` lane into one corpus-wide
//! count per scalar. From that it reports distinct-scalar/letter/nonletter
//! totals, how many scalars would be "rare" at a few candidate floors, the
//! corpus's dominant script bucket, and letters used exactly once.
//!
//! When `corpora/calibration-corpora/` exists (Will's laptop only — never a
//! `#[test]`), the same per-corpus figures are computed over every bible
//! there and folded into per-dominant-script aggregates instead of printing
//! one row per corpus, so the report stays readable at ~1500 corpora.
//! Absent, that section is skipped; the test-tier report above still runs.
//!
//! Not a test: it prints tables for the ledger rather than asserting on
//! them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mise::unicode::class_of;
use rustc_hash::FxHashMap;
use sous_core::{BookKey, ChapterInput, ChapterKey, ChapterPass, ScalarKey, Substrate};

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
    let dir = corpora_dir();

    println!("### test tier (corpora/*.txt) ###\n");
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        assert!(
            path.is_file(),
            "test-tier corpus {} must be present at {}",
            name,
            path.display()
        );
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        let census = census_of(&raw, &path);
        print_report(name, &census);
        println!();
    }

    // The gitignored in-tree location, and Will's laptop-local sibling
    // checkout of the v1 spike's vref corpora — whichever exists first.
    let candidates = [
        dir.join("calibration-corpora"),
        PathBuf::from("/Users/willkelly/Documents/Work/Code/scripture-sous-chef/corpora/vref"),
    ];
    match candidates.iter().find(|p| p.is_dir()) {
        Some(calibration_dir) => {
            println!(
                "\n### calibration sweep ({}) ###\n",
                calibration_dir.display()
            );
            run_calibration(calibration_dir);
        }
        None => {
            println!(
                "\n(no calibration corpora found at any of {:?} — skipping the sweep)",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
            );
        }
    }
}

// ── one corpus's scalar census ──────────────────────────────────────────

struct Census {
    /// Corpus-wide count per scalar, summed across every chapter's
    /// `ChapterRow::scalars()` lane.
    totals: FxHashMap<ScalarKey, u64>,
    text_bytes: u64,
    chapters: u32,
}

/// `BOOK C:V<TAB>text`; `None` for a front-matter row (chapter `?`), a hard
/// failure for anything else that doesn't fit the shape.
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

/// A `map` call needs *a* book key, not the right one — it never reads it,
/// since the walk is a pure function of chapter text.
fn book_key_of(code: &str) -> BookKey {
    let mut bytes = [b' '; 3];
    for (slot, byte) in bytes.iter_mut().zip(code.as_bytes()) {
        *slot = byte.to_ascii_uppercase();
    }
    BookKey::new(bytes)
}

fn census_of(raw: &str, path: &Path) -> Census {
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

    let substrate = Substrate;
    let mut totals: FxHashMap<ScalarKey, u64> = FxHashMap::default();
    let mut text_bytes = 0u64;
    let n_chapters = chapters.len() as u32;

    for ((book, chapter), text) in &chapters {
        text_bytes += text.len() as u64;
        let input = ChapterInput {
            text,
            verses: &[],
            key: ChapterKey::new(book_key_of(book), *chapter as u16),
        };
        let row = substrate.map(input);
        for &(scalar, count) in row.scalars() {
            *totals.entry(scalar).or_insert(0) += u64::from(count);
        }
    }

    Census {
        totals,
        text_bytes,
        chapters: n_chapters,
    }
}

// ── classification ───────────────────────────────────────────────────────

/// `None` for the pooled digit lane, which has no single char and is always
/// a nonletter.
fn is_letter(key: ScalarKey) -> bool {
    match key.scalar() {
        Some(c) => class_of(c).is_alphabetic(),
        None => false,
    }
}

/// Coarse Unicode-block buckets, coarse enough to name a "dominant script"
/// without pulling in a script-property table this crate doesn't carry.
fn script_bucket(c: char) -> &'static str {
    let cp = c as u32;
    match cp {
        0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x024F | 0x1E00..=0x1EFF => "Latin",
        0x0370..=0x03FF | 0x1F00..=0x1FFF => "Greek",
        0x0400..=0x052F => "Cyrillic",
        0x0590..=0x05FF => "Hebrew",
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF => "Arabic",
        0x0900..=0x097F => "Devanagari",
        0x1200..=0x137F => "Ethiopic",
        0x2E80..=0x2EFF
        | 0x3040..=0x30FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0x20000..=0x2FFFF => "CJK/Han",
        _ => "Other",
    }
}

// ── report shape (shared by the test tier and the calibration sweep) ────

struct Shape {
    distinct_total: usize,
    distinct_letters: usize,
    distinct_nonletters: usize,
    /// `(< 3, < 5, < 10)`, letters then nonletters.
    rare_letters: (usize, usize, usize),
    rare_nonletters: (usize, usize, usize),
    dominant_script: &'static str,
    dominant_script_letters: usize,
    letters_used_once: usize,
}

fn shape_of(census: &Census) -> Shape {
    let mut distinct_letters = 0usize;
    let mut distinct_nonletters = 0usize;
    let mut rare_letters = (0usize, 0usize, 0usize);
    let mut rare_nonletters = (0usize, 0usize, 0usize);
    let mut letters_used_once = 0usize;
    let mut script_counts: FxHashMap<&'static str, usize> = FxHashMap::default();

    for (&key, &count) in &census.totals {
        if is_letter(key) {
            distinct_letters += 1;
            if count == 1 {
                letters_used_once += 1;
            }
            if count < 3 {
                rare_letters.0 += 1;
            }
            if count < 5 {
                rare_letters.1 += 1;
            }
            if count < 10 {
                rare_letters.2 += 1;
            }
            if let Some(c) = key.scalar() {
                *script_counts.entry(script_bucket(c)).or_insert(0) += 1;
            }
        } else {
            distinct_nonletters += 1;
            if count < 3 {
                rare_nonletters.0 += 1;
            }
            if count < 5 {
                rare_nonletters.1 += 1;
            }
            if count < 10 {
                rare_nonletters.2 += 1;
            }
        }
    }

    let (dominant_script, dominant_script_letters) = script_counts
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .unwrap_or(("(none)", 0));

    Shape {
        distinct_total: census.totals.len(),
        distinct_letters,
        distinct_nonletters,
        rare_letters,
        rare_nonletters,
        dominant_script,
        dominant_script_letters,
        letters_used_once,
    }
}

fn print_report(name: &str, census: &Census) {
    let shape = shape_of(census);
    println!(
        "=== {name} ({} chapters, {:.2} MB text) ===",
        census.chapters,
        census.text_bytes as f64 / 1e6
    );
    println!(
        "distinct scalars {:>6}   letters {:>6}   nonletters {:>6}",
        shape.distinct_total, shape.distinct_letters, shape.distinct_nonletters
    );
    println!(
        "rare letters      < 3 {:>5}   < 5 {:>5}   < 10 {:>5}",
        shape.rare_letters.0, shape.rare_letters.1, shape.rare_letters.2
    );
    println!(
        "rare nonletters   < 3 {:>5}   < 5 {:>5}   < 10 {:>5}",
        shape.rare_nonletters.0, shape.rare_nonletters.1, shape.rare_nonletters.2
    );
    println!(
        "dominant script   {} ({} of {} distinct letters)   letters used once {}",
        shape.dominant_script,
        shape.dominant_script_letters,
        shape.distinct_letters,
        shape.letters_used_once
    );
}

// ── calibration sweep ────────────────────────────────────────────────────

fn percentile(sorted: &[usize], p: f64) -> usize {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p * (sorted.len() as f64 - 1.0)).round() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn stats(values: &mut [usize]) -> (usize, usize, usize) {
    values.sort_unstable();
    (
        percentile(values, 0.5),
        percentile(values, 0.9),
        values.last().copied().unwrap_or(0),
    )
}

struct ScriptAgg {
    corpora: usize,
    distinct_letters: Vec<usize>,
    letters_used_once: Vec<usize>,
    roster_letters_floor5: Vec<usize>,
    roster_nonletters_floor5: Vec<usize>,
}

fn run_calibration(dir: &Path) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();

    if files.is_empty() {
        println!("(calibration-corpora has no .txt files — nothing to sweep)");
        return;
    }

    let mut by_script: FxHashMap<&'static str, ScriptAgg> = FxHashMap::default();
    let mut failures = 0usize;

    for path in &files {
        let Ok(raw) = std::fs::read_to_string(path) else {
            failures += 1;
            continue;
        };
        // A calibration file may not group into chapters the way the test
        // tier does (encoding noise, a non-vref export); skip it rather than
        // panic, since this sweep is a by-hand pass over ~1500 unaudited
        // files, not the finish-line gate.
        let census = std::panic::catch_unwind(|| census_of(&raw, path));
        let Ok(census) = census else {
            failures += 1;
            continue;
        };
        if census.totals.is_empty() {
            continue;
        }
        let shape = shape_of(&census);

        let agg = by_script.entry(shape.dominant_script).or_insert(ScriptAgg {
            corpora: 0,
            distinct_letters: Vec::new(),
            letters_used_once: Vec::new(),
            roster_letters_floor5: Vec::new(),
            roster_nonletters_floor5: Vec::new(),
        });
        agg.corpora += 1;
        agg.distinct_letters.push(shape.distinct_letters);
        agg.letters_used_once.push(shape.letters_used_once);
        agg.roster_letters_floor5.push(shape.rare_letters.1);
        agg.roster_nonletters_floor5.push(shape.rare_nonletters.1);
    }

    println!(
        "{} corpora attempted, {} skipped (unreadable or unparseable)\n",
        files.len(),
        failures
    );

    // Fixed order so the named buckets from the brief always show, even at
    // zero corpora, with anything else trailing after them.
    let named = [
        "CJK/Han",
        "Hebrew",
        "Arabic",
        "Devanagari",
        "Ethiopic",
        "Greek",
        "Cyrillic",
        "Latin",
    ];
    let mut scripts: Vec<&'static str> = by_script.keys().copied().collect();
    scripts.sort_by_key(|s| named.iter().position(|n| n == s).unwrap_or(named.len() + 1));
    for name in named {
        if !scripts.contains(&name) {
            scripts.push(name);
        }
    }
    scripts.dedup();

    println!(
        "{:<12}{:>8}{:>26}{:>26}{:>26}",
        "script",
        "corpora",
        "distinct letters (med/p90/max)",
        "letters used once (med/p90/max)",
        "roster @5 letters (med/p90/max)"
    );
    for name in &scripts {
        let Some(agg) = by_script.get_mut(name) else {
            println!("{name:<12}{:>8}", 0);
            continue;
        };
        let (dl_med, dl_p90, dl_max) = stats(&mut agg.distinct_letters);
        let (lo_med, lo_p90, lo_max) = stats(&mut agg.letters_used_once);
        let (rl_med, rl_p90, rl_max) = stats(&mut agg.roster_letters_floor5);
        let (rn_med, rn_p90, rn_max) = stats(&mut agg.roster_nonletters_floor5);
        println!(
            "{:<12}{:>8}{:>8}{:>8}{:>8}   {:>6}{:>6}{:>6}   {:>6}{:>6}{:>6}",
            name,
            agg.corpora,
            dl_med,
            dl_p90,
            dl_max,
            lo_med,
            lo_p90,
            lo_max,
            rl_med,
            rl_p90,
            rl_max
        );
        println!(
            "{:<12}{:>8}  roster @5 nonletters (med/p90/max): {:>6}{:>6}{:>6}",
            "", "", rn_med, rn_p90, rn_max
        );
    }
}
