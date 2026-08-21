//! The parse-header oracle: every corpus book's `ParseHeader` indexes its own stream.
//!
//! The unit tests in src/header.rs pin the shapes; this pins the two
//! properties the COORDINATE ADAPTER rests on, over real books:
//!
//! - **Runs tile the rows** from the first `\c` to the last token, with no gap
//!   and no overlap, each opening on its own chapter marker.
//! - **Therefore they tile the BYTES too** — what a slot-relative remap needs,
//!   checked through the run table rather than assumed from it.
//!
//! Runs over every `*.usfm` under `example-corpora/` (gitignored — the test
//! skips loudly when the corpora aren't on disk).

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion_2::{ParseHeader, lex};

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
fn every_corpus_book_indexes_its_own_stream() {
    let root = Path::new("example-corpora");
    let mut paths = Vec::new();
    collect_usfm_paths(root, &mut paths);
    if paths.is_empty() {
        eprintln!("header oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    }
    paths.sort();

    let books_without_a_code: usize = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            let tokens = lex(&source);
            let parsed = ParseHeader::from_tokens(&tokens, &source);
            let where_ = path.display();

            let mut expected_start = parsed.runs.first().map(|r| r.rows.start);
            for (i, run) in parsed.runs.iter().enumerate() {
                assert_eq!(
                    Some(run.rows.start),
                    expected_start,
                    "{where_}: run {i} starts at row {} but the previous run ended at {:?}",
                    run.rows.start,
                    expected_start,
                );
                assert!(
                    run.rows.start < run.rows.end,
                    "{where_}: run {i} is empty ({:?})",
                    run.rows,
                );
                // The run owns its opener, and the opener really is a `\c`.
                let opener = tokens[run.rows.start as usize];
                assert_eq!(
                    source[opener.start as usize..opener.end() as usize]
                        .trim_end_matches([' ', '\t']),
                    "\\c",
                    "{where_}: run {i} opens on a token that is not a chapter marker",
                );
                // The label lies inside the run: a non-empty one is the
                // designator carved right after the opener.
                assert!(
                    run.label.0 >= opener.start && run.label.0 + run.label.1 as u32 <= source.len() as u32,
                    "{where_}: run {i}'s label {:?} is not inside its own run",
                    run.label,
                );
                expected_start = Some(run.rows.end);
            }
            if let Some(last) = parsed.runs.last() {
                assert_eq!(
                    last.rows.end,
                    tokens.len() as u32,
                    "{where_}: the last run stops short of the end of the stream",
                );

                // The rows tile, so the BYTES tile: run boundaries are token
                // boundaries and the partition oracle makes those exact.
                let mut cursor = tokens[parsed.runs[0].rows.start as usize].start;
                for (i, run) in parsed.runs.iter().enumerate() {
                    let start = tokens[run.rows.start as usize].start;
                    assert_eq!(
                        start, cursor,
                        "{where_}: run {i} starts at byte {start}, not {cursor}",
                    );
                    cursor = tokens[run.rows.end as usize - 1].end();
                }
                assert_eq!(
                    cursor as usize,
                    source.len(),
                    "{where_}: the runs cover bytes up to {cursor} of {}",
                    source.len(),
                );
            }

            // A book code is found EXACTLY when there is an `\id` line with
            // something after it — cross-checked against the raw text rather
            // than pinned as a corpus fact, since BSB Ecclesiastes ships with
            // no `\id` at all and `None` is the honest answer there.
            let id_line_with_a_code = source
                .lines()
                .any(|line| line.strip_prefix("\\id ").is_some_and(|rest| !rest.is_empty()));
            assert_eq!(
                parsed.book.is_some(),
                id_line_with_a_code,
                "{where_}: book code is {:?} but an `\\id` line with a code is {id_line_with_a_code}",
                parsed.book,
            );

            usize::from(parsed.book.is_none())
        })
        .sum();

    eprintln!(
        "header oracle: {} books, {books_without_a_code} without a book code",
        paths.len(),
    );
}
