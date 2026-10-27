//! `for_each_unit` tiles real text, and its words are `for_each_word`'s.
//!
//! ```text
//! for every corpus text:
//!     units are gapless, non-empty, open on 0 and close on text.len()
//!     the units flagged word == for_each_word's spans
//! ```
//!
//! Instrument: VOLUME — the sous 8-corpus tier, `corpora/*.txt`, for script
//! spread, and onion's test tier, `testData/exampleCorpora`. Absent bytes are
//! a loud failure, never a silent skip.

use std::path::{Path, PathBuf};

use mise::words::{for_each_unit, for_each_word};

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

fn tiles(text: &str, label: &str) {
    let mut at = 0;
    let mut unit_words = Vec::new();
    for_each_unit(text, |from, to, word| {
        assert_eq!(from, at, "{label}: gapless at {from}");
        assert!(to > from, "{label}: empty unit at {from}");
        at = to;
        if word {
            unit_words.push((from, to));
        }
    });
    assert_eq!(at as usize, text.len(), "{label}: closes on the end");
    let mut words = Vec::new();
    for_each_word(text, |from, to| words.push((from, to)));
    assert_eq!(unit_words, words, "{label}: the words are for_each_word's");
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("test-tier text {path:?} must be present: {error}"))
}

#[test]
fn units_tile_the_sous_tier() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpora");
    for name in FILES {
        tiles(&read(&root.join(name)), name);
    }
}

#[test]
fn units_tile_the_onion_tier() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testData/exampleCorpora");
    let mut books: Vec<PathBuf> = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap_or_else(|e| panic!("{root:?}: {e}")) {
        let dir = dir.unwrap().path();
        if dir.is_dir() {
            for file in std::fs::read_dir(&dir).unwrap() {
                let file = file.unwrap().path();
                if file.extension().is_some_and(|ext| ext == "usfm") {
                    books.push(file);
                }
            }
        }
    }
    assert!(
        books.len() == 160,
        "the onion tier holds its books: {}",
        books.len()
    );
    for book in &books {
        tiles(&read(book), &book.display().to_string());
    }
}
