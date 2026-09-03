// EXPERIMENT — NOT COMPILED. See ../experiments/README.md.
//
// Question: for a site rescan that must locate every occurrence of N needle
//   patterns in a book's text, where does one memchr pass per needle stop
//   beating one memmem pass per needle, and where does a single
//   Aho-Corasick pass over all needles beat both? Crossover as a function of
//   N (1/4/10/40), hit density (rare/common/pairs), on three scripts.
//
// Date: 2026-09-03.
//
// Numbers: M1, one core, `cargo bench --release` in a throwaway crate
//   (`memchr = "2"`, `aho-corasick = "1"`, `divan = "0.1.21"`,
//   `sous-core = { path = "../core" }`), 30 samples, over
//   `../../corpora/{WA-en-ulb,hin2017,amh}.txt`. Full tables and the
//   four-sentence reading live in the task response this file shipped
//   from, not repeated here.
//
// Verdict: memmem::Finder per needle ships for D2b. memchr on a UTF-8 lead byte
//   verifies every Devanagari scalar (shared 0xE0) and runs 6× slower; Aho-Corasick
//   pays only at ≥10 distinct rare needles per book, which the firing set does not reach.
//
// Winner: to be built in D2b as sous_core::sites over memmem; this file is the comparator.

//! Site-rescan crossover: memchr-per-needle vs memmem-per-needle vs a single
//! Aho-Corasick pass, at N in {1, 4, 10, 40} needles and three density sets
//! (RARE, COMMON, PAIRS), on three corpora (Latin, Devanagari, Ethiopic).
//!
//!     cargo bench --release
//!     cargo bench --release -- amh      # one corpus
//!
//! `substrate_walk` is the ceiling: Substrate::map's classifier walk over the
//! same text, the real per-scalar cost any rescan pass sits above.
//!
//! Needle tables are corpus-derived (see needle_tables.rs) and padded past
//! their real count with filler glyphs verified absent from all three
//! corpora, so N=10/40 rows dilute density on purpose without changing what
//! N=1/4 measure.

mod needle_tables {
    // Generated needle tables: real corpus-derived needles, padded to 40
    // with filler glyphs (Cyrillic/Greek/math/symbol, verified absent from all
    // three corpora) so N=10/40 sets keep the density story visible.

    pub const EN_RARE: [&[u8]; 40] = [
            b"\x5b",
            b"\x5d",
            b"\xe2\x80\x9c",
            b"\xe2\x80\x9d",
            b"\xe2\x80\x98",
            b"\xe2\x80\x99",
            b"\xe2\x80\x93",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
            b"\xce\xb2",
            b"\xce\xb3",
            b"\xce\xb4",
            b"\xce\xb5",
            b"\xce\xb8",
            b"\xce\xbb",
            b"\xce\xbc",
            b"\xcf\x80",
            b"\xce\xa9",
            b"\xe2\x88\x91",
            b"\xe2\x88\x86",
            b"\xe2\x88\x9a",
            b"\xe2\x88\x9e",
            b"\xe2\x89\x88",
            b"\xe2\x89\xa0",
            b"\xe2\x89\xa4",
            b"\xe2\x89\xa5",
            b"\xe2\x99\xa0",
            b"\xe2\x99\xa3",
            b"\xe2\x99\xa5",
            b"\xe2\x99\xa6",
            b"\xe2\x98\xba",
            b"\xe2\x98\xbb",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
        ];

    pub const EN_COMMON: [&[u8]; 40] = [
            b"\x2c",
            b"\x2e",
            b"\x22",
            b"\x3b",
            b"\x3a",
            b"\x3f",
            b"\x21",
            b"\x2d",
            b"\xe2\x80\x94",
            b"\x27",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
            b"\xce\xb2",
            b"\xce\xb3",
            b"\xce\xb4",
            b"\xce\xb5",
            b"\xce\xb8",
            b"\xce\xbb",
            b"\xce\xbc",
            b"\xcf\x80",
            b"\xce\xa9",
            b"\xe2\x88\x91",
            b"\xe2\x88\x86",
            b"\xe2\x88\x9a",
            b"\xe2\x88\x9e",
            b"\xe2\x89\x88",
            b"\xe2\x89\xa0",
            b"\xe2\x89\xa4",
            b"\xe2\x89\xa5",
            b"\xe2\x99\xa0",
            b"\xe2\x99\xa3",
            b"\xe2\x99\xa5",
            b"\xe2\x99\xa6",
            b"\xe2\x98\xba",
            b"\xe2\x98\xbb",
        ];

    pub const EN_PAIRS: [&[u8]; 40] = [
            b"\x2e\x20",
            b"\x2c\x22",
            b"\x3f\x22",
            b"\x2e\x22",
            b"\x21\x22",
            b"\x3b\x20",
            b"\x2c\x20",
            b"\xd0\x96\xd0\x96",
            b"\xd0\xa4\xd0\xa4",
            b"\xd0\xa9\xd0\xa9",
            b"\xd0\xad\xd0\xad",
            b"\xd0\xae\xd0\xae",
            b"\xd0\xaf\xd0\xaf",
            b"\xce\xb1\xce\xb1",
            b"\xce\xb2\xce\xb2",
            b"\xce\xb3\xce\xb3",
            b"\xce\xb4\xce\xb4",
            b"\xce\xb5\xce\xb5",
            b"\xce\xb8\xce\xb8",
            b"\xce\xbb\xce\xbb",
            b"\xce\xbc\xce\xbc",
            b"\xcf\x80\xcf\x80",
            b"\xce\xa9\xce\xa9",
            b"\xe2\x88\x91\xe2\x88\x91",
            b"\xe2\x88\x86\xe2\x88\x86",
            b"\xe2\x88\x9a\xe2\x88\x9a",
            b"\xe2\x88\x9e\xe2\x88\x9e",
            b"\xe2\x89\x88\xe2\x89\x88",
            b"\xe2\x89\xa0\xe2\x89\xa0",
            b"\xe2\x89\xa4\xe2\x89\xa4",
            b"\xe2\x89\xa5\xe2\x89\xa5",
            b"\xe2\x99\xa0\xe2\x99\xa0",
            b"\xe2\x99\xa3\xe2\x99\xa3",
            b"\xe2\x99\xa5\xe2\x99\xa5",
            b"\xe2\x99\xa6\xe2\x99\xa6",
            b"\xe2\x98\xba\xe2\x98\xba",
            b"\xe2\x98\xbb\xe2\x98\xbb",
            b"\xd0\x96\xd0\x96",
            b"\xd0\xa4\xd0\xa4",
            b"\xd0\xa9\xd0\xa9",
        ];

    pub const HI_RARE: [&[u8]; 40] = [
            b"\xe2\x80\x93",
            b"\x5b",
            b"\x5d",
            b"\xe2\x80\x94",
            b"\xe2\x80\x8d",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
            b"\xce\xb2",
            b"\xce\xb3",
            b"\xce\xb4",
            b"\xce\xb5",
            b"\xce\xb8",
            b"\xce\xbb",
            b"\xce\xbc",
            b"\xcf\x80",
            b"\xce\xa9",
            b"\xe2\x88\x91",
            b"\xe2\x88\x86",
            b"\xe2\x88\x9a",
            b"\xe2\x88\x9e",
            b"\xe2\x89\x88",
            b"\xe2\x89\xa0",
            b"\xe2\x89\xa4",
            b"\xe2\x89\xa5",
            b"\xe2\x99\xa0",
            b"\xe2\x99\xa3",
            b"\xe2\x99\xa5",
            b"\xe2\x99\xa6",
            b"\xe2\x98\xba",
            b"\xe2\x98\xbb",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
        ];

    pub const HI_COMMON: [&[u8]; 40] = [
            b"\xe0\xa4\xbe",
            b"\xe0\xa5\x87",
            b"\xe0\xa5\x8d",
            b"\xe0\xa5\x8b",
            b"\xe0\xa4\xbf",
            b"\xe0\xa5\x80",
            b"\xe0\xa4\x82",
            b"\xe0\xa5\x81",
            b"\x2c",
            b"\xe0\xa5\xa4",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
            b"\xce\xb2",
            b"\xce\xb3",
            b"\xce\xb4",
            b"\xce\xb5",
            b"\xce\xb8",
            b"\xce\xbb",
            b"\xce\xbc",
            b"\xcf\x80",
            b"\xce\xa9",
            b"\xe2\x88\x91",
            b"\xe2\x88\x86",
            b"\xe2\x88\x9a",
            b"\xe2\x88\x9e",
            b"\xe2\x89\x88",
            b"\xe2\x89\xa0",
            b"\xe2\x89\xa4",
            b"\xe2\x89\xa5",
            b"\xe2\x99\xa0",
            b"\xe2\x99\xa3",
            b"\xe2\x99\xa5",
            b"\xe2\x99\xa6",
            b"\xe2\x98\xba",
            b"\xe2\x98\xbb",
        ];

    pub const HI_PAIRS: [&[u8]; 40] = [
            b"\xe0\xa5\xa4\x20",
            b"\xe0\xa5\xa4\xe2\x80\x9d",
            b"\x3b\x20",
            b"\xe0\xa4\x82\x20",
            b"\xe0\xa4\x97\xe0\xa5\x8d",
            b"\xe0\xa4\xb0\xe0\xa5\x87",
            b"\xe0\xa4\xaf\xe0\xa5\x80",
            b"\x2c\x20",
            b"\xd0\x96\xd0\x96",
            b"\xd0\xa4\xd0\xa4",
            b"\xd0\xa9\xd0\xa9",
            b"\xd0\xad\xd0\xad",
            b"\xd0\xae\xd0\xae",
            b"\xd0\xaf\xd0\xaf",
            b"\xce\xb1\xce\xb1",
            b"\xce\xb2\xce\xb2",
            b"\xce\xb3\xce\xb3",
            b"\xce\xb4\xce\xb4",
            b"\xce\xb5\xce\xb5",
            b"\xce\xb8\xce\xb8",
            b"\xce\xbb\xce\xbb",
            b"\xce\xbc\xce\xbc",
            b"\xcf\x80\xcf\x80",
            b"\xce\xa9\xce\xa9",
            b"\xe2\x88\x91\xe2\x88\x91",
            b"\xe2\x88\x86\xe2\x88\x86",
            b"\xe2\x88\x9a\xe2\x88\x9a",
            b"\xe2\x88\x9e\xe2\x88\x9e",
            b"\xe2\x89\x88\xe2\x89\x88",
            b"\xe2\x89\xa0\xe2\x89\xa0",
            b"\xe2\x89\xa4\xe2\x89\xa4",
            b"\xe2\x89\xa5\xe2\x89\xa5",
            b"\xe2\x99\xa0\xe2\x99\xa0",
            b"\xe2\x99\xa3\xe2\x99\xa3",
            b"\xe2\x99\xa5\xe2\x99\xa5",
            b"\xe2\x99\xa6\xe2\x99\xa6",
            b"\xe2\x98\xba\xe2\x98\xba",
            b"\xe2\x98\xbb\xe2\x98\xbb",
            b"\xd0\x96\xd0\x96",
            b"\xd0\xa4\xd0\xa4",
        ];

    pub const AM_RARE: [&[u8]; 40] = [
            b"\x2a",
            b"\x21",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
            b"\xce\xb2",
            b"\xce\xb3",
            b"\xce\xb4",
            b"\xce\xb5",
            b"\xce\xb8",
            b"\xce\xbb",
            b"\xce\xbc",
            b"\xcf\x80",
            b"\xce\xa9",
            b"\xe2\x88\x91",
            b"\xe2\x88\x86",
            b"\xe2\x88\x9a",
            b"\xe2\x88\x9e",
            b"\xe2\x89\x88",
            b"\xe2\x89\xa0",
            b"\xe2\x89\xa4",
            b"\xe2\x89\xa5",
            b"\xe2\x99\xa0",
            b"\xe2\x99\xa3",
            b"\xe2\x99\xa5",
            b"\xe2\x99\xa6",
            b"\xe2\x98\xba",
            b"\xe2\x98\xbb",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
            b"\xce\xb2",
        ];

    pub const AM_COMMON: [&[u8]; 40] = [
            b"\xe1\x8d\xa2",
            b"\xe1\x8d\xa5",
            b"\xe1\x8d\xa1",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
            b"\xce\xb2",
            b"\xce\xb3",
            b"\xce\xb4",
            b"\xce\xb5",
            b"\xce\xb8",
            b"\xce\xbb",
            b"\xce\xbc",
            b"\xcf\x80",
            b"\xce\xa9",
            b"\xe2\x88\x91",
            b"\xe2\x88\x86",
            b"\xe2\x88\x9a",
            b"\xe2\x88\x9e",
            b"\xe2\x89\x88",
            b"\xe2\x89\xa0",
            b"\xe2\x89\xa4",
            b"\xe2\x89\xa5",
            b"\xe2\x99\xa0",
            b"\xe2\x99\xa3",
            b"\xe2\x99\xa5",
            b"\xe2\x99\xa6",
            b"\xe2\x98\xba",
            b"\xe2\x98\xbb",
            b"\xd0\x96",
            b"\xd0\xa4",
            b"\xd0\xa9",
            b"\xd0\xad",
            b"\xd0\xae",
            b"\xd0\xaf",
            b"\xce\xb1",
        ];

    pub const AM_PAIRS: [&[u8]; 40] = [
            b"\xe1\x8d\xa2\x20",
            b"\xe1\x8d\xa5\x20",
            b"\xe1\x8d\xa1\x20",
            b"\xe1\x8d\xa2\x0a",
            b"\xe1\x88\x9d\xe1\x8d\xa2",
            b"\xe1\x89\xb5\xe1\x8d\xa2",
            b"\xd0\x96\xd0\x96",
            b"\xd0\xa4\xd0\xa4",
            b"\xd0\xa9\xd0\xa9",
            b"\xd0\xad\xd0\xad",
            b"\xd0\xae\xd0\xae",
            b"\xd0\xaf\xd0\xaf",
            b"\xce\xb1\xce\xb1",
            b"\xce\xb2\xce\xb2",
            b"\xce\xb3\xce\xb3",
            b"\xce\xb4\xce\xb4",
            b"\xce\xb5\xce\xb5",
            b"\xce\xb8\xce\xb8",
            b"\xce\xbb\xce\xbb",
            b"\xce\xbc\xce\xbc",
            b"\xcf\x80\xcf\x80",
            b"\xce\xa9\xce\xa9",
            b"\xe2\x88\x91\xe2\x88\x91",
            b"\xe2\x88\x86\xe2\x88\x86",
            b"\xe2\x88\x9a\xe2\x88\x9a",
            b"\xe2\x88\x9e\xe2\x88\x9e",
            b"\xe2\x89\x88\xe2\x89\x88",
            b"\xe2\x89\xa0\xe2\x89\xa0",
            b"\xe2\x89\xa4\xe2\x89\xa4",
            b"\xe2\x89\xa5\xe2\x89\xa5",
            b"\xe2\x99\xa0\xe2\x99\xa0",
            b"\xe2\x99\xa3\xe2\x99\xa3",
            b"\xe2\x99\xa5\xe2\x99\xa5",
            b"\xe2\x99\xa6\xe2\x99\xa6",
            b"\xe2\x98\xba\xe2\x98\xba",
            b"\xe2\x98\xbb\xe2\x98\xbb",
            b"\xd0\x96\xd0\x96",
            b"\xd0\xa4\xd0\xa4",
            b"\xd0\xa9\xd0\xa9",
            b"\xd0\xad\xd0\xad",
        ];

}

use std::sync::LazyLock;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind, MatchKind};
use divan::{Bencher, counter::BytesCount};
use memchr::memmem;
use sous_core::substrate::Substrate;
use sous_core::{BookKey, ChapterInput, ChapterKey, ChapterPass};

const ROOT: &str = "/Users/willkelly/Documents/Work/Code/usfm_onion_2/corpora/";
const FILES: [&str; 3] = ["WA-en-ulb.txt", "hin2017.txt", "amh.txt"];

/// One joined string per corpus: the tab-separated text column of every vref
/// line, space-joined, matching how `roofline.rs`'s `group` builds chapters.
static CORPORA: LazyLock<Vec<(&'static str, String)>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|name| {
            let path = format!("{ROOT}{name}");
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("corpus {path} must be present: {error}"));
            let mut text = String::with_capacity(raw.len());
            for line in raw.lines() {
                if let Some((_refpart, body)) = line.split_once('\t') {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(body);
                }
            }
            (*name, text)
        })
        .collect()
});

fn corpus(name: &str) -> &'static str {
    &CORPORA
        .iter()
        .find(|(file, _)| *file == name)
        .expect("bench arg names a listed corpus")
        .1
}

fn main() {
    // Print hit counts once, up front, so density is on the record even
    // though divan's own stdout is all timing.
    report_hit_counts();
    divan::main()
}

// ── Needle sets ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
enum Density {
    Rare,
    Common,
    Pairs,
}

const DENSITIES: [Density; 3] = [Density::Rare, Density::Common, Density::Pairs];
const NS: [usize; 4] = [1, 4, 10, 40];

/// One divan `args` axis flattening corpus × density, since divan matrixes
/// a single dynamic `args` list against `consts`, not two dynamic lists.
/// Labels are "<corpus file>|<density>"; `resolve` splits them back out.
const CASES: [&str; 9] = [
    "WA-en-ulb.txt|rare",
    "WA-en-ulb.txt|common",
    "WA-en-ulb.txt|pairs",
    "hin2017.txt|rare",
    "hin2017.txt|common",
    "hin2017.txt|pairs",
    "amh.txt|rare",
    "amh.txt|common",
    "amh.txt|pairs",
];

fn resolve(case: &str) -> (&'static str, Density) {
    let (corpus_name, density) = case.split_once('|').expect("case is corpus|density");
    let corpus_name = FILES
        .iter()
        .find(|f| **f == corpus_name)
        .expect("case names a listed corpus");
    let density = match density {
        "rare" => Density::Rare,
        "common" => Density::Common,
        "pairs" => Density::Pairs,
        other => panic!("unknown density label {other}"),
    };
    (corpus_name, density)
}

fn pool(corpus_name: &str, density: Density) -> &'static [&'static [u8]] {
    use needle_tables::*;
    match (corpus_name, density) {
        ("WA-en-ulb.txt", Density::Rare) => &EN_RARE,
        ("WA-en-ulb.txt", Density::Common) => &EN_COMMON,
        ("WA-en-ulb.txt", Density::Pairs) => &EN_PAIRS,
        ("hin2017.txt", Density::Rare) => &HI_RARE,
        ("hin2017.txt", Density::Common) => &HI_COMMON,
        ("hin2017.txt", Density::Pairs) => &HI_PAIRS,
        ("amh.txt", Density::Rare) => &AM_RARE,
        ("amh.txt", Density::Common) => &AM_COMMON,
        ("amh.txt", Density::Pairs) => &AM_PAIRS,
        _ => panic!("no needle pool for {corpus_name} / {density:?}"),
    }
}

fn needles(corpus_name: &str, density: Density, n: usize) -> Vec<&'static [u8]> {
    pool(corpus_name, density)[..n].to_vec()
}

fn label(density: Density) -> &'static str {
    match density {
        Density::Rare => "rare",
        Density::Common => "common",
        Density::Pairs => "pairs",
    }
}

/// The three engines must all agree on hit count; ground truth is a plain
/// `str::matches` scan (allowed to be slow, it runs once at startup).
fn ground_truth(text: &str, needle: &[u8]) -> u64 {
    let Ok(needle_str) = std::str::from_utf8(needle) else {
        return 0;
    };
    text.matches(needle_str).count() as u64
}

fn report_hit_counts() {
    println!("\n=== needle set hit counts (ground truth, str::matches) ===");
    for (corpus_name, text) in CORPORA.iter() {
        println!("--- {corpus_name} ({} bytes) ---", text.len());
        for density in DENSITIES {
            for n in NS {
                let set = needles(corpus_name, density, n);
                let hits: u64 = set.iter().map(|needle| ground_truth(text, needle)).sum();
                println!(
                    "  {:<7} N={:<3} hits={:<8} ({:.4}% of bytes)",
                    label(density),
                    n,
                    hits,
                    100.0 * hits as f64 * average_len(&set) as f64 / text.len() as f64
                );
            }
        }
    }
    println!();
}

fn average_len(set: &[&[u8]]) -> usize {
    if set.is_empty() {
        return 0;
    }
    set.iter().map(|n| n.len()).sum::<usize>() / set.len()
}

// ── Engine 1: one memchr pass per needle ────────────────────────────────
//
// Lead-byte memchr, then verify the rest of the needle at each candidate
// position. Correct for single-byte needles (no verify needed) and for
// multi-byte UTF-8 scalars and ASCII pairs alike.

fn memchr_count(text: &[u8], needle: &[u8]) -> u64 {
    let lead = needle[0];
    let rest = &needle[1..];
    let mut count = 0u64;
    for pos in memchr::memchr_iter(lead, text) {
        if text[pos + 1..].starts_with(rest) {
            count += 1;
        }
    }
    count
}

#[divan::bench(args = CASES, consts = NS)]
fn memchr_per_needle<const N: usize>(bencher: Bencher, case: &str) {
    let (name, density) = resolve(case);
    let text = corpus(name);
    let set = needles(name, density, N);
    bencher.counter(BytesCount::of_str(text)).bench_local(|| {
        let bytes = text.as_bytes();
        set.iter()
            .map(|needle| memchr_count(bytes, needle))
            .sum::<u64>()
    });
}

// ── Engine 2: one memmem pass per needle ────────────────────────────────

#[divan::bench(args = CASES, consts = NS)]
fn memmem_per_needle<const N: usize>(bencher: Bencher, case: &str) {
    let (name, density) = resolve(case);
    let text = corpus(name);
    let set = needles(name, density, N);
    let finders: Vec<memmem::Finder> = set.iter().map(|n| memmem::Finder::new(n)).collect();
    bencher.counter(BytesCount::of_str(text)).bench_local(|| {
        let bytes = text.as_bytes();
        finders
            .iter()
            .map(|finder| finder.find_iter(bytes).count() as u64)
            .sum::<u64>()
    });
}

// ── Engine 3: one Aho-Corasick pass over all needles ────────────────────

fn build_ac(set: &[&[u8]], kind: Option<AhoCorasickKind>) -> AhoCorasick {
    let mut builder = AhoCorasickBuilder::new();
    builder.match_kind(MatchKind::LeftmostFirst);
    if let Some(kind) = kind {
        builder.kind(Some(kind));
    }
    builder
        .build(set)
        .expect("needle set builds into an automaton")
}

#[divan::bench(args = CASES, consts = NS)]
fn aho_corasick_all<const N: usize>(bencher: Bencher, case: &str) {
    let (name, density) = resolve(case);
    let text = corpus(name);
    let set = needles(name, density, N);
    let ac = build_ac(&set, None);
    bencher
        .counter(BytesCount::of_str(text))
        .bench_local(|| ac.find_iter(text.as_bytes()).count() as u64);
}

#[divan::bench(args = CASES, consts = NS)]
fn aho_corasick_dfa<const N: usize>(bencher: Bencher, case: &str) {
    let (name, density) = resolve(case);
    let text = corpus(name);
    let set = needles(name, density, N);
    let ac = build_ac(&set, Some(AhoCorasickKind::DFA));
    bencher
        .counter(BytesCount::of_str(text))
        .bench_local(|| ac.find_iter(text.as_bytes()).count() as u64);
}

// ── Ceiling: the substrate walk ─────────────────────────────────────────

#[divan::bench(args = FILES)]
fn substrate_walk(bencher: Bencher, name: &str) {
    let text = corpus(name);
    let key = ChapterKey::new(BookKey::new(*b"MRK"), 1);
    bencher.counter(BytesCount::of_str(text)).bench(|| {
        Substrate
            .map(ChapterInput {
                text,
                verses: &[],
                key,
            })
            .scalar_count()
    });
}
