//! Allocation-site attribution for the ~58 MiB `working_set` reports as
//! unattributed at every rebuildable budget. Reuses `working_set`'s corpus load
//! and churn recipe verbatim (see that file's doc comment) but swaps the
//! counting allocator for `dhat`, whose JSON records bytes-at-global-max per
//! allocation site (file:line + backtrace), not just a running total.
//!
//! ```text
//! cargo run -p usfm_galley --release --example heap_profile
//! ```
//!
//! Writes `dhat-heap.json` into the crate's `debug/` (gitignored). Read it
//! with `dh_view.html` (https://nnethercote.github.io/dh_view/dh_view.html)
//! or parse it directly — it is plain JSON, no viewer required.

use rustc_hash::FxHashMap;
use sous_core::Brigade;
use usfm_galley::sous::Expediter;
use usfm_galley::{Role, onion};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const CORPUS_DIR: &str = "en_ulb";
const KEYSTROKES: usize = 1_000;
const WARMER_BUDGET_MIB: usize = 8;

fn load_corpus() -> Vec<(String, String)> {
    let dir = format!(
        "{}/../testData/exampleCorpora/{CORPUS_DIR}",
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

/// Same offset recipe as `working_set::edit_point`.
fn edit_point(text: &str) -> usize {
    let starts = onion::chunk::pre_scan(text.as_bytes()).starts;
    let chunk = if starts.len() > 1 {
        starts.len() / 2
    } else {
        0
    };
    let from = starts[chunk];
    let to = starts.get(chunk + 1).copied().unwrap_or(text.len() as u32);
    let tokens = onion::lex(text);
    tokens
        .iter()
        .find(|token| {
            token.kind() == onion::TokenKind::Text
                && token.start >= from
                && token.start < to
                && token.len > 0
        })
        .or_else(|| {
            tokens
                .iter()
                .find(|token| token.kind() == onion::TokenKind::Text && token.len > 0)
        })
        .map(|token| token.start as usize)
        .unwrap_or(text.len())
}

fn main() {
    std::fs::create_dir_all(format!("{}/../debug", env!("CARGO_MANIFEST_DIR"))).ok();
    let out_path = format!("{}/../debug/dhat-heap.json", env!("CARGO_MANIFEST_DIR"));
    let _profiler = dhat::Profiler::builder().file_name(&out_path).build();

    let corpus = load_corpus();
    let churn_books: Vec<usize> = (0..corpus.len()).collect();
    let edit_points: FxHashMap<usize, usize> = churn_books
        .iter()
        .map(|&i| (i, edit_point(&corpus[i].1)))
        .collect();
    let mut live_texts: FxHashMap<usize, String> = churn_books
        .iter()
        .map(|&i| (i, corpus[i].1.clone()))
        .collect();

    let mut sous = Expediter::new(Brigade::default(), WARMER_BUDGET_MIB << 20);
    for (id, text) in &corpus {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    sous.publish().unwrap();

    for step in 1..=KEYSTROKES {
        let index = churn_books[(step - 1) % churn_books.len()];
        let text = live_texts.get_mut(&index).unwrap();
        let at = edit_points[&index].min(text.len());
        text.insert(at, 'x');
        sous.update(corpus[index].0.as_str(), Role::Target, text.as_str())
            .unwrap();
        sous.publish().unwrap();
    }

    println!("wrote {out_path}");
    println!(
        "Expediter::resident_bytes()={} Pantry::text_bytes()={} chunk_stats().resident_bytes={}",
        sous.resident_bytes(),
        sous.pantry().text_bytes(),
        sous.pantry().chunk_stats().resident_bytes,
    );
    // Keep the harness's own copies alive until here so dhat's global-max
    // snapshot (taken at drop, below) sees them — same reason `working_set`
    // subtracts them explicitly rather than dropping early.
    drop((corpus, live_texts));
}
