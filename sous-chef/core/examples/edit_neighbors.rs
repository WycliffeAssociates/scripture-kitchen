//! Cost and yield of the idea-shelf candidate rule "typos as one edit from a
//! frequent word" (`planning/plans/stage4-word-decisions.md`, 2026-09-07),
//! re-measured 2026-09-07 with three precision filters and a rayon-parallel
//! sweep, per Will's framing: not a channel — a cold, MANUAL "check for
//! possible typos" action, its own part of sous, if the yield is real.
//!
//! No BK-tree: for every word under the rarity floor, generate its
//! Damerau-Levenshtein-1 neighbourhood (one insertion, deletion,
//! substitution, or adjacent transposition of scalars) over the corpus's own
//! letter alphabet, and look each variant up by hash in the word→count map
//! the walk already builds. A candidate is a rare word with at least one
//! variant at or above the frequent floor.
//!
//! Three filters, each independently switchable:
//!   (a) `--skip-title`   — drop a rare word that ever occurred in Title
//!                          form (first letter upper, rest lower) anywhere
//!                          in the corpus: `sous_core::words::Form::Title`
//!                          per occurrence marks it a likely proper name.
//!   (b) `--min-scalars`  — require the rare word to hold >= 4 scalars.
//!   (c) `--shared-first` — require the frequent neighbour to share the
//!                          rare word's first scalar.
//!
//! ```text
//! cargo run -p sous-core --release --example edit_neighbors
//!   # no flags: full report — the 8-combination filter sweep, serial vs
//!   # parallel timing, and the top-25 ranked candidates for the three
//!   # named corpora, all filters on.
//! cargo run -p sous-core --release --example edit_neighbors -- --skip-title --min-scalars
//!   # flags given: single-combination report, that combination only.
//! ```
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text`) the way
//! `examples/word_volume.rs` does, and hands each verse's text straight to
//! `sous_core::words::for_each_word` — verse boundaries do not affect word
//! membership, so no chapter assembly or `Substrate`/`Words` pass is needed,
//! just the raw walk and a case-folded count table.
//!
//! Not a test: it prints numbers for the ledger rather than asserting on them.

use std::path::PathBuf;
use std::time::Instant;

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use sous_core::words::{Form, for_each_word};

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

/// The three corpora the top-25 ranked read is printed for.
const NAMED_CORPORA: &[&str] = &["WA-en-ulb", "swhulb", "francl"];

/// Fewer occurrences than this is a rare word: a typo candidate.
const RARE_CEILING: u32 = 5;
/// At least this many occurrences makes a neighbour "frequent" — the word a
/// typo would have meant.
const FREQUENT_FLOOR: u32 = 200;
/// A rare word must hold at least this many scalars to pass `--min-scalars`.
const MIN_RARE_SCALARS: usize = 4;
/// How many top-ranked candidates to print per named corpus.
const TOP_N: usize = 25;

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

/// `BOOK C:V<TAB>text`; `None` for a front-matter row.
fn verse_text(line: &str) -> Option<&str> {
    line.split_once('\t').map(|(_, text)| text)
}

/// A folded word's count plus whether any occurrence stood in Title form —
/// the signal `--skip-title` reads to treat it as a likely proper name.
#[derive(Clone, Copy)]
struct WordStats {
    count: u32,
    saw_title: bool,
}

/// Every word in `raw`, case-folded the way the walk folds them (the first
/// scalar of `char::to_lowercase`), counted, with each folded word's Title
/// occurrences flagged. Verse boundaries do not join or split words, so each
/// line is walked on its own.
fn word_counts(raw: &str) -> FxHashMap<String, WordStats> {
    let mut counts: FxHashMap<String, WordStats> = FxHashMap::default();
    let mut folded = String::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let Some(text) = verse_text(line) else {
            continue;
        };
        for_each_word(text, &[], |occurrence| {
            folded.clear();
            for scalar in text[occurrence.from as usize..occurrence.to as usize].chars() {
                folded.push(scalar.to_lowercase().next().unwrap_or(scalar));
            }
            let entry = counts.entry(folded.clone()).or_insert(WordStats {
                count: 0,
                saw_title: false,
            });
            entry.count += 1;
            entry.saw_title |= occurrence.form == Form::Title;
        });
    }
    counts
}

/// The distinct scalars this corpus's words are built from — not a fixed
/// ASCII set, so Amharic and Devanagari get their own alphabets.
fn alphabet_of(counts: &FxHashMap<String, WordStats>) -> Vec<char> {
    let mut set = std::collections::BTreeSet::new();
    for word in counts.keys() {
        for scalar in word.chars() {
            set.insert(scalar);
        }
    }
    set.into_iter().collect()
}

/// Calls `visit` with every scalar string one Damerau-Levenshtein edit away
/// from `word`, over `alphabet`. Duplicates are possible (a substitution can
/// reproduce an insertion's result) and left in: the count is of lookups
/// generated, not of distinct variants.
fn edit_neighbors(word: &[char], alphabet: &[char], mut visit: impl FnMut(&[char])) {
    let mut buf: Vec<char> = Vec::with_capacity(word.len() + 1);

    // Deletions: one scalar removed.
    for i in 0..word.len() {
        buf.clear();
        buf.extend_from_slice(&word[..i]);
        buf.extend_from_slice(&word[i + 1..]);
        visit(&buf);
    }

    // Insertions: one scalar added at every gap.
    for i in 0..=word.len() {
        for &letter in alphabet {
            buf.clear();
            buf.extend_from_slice(&word[..i]);
            buf.push(letter);
            buf.extend_from_slice(&word[i..]);
            visit(&buf);
        }
    }

    // Substitutions: one scalar replaced by a different one.
    for i in 0..word.len() {
        for &letter in alphabet {
            if letter == word[i] {
                continue;
            }
            buf.clear();
            buf.extend_from_slice(&word[..i]);
            buf.push(letter);
            buf.extend_from_slice(&word[i + 1..]);
            visit(&buf);
        }
    }

    // Adjacent transpositions.
    for i in 0..word.len().saturating_sub(1) {
        if word[i] == word[i + 1] {
            continue; // swapping two equal scalars is not an edit
        }
        buf.clear();
        buf.extend_from_slice(word);
        buf.swap(i, i + 1);
        visit(&buf);
    }
}

/// One rare word's best frequent neighbour, if the sweep found one.
struct Candidate {
    rare: String,
    rare_count: u32,
    frequent: String,
    frequent_count: u32,
}

/// The three precision filters, each independently switchable.
#[derive(Clone, Copy, Default)]
struct Filters {
    skip_title: bool,
    min_rare_scalars: bool,
    shared_first_scalar: bool,
}

impl Filters {
    /// All eight on/off combinations, in a fixed order (all-off first,
    /// all-on last) so a sweep table reads as a truth table.
    fn all_combinations() -> [Filters; 8] {
        let mut combos = [Filters::default(); 8];
        for (i, combo) in combos.iter_mut().enumerate() {
            combo.skip_title = i & 0b100 != 0;
            combo.min_rare_scalars = i & 0b010 != 0;
            combo.shared_first_scalar = i & 0b001 != 0;
        }
        combos
    }

    /// A 3-letter code: uppercase = filter on, lowercase = off, in order
    /// title/min-scalars/shared-first.
    fn label(self) -> String {
        format!(
            "{}{}{}",
            if self.skip_title { 'T' } else { 't' },
            if self.min_rare_scalars { 'M' } else { 'm' },
            if self.shared_first_scalar { 'S' } else { 's' },
        )
    }
}

/// One rare word's outcome: its candidate, if any, plus the variant and
/// lookup counts it cost — summed by the caller across the whole sweep.
struct WordOutcome {
    candidate: Option<Candidate>,
    variants: u64,
    lookups: u64,
}

/// Runs the edit-neighbourhood sweep for one rare word under `filters`.
/// `--shared-first` is applied before the lookup (a cheap scalar compare),
/// so it also cuts `lookups` below `variants`; `--skip-title` and
/// `--min-scalars` are applied before any neighbourhood is generated at
/// all, so a filtered-out word costs nothing.
fn process_rare_word(
    word: &str,
    stats: WordStats,
    counts: &FxHashMap<String, WordStats>,
    alphabet: &[char],
    filters: Filters,
) -> WordOutcome {
    let empty = WordOutcome {
        candidate: None,
        variants: 0,
        lookups: 0,
    };
    if filters.skip_title && stats.saw_title {
        return empty;
    }
    let chars: Vec<char> = word.chars().collect();
    if filters.min_rare_scalars && chars.len() < MIN_RARE_SCALARS {
        return empty;
    }
    let Some(&rare_first) = chars.first() else {
        return empty;
    };

    let mut variants = 0u64;
    let mut lookups = 0u64;
    let mut best: Option<(&str, u32)> = None;
    let mut scratch = String::new();
    edit_neighbors(&chars, alphabet, |variant| {
        variants += 1;
        if filters.shared_first_scalar && variant.first() != Some(&rare_first) {
            return;
        }
        scratch.clear();
        scratch.extend(variant.iter());
        lookups += 1;
        if let Some((key, neighbor_stats)) = counts.get_key_value(scratch.as_str())
            && neighbor_stats.count >= FREQUENT_FLOOR
            && best.is_none_or(|(_, c)| neighbor_stats.count > c)
        {
            best = Some((key.as_str(), neighbor_stats.count));
        }
    });

    let candidate = best.map(|(frequent, frequent_count)| Candidate {
        rare: word.to_string(),
        rare_count: stats.count,
        frequent: frequent.to_string(),
        frequent_count,
    });
    WordOutcome {
        candidate,
        variants,
        lookups,
    }
}

struct Measurement {
    distinct_words: usize,
    rare_words: usize,
    variants: u64,
    lookups: u64,
    candidates: Vec<Candidate>,
    elapsed_ms: f64,
}

/// Sweeps every rare word in `counts` under `filters`, serially or with
/// rayon depending on `parallel`. Timing brackets only this loop, matching
/// what the original single-threaded measurement timed.
fn measure(
    counts: &FxHashMap<String, WordStats>,
    alphabet: &[char],
    filters: Filters,
    parallel: bool,
) -> Measurement {
    let rare: Vec<(&String, WordStats)> = counts
        .iter()
        .filter(|(_, s)| s.count < RARE_CEILING)
        .map(|(w, &s)| (w, s))
        .collect();

    let started = Instant::now();
    let outcomes: Vec<WordOutcome> = if parallel {
        rare.par_iter()
            .map(|&(word, stats)| process_rare_word(word, stats, counts, alphabet, filters))
            .collect()
    } else {
        rare.iter()
            .map(|&(word, stats)| process_rare_word(word, stats, counts, alphabet, filters))
            .collect()
    };
    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;

    let mut variants = 0u64;
    let mut lookups = 0u64;
    let mut candidates = Vec::new();
    for outcome in outcomes {
        variants += outcome.variants;
        lookups += outcome.lookups;
        if let Some(candidate) = outcome.candidate {
            candidates.push(candidate);
        }
    }

    Measurement {
        distinct_words: counts.len(),
        rare_words: rare.len(),
        variants,
        lookups,
        candidates,
        elapsed_ms,
    }
}

fn print_measurement_row(name: &str, label: &str, measurement: &Measurement) {
    let us_per_rare = if measurement.rare_words == 0 {
        0.0
    } else {
        measurement.elapsed_ms * 1_000.0 / measurement.rare_words as f64
    };
    println!(
        "{:<12}{:<6}{:>10}{:>8}{:>12}{:>12}{:>12}{:>10.1}{:>12.2}",
        name,
        label,
        measurement.distinct_words,
        measurement.rare_words,
        measurement.variants,
        measurement.lookups,
        measurement.candidates.len(),
        measurement.elapsed_ms,
        us_per_rare,
    );
}

fn print_top_n(name: &str, candidates: &mut [Candidate]) {
    println!("\n{name}:");
    if candidates.is_empty() {
        println!("  (no candidates)");
        return;
    }
    candidates.sort_unstable_by_key(|c| std::cmp::Reverse(c.frequent_count));
    for c in candidates.iter().take(TOP_N) {
        println!(
            "  {}({}) -> {}({})",
            c.rare, c.rare_count, c.frequent, c.frequent_count
        );
    }
}

/// `--skip-title`, `--min-scalars`, `--shared-first`, any combination. No
/// flags means "run the full report" (see `main`'s doc comment).
fn parse_filters_from_args() -> Option<Filters> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return None;
    }
    let mut filters = Filters::default();
    for arg in &args {
        match arg.as_str() {
            "--skip-title" => filters.skip_title = true,
            "--min-scalars" => filters.min_rare_scalars = true,
            "--shared-first" => filters.shared_first_scalar = true,
            other => {
                panic!("unknown flag {other}; expected --skip-title, --min-scalars, --shared-first")
            }
        }
    }
    Some(filters)
}

fn header() {
    println!(
        "{:<12}{:<6}{:>10}{:>8}{:>12}{:>12}{:>12}{:>10}{:>12}",
        "corpus", "flags", "distinct", "rare", "variants", "lookups", "candidates", "ms", "us/rare"
    );
}

fn main() {
    let dir = corpora_dir();
    let requested = parse_filters_from_args();

    println!(
        "one-edit-from-a-frequent-word: rare < {RARE_CEILING}, frequent >= {FREQUENT_FLOOR}, \
         min-scalars filter >= {MIN_RARE_SCALARS}"
    );
    println!("flags key: T/t skip-title  M/m min-scalars  S/s shared-first\n");

    // Pre-load every corpus's counts and alphabet once; every combination
    // and both timing runs reuse the same tables.
    let mut loaded: Vec<(&str, FxHashMap<String, WordStats>, Vec<char>)> = Vec::new();
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        assert!(!raw.is_empty(), "{} is empty", path.display());
        let counts = word_counts(&raw);
        let alphabet = alphabet_of(&counts);
        loaded.push((name, counts, alphabet));
    }

    let combos: Vec<Filters> = match requested {
        Some(f) => vec![f],
        None => Filters::all_combinations().to_vec(),
    };

    println!("### filter-combination sweep (parallel) ###");
    header();
    let mut named_top: Vec<(&str, Vec<Candidate>)> = Vec::new();
    let all_on = Filters {
        skip_title: true,
        min_rare_scalars: true,
        shared_first_scalar: true,
    };
    for (name, counts, alphabet) in &loaded {
        for &combo in &combos {
            let measurement = measure(counts, alphabet, combo, true);
            print_measurement_row(name, &combo.label(), &measurement);
            // Keep the all-filters-on candidates for the named corpora's
            // top-25 read, whether this run swept every combo or the user
            // asked for exactly this one.
            if combo.label() == all_on.label() && NAMED_CORPORA.contains(name) {
                named_top.push((name, measurement.candidates));
            }
        }
    }

    println!("\n### serial vs parallel wall time, no filters ###");
    println!(
        "{:<12}{:>12}{:>12}{:>10}",
        "corpus", "serial ms", "parallel ms", "speedup"
    );
    for (name, counts, alphabet) in &loaded {
        let baseline = Filters::default();
        let serial = measure(counts, alphabet, baseline, false);
        let parallel = measure(counts, alphabet, baseline, true);
        let speedup = if parallel.elapsed_ms > 0.0 {
            serial.elapsed_ms / parallel.elapsed_ms
        } else {
            0.0
        };
        println!(
            "{:<12}{:>12.1}{:>12.1}{:>9.2}x",
            name, serial.elapsed_ms, parallel.elapsed_ms, speedup
        );
    }

    // If the run asked for a specific combination rather than the full
    // sweep, the top-25 read should honour that combination, not
    // necessarily all-filters-on.
    if requested.is_some() {
        named_top.clear();
        let combo = combos[0];
        for (name, counts, alphabet) in &loaded {
            if !NAMED_CORPORA.contains(name) {
                continue;
            }
            let measurement = measure(counts, alphabet, combo, true);
            named_top.push((name, measurement.candidates));
        }
        println!(
            "\n### top {TOP_N} candidates by frequent-neighbour count, flags {} ###",
            combo.label()
        );
    } else {
        println!("\n### top {TOP_N} candidates by frequent-neighbour count, all filters on ###");
    }
    for (name, mut candidates) in named_top {
        print_top_n(name, &mut candidates);
    }
}
