//! The toc oracle: every corpus book's `Toc` indexes its own stream.
//!
//! The unit tests in src/toc.rs pin the shapes; this pins the invariants the
//! coordinate queries rest on, over real books:
//!
//! - **Chapter rows tile `0..source.len()`** — no gap, no overlap, first row
//!   at byte 0, last row at the end, each row after 0 opening on a `\c`.
//! - **Verse anchors are sorted and housed** — ascending by `at`, each inside
//!   its own chapter's span, each carrying that chapter's number.
//! - **`locate` is total** and round-trips every anchor.
//! - **Counts reconcile with the token stream**, so a row can neither be
//!   invented nor dropped.
//!
//! Runs over every `*.usfm` under `example-corpora/` (gitignored — the test
//! skips loudly when the corpora aren't on disk).

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion::tables::generated;
use usfm_onion::tables::schema::MarkerKind;
use usfm_onion::{Token, TokenKind, lex, toc};

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

/// How many non-nested rows of one kind the stream carries — the count the Toc
/// must reproduce exactly, read off the same fact the pass reads (the ROW's
/// kind, not the payload).
fn markers_of(tokens: &[Token], kind: MarkerKind) -> usize {
    tokens
        .iter()
        .filter(|token| {
            token.kind() == TokenKind::Marker { nested: false }
                && generated::kind(token.marker_idx) == kind
        })
        .count()
}

/// What one book contributes to the corpus totals.
#[derive(Default)]
struct Tally {
    chapters: u64,
    verses: u64,
    /// Chapter rows past row 0 whose number is 0 — an absent or malformed `\c`
    /// designator.
    nameless_chapters: u64,
    /// Verse anchors with no number — an absent or malformed `\v` designator.
    nameless_verses: u64,
    bridges: u64,
    without_a_book_code: u64,
}

#[test]
fn every_corpus_book_indexes_its_own_stream() {
    let paths = corpus();
    if paths.is_empty() {
        eprintln!("toc oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    }

    let tallies: Vec<Tally> = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            let bytes = source.as_bytes();
            let tokens = lex(&source);
            let toc = toc(bytes, &tokens);
            let where_ = path.display();

            // ---- chapters tile ------------------------------------------
            assert_eq!(
                toc.chapters[0].start, 0,
                "{where_}: the first chapter row does not start at byte 0"
            );
            assert_eq!(
                toc.chapters[0].number, 0,
                "{where_}: row 0 is not chapter 0"
            );
            assert_eq!(
                toc.chapters.last().unwrap().end as usize,
                source.len(),
                "{where_}: the last chapter row stops short of the source"
            );
            for (i, pair) in toc.chapters.windows(2).enumerate() {
                assert_eq!(
                    pair[0].end, pair[1].start,
                    "{where_}: chapter rows {i} and {} neither meet nor overlap",
                    i + 1
                );
                // Every row past 0 opens on a real chapter marker.
                let opener = tokens
                    .binary_search_by_key(&pair[1].start, |t| t.start)
                    .map(|row| tokens[row])
                    .unwrap_or_else(|_| panic!("{where_}: row {} is not at a token", i + 1));
                assert_eq!(
                    source[opener.start as usize..opener.end() as usize]
                        .trim_end_matches([' ', '\t']),
                    "\\c",
                    "{where_}: chapter row {} opens on something that is not `\\c`",
                    i + 1
                );
            }

            // ---- verses are sorted and housed ---------------------------
            let mut previous = 0u32;
            for (i, anchor) in toc.verses.iter().enumerate() {
                assert!(
                    anchor.at >= previous,
                    "{where_}: verse anchor {i} at {} is behind its predecessor {previous}",
                    anchor.at
                );
                previous = anchor.at;

                let row = toc
                    .chapters
                    .iter()
                    .find(|row| row.span().contains(&anchor.at))
                    .unwrap_or_else(|| panic!("{where_}: verse anchor {i} is in no chapter"));
                assert_eq!(
                    anchor.chapter, row.number,
                    "{where_}: verse anchor {i} claims chapter {} inside chapter {}",
                    anchor.chapter, row.number
                );
                assert!(
                    anchor.first <= anchor.last,
                    "{where_}: verse anchor {i} runs backwards"
                );

                // locate() round-trips the anchor it was built from.
                let sid = toc.locate(anchor.at);
                assert_eq!(
                    (sid.chapter, sid.first, sid.last),
                    (anchor.chapter, anchor.first, anchor.last),
                    "{where_}: locate disagrees with verse anchor {i}"
                );
                assert_eq!(
                    anchor.at, tokens[anchor.token as usize].start,
                    "{where_}: verse anchor {i} and its token disagree about where it starts"
                );
            }

            // ---- locate is total ----------------------------------------
            // Every chapter boundary, plus a stride over the whole file: the
            // answer must always be the row that actually holds the byte.
            let boundaries = toc.chapters.iter().flat_map(|row| [row.start, row.end]);
            let stride = (0..source.len() as u32).step_by(997);
            for at in boundaries.chain(stride).chain([u32::MAX]) {
                let sid = toc.locate(at);
                let row = toc
                    .chapters
                    .iter()
                    .rev()
                    .find(|row| row.start <= at)
                    .unwrap();
                assert_eq!(
                    sid.chapter, row.number,
                    "{where_}: locate({at}) named chapter {} where the table says {}",
                    sid.chapter, row.number
                );
                assert_eq!(sid.book, toc.book, "{where_}: locate({at}) lost the book");
            }

            // ---- counts reconcile with the stream -----------------------
            assert_eq!(
                toc.chapters.len() - 1,
                markers_of(&tokens, MarkerKind::Chapter),
                "{where_}: chapter rows do not match the `\\c` markers"
            );
            assert_eq!(
                toc.verses.len(),
                markers_of(&tokens, MarkerKind::Verse),
                "{where_}: verse anchors do not match the `\\v` markers"
            );

            // A book code is found EXACTLY when there is an `\id` line with
            // something after it — cross-checked against the raw text rather
            // than pinned as a corpus fact, since BSB Ecclesiastes ships with
            // no `\id` at all.
            let id_line_with_a_code = source
                .lines()
                .any(|line| line.strip_prefix("\\id ").is_some_and(|rest| !rest.is_empty()));
            assert_eq!(
                toc.book_token.is_some(),
                id_line_with_a_code,
                "{where_}: book token is {:?} but an `\\id` line with a code is {id_line_with_a_code}",
                toc.book_token,
            );
            assert_eq!(
                toc.book == [0; 3],
                toc.book_token.is_none(),
                "{where_}: the book code and its token disagree about existing"
            );

            Tally {
                chapters: (toc.chapters.len() - 1) as u64,
                verses: toc.verses.len() as u64,
                nameless_chapters: toc.chapters[1..]
                    .iter()
                    .filter(|row| row.number == 0)
                    .count() as u64,
                nameless_verses: toc.verses.iter().filter(|v| v.first == 0).count() as u64,
                bridges: toc.verses.iter().filter(|v| v.first != v.last).count() as u64,
                without_a_book_code: u64::from(toc.book_token.is_none()),
            }
        })
        .collect();

    let sum = |f: fn(&Tally) -> u64| tallies.iter().map(f).sum::<u64>();
    let chapters = sum(|t| t.chapters);
    let verses = sum(|t| t.verses);
    eprintln!(
        "toc oracle: {} books, {chapters} chapters, {verses} verses, {} bridges, {} without a book code",
        paths.len(),
        sum(|t| t.bridges),
        sum(|t| t.without_a_book_code),
    );

    assert_eq!(
        paths.len(),
        226,
        "corpus size changed — re-read the numbers"
    );
    // RECONCILED with tests/lint_corpus.rs over the same 226 books: lint pins
    // `designator-malformed` at 2 (bdf_reg ACT `\v +`, en_ulb ZEC `\v 7"`) and
    // `chapter-without-designator` at 0. Those are exactly the rows the Toc
    // degrades to number 0 — two verses, no chapters. If either side moves
    // alone, one of them is wrong.
    assert_eq!(sum(|t| t.nameless_verses), 2);
    assert_eq!(sum(|t| t.nameless_chapters), 0);
    // …and lint pins the one book with no `\id` (BSB Ecclesiastes).
    assert_eq!(sum(|t| t.without_a_book_code), 1);
}
