//! Every `common_marker_checks` arm must be INVISIBLE: `lex` (fast checks on)
//! and `lex_general_path_only` must produce identical token streams — spans,
//! kinds, and marker indices, token for token. The general path is the
//! definition; an arm that changes output is a bug, whatever it benches.
//!
//! Runs over every `*.usfm` under `example-corpora/` (gitignored — skips
//! loudly when absent), same corpus discipline as the partition oracle.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion_2::{lex, lex_general_path_only};

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
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("fast-path identity SKIPPED: no *.usfm under example-corpora/");
        return;
    }
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

    eprintln!("fast-path identity: {} books, streams identical", paths.len());
}
