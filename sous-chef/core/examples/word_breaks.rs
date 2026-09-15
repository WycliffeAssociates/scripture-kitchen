//! Our word rule against UAX #29, per corpus and per dominant script.
//!
//! ```text
//! cargo run -p sous-core --release --example word_breaks
//!   corpus        script      words   disagree      %   top disagreements
//!   WA-en-ulb     Latin     789,012      1,234   0.16   don't, Lord's, …
//! ```
//!
//! Eight corpora are not enough to trust "a run of letters and glue, extended
//! through one nonletter with a letter on both sides". So this counts, over
//! the whole fleet, the words that rule finds which
//! `unicode_segmentation::split_word_bound_indices` cuts somewhere else. A
//! word agrees when UAX 29 has a boundary at each of its ends and none inside
//! it. If a script we care about disagrees, the crate becomes the runtime
//! fallback for that script, which is what v1 did.
//!
//! Scripts written without spaces (Thai, Lao, Khmer, Burmese) defeat both
//! rules without a dictionary; the word lanes abstain there, and the table
//! says so rather than pretending the disagreement is a bug.
//!
//! Not a test: it prints tables for the ledger rather than asserting on them.
//! Reads `corpora/*.txt` and then the 1,504-corpus vref tier, which is
//! local-only and not committed — it panics rather than skip silently.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mise::unicode::class_of;
use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::words::for_each_word;
use unicode_segmentation::UnicodeSegmentation;

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

/// Will's laptop-local sibling checkout of the v1 spike's vref corpora, the
/// same two candidates `examples/pattern_volume.rs` looks in.
const VREF: &str = "/Users/willkelly/Documents/Work/Code/scripture-sous-chef/corpora/vref";

/// Words kept per corpus before the disagreement roster stops growing; the
/// tail is a long one and the top ten is what the ledger row carries.
const ROSTER_CAP: usize = 20_000;

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

fn main() {
    let dir = corpora_dir();

    println!("### test tier (corpora/*.txt) ###\n");
    header();
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        let report = compare(&raw);
        row(name, &report);
        println!("      top: {}", top_ten(&report));
    }

    let candidates = [dir.join("calibration-corpora"), PathBuf::from(VREF)];
    let fleet = candidates
        .iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| panic!("no fleet corpora at {candidates:?}; this tier is local-only"));
    println!("\n### fleet sweep ({}) ###\n", fleet.display());
    sweep(fleet);
}

// ── One corpus ──────────────────────────────────────────────────────────

#[derive(Default)]
struct Report {
    script: &'static str,
    words: u64,
    disagree: u64,
    roster: FxHashMap<String, u32>,
}

impl Report {
    fn share(&self) -> f64 {
        if self.words == 0 {
            0.0
        } else {
            100.0 * self.disagree as f64 / self.words as f64
        }
    }
}

/// Our words over one corpus's verse text, against UAX 29's boundaries.
fn compare(raw: &str) -> Report {
    let mut out = Report::default();
    let mut scripts: FxHashMap<&'static str, u64> = FxHashMap::default();
    let mut bounds: FxHashSet<usize> = FxHashSet::default();

    for line in raw.lines() {
        let Some((_, text)) = line.split_once('\t') else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        for scalar in text.chars() {
            if class_of(scalar).is_alphabetic() {
                *scripts.entry(script_of(scalar)).or_insert(0) += 1;
            }
        }

        bounds.clear();
        bounds.extend(text.split_word_bound_indices().map(|(at, _)| at));
        bounds.insert(text.len());

        for_each_word(text, &[], |word| {
            out.words += 1;
            let (from, to) = (word.from as usize, word.to as usize);
            let split = !bounds.contains(&from)
                || !bounds.contains(&to)
                || text[from..to]
                    .char_indices()
                    .any(|(at, _)| at > 0 && bounds.contains(&(from + at)));
            if split {
                out.disagree += 1;
                if out.roster.len() < ROSTER_CAP {
                    *out.roster.entry(text[from..to].to_string()).or_insert(0) += 1;
                } else if let Some(count) = out.roster.get_mut(&text[from..to]) {
                    *count += 1;
                }
            }
        });
    }

    out.script = scripts
        .into_iter()
        .max_by_key(|entry| entry.1)
        .map_or("None", |entry| entry.0);
    out
}

fn header() {
    println!(
        "{:<14}{:<12}{:>12}{:>12}{:>8}",
        "corpus", "script", "words", "disagree", "%"
    );
}

fn row(name: &str, report: &Report) {
    println!(
        "{:<14}{:<12}{:>12}{:>12}{:>8.3}",
        name,
        report.script,
        report.words,
        report.disagree,
        report.share()
    );
}

fn top_ten(report: &Report) -> String {
    let mut rows: Vec<(&String, &u32)> = report.roster.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    rows.iter()
        .take(10)
        .map(|(word, count)| format!("{word:?}\u{d7}{count}"))
        .collect::<Vec<_>>()
        .join("  ")
}

// ── The fleet ───────────────────────────────────────────────────────────

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let at = ((p * (sorted.len() as f64 - 1.0)).round() as usize).min(sorted.len() - 1);
    sorted[at]
}

/// One dominant script's fleet totals: a share per corpus, the summed counts,
/// and one merged disagreement roster.
#[derive(Default)]
struct Lane {
    shares: Vec<f64>,
    words: u64,
    disagree: u64,
    roster: FxHashMap<String, u32>,
}

fn sweep(dir: &Path) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} holds no .txt corpora", dir.display());

    // Per dominant script: every corpus's disagreement share, the summed
    // counts, and one merged roster.
    let mut by_script: BTreeMap<&'static str, Lane> = BTreeMap::new();
    let mut skipped = 0usize;

    for path in &files {
        let Ok(raw) = std::fs::read_to_string(path) else {
            skipped += 1;
            continue;
        };
        let report = compare(&raw);
        if report.words == 0 {
            skipped += 1;
            continue;
        }
        let lane = by_script.entry(report.script).or_default();
        lane.shares.push(report.share());
        lane.words += report.words;
        lane.disagree += report.disagree;
        for (word, count) in report.roster {
            *lane.roster.entry(word).or_insert(0) += count;
        }
    }

    println!(
        "{} corpora attempted, {skipped} skipped (unreadable or wordless)\n",
        files.len()
    );
    println!(
        "{:<12}{:>8}{:>14}{:>12}{:>9}{:>9}{:>9}",
        "script", "corpora", "words", "disagree", "overall%", "p50%", "p90%"
    );
    for (script, lane) in &mut by_script {
        let Lane {
            shares,
            words,
            disagree,
            roster,
        } = lane;
        shares.sort_by(|a, b| a.partial_cmp(b).expect("a share is finite"));
        println!(
            "{:<12}{:>8}{:>14}{:>12}{:>9.3}{:>9.3}{:>9.3}",
            script,
            shares.len(),
            words,
            disagree,
            100.0 * *disagree as f64 / *words as f64,
            percentile(shares, 0.5),
            percentile(shares, 0.9),
        );
        let mut rows: Vec<(&String, &u32)> = roster.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let top: Vec<String> = rows
            .iter()
            .take(10)
            .map(|(word, count)| format!("{word:?}\u{d7}{count}"))
            .collect();
        println!("      top: {}", top.join("  "));
    }
}

// ── A coarse script lane, for this example only ─────────────────────────

/// `sous-core::unicode` carries no script property: the charter's rule is that
/// a lane returns with its consumer, and this is the consumer. Coarse block
/// ranges are enough to name a corpus's dominant script.
fn script_of(scalar: char) -> &'static str {
    match scalar as u32 {
        0x0041..=0x024F | 0x1E00..=0x1EFF => "Latin",
        0x0370..=0x03FF | 0x1F00..=0x1FFF => "Greek",
        0x0400..=0x052F => "Cyrillic",
        0x0530..=0x058F => "Armenian",
        0x0590..=0x05FF => "Hebrew",
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF => "Arabic",
        0x0700..=0x074F => "Syriac",
        0x0900..=0x097F => "Devanagari",
        0x0980..=0x09FF => "Bengali",
        0x0A00..=0x0A7F => "Gurmukhi",
        0x0A80..=0x0AFF => "Gujarati",
        0x0B00..=0x0B7F => "Oriya",
        0x0B80..=0x0BFF => "Tamil",
        0x0C00..=0x0C7F => "Telugu",
        0x0C80..=0x0CFF => "Kannada",
        0x0D00..=0x0D7F => "Malayalam",
        0x0D80..=0x0DFF => "Sinhala",
        0x0E00..=0x0E7F => "Thai",
        0x0E80..=0x0EFF => "Lao",
        0x0F00..=0x0FFF => "Tibetan",
        0x1000..=0x109F => "Myanmar",
        0x10A0..=0x10FF => "Georgian",
        0x1100..=0x11FF | 0xAC00..=0xD7AF => "Hangul",
        0x1200..=0x139F => "Ethiopic",
        0x13A0..=0x13FF => "Cherokee",
        0x1780..=0x17FF => "Khmer",
        0x3040..=0x30FF => "Kana",
        0x3400..=0x4DBF | 0x4E00..=0x9FFF => "Han",
        _ => "Other",
    }
}
