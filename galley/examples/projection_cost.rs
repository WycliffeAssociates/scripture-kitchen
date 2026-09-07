//! What it costs to get a book's *projected* text (verse text, markup
//! masked out) again: rebuild it from cached products, or keep a second
//! copy resident.
//!
//! ```text
//! cargo run -p usfm_galley --release --example projection_cost
//! ```
//!
//! Two public paths reach a projected `&str`:
//! - [`usfm_galley::sous::OnionBook::from_parts`] — the resident path
//!   [`usfm_galley::sous::Expediter::index_book`] itself uses: clone a
//!   Pantry-retained `Mask` and `Toc`, gather the projected text over them,
//!   no lexing.
//! - [`usfm_galley::sous::OnionBook::parse`] — the cold path: lex, build the
//!   CST, derive the mask and TOC, then the same gather. What a book costs
//!   the first time, with nothing cached yet.
//!
//! Over the 66-book `testData/exampleCorpora/en_ulb` corpus (the same one
//! `galley/benches/expediter.rs` loads), each book is timed at best of 20
//! `std::time::Instant` samples per row.

use std::time::{Duration, Instant};

use sous_core::ProjectedBook;
use usfm_galley::sous::OnionBook;
use usfm_galley::{Pantry, Role};

const SAMPLES: u32 = 20;

fn load_corpus() -> Vec<(String, String)> {
    let dir = format!(
        "{}/../testData/exampleCorpora/en_ulb",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "usfm"))
        .collect();
    paths.sort();
    let books: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    assert_eq!(books.len(), 66, "{dir} is a whole-Bible corpus");
    books
}

/// Best of `SAMPLES` calls to `f`, discarding the value each call produces.
fn best_of<T>(mut f: impl FnMut() -> T) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let out = f();
        let elapsed = start.elapsed();
        std::hint::black_box(out);
        best = best.min(elapsed);
    }
    best
}

struct Row {
    id: String,
    raw_len: usize,
    projected_len: usize,
    from_parts: Duration,
    parse: Duration,
}

fn main() {
    let corpus = load_corpus();

    // One registration per book, so `Pantry` derives and retains the mask
    // and TOC exactly once each — the products a resident host already has
    // in hand before either measured call.
    let mut pantry = Pantry::new(64 << 20);
    let mut rows = Vec::with_capacity(corpus.len());

    for (id, text) in &corpus {
        let entry = pantry.update(id.as_str(), Role::Target, text).unwrap();
        let mask = entry.mask().unwrap().clone();
        let toc = entry.toc().clone();
        drop(entry);

        // 1. Projection only, from the cached products: clone the retained
        // `Mask` and `Toc` (what `Expediter::index_book` does per book) and
        // gather the projected text over them. No lexing.
        let from_parts = best_of(|| {
            OnionBook::from_parts(text, mask.clone(), toc.clone())
                .unwrap()
                .text()
                .len()
        });

        // 2. Full re-derivation from raw text: lex, build the CST, derive
        // the mask and TOC, then the same gather.
        let parse = best_of(|| OnionBook::parse(text).unwrap().text().len());

        let projected_len = OnionBook::from_parts(text, mask.clone(), toc.clone())
            .unwrap()
            .text()
            .len();

        rows.push(Row {
            id: id.clone(),
            raw_len: text.len(),
            projected_len,
            from_parts,
            parse,
        });
    }

    let total_raw: usize = rows.iter().map(|row| row.raw_len).sum();
    let total_projected: usize = rows.iter().map(|row| row.projected_len).sum();
    let total_from_parts: Duration = rows.iter().map(|row| row.from_parts).sum();
    let total_parse: Duration = rows.iter().map(|row| row.parse).sum();

    println!(
        "{:<16} {:>10} {:>12} {:>14} {:>14}",
        "book", "raw bytes", "projected B", "from_parts ns", "parse ns"
    );
    for row in &rows {
        println!(
            "{:<16} {:>10} {:>12} {:>14} {:>14}",
            row.id,
            row.raw_len,
            row.projected_len,
            row.from_parts.as_nanos(),
            row.parse.as_nanos(),
        );
    }

    let mb_per_s = |bytes: usize, elapsed: Duration| {
        if elapsed.is_zero() {
            f64::INFINITY
        } else {
            (bytes as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64()
        }
    };

    println!();
    println!(
        "totals: {total_raw} raw bytes, {total_projected} projected bytes over {} books",
        rows.len()
    );
    println!(
        "from_parts (cached products): {:.1} ms total, {:.1} ns/book avg, {:.1} MB/s of raw text",
        total_from_parts.as_secs_f64() * 1e3,
        total_from_parts.as_nanos() as f64 / rows.len() as f64,
        mb_per_s(total_raw, total_from_parts),
    );
    println!(
        "parse (full re-derivation):   {:.1} ms total, {:.1} ns/book avg, {:.1} MB/s of raw text",
        total_parse.as_secs_f64() * 1e3,
        total_parse.as_nanos() as f64 / rows.len() as f64,
        mb_per_s(total_raw, total_parse),
    );
    println!(
        "a resident second copy of the projected text would cost {} bytes ({:.1}% of raw)",
        total_projected,
        100.0 * total_projected as f64 / total_raw as f64,
    );
}
