//! What a declared source costs a resident host, per book and per Bible.
//!
//!     cargo run -p usfm_galley --release --example reference_bytes
//!
//! Registers the committed 66-book `testData/exampleCorpora/en_ulb` twice —
//! once as `Role::Target`, once as `Role::Reference` — and reports what each
//! role's own products weigh. The chunk cache is shared and budget-bound, so it is
//! subtracted from both: what is left is exactly the per-book retention the
//! role chose. Not a test; it prints a row for the ledger.

use usfm_galley::{Pantry, Retain, Role, SourceLanes};

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
    let mut words = 0usize;
    let mut hashes = 0usize;
    let roles = [
        (Role::Target, SourceLanes::Lengths, "Target"),
        (Role::Reference, SourceLanes::Lengths, "Reference"),
        (
            Role::Reference,
            SourceLanes::LengthsAndWords,
            "Reference+words",
        ),
    ];
    for (role, lanes, label) in roles {
        // A fresh Pantry per role, so the shared chunk cache cannot make the
        // second role look cheaper than it is.
        let mut pantry = Pantry::new(64 << 20);
        let retain = match role {
            Role::Target => Retain::Text,
            Role::Reference => Retain::ProductsOnly,
        };
        for (id, text) in &books {
            let entry = pantry
                .update_with(id.as_str(), role, retain, lanes, text)
                .unwrap();
            if lanes == SourceLanes::LengthsAndWords {
                verses += entry.verse_lengths().unwrap().len();
                let lane = entry.verse_words().unwrap();
                words += lane.resident_bytes();
                hashes += (0..lane.len())
                    .map(|at| lane.verse(at).len())
                    .sum::<usize>();
            }
        }
        let own = pantry.resident_bytes() - pantry.chunk_stats().resident_bytes;
        println!(
            "{label:<16} own products {:>10} B  ({:>6.2} MB, {:>5.1}% of raw)  text {:>9} B",
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
    println!(
        "source-copy lane {words} B ({:.2} MB, {:.1}% of raw): {hashes} distinct words \
         over {verses} verses, {:.1} per verse \u{2014} paid only under \
         SourceLanes::LengthsAndWords",
        words as f64 / 1e6,
        words as f64 / raw as f64 * 100.0,
        hashes as f64 / verses as f64,
    );
}
