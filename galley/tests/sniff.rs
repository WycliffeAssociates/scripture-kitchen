//! Which scheme each example corpus sniffs as, pinned.
//!
//! Instrument: VOLUME, `testData/exampleCorpora` (en_ulb, examples.bsb,
//! bdf_reg), each sniffed as one whole text. A change to the rules, the
//! voting or the inventory moves these numbers, so it is a comparison and not
//! a claim.

use usfm_galley::versification::sniff;

fn corpus(name: &str) -> Vec<String> {
    let dir = format!(
        "{}/../testData/exampleCorpora/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "usfm"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no *.usfm in {dir}");
    paths
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect()
}

/// `name agree-disagree` for the top three, the confidence, the vote count,
/// and the dissent against the winner.
fn summary(name: &str) -> String {
    let books = corpus(name);
    let sniff = sniff(books.iter().map(String::as_str));
    let top: Vec<String> = sniff
        .ranked
        .iter()
        .take(3)
        .map(|s| format!("{} {}-{}", s.scheme.name(), s.agree, s.disagree))
        .collect();
    let dissent: Vec<&str> = sniff.dissent.iter().map(|d| d.rule).collect();
    format!(
        "{:?} on {} votes: {} | dissent {dissent:?} | unmatched {:?}",
        sniff.confidence,
        sniff.voted,
        top.join(", "),
        sniff.unmatched
    )
}

/// en_ulb is `ulb`, and only one rule tells it from `eng`: Acts 19:41 and
/// 2 Corinthians 13:14, which en_ulb folds into the verse before.
#[test]
fn en_ulb_is_ulb() {
    assert_eq!(
        summary("en_ulb"),
        "Decided on 250 votes: ulb 250-0, eng 249-1, org 146-104 | dissent [] | unmatched []"
    );
}

/// The BSB numbers like `eng` but for the two famous English splits it
/// doesn't make: 3 John 14-15 and Revelation 12:17/13:1. Those come back as
/// dissent, not as a reason to call it something else.
#[test]
fn the_bsb_is_eng_with_its_two_known_dissents() {
    assert_eq!(
        summary("examples.bsb"),
        "Decided on 246 votes: eng 244-2, ulb 243-3, org 140-106 \
         | dissent [\"3Jn.14:15\", \"Rev.12:17-13.1a\"] | unmatched [\"2Co.13:12-13\"]"
    );
}

/// A New Testament on its own has few passages where traditions differ:
/// seven rules vote, so the answer only leans.
#[test]
fn a_new_testament_alone_only_leans() {
    assert_eq!(
        summary("bdf_reg"),
        "Leaning on 7 votes: eng 7-0, org 6-1, lxx 6-1 | dissent [] | unmatched [\"2Co.13:12-13\"]"
    );
}
