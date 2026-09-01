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
//! not a profile-guided guess. Numbers are recorded in roadmap.md's evidence
//! table; nothing here asserts on them.
//!
//! Byte source: the committed test-tier corpora only. A missing file is a
//! loud failure, never a silent skip.

use std::sync::LazyLock;

use divan::{Bencher, counter::BytesCount};

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
