//! Fleet sweep for the atom rule, over the gitignored calibration corpora.
//!
//!     cargo run -p sous-core --release --example atom_fleet
//!
//! Not a test: it reads the workspace's `corpora/calibration-corpora/`, which
//! CI does not have. It fails loudly when that directory is absent rather than
//! passing by doing nothing. Record what it found in evidence.md.

use std::path::{Path, PathBuf};

use sous_core::unicode::atoms::is_atom_boundary;
use unicode_segmentation::UnicodeSegmentation;

fn main() {
    let dir = std::env::args().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora/calibration-corpora"),
        PathBuf::from,
    );
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| {
            panic!(
                "the calibration fleet at {} must be present: {error}",
                dir.display()
            )
        })
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} holds no .txt corpora", dir.display());

    let (mut clusters, mut split, mut flagged) = (0u64, 0u64, 0u32);
    for path in &files {
        let (n, bad) = sweep(path);
        clusters += n;
        split += bad;
        if bad > 0 {
            flagged += 1;
            println!("{}: {bad} of {n} clusters split", name(path));
        }
    }
    println!(
        "\nfleet: {} corpora, {clusters} clusters, {split} split by the atom rule, \
         {flagged} corpora affected",
        files.len()
    );
}

fn name(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn sweep(path: &Path) -> (u64, u64) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
    let (mut clusters, mut split, mut at) = (0u64, 0u64, 0usize);
    for cluster in text.graphemes(true) {
        clusters += 1;
        for inner in 1..cluster.len() {
            if cluster.is_char_boundary(inner) && is_atom_boundary(&text, at + inner) {
                split += 1;
                break;
            }
        }
        at += cluster.len();
    }
    (clusters, split)
}
