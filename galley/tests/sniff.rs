//! Which scheme each example corpus sniffs as, and what it is missing or has
//! too much of against that scheme, pinned.
//!
//! Instrument: VOLUME, `testData/exampleCorpora` (en_ulb, examples.bsb,
//! bdf_reg), each sniffed as one whole text. A change to the rules, the
//! voting, the inventory or the counts moves these numbers, so it is a
//! comparison and not a claim.

use usfm_galley::versification::{lint_against, sniff};
use usfm_onion::{Sid, lex, toc};

/// Every `chapter-ends-early` / `verse-beyond-scheme` in a corpus against a
/// scheme, as `code@BOOK c:v (scheme's last verse)`.
fn against(name: &str, scheme: &str) -> Vec<String> {
    let scheme = canon::scheme(scheme).unwrap();
    let mut out = Vec::new();
    for source in corpus(name) {
        let tokens = lex(&source);
        let toc = toc(source.as_bytes(), &tokens);
        for o in lint_against(&source, scheme) {
            let at: Sid = toc.locate(tokens[o.anchor as usize].start);
            let row = &usfm_onion::lint::LINT_ROWS[o.code as usize];
            out.push(format!("{}@{at} ({})", row.name, o.aux));
        }
    }
    out
}

/// Against its own scheme en_ulb is complete; against `eng` it is short
/// exactly where the two schemes differ.
#[test]
fn en_ulb_is_complete_against_ulb_and_short_of_eng_twice() {
    assert!(against("en_ulb", "ulb").is_empty());
    assert_eq!(
        against("en_ulb", "eng"),
        [
            "chapter-ends-early@ACT 19:40 (41)",
            "chapter-ends-early@2CO 13:13 (14)"
        ]
    );
}

/// The BSB's two known differences from `eng`, as findings.
#[test]
fn the_bsb_ends_two_chapters_early_against_eng() {
    assert_eq!(
        against("examples.bsb", "eng"),
        [
            "chapter-ends-early@3JN 1:14 (15)",
            "chapter-ends-early@REV 12:17 (18)"
        ]
    );
}

/// A draft missing chapters' last verses: no gap lint can see these, because
/// nothing follows the missing verse.
#[test]
fn bdf_reg_is_missing_five_last_verses() {
    assert_eq!(
        against("bdf_reg", "eng"),
        [
            "chapter-ends-early@JHN 20:30 (31)",
            "chapter-ends-early@ACT 6:14 (15)",
            "chapter-ends-early@GAL 4:30 (31)",
            "chapter-ends-early@REV 16:20 (21)",
            "chapter-ends-early@REV 20:14 (15)"
        ]
    );
}

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
