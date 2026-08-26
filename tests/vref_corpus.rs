//! The vref corpus test: the two parallel files line up, over every book.
//!
//! The unit tests in src/vref.rs pin the shapes; this pins the property the
//! whole format rests on, over real books: **line N of the keys file and line N
//! of the lines file are the same verse.** So:
//!
//! - the two renderings have the same line count, and it is the iterator's item
//!   count;
//! - no line contains a newline or a tab, which would forge a line or a column;
//! - the `<range>` lines are exactly the verses the bridges cover.
//!
//! Runs over every `*.usfm` under `example-corpora/` (gitignored — the test
//! skips loudly when the corpora aren't on disk).

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion::mask::{Filter, mask};
use usfm_onion::vref::{self, RANGE};
use usfm_onion::{cst, lex, toc, verses};

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
    paths.sort();
    paths
}

/// Lines in a rendering. Empty renders as no lines; otherwise the separators
/// are one fewer than the lines.
fn line_count(rendered: &str) -> usize {
    if rendered.is_empty() {
        0
    } else {
        rendered.split('\n').count()
    }
}

#[test]
fn every_corpus_book_renders_two_files_that_line_up() {
    let paths = corpus();
    if paths.is_empty() {
        eprintln!("vref corpus SKIPPED: no *.usfm under example-corpora/");
        return;
    }

    let tallies: Vec<(u64, u64, u64)> = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).expect("readable book");
            let bytes = source.as_bytes();
            let where_ = path.display();
            let tokens = lex(&source);
            let cst = cst::build(&tokens);
            let toc = toc(bytes, &tokens);
            let m = mask(bytes, &tokens, &cst, &Filter::verse_text());

            let items: Vec<_> = verses(&toc, &m, bytes).collect();
            let keys = vref::keys(&toc, &m, bytes);
            let lines = vref::lines(&toc, &m, bytes, true);
            assert_eq!(
                line_count(&keys),
                items.len(),
                "{where_}: the keys file lost a line"
            );
            assert_eq!(
                line_count(&lines),
                items.len(),
                "{where_}: keys and lines disagree about how many verses there are"
            );
            assert_eq!(
                line_count(&vref::joined(&toc, &m, bytes, true)),
                items.len(),
                "{where_}: the joined rendering lost a line"
            );

            for (sid, text) in &items {
                let sid = sid.to_string();
                assert!(
                    !sid.contains(['\n', '\t']),
                    "{where_}: the sid {sid:?} would forge a line"
                );
                assert!(
                    !text.contains('\n'),
                    "{where_}: {sid}'s text carries a newline"
                );
                assert!(
                    !text.contains('\t'),
                    "{where_}: {sid}'s text carries a tab, which the joined form uses as its column"
                );
            }

            // `<range>` lines are exactly the verses the bridges cover — the
            // reason a bridge does not slide later verses onto wrong lines.
            let covered: u64 = toc
                .verses
                .iter()
                .filter(|v| v.chapter != 0)
                .map(|v| u64::from(v.last.saturating_sub(v.first)))
                .sum();
            let ranged = items.iter().filter(|(_, text)| text == RANGE).count() as u64;
            assert_eq!(
                ranged, covered,
                "{where_}: {ranged} <range> lines for {covered} covered verses"
            );

            // Every anchor outside the front matter takes exactly one line, and
            // the covered verses take theirs on top.
            let anchors = toc.verses.iter().filter(|v| v.chapter != 0).count() as u64;
            assert_eq!(
                items.len() as u64,
                anchors + covered,
                "{where_}: line count is not one per verse slot"
            );

            (1, items.len() as u64, covered)
        })
        .collect();

    let sum = |f: fn(&(u64, u64, u64)) -> u64| tallies.iter().map(f).sum::<u64>();
    eprintln!(
        "vref corpus: {} books, {} lines, {} of them <range>",
        sum(|t| t.0),
        sum(|t| t.1),
        sum(|t| t.2),
    );
    // RECONCILED with tests/toc_oracle.rs, which counts 3 bridges over the same
    // 226 books: each spans two verses, so each owes exactly one `<range>` line.
    assert_eq!(sum(|t| t.2), 3);
}
