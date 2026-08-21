//! The utf16 oracle: the stride index agrees with a naive `char` walk at EVERY
//! character boundary of every real file we have.
//!
//! The unit tests in src/utf16.rs pin the shapes and the synthetic zoo (stride
//! boundaries split mid-character, supplementary-plane pairs); this pins the
//! two invariants over real text, exhaustively:
//!
//! - `to_utf16(byte)` equals the walk's count at every boundary.
//! - `to_byte(to_utf16(byte)) == byte` at every boundary.
//! - the index is one u32 per `STRIDE` bytes.
//!
//! Files: every `*.usfm` under `example-corpora/` (ASCII-dominant English) plus
//! `testData/samples-from-wild/hindi-IRV1/` (dense Devanagari — 3 bytes per
//! character, where byte and UTF-16 offsets drift on nearly every character).
//! Both trees are gitignored; the test skips loudly when neither is mounted.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion_2::utf16::{STRIDE, Utf16Index};

fn collect_usfm_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_usfm_paths(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            paths.push(path);
        }
    }
}

fn corpus() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    collect_usfm_paths(
        Path::new("testData/samples-from-wild/hindi-IRV1"),
        &mut paths,
    );
    paths.sort();
    paths
}

#[test]
fn every_corpus_boundary_matches_a_char_walk() {
    let paths = corpus();
    if paths.is_empty() {
        eprintln!("utf16 oracle SKIPPED: no *.usfm under example-corpora/ or testData/");
        return;
    }

    let boundaries: u64 = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            let ix = Utf16Index::new(source.as_bytes());
            let where_ = path.display();

            assert_eq!(
                ix.index_bytes(),
                (source.len() / STRIDE + 1) * 4,
                "{where_}: index is not one u32 per stride"
            );

            let mut utf16 = 0u32;
            let mut checked = 0u64;
            for (byte, ch) in source.char_indices() {
                let byte = byte as u32;
                assert_eq!(
                    ix.to_utf16(byte),
                    utf16,
                    "{where_}: to_utf16({byte}) disagrees with the walk"
                );
                assert_eq!(
                    ix.to_byte(utf16),
                    byte,
                    "{where_}: to_byte({utf16}) does not come back to {byte}"
                );
                utf16 += ch.len_utf16() as u32;
                checked += 1;
            }
            // The end offset is a boundary too, and the only one the trailing
            // partial stride block can be asked about.
            assert_eq!(ix.len_utf16(), utf16, "{where_}: total length");
            assert_eq!(ix.to_utf16(source.len() as u32), utf16, "{where_}: end fwd");
            assert_eq!(
                ix.to_byte(utf16),
                source.len() as u32,
                "{where_}: end reverse"
            );
            checked + 1
        })
        .sum();

    println!(
        "utf16 oracle: {} files, {boundaries} character boundaries, both directions",
        paths.len()
    );
}
