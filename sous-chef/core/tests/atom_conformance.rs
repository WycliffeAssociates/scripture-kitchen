//! Charter invariant 6, pinned twice: against the UCD's own break test and
//! against `unicode-segmentation` over the committed 8-corpus test tier.
//!
//! The claim is exactly one sentence — *no atom boundary falls inside a UAX
//! #29 grapheme cluster* — so both gates check that and nothing wider. The
//! atom rule may merge adjacent clusters; that is documented over-widening,
//! not a failure.

use std::path::PathBuf;

use sous_core::TextRange;
use sous_core::unicode::atoms::{is_atom_boundary, widen_to_atoms};
use unicode_segmentation::UnicodeSegmentation;

const CORPORA: [&str; 8] = [
    "WA-en-ulb.txt",
    "amh.txt",
    "francl.txt",
    "grcsr.txt",
    "hin2017.txt",
    "nya.txt",
    "spaRV1909.txt",
    "swhulb.txt",
];

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} must be present: {error}", path.display()))
}

/// Every `÷`/`×` line of `GraphemeBreakTest.txt` as (text, break offsets).
fn break_test_cases() -> Vec<(String, Vec<usize>)> {
    let text = read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/ucd/GraphemeBreakTest.txt"));
    text.lines()
        .filter_map(|line| {
            let body = line.split('#').next().unwrap_or("").trim();
            if body.is_empty() {
                return None;
            }
            let mut built = String::new();
            let mut breaks = Vec::new();
            for token in body.split_whitespace() {
                match token {
                    "\u{f7}" => breaks.push(built.len()),
                    "\u{d7}" => {}
                    hex => built.push(
                        char::from_u32(u32::from_str_radix(hex, 16).expect("hex scalar"))
                            .expect("a test scalar"),
                    ),
                }
            }
            Some((built, breaks))
        })
        .collect()
}

#[test]
fn no_atom_boundary_falls_inside_a_graphemebreaktest_cluster() {
    let cases = break_test_cases();
    assert!(cases.len() > 500, "the pristine test file has ~766 cases");
    let mut invented = Vec::new();
    for (text, breaks) in &cases {
        for at in 0..=text.len() {
            if !text.is_char_boundary(at) {
                continue;
            }
            if is_atom_boundary(text, at) && !breaks.contains(&at) {
                invented.push(format!("{:?} at byte {at}", text.escape_debug().to_string()));
            }
        }
    }
    assert!(
        invented.is_empty(),
        "{} invented boundaries; first: {:?}",
        invented.len(),
        &invented[..invented.len().min(5)]
    );
}

#[test]
fn widening_any_sub_range_of_a_cluster_returns_the_whole_cluster() {
    for (text, breaks) in break_test_cases() {
        for cluster in breaks.windows(2) {
            let (start, end) = (cluster[0], cluster[1]);
            for from in start..end {
                if !text.is_char_boundary(from) {
                    continue;
                }
                for to in from + 1..=end {
                    if !text.is_char_boundary(to) {
                        continue;
                    }
                    let widened =
                        widen_to_atoms(&text, TextRange::new(from as u32, to as u32).unwrap());
                    assert!(
                        widened.from() as usize <= start && widened.to() as usize >= end,
                        "{:?}: {from}..{to} widened to {}..{} which does not contain {start}..{end}",
                        text.escape_debug().to_string(),
                        widened.from(),
                        widened.to()
                    );
                }
            }
        }
    }
}

#[test]
fn the_test_tier_has_no_cluster_the_atom_rule_would_split() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpora");
    let mut report = Vec::new();
    for name in CORPORA {
        let text = read(root.join(name));
        let split = split_clusters(&text);
        report.push(format!("{name}: {split}"));
        assert_eq!(split, 0, "{name} has {split} clusters split by the atom rule");
    }
    println!("clusters split by the atom rule — {}", report.join(", "));
}

/// Clusters holding at least one internal atom boundary.
pub fn split_clusters(text: &str) -> u64 {
    let mut at = 0usize;
    let mut split = 0u64;
    for cluster in text.graphemes(true) {
        for inner in 1..cluster.len() {
            if cluster.is_char_boundary(inner) && is_atom_boundary(text, at + inner) {
                split += 1;
                break;
            }
        }
        at += cluster.len();
    }
    split
}
