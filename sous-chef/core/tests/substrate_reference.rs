//! `Substrate::map` against a naive per-scalar reference, lane by lane.
//!
//! The reference below is written to be obviously right, not fast: plain
//! `chars()`, `BTreeMap`s, and one pass per lane. The production walk is one
//! SWAR-chunked pass with dense chapter-local ids, so the two agreeing over
//! the committed tier is what makes the interning trustworthy.
//!
//! Chapters come from `corpora/*.txt` (vref: `BOOK C:V<TAB>text`), grouped by
//! book and chapter exactly as `examples/observation_size.rs` groups them, so
//! the byte figures printed here compare with the ledger's sizing row.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sous_core::substrate::{
    Case, ChapterRow, Edge, OuterClass, PairKey, RUN_BUCKETS, RunLengths, ScalarKey, Substrate,
};
use sous_core::unicode::{class_of, is_glue};
use sous_core::{BookKey, ChapterInput, ChapterKey, ChapterPass};

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

// ── The reference ───────────────────────────────────────────────────────

#[derive(Debug, Default, PartialEq, Eq)]
struct Reference {
    scalars: BTreeMap<ScalarKey, u32>,
    pairs: BTreeMap<PairKey, u32>,
    runs: BTreeMap<Vec<ScalarKey>, u32>,
    run_lengths: BTreeMap<ScalarKey, RunLengths>,
    follows: BTreeMap<ScalarKey, [u32; 3]>,
    lead: RefEdge,
    trail: RefEdge,
    scalar_count: u32,
    word_count: u32,
}

/// One edge spelled out field by field, so the reference builds it without a
/// constructor the library does not otherwise need.
#[derive(Debug, Default, PartialEq, Eq)]
struct RefEdge {
    outer: OuterClass,
    open_pair: Option<(ScalarKey, OuterClass)>,
    open_follow: Option<ScalarKey>,
    edge_case: Option<Case>,
    blank: bool,
}

impl RefEdge {
    fn of(edge: Edge) -> Self {
        Self {
            outer: edge.outer(),
            open_pair: edge.open_pair(),
            open_follow: edge.open_follow(),
            edge_case: edge.edge_case(),
            blank: edge.blank(),
        }
    }
}

fn key_of(c: char) -> ScalarKey {
    if class_of(c).is_decimal_digit() {
        ScalarKey::DIGITS
    } else {
        ScalarKey::of(c)
    }
}

fn is_nonletter(c: char) -> bool {
    let class = class_of(c);
    !class.is_alphabetic() && !is_glue(c) && !class.is_whitespace()
}

fn outer(c: Option<char>) -> OuterClass {
    match c {
        None => OuterClass::Edge,
        Some(c) => {
            let class = class_of(c);
            if class.is_whitespace() {
                OuterClass::Space
            } else if class.is_decimal_digit() {
                OuterClass::Digit
            } else if class.is_alphabetic() || is_glue(c) {
                OuterClass::Letter
            } else {
                OuterClass::Nonletter
            }
        }
    }
}

fn case_of(c: char) -> Case {
    let class = class_of(c);
    if class.is_uppercase() {
        Case::Upper
    } else if class.is_lowercase() {
        Case::Lower
    } else {
        Case::Uncased
    }
}

fn word_member(c: char) -> bool {
    let class = class_of(c);
    class.is_alphabetic() || is_glue(c) || class.is_decimal_digit()
}

/// Every maximal nonletter run as `(start index, scalars)`.
fn runs_of(chars: &[char]) -> Vec<(usize, Vec<ScalarKey>)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if !is_nonletter(chars[at]) {
            at += 1;
            continue;
        }
        let start = at;
        let mut run = Vec::new();
        while at < chars.len() && is_nonletter(chars[at]) {
            run.push(key_of(chars[at]));
            at += 1;
        }
        out.push((start, run));
    }
    out
}

fn reference(text: &str) -> Reference {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Reference {
        scalar_count: chars.len() as u32,
        ..Reference::default()
    };

    for &c in &chars {
        if !is_glue(c) {
            *out.scalars.entry(key_of(c)).or_default() += 1;
        }
    }

    for (index, &c) in chars.iter().enumerate() {
        if !is_nonletter(c) {
            continue;
        }
        let key = PairKey::new(
            key_of(c),
            outer(index.checked_sub(1).map(|at| chars[at])),
            outer(chars.get(index + 1).copied()),
        );
        *out.pairs.entry(key).or_default() += 1;
    }

    let runs = runs_of(&chars);
    for (_, run) in &runs {
        *out.runs.entry(run.clone()).or_default() += 1;
        let mut at = 0;
        while at < run.len() {
            let mut end = at + 1;
            while end < run.len() && run[end] == run[at] {
                end += 1;
            }
            out.run_lengths.entry(run[at]).or_default()[(end - at).min(RUN_BUCKETS) - 1] += 1;
            at = end;
        }
    }

    // A run terminal follows into the first non-whitespace scalar past it,
    // when that scalar is a letter.
    for (start, run) in &runs {
        let after = start + run.len();
        let next = chars[after..]
            .iter()
            .find(|c| !class_of(**c).is_whitespace());
        if let Some(&letter) = next
            && class_of(letter).is_alphabetic()
        {
            out.follows.entry(run[run.len() - 1]).or_default()[case_of(letter) as usize] += 1;
        }
    }
    let mut in_word = false;
    for &c in &chars {
        if word_member(c) {
            if !in_word {
                out.word_count += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }

    out.lead = lead_edge(&chars);
    out.trail = trail_edge(&chars, &runs);
    out
}

fn lead_edge(chars: &[char]) -> RefEdge {
    let Some(&first) = chars.first() else {
        return RefEdge::default();
    };
    RefEdge {
        outer: outer(Some(first)),
        open_pair: is_nonletter(first).then(|| (key_of(first), outer(chars.get(1).copied()))),
        open_follow: None,
        edge_case: chars
            .iter()
            .find(|c| !class_of(**c).is_whitespace())
            .filter(|c| class_of(**c).is_alphabetic())
            .map(|c| case_of(*c)),
        blank: false,
    }
}

fn trail_edge(chars: &[char], runs: &[(usize, Vec<ScalarKey>)]) -> RefEdge {
    let Some(&last) = chars.last() else {
        return RefEdge::default();
    };
    RefEdge {
        outer: outer(Some(last)),
        open_pair: is_nonletter(last).then(|| {
            (
                key_of(last),
                outer(chars.len().checked_sub(2).map(|at| chars[at])),
            )
        }),
        // Only whitespace between the last run and the end: the letter that
        // resolves this follow is the next chapter's business.
        open_follow: runs.last().and_then(|(start, run)| {
            chars[start + run.len()..]
                .iter()
                .all(|c| class_of(*c).is_whitespace())
                .then(|| run[run.len() - 1])
        }),
        edge_case: None,
        blank: chars.iter().all(|c| class_of(*c).is_whitespace()),
    }
}

fn observed(row: &ChapterRow) -> Reference {
    Reference {
        scalars: row.scalars().iter().copied().collect(),
        pairs: row.pairs().iter().copied().collect(),
        runs: row
            .runs()
            .map(|(atoms, count)| (atoms.to_vec(), count))
            .collect(),
        run_lengths: row.run_lengths().into_iter().collect(),
        follows: row
            .follows()
            .iter()
            .map(|(key, counts)| {
                (
                    *key,
                    [
                        counts.get(Case::Upper),
                        counts.get(Case::Lower),
                        counts.get(Case::Uncased),
                    ],
                )
            })
            .collect(),
        lead: RefEdge::of(row.lead()),
        trail: RefEdge::of(row.trail()),
        scalar_count: row.scalar_count(),
        word_count: row.word_count(),
    }
}

fn map(text: &str) -> ChapterRow {
    Substrate.map(ChapterInput {
        text,
        verses: &[],
        key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
    })
}

fn assert_agrees(text: &str) {
    let row = map(text);
    let observed = observed(&row);
    let expected = reference(text);
    assert_eq!(observed.scalars, expected.scalars, "scalars for {text:?}");
    assert_eq!(observed.pairs, expected.pairs, "pairs for {text:?}");
    assert_eq!(observed.runs, expected.runs, "runs for {text:?}");
    assert_eq!(
        observed.run_lengths, expected.run_lengths,
        "run lengths for {text:?}"
    );
    assert_eq!(observed.follows, expected.follows, "follows for {text:?}");
    assert_eq!(observed.lead, expected.lead, "lead edge for {text:?}");
    assert_eq!(observed.trail, expected.trail, "trail edge for {text:?}");
    assert_eq!(observed.scalar_count, expected.scalar_count);
    assert_eq!(observed.word_count, expected.word_count);
}

// ── The in-loop sample ──────────────────────────────────────────────────

/// One string per shape the walk branches on, so a break shows up here before
/// the ignored corpus oracle is worth running.
const SAMPLE: &[&str] = &[
    "",
    " ",
    "   \n\t ",
    ",",
    "a",
    "a,",
    ",a",
    ",,",
    "He said, \u{201C}Go.\u{201D}",
    "12,345.67 and \u{967}\u{968}\u{969}",
    "ng'ombe na ng'ombe",
    "e\u{301}te\u{301} \u{915}\u{94d}\u{937} \u{915}\u{200d}\u{915}",
    "\u{5d0}\u{5d1}. \u{5d2}\u{5d3}",
    "Amharic\u{1362}\u{1362} \u{1200}\u{1208}",
    "one,..,two ... three",
    "\u{a0}\u{a0}word\u{feff}",
    "\u{fdd0}\u{0}\u{7f}x",
    "trailing run ...   ",
    "\u{1F9C5}\u{1F9C5}! ok",
    "\u{ab} guillemets \u{bb} \u{202f}!",
];

#[test]
fn map_equals_the_reference_over_a_synthetic_sample() {
    for text in SAMPLE {
        assert_agrees(text);
    }
}

// ── The corpus oracle ───────────────────────────────────────────────────

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

/// `None` for a front-matter row (chapter `?`); anything else that does not
/// fit `BOOK C:V<TAB>text` is a hard failure.
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
    Some((book, chapter.parse().ok()?, text))
}

fn chapters_of(path: &Path) -> Vec<String> {
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
    chapters.into_values().collect()
}

fn percentile(sorted: &[usize], p: f64) -> usize {
    let at = ((p * (sorted.len() as f64 - 1.0)).round() as usize).min(sorted.len() - 1);
    sorted[at]
}

#[test]
#[ignore = "exhaustive oracle: every chapter of the 8-corpus tier walked twice; run --include-ignored at pass end"]
fn map_equals_the_reference_over_the_whole_tier() {
    let dir = corpora_dir();
    let mut sizes = Vec::new();
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        assert!(
            path.is_file(),
            "test-tier corpus {name} must be present at {}",
            path.display()
        );
        let chapters = chapters_of(&path);
        let mut corpus_sizes = Vec::with_capacity(chapters.len());
        for text in &chapters {
            assert_agrees(text);
            corpus_sizes.push(map(text).resident_bytes());
        }
        corpus_sizes.sort_unstable();
        println!(
            "{name:<12}{:>6} chapters  median {:>6} B  p90 {:>6} B  max {:>6} B",
            corpus_sizes.len(),
            percentile(&corpus_sizes, 0.5),
            percentile(&corpus_sizes, 0.9),
            corpus_sizes[corpus_sizes.len() - 1],
        );
        sizes.extend(corpus_sizes);
    }

    sizes.sort_unstable();
    let (median, p90) = (percentile(&sizes, 0.5), percentile(&sizes, 0.9));
    println!(
        "tier         {:>6} chapters  median {median:>6} B  p90 {p90:>6} B",
        sizes.len()
    );
    assert!(median <= 1536, "median observation is {median} B");
    assert!(p90 <= 2048, "p90 observation is {p90} B");
}
