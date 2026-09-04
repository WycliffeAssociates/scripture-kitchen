//! Every `common_marker_checks` arm must be INVISIBLE: `lex` (fast checks on)
//! and `lex_general_path_only` must produce identical token streams — spans,
//! kinds, and marker indices, token for token. The general path is the
//! definition; an arm that changes output is a bug, whatever it benches.
//!
//! Instrument: VOLUME — the whole test tier, `testData/exampleCorpora` (12.8 MB,
//! 160 books). Absent bytes are a loud failure, never a silent skip.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion::{lex, lex_general_path_only};

fn collect_usfm_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_usfm_paths(&path, &mut *paths);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            paths.push(path);
        }
    }
}

#[test]
fn fast_paths_are_token_identical_to_the_general_path() {
    let mut paths = Vec::new();
    collect_usfm_paths(
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../testData/exampleCorpora"
        )),
        &mut paths,
    );
    assert!(
        !paths.is_empty(),
        "no *.usfm under testData/exampleCorpora/"
    );
    paths.sort();

    paths.par_iter().for_each(|path| {
        let source = std::fs::read_to_string(path).unwrap();
        let fast = lex(&source);
        let general = lex_general_path_only(&source);
        assert_eq!(
            fast.len(),
            general.len(),
            "{}: token count differs (fast {}, general {})",
            path.display(),
            fast.len(),
            general.len()
        );
        for (i, (f, g)) in fast.iter().zip(&general).enumerate() {
            assert_eq!(
                f,
                g,
                "{}: token {i} differs — around: {:?}",
                path.display(),
                &source[(g.start as usize).saturating_sub(20)
                    ..((g.start as usize) + 20).min(source.len())],
            );
        }
    });

    eprintln!(
        "fast-path identity: {} books, streams identical",
        paths.len()
    );
}
