//! The lifted CST partition oracle: every token is in exactly one child list,
//! and the shared in-order iterator recovers the scanner's token sequence.
//!
//! Instrument: VOLUME — the whole test tier, `testData/exampleCorpora` (12.8 MB,
//! 160 books). Absent bytes are a loud failure, never a silent skip.

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

/// Node ids ascend with their opening markers, so a row index IS document
/// order.
///
/// The wire's `walkNodes()` reads the nodes section straight through instead of
/// walking the child arena, which is only document order if this holds. It is a
/// property of how the builder assigns ids, not something a type enforces — so
/// it is asserted here rather than assumed there.
#[test]
fn node_ids_ascend_with_their_opening_markers() {
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
        let tokens = lex(&source);
        let cst = build(&tokens);
        let mut previous = 0u32;
        for id in 1..cst.nodes.len() as u32 {
            let start = cst.extent(id, &tokens).start;
            assert!(
                start >= previous,
                "{}: node {id} starts at {start}, behind node {} at {previous} — a row \
                 index is no longer document order, and wire::walkNodes is wrong",
                path.display(),
                id - 1,
            );
            previous = start;
        }
    });
}
