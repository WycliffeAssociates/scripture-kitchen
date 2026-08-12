//! The partition oracle: `concat(all spans) == source`, byte for byte.
//!
//! This is THE standing invariant — the lexer partitions its input
//! losslessly, so save is concatenation and refuse-never-invent holds
//! because there is nothing outside the spans. Checked in its stronger
//! contiguous form (each token starts exactly where the previous one
//! ended, and the last one ends at EOF), which implies concat equality
//! and pinpoints the first divergence instead of diffing two books.
//!
//! Runs over every `*.usfm` under `example-corpora/` (gitignored — the
//! test skips loudly when the corpora aren't on disk).

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion_2::lex;

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

#[test]
fn every_corpus_book_partitions_losslessly() {
    let root = Path::new("example-corpora");
    let mut paths = Vec::new();
    collect_usfm_paths(root, &mut paths);
    if paths.is_empty() {
        eprintln!("partition oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    }
    paths.sort();

    paths.par_iter().for_each(|path| {
        let source = std::fs::read_to_string(path).unwrap();
        let tokens = lex(&source);

        let mut cursor: usize = 0;
        for (i, token) in tokens.iter().enumerate() {
            assert_eq!(
                token.start as usize,
                cursor,
                "{}: token {i} ({:?}) starts at {} but previous span ended at {} — \
                 around: {:?}",
                path.display(),
                token.kind(),
                token.start,
                cursor,
                &source[cursor.saturating_sub(20)..(cursor + 20).min(source.len())],
            );
            cursor += token.len as usize;
        }
        assert_eq!(
            cursor,
            source.len(),
            "{}: spans end at {cursor} but the source is {} bytes",
            path.display(),
            source.len(),
        );
    });

    eprintln!("partition oracle: {} books, all lossless", paths.len());
}
