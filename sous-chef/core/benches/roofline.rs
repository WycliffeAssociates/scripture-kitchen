//! Hardware ceilings the hygiene scan is measured against, single core.
//!
//!     cargo bench -p sous-core                     # all corpora, all passes
//!     cargo bench -p sous-core -- hygiene          # one pass
//!
//!     roofline            fastest │ median │ bytes/s
//!     ├─ scalar   ...     x ms    │        │  ~1 GiB/s   dependent byte chain
//!     ├─ vectorized ...   x ms    │        │ ~30 GiB/s   autovectorized compare
//!     ├─ memchr3 ...      x ms    │        │ ~10 GiB/s   three fixed needles
//!     ╰─ hygiene ...      x ms    │        │             the real pass
//!
//! Every pass below is an intentional subtraction from the ceiling above it,
//! not a profile-guided guess. Numbers are recorded in evidence.md;
//! nothing here asserts on them.
//!
//! Byte source: the committed test-tier corpora only. A missing file is a
//! loud failure, never a silent skip.

use std::sync::LazyLock;

use divan::{
    Bencher,
    counter::{BytesCount, ItemsCount},
};
use sous_core::unicode::lookup::{walk, walk_trie, walk_trie_swar};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../corpora/");
const FILES: [&str; 8] = [
    "WA-en-ulb.txt",
    "amh.txt",
    "francl.txt",
    "grcsr.txt",
    "hin2017.txt",
    "nya.txt",
    "spaRV1909.txt",
    "swhulb.txt",
];

static CORPORA: LazyLock<Vec<(&'static str, String)>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|name| {
            let path = format!("{ROOT}{name}");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("test-tier corpus {path} must be present: {error}"));
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
    divan::main()
}

/// A dependent multiply chain: one byte per step, no SIMD possible.
#[divan::bench(args = FILES)]
fn scalar(bencher: Bencher, name: &str) {
    let bytes = corpus(name).as_bytes();
    bencher.counter(BytesCount::of_slice(bytes)).bench(|| {
        bytes.iter().fold(0u64, |acc, &b| {
            acc.wrapping_mul(0x100_0000_01b3).wrapping_add(u64::from(b))
        })
    });
}

/// An autovectorized compare-and-count: the memory/SIMD ceiling.
#[divan::bench(args = FILES)]
fn vectorized(bencher: Bencher, name: &str) {
    let bytes = corpus(name).as_bytes();
    bencher.counter(BytesCount::of_slice(bytes)).bench(|| {
        bytes
            .iter()
            .fold(0u32, |acc, &b| acc + u32::from(b < 0x20 || b == 0x7f))
    });
}

/// The three lead bytes hygiene's needle filter looks for.
#[divan::bench(args = FILES)]
fn memchr3(bencher: Bencher, name: &str) {
    let bytes = corpus(name).as_bytes();
    bencher
        .counter(BytesCount::of_slice(bytes))
        .bench(|| memchr::memchr3_iter(0x5c, 0xc2, 0xef, bytes).count());
}

/// The real pass: both fast filters plus marker search over clean text.
#[divan::bench(args = FILES)]
fn hygiene(bencher: Bencher, name: &str) {
    let text = corpus(name);
    bencher
        .counter(BytesCount::of_str(text))
        .bench(|| sous_core::hygiene::scan(text).len());
}

// ── The classifier walk (Stage 1) ───────────────────────────────────────
//
// One `Class` per scalar over the whole corpus. Read against the ceilings
// above: the walk is a dependent load chain, so the scalar row is its real
// neighbour, not `memchr3`. Rejected candidates live in
// `sous-chef/experiments/`.

fn classified(bencher: Bencher, name: &str, walk: impl Fn(&str) -> u64 + Sync) {
    let text = corpus(name);
    bencher
        .counter(BytesCount::of_str(text))
        .counter(ItemsCount::new(text.chars().count()))
        .bench(|| walk(text));
}

/// The plain per-scalar walk: decode, then one two-level lookup.
#[divan::bench(args = FILES)]
fn class_two_level(bencher: Bencher, name: &str) {
    classified(bencher, name, walk);
}

/// The byte trie walking raw UTF-8, no scalar decode.
#[divan::bench(args = FILES)]
fn lane_trie_bytes(bencher: Bencher, name: &str) {
    classified(bencher, name, walk_trie);
}

/// The shipped lane: an eight-byte SWAR ASCII chunk over the byte trie.
#[divan::bench(args = FILES)]
fn lane_trie_swar(bencher: Bencher, name: &str) {
    classified(bencher, name, walk_trie_swar);
}
