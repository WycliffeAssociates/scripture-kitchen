//! Cost and yield of the idea-shelf candidate rule "typos as one edit from a
//! frequent word" (`planning/plans/stage4-word-decisions.md`, 2026-09-07),
//! measured before anyone builds it.
//!
//! No BK-tree: for every word under the rarity floor, generate its
//! Damerau-Levenshtein-1 neighbourhood (one insertion, deletion,
//! substitution, or adjacent transposition of scalars) over the corpus's own
//! letter alphabet, and look each variant up by hash in the word→count map
//! the walk already builds. A candidate is a rare word with at least one
//! variant at or above the frequent floor.
//!
//! ```text
//! cargo run -p sous-core --release --example edit_neighbors
//!   corpus       distinct    rare  variants  lookups  candidates  ms  µs/rare
//!   WA-en-ulb       …
//!   …
//!   25 random candidates, WA-en-ulb:
//!     amoung(1) → among(1234)
//!     …
//! ```
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text`) the way
//! `examples/word_volume.rs` does, and hands each verse's text straight to
//! `sous_core::words::walk::for_each_word` — verse boundaries do not affect
//! word membership, so no chapter assembly or `Substrate`/`Words` pass is
//! needed, just the raw walk and a case-folded count table.
//!
//! Not a test: it prints numbers for the ledger rather than asserting on them.

use std::path::PathBuf;
use std::time::Instant;

use rustc_hash::FxHashMap;
use sous_core::words::walk::for_each_word;

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

/// Fewer occurrences than this is a rare word: a typo candidate.
const RARE_CEILING: u32 = 5;
/// At least this many occurrences makes a neighbour "frequent" — the word a
/// typo would have meant.
const FREQUENT_FLOOR: u32 = 200;
/// How many candidates to sample per corpus for the eyeball read.
const SAMPLE_SIZE: usize = 25;

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

/// `BOOK C:V<TAB>text`; `None` for a front-matter row.
fn verse_text(line: &str) -> Option<&str> {
    line.split_once('\t').map(|(_, text)| text)
}

/// Every word in `raw`, case-folded the way the walk folds them (the first
/// scalar of `char::to_lowercase`), counted. Verse boundaries do not join or
/// split words, so each line is walked on its own.
fn word_counts(raw: &str) -> FxHashMap<String, u32> {
    let mut counts: FxHashMap<String, u32> = FxHashMap::default();
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
            *counts.entry(folded.clone()).or_insert(0) += 1;
        });
    }
    counts
}

/// The distinct scalars this corpus's words are built from — not a fixed
/// ASCII set, so Amharic and Devanagari get their own alphabets.
fn alphabet_of(counts: &FxHashMap<String, u32>) -> Vec<char> {
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

struct Measurement {
    distinct_words: usize,
    rare_words: usize,
    variants: u64,
    lookups: u64,
    candidates: Vec<Candidate>,
    elapsed_ms: f64,
}

fn measure(raw: &str) -> Measurement {
    let counts = word_counts(raw);
    let alphabet = alphabet_of(&counts);
    let distinct_words = counts.len();

    let started = Instant::now();
    let mut variants = 0u64;
    let mut lookups = 0u64;
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut rare_words = 0usize;
    let mut scratch = String::new();

    for (word, &rare_count) in &counts {
        if rare_count >= RARE_CEILING {
            continue;
        }
        rare_words += 1;
        let chars: Vec<char> = word.chars().collect();
        let mut best: Option<(&str, u32)> = None;
        edit_neighbors(&chars, &alphabet, |variant| {
            variants += 1;
            scratch.clear();
            scratch.extend(variant.iter());
            lookups += 1;
            if let Some((key, &count)) = counts.get_key_value(scratch.as_str())
                && count >= FREQUENT_FLOOR
                && best.is_none_or(|(_, c)| count > c)
            {
                best = Some((key.as_str(), count));
            }
        });
        if let Some((frequent, frequent_count)) = best {
            candidates.push(Candidate {
                rare: word.clone(),
                rare_count,
                frequent: frequent.to_string(),
                frequent_count,
            });
        }
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;

    Measurement {
        distinct_words,
        rare_words,
        variants,
        lookups,
        candidates,
        elapsed_ms,
    }
}

/// A tiny xorshift64 PRNG so the sample is reproducible without a `rand`
/// dependency. Seeded from the corpus name so each corpus's sample differs
/// but reruns agree.
struct Xorshift64(u64);

impl Xorshift64 {
    fn seeded(name: &str) -> Self {
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        for byte in name.bytes() {
            seed ^= u64::from(byte);
            seed = seed.wrapping_mul(0x100000001B3);
        }
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn index(&mut self, len: usize) -> usize {
        (self.next() % len as u64) as usize
    }
}

/// `count` distinct indices in `0..len`, order unspecified.
fn sample_indices(rng: &mut Xorshift64, len: usize, count: usize) -> Vec<usize> {
    if len <= count {
        return (0..len).collect();
    }
    let mut chosen = std::collections::BTreeSet::new();
    while chosen.len() < count {
        chosen.insert(rng.index(len));
    }
    chosen.into_iter().collect()
}

fn main() {
    let dir = corpora_dir();

    println!(
        "one-edit-from-a-frequent-word: rare < {RARE_CEILING}, frequent >= {FREQUENT_FLOOR}\n"
    );
    println!(
        "{:<12}{:>10}{:>8}{:>12}{:>12}{:>12}{:>10}{:>12}",
        "corpus", "distinct", "rare", "variants", "lookups", "candidates", "ms", "us/rare"
    );

    let mut samples: Vec<(&str, Vec<Candidate>)> = Vec::new();

    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        assert!(!raw.is_empty(), "{} is empty", path.display());

        let measurement = measure(&raw);
        let us_per_rare = if measurement.rare_words == 0 {
            0.0
        } else {
            measurement.elapsed_ms * 1_000.0 / measurement.rare_words as f64
        };
        println!(
            "{:<12}{:>10}{:>8}{:>12}{:>12}{:>12}{:>10.1}{:>12.2}",
            name,
            measurement.distinct_words,
            measurement.rare_words,
            measurement.variants,
            measurement.lookups,
            measurement.candidates.len(),
            measurement.elapsed_ms,
            us_per_rare,
        );

        samples.push((name, measurement.candidates));
    }

    println!("\n### {SAMPLE_SIZE} random candidates per corpus ###");
    for (name, candidates) in &samples {
        println!("\n{name}:");
        if candidates.is_empty() {
            println!("  (no candidates)");
            continue;
        }
        let mut rng = Xorshift64::seeded(name);
        let mut indices = sample_indices(&mut rng, candidates.len(), SAMPLE_SIZE);
        indices.sort_unstable();
        for &i in &indices {
            let c = &candidates[i];
            println!(
                "  {}({}) -> {}({})",
                c.rare, c.rare_count, c.frequent, c.frequent_count
            );
        }
    }
}
