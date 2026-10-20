//! What it costs to locate every verse of a large book and drain its raw runs.
//!
//! ```text
//! cargo run -p usfm_galley --release --example locate_cost
//! ```
//!
//! Over `testData/exampleCorpora/en_ulb/19-PSA.usfm`, best of 20
//! `std::time::Instant` samples.

use std::time::{Duration, Instant};

use sous_core::ProjectedBook;
use usfm_galley::sous::OnionBook;

const SAMPLES: u32 = 20;

fn main() {
    let path = format!(
        "{}/../testData/exampleCorpora/en_ulb/19-PSA.usfm",
        env!("CARGO_MANIFEST_DIR")
    );
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let book = OnionBook::parse(&source).unwrap();
    let verses: Vec<_> = book.verses().map(|verse| verse.text()).collect();

    let mut best = Duration::MAX;
    let mut runs = 0;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        runs = verses
            .iter()
            .map(|&range| book.locate(range).unwrap().spans.count())
            .sum::<usize>();
        best = best.min(started.elapsed());
    }
    println!(
        "{} verses, {} mask ranges, {runs} runs: best {best:?} of {SAMPLES}",
        verses.len(),
        book.mask().ranges.len(),
    );
}
