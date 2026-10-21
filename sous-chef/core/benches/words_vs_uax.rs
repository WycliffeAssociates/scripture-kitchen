//! `words/1` against UAX #29: the same verse text split into word spans, no
//! hashing, so only the boundary decisions are timed.
//!
//!     cargo bench -p sous-core --bench words_vs_uax
//!
//!     words_vs_uax          fastest │ bytes/s
//!     ├─ words1 ...         x ms    │          mise::words::for_each_word
//!     ├─ uax_bounds ...     x ms    │          split_word_bound_indices, segments holding a letter or digit
//!     ╰─ uax_words ...      x ms    │          unicode_words()
//!
//! Numbers go to evidence.md; nothing here asserts on them. The input is each
//! test-tier corpus's verse text, references dropped, one verse per line.

use std::sync::LazyLock;

use divan::{Bencher, counter::BytesCount};
use unicode_segmentation::UnicodeSegmentation;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpora/");
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

static VERSES: LazyLock<Vec<(&'static str, String)>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|name| {
            let path = format!("{ROOT}{name}");
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("test-tier corpus {path} must be present: {error}"));
            let mut text = String::with_capacity(raw.len());
            for line in raw.lines() {
                if let Some((_, verse)) = line.split_once('\t') {
                    text.push_str(verse);
                    text.push('\n');
                }
            }
            (*name, text)
        })
        .collect()
});

fn verses(name: &str) -> &'static str {
    &VERSES
        .iter()
        .find(|(file, _)| *file == name)
        .expect("bench arg names a listed corpus")
        .1
}

fn main() {
    divan::main()
}

#[divan::bench(args = FILES)]
fn words1(bencher: Bencher, name: &str) {
    let text = verses(name);
    bencher.counter(BytesCount::of_str(text)).bench(|| {
        let mut total = 0u64;
        mise::words::for_each_word(text, |from, to| total += u64::from(to - from));
        total
    });
}

#[divan::bench(args = FILES)]
fn uax_bounds(bencher: Bencher, name: &str) {
    let text = verses(name);
    bencher.counter(BytesCount::of_str(text)).bench(|| {
        text.split_word_bound_indices()
            .filter(|(_, segment)| segment.chars().any(char::is_alphanumeric))
            .map(|(_, segment)| segment.len() as u64)
            .sum::<u64>()
    });
}

#[divan::bench(args = FILES)]
fn uax_words(bencher: Bencher, name: &str) {
    let text = verses(name);
    bencher.counter(BytesCount::of_str(text)).bench(|| {
        text.unicode_words()
            .map(|word| word.len() as u64)
            .sum::<u64>()
    });
}
