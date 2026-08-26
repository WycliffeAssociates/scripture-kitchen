//! The mask oracle: every corpus book's `Mask` is its own honest offset map.
//!
//! The unit tests in src/mask.rs pin what each recipe KEEPS; this pins the
//! properties every consumer rests on, over real books and both recipes:
//!
//! - **The range set is well formed** — ascending, disjoint, non-empty,
//!   MAXIMAL (no two ranges adjacent), inside the source, and `starts` is the
//!   exact prefix sum.
//! - **`text() == concat(iter())`**, and its length is `len()`.
//! - **Every mask byte round-trips**: `text()[i] == source[to_source(i)]` and
//!   `to_source(from_source(b)) == b` for every kept byte.
//! - **`from_source` is `None` on exactly the dropped bytes** — checked at every
//!   byte of every book, not sampled.
//! - **A keep-everything filter is the identity**: one range covering the whole
//!   file. The walk cannot silently lose a token shape it never met.
//!
//! Runs over every `*.usfm` under `example-corpora/` (gitignored — the test
//! skips loudly when the corpora aren't on disk).

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use usfm_onion::mask::{Action, Filter, Mask, TextRule, mask};
use usfm_onion::tables::schema::MarkerKind;
use usfm_onion::{cst, lex};

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

/// The filter that must drop nothing at all — the mask's partition oracle.
fn keep_everything() -> Filter {
    Filter {
        kinds: [Action::Keep; MarkerKind::COUNT],
        markers: Vec::new(),
        unknowns: Action::Keep,
        text: TextRule::All,
        newlines: true,
        attr_lists: true,
        opt_breaks: true,
    }
}

/// Every invariant, at every byte. Returns the bytes it checked.
fn check(label: &str, source: &[u8], m: &Mask) -> u64 {
    assert_eq!(
        m.ranges.len(),
        m.starts.len(),
        "{label}: one start per range"
    );
    let mut running = 0u32;
    let mut previous_end = 0u32;
    for (row, range) in m.ranges.iter().enumerate() {
        assert!(range.start < range.end, "{label}: range {row} is empty");
        assert!(
            range.end as usize <= source.len(),
            "{label}: range {row} leaves the source"
        );
        assert!(
            row == 0 || previous_end < range.start,
            "{label}: range {row} touches its predecessor — ranges must be maximal"
        );
        assert_eq!(
            m.starts[row], running,
            "{label}: starts[{row}] is not the prefix sum"
        );
        running += range.end - range.start;
        previous_end = range.end;
    }
    assert_eq!(m.len(), running, "{label}: len is the sum of range lengths");

    let text = m.text(source);
    assert_eq!(
        text.len() as u32,
        m.len(),
        "{label}: text length is the mask length"
    );
    let mut concatenated = String::with_capacity(text.len());
    for part in m.iter(source) {
        concatenated.push_str(part);
    }
    assert_eq!(text, concatenated, "{label}: text() != concat(iter())");

    for offset in 0..m.len() {
        let byte = m.to_source(offset);
        assert_eq!(
            text.as_bytes()[offset as usize],
            source[byte as usize],
            "{label}: text()[{offset}] != source[to_source({offset})]"
        );
        assert_eq!(
            m.from_source(byte),
            Some(offset),
            "{label}: to_source(from_source({byte})) != {byte}"
        );
    }
    if let Some(last) = m.ranges.last() {
        assert_eq!(m.to_source(m.len()), last.end, "{label}: the mask's end");
        assert_eq!(
            m.to_source(u32::MAX),
            last.end,
            "{label}: past the end clamps"
        );
    }

    // `from_source` is None on EXACTLY the dropped bytes.
    let mut kept = vec![false; source.len()];
    for range in &m.ranges {
        kept[range.start as usize..range.end as usize].fill(true);
    }
    for (byte, kept) in kept.iter().enumerate() {
        assert_eq!(
            m.from_source(byte as u32).is_some(),
            *kept,
            "{label}: from_source disagrees about source byte {byte}"
        );
    }
    source.len() as u64
}

#[test]
fn every_corpus_book_masks_to_an_honest_offset_map() {
    let paths = corpus();
    if paths.is_empty() {
        eprintln!("mask oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    }

    /// A named recipe constructor — factored out so the array's type is
    /// readable at the one place it is written.
    type Recipe = (&'static str, fn() -> Filter);
    let recipes: [Recipe; 3] = [
        ("verse_text", Filter::verse_text),
        ("structure", Filter::structure),
        ("keep-everything", keep_everything),
    ];

    let tallies: Vec<(u64, u64, u64)> = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read(path).expect("readable book");
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let tokens = lex(std::str::from_utf8(&source).expect("UTF-8 corpus"));
            let cst = cst::build(&tokens);

            let mut bytes_checked = 0;
            let mut kept = 0;
            for (recipe, build) in recipes {
                let filter = build();
                let m = mask(&source, &tokens, &cst, &filter);
                if recipe == "keep-everything" {
                    assert_eq!(
                        m.ranges,
                        vec![0..source.len() as u32],
                        "{name}: a keep-everything filter must be the identity"
                    );
                }
                bytes_checked += check(&format!("{name}/{recipe}"), &source, &m);
                kept += u64::from(m.len());
            }
            (1, bytes_checked, kept)
        })
        .collect();

    let books: u64 = tallies.iter().map(|t| t.0).sum();
    let bytes: u64 = tallies.iter().map(|t| t.1).sum();
    let kept: u64 = tallies.iter().map(|t| t.2).sum();
    eprintln!(
        "mask oracle: {books} books × {} recipes, {bytes} source bytes checked, {kept} mask bytes",
        recipes.len()
    );
}

/// The one property a whole-corpus sweep cannot see: the two recipes disagree.
/// A mask that kept everything, or nothing, would pass every invariant above.
#[test]
fn the_recipes_actually_differ() {
    let paths = corpus();
    let Some(path) = paths.first() else {
        eprintln!("mask oracle SKIPPED: no *.usfm under example-corpora/");
        return;
    };
    let source = std::fs::read(path).expect("readable book");
    let tokens = lex(std::str::from_utf8(&source).expect("UTF-8 corpus"));
    let cst = cst::build(&tokens);

    let prose = mask(&source, &tokens, &cst, &Filter::verse_text());
    let skeleton = mask(&source, &tokens, &cst, &Filter::structure());
    assert!(!prose.is_empty() && !skeleton.is_empty());
    assert!(
        prose.len() > skeleton.len() * 4,
        "verse text should dwarf the skeleton: {} vs {}",
        prose.len(),
        skeleton.len()
    );
    // Neither is the whole file, and neither contains a marker backslash it
    // should have dropped.
    assert!(prose.len() < source.len() as u32);
    assert!(!prose.text(&source).contains("\\v "));
    assert!(skeleton.text(&source).contains("\\v "));
}
