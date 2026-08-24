//! What `format` does to 226 real books — the invariants, at corpus scale.
//!
//! Seven claims, all of them checked per book on the default bundle:
//! determinism, one valid transaction, a partition that still holds,
//! convergence in ONE pass, conservation of every meaning-bearing diagnostic,
//! sanctity of verse text, and — with `remove_markers: ["s5"]` — that a
//! wholesale removal takes exactly what it was asked for and nothing else.
//!
//! The corpus is gitignored, so this test skips loudly when it is not mounted.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion_2::cst::build;
use usfm_onion_2::lint::{Code, LINT_ROWS, check_edits, lint};
use usfm_onion_2::{Filter, FormatOptions, format, format_edits, lex, mask};

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

fn corpus() -> Option<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("format corpus SKIPPED: no *.usfm under example-corpora/");
        return None;
    }
    paths.sort();
    Some(paths)
}

/// Per-code finding counts.
fn counts(source: &[u8]) -> [u64; LINT_ROWS.len()] {
    let text = core::str::from_utf8(source).expect("corpus is UTF-8");
    let tokens = lex(text);
    let cst = build(&tokens);
    let mut counts = [0u64; LINT_ROWS.len()];
    for obs in &lint(source, &tokens, &cst).observations {
        counts[obs.code as usize] += 1;
    }
    counts
}

/// The verse text a `verse_text` mask reads, with every whitespace run squeezed
/// to one space. What survives this comparison is CONTENT — the only thing the
/// default bundle is allowed to leave alone.
fn verse_words(source: &[u8]) -> String {
    let text = core::str::from_utf8(source).expect("corpus is UTF-8");
    let tokens = lex(text);
    let cst = build(&tokens);
    let mask = mask(source, &tokens, &cst, &Filter::verse_text());
    mask.text(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn the_default_bundle_holds_every_invariant_over_the_corpus() {
    let Some(paths) = corpus() else { return };
    assert_eq!(
        paths.len(),
        226,
        "corpus size changed — re-read the numbers"
    );

    let edited: Vec<u64> = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read(path).unwrap();
            let opts = FormatOptions::default();
            let edits = format_edits(&source, &opts);

            // 4. ONE TRANSACTION: sorted, disjoint, in bounds, char-aligned.
            check_edits(&source, &edits)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));

            // 6. DETERMINISM: same bytes, same options, same answer.
            assert_eq!(
                format_edits(&source, &opts),
                edits,
                "{}: a second run disagreed",
                path.display()
            );

            let out = format(&source, &opts);
            let text = core::str::from_utf8(&out)
                .unwrap_or_else(|_| panic!("{}: format broke UTF-8", path.display()));

            // THE PARTITION, on text nobody has lexed before: the formatted
            // bytes re-lex into spans that tile them exactly.
            let mut cursor = 0usize;
            for token in &lex(text) {
                assert_eq!(
                    token.start as usize,
                    cursor,
                    "{}: the formatted text does not partition",
                    path.display()
                );
                cursor += token.len as usize;
            }
            assert_eq!(
                cursor,
                out.len(),
                "{}: the formatted text does not partition",
                path.display()
            );

            // 1. IDEMPOTENCE, in its strong form: re-walking the OUTPUT finds
            // nothing left for any Form row, formatter-bit row or opted repair.
            let left = format_edits(&out, &opts);
            assert!(
                left.is_empty(),
                "{}: {} edits still wanted after one pass ({:?})",
                path.display(),
                left.len(),
                &left[..left.len().min(3)]
            );

            // 2. VERSE TEXT IS CONTENT. Nothing but whitespace forms moved.
            assert_eq!(
                verse_words(&out),
                verse_words(&source),
                "{}: verse text changed",
                path.display()
            );

            // 5. MEANING-BEARING DIAGNOSTICS ARE CONSERVED: for every code the
            // formatter does NOT own, the count is the same before and after.
            let (before, after) = (counts(&source), counts(&out));
            for row in LINT_ROWS.iter().filter(|row| !row.formats()) {
                assert_eq!(
                    before[row.code as usize],
                    after[row.code as usize],
                    "{}: formatting changed the {} count",
                    path.display(),
                    row.name
                );
            }
            edits.len() as u64
        })
        .collect();

    // The invariants are only evidence if the formatter actually did something,
    // so the transaction size is pinned like every other corpus number: 108,247
    // edits over 113 MB — en_ulb 48,251, en_ult 20,971, examples.bsb 31,085,
    // bdf_reg 7,940. The corpora are already tidy, so what this mostly is: verse
    // breaks joined into their paragraphs (the default axis), the blank line
    // above every `\s5`, and the `\p` a paragraph-less run owes.
    let total: u64 = edited.iter().sum();
    assert_eq!(total, 108_247);
    assert!(
        edited.iter().all(|count| *count > 0),
        "a book formatted to nothing"
    );
}

/// The `\s5` case the parameterized row exists for: 13,636 unfoldingWord chunk
/// markers across en_ulb, gone in one transaction, with the verse text untouched
/// and no `\s5` left anywhere.
#[test]
fn removing_s5_takes_the_chunk_markers_and_nothing_else() {
    let Some(paths) = corpus() else { return };
    let removed: u64 = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read(path).unwrap();
            let opts = FormatOptions {
                remove_markers: &["s5"],
                ..FormatOptions::default()
            };
            let out = format(&source, &opts);
            assert!(
                !out.windows(4).any(|window| window == b"\\s5\n") && !out.ends_with(b"\\s5"),
                "{}: an \\s5 survived",
                path.display()
            );
            assert_eq!(
                verse_words(&out),
                verse_words(&source),
                "{}: verse text changed",
                path.display()
            );
            assert!(
                format_edits(&out, &opts).is_empty(),
                "{}: did not converge in one pass",
                path.display()
            );
            // The `\s5` was row 0, so its removal is visible as `unknown-marker`
            // going to zero — the loudest proof that the extents really went.
            let after = counts(&out);
            assert_eq!(after[Code::UnknownMarker as usize], 0, "{}", path.display());
            counts(&source)[Code::UnknownMarker as usize]
        })
        .sum();
    assert_eq!(removed, 13_636);
}
