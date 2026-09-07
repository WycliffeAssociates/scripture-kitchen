//! What a declared source costs a resident host, per book and per Bible.
//!
//!     cargo run -p usfm_galley --release --example reference_bytes
//!
//! Registers the committed 66-book `testData/exampleCorpora/en_ulb` twice —
//! once as `Role::Target`, once as `Role::Reference` — and reports what each
//! role's own products weigh. The Warmer is shared and budget-bound, so it is
//! subtracted from both: what is left is exactly the per-book retention the
//! role chose. Not a test; it prints a row for the ledger.

use usfm_galley::{Pantry, Role};

const CORPUS_DIR: &str = "en_ulb";

fn main() {
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
    let raw: usize = books.iter().map(|(_, text)| text.len()).sum();

    let mut verses = 0usize;
    for (role, label) in [(Role::Target, "Target"), (Role::Reference, "Reference")] {
        // A fresh Pantry per role, so the shared Warmer cache cannot make the
        // second role look cheaper than it is.
        let mut pantry = Pantry::new(64 << 20);
        for (id, text) in &books {
            let entry = pantry.update(id.as_str(), role, text).unwrap();
            if role == Role::Reference {
                verses += entry.verse_lengths().unwrap().len();
            }
        }
        let own = pantry.resident_bytes() - pantry.warmer().resident_bytes();
        println!(
            "{label:<10} own products {:>10} B  ({:>6.2} MB, {:>5.1}% of raw)  text {:>9} B",
            own,
            own as f64 / 1e6,
            own as f64 / raw as f64 * 100.0,
            pantry.text_bytes(),
        );
    }
    println!(
        "raw {raw} B over {} books, {verses} reference verse rows at {} B each",
        books.len(),
        size_of::<sous_core::SourceVerse>(),
    );
}
