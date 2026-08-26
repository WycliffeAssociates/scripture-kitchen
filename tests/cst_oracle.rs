//! The lifted CST partition oracle: every token is in exactly one child list,
//! and the shared in-order iterator recovers the scanner's token sequence.
//!
//! The corpus is gitignored, so this test skips loudly when it is not mounted.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion::{cst::build, lex};

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
fn every_corpus_book_partitions_into_cst_children() {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("cst oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    }
    paths.sort();

    paths.par_iter().for_each(|path| {
        let source = std::fs::read_to_string(path).unwrap();
        let tokens = lex(&source);
        let cst = build(&tokens);

        let mut seen = vec![0u8; tokens.len()];
        for &child_id in &cst.child_ids {
            if child_id & (1 << 31) == 0 {
                let token = child_id as usize;
                assert!(
                    token < tokens.len(),
                    "{}: invalid token id {token}",
                    path.display()
                );
                seen[token] += 1;
            }
        }
        assert!(
            seen.iter().all(|count| *count == 1),
            "{}: token partition counts: {seen:?}",
            path.display()
        );

        let ordered: Vec<u32> = cst.in_order().collect();
        let expected: Vec<u32> = (0..tokens.len() as u32).collect();
        assert_eq!(
            ordered,
            expected,
            "{}: CST in_order changed token order",
            path.display()
        );
    });

    eprintln!("cst oracle: {} books, all tokens partitioned", paths.len());
}
