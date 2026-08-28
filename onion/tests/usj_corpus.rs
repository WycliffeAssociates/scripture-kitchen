//! THE USJ ORACLE: every `<validated>pass</validated>` case in `testData/`
//! with a `origin.json`, compared as `serde_json::Value == Value`.
//!
//! ```text
//! testData/basic/minimal/origin.usfm  --lex--> --cst::build--> --usj()-->
//!     {"type":"USJ","version":"3.1","content":[…]}   ==   origin.json
//! ```
//!
//! testData is the COMMITTEE'S data, so the comparison is EXACT: no text
//! normalizer, no key-order munging beyond what `Value ==` already forgives. A
//! divergence is reported as the first JSON path that differs
//! (`content[3].content[0].marker`).
//!
//! `<validated>fail</validated>` cases stay out — that is where usfm-grammar's
//! `unmatched` damage shapes live, and our answer to damage is lint.

#![cfg(feature = "usj")]

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::Value;

use usfm_onion::{cst, lex, usj::usj};

/// The cases this pin does NOT claim, matched by path suffix. A case may only
/// leave the pin if its reason fits on ONE line, in one of three categories:
/// **ERRATUM** (the fixture states something its source bytes do not),
/// **PURPOSEFUL** (the fixtures contradict each other and we follow the
/// majority), **OPEN** (awaiting a ruling — currently none).
const EXCLUDED: &[(&str, &str)] = &[
    // ---- ERRATUM: the fixture disagrees with its own origin.usfm ----------
    (
        "biblica/CrossRefWithPipe",
        "ERRATUM: phantom trailing space — the source ends at `the ground,` with no newline (EOF is a block seam)",
    ),
    (
        "specExamples/table",
        "ERRATUM: a cell keeps the raw newline + line indent (`…Zurishaddai\\n        `) that every other fixture folds",
    ),
    (
        "specExamples/extended/contentCatogories1",
        "ERRATUM: literal newlines inside a note's text where the corpus otherwise collapses them to one space",
    ),
    (
        "special-cases/empty-attributes",
        "ERRATUM: invents a trailing space on `\\w ആകാശവും|lemma=…` content, and dumps a raw attribute list into another `\\w`'s content",
    ),
    (
        "biblica/PublishingVersesWithFormatting",
        "ERRATUM: `code` is `XXA` where the source says `\\id MAT` (and the description is kept verbatim)",
    ),
    (
        "advanced/complex",
        "ERRATUM: reads `\\k Book: \\k*` without its content space (minority of one) and omits every chapter/verse sid",
    ),
    (
        "advanced/footnote-structures",
        "ERRATUM: omits every chapter/verse sid the rest of the corpus carries",
    ),
    (
        "specExamples/footnote",
        "ERRATUM: invents a leading space on the `\\fv*\\ft As the scripture` seam that its own origin.xml does not carry (the `\\fv` graft itself now matches)",
    ),
    (
        "usfmjsTests/usfmBodyTestD",
        "ERRATUM: reads `\\fqa … \\fv 8\\fv* tail` as three note-level siblings where its OWN origin.xml nests both the `\\fv` and the tail inside `\\fqa`",
    ),
    (
        "special-cases/figure_with_quotes_in_desc",
        "ERRATUM: unescapes `alt=\"He said: \\\"…\\\"\"` — USFM defines no escapes, so `\\\"` lexes as a marker (src/attributes.rs's stated law)",
    ),
    // ---- PURPOSEFUL: the fixtures contradict, we follow the majority ------
    //
    // Unknown-marker pop-all recovery: `\s5` occurs 299 times in 21
    // validated-pass fixtures; 19 read our way, and the two that do not read it
    // two DIFFERENT ways.
    (
        "usfmjsTests/luk_quotes",
        "PURPOSEFUL: wants `\\s5` to swallow the following `\\v 17` text; 299 sibling `\\s5` occurrences want our reading, so pop-all recovery stands",
    ),
    (
        "usfmjsTests/usfm-body-testF",
        "PURPOSEFUL: wants `\\s5` inside `\\esb` to leave the sidebar open (and NOT swallow, unlike luk_quotes); pop-all recovery stands",
    ),
    (
        "specExamples/milestone",
        "PURPOSEFUL: wants the row-0 milestone `\\zms\\*` to leave `\\q1` open; also carries the literal-newline erratum (`answer ...\\n  `)",
    ),
    // Delimiter space at a seam: a space on either side of a marker seam
    // serializes to the same USFM, so USJ has two truthful spellings and the
    // fixtures use both. We fold the delimiter, which is the majority reading.
    (
        "usfmjsTests/isa_inline_quotes",
        "PURPOSEFUL: puts the `\\fqa men \\ft ,` seam space at the START of the `\\ft` content; we fold it as the delimiter",
    ),
    (
        "usfmjsTests/isa_verse_span",
        "PURPOSEFUL: same seam space, same folded delimiter — either side serializes identically",
    ),
    (
        "usfmjsTests/misc_footnotes",
        "PURPOSEFUL: same seam space, same folded delimiter — either side serializes identically",
    ),
    (
        "usfmjsTests/pro_quotes",
        "PURPOSEFUL: same seam space, same folded delimiter — either side serializes identically",
    ),
    (
        "usfmjsTests/tit_1_12_footnote",
        "PURPOSEFUL: same seam space, same folded delimiter — either side serializes identically",
    ),
    (
        "usfmjsTests/isa_footnote",
        "PURPOSEFUL: the seam space on the OTHER side — kept as a trailing space on `\\fqa`'s content, where we fold it",
    ),
    (
        "paratextTests/WordlistMarkerMissingFromGlossaryCitationForms",
        "PURPOSEFUL: invents a space at the `definition\\v 2` seam, where the source has none",
    ),
];

#[test]
fn usj_matches_every_validated_pass_fixture() {
    let root = Path::new("../testData/usfmtc");
    if !root.is_dir() {
        eprintln!("usj corpus SKIPPED: no testData/usfmtc/");
        return;
    }

    let mut cases = Vec::new();
    collect(root, &mut cases);
    cases.sort();
    // PINNED so the pin cannot shrink quietly: 207 validated-pass cases with an
    // `origin.json`, 20 of them excluded above. A new fixture or a mistyped
    // exclusion suffix moves this number and must be read.
    assert_eq!(
        cases.len(),
        187,
        "testData/usfmtc/ yielded {} claimable cases, expected 187 (207 validated-pass minus {} excluded)",
        cases.len(),
        EXCLUDED.len()
    );

    let results: Vec<Result<(), String>> = cases.par_iter().map(|case| check(case)).collect();

    let mut failures: Vec<&String> = Vec::new();
    for result in &results {
        if let Err(story) = result {
            failures.push(story);
        }
    }
    let passed = results.len() - failures.len();
    eprintln!(
        "usj corpus: {passed}/{} fixtures match ({} excluded)",
        results.len(),
        EXCLUDED.len()
    );
    if !failures.is_empty() {
        let mut report = String::new();
        for story in &failures {
            report.push_str(story);
            report.push('\n');
        }
        panic!(
            "{}/{} fixtures diverge:\n{report}",
            failures.len(),
            results.len()
        );
    }
}

/// Every directory holding `metadata.xml` + `origin.json` + `origin.usfm`
/// whose metadata CONTAINS `<validated>pass</validated>` — a plain string
/// search, deliberately no XML parser for one flag.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        }
    }
    let metadata = dir.join("metadata.xml");
    if !dir.join("origin.json").is_file() || !dir.join("origin.usfm").is_file() {
        return;
    }
    let Ok(metadata) = std::fs::read_to_string(&metadata) else {
        return;
    };
    if !metadata.contains("<validated>pass</validated>") {
        return;
    }
    if EXCLUDED
        .iter()
        .any(|(suffix, _)| dir.to_string_lossy().ends_with(suffix))
    {
        return;
    }
    out.push(dir.to_path_buf());
}

fn check(case: &Path) -> Result<(), String> {
    let source = std::fs::read(case.join("origin.usfm")).expect("readable origin.usfm");
    let expected_bytes = std::fs::read(case.join("origin.json")).expect("readable origin.json");

    let tokens = lex(std::str::from_utf8(&source).expect("utf-8 origin.usfm"));
    let cst = cst::build(&tokens);
    let ours = usj(&source, &tokens, &cst);

    let expected: Value = serde_json::from_slice(&expected_bytes)
        .map_err(|error| format!("{}: fixture is not JSON: {error}", case.display()))?;
    let actual: Value = serde_json::from_str(&ours).map_err(|error| {
        format!(
            "{}: OUR output is not JSON: {error}\n{ours}",
            case.display()
        )
    })?;

    if actual == expected {
        return Ok(());
    }
    let path = first_divergence("", &actual, &expected)
        .unwrap_or_else(|| "<equal by walk but not by ==>".to_string());
    Err(format!("{}: {path}", case.display()))
}

/// The first path at which two values differ, in a shape a reader can paste
/// into a fixture: `content[3].content[0].marker`.
fn first_divergence(at: &str, ours: &Value, theirs: &Value) -> Option<String> {
    match (ours, theirs) {
        (Value::Array(ours), Value::Array(theirs)) => {
            for (index, (a, b)) in ours.iter().zip(theirs).enumerate() {
                if let Some(found) = first_divergence(&format!("{at}[{index}]"), a, b) {
                    return Some(found);
                }
            }
            if ours.len() != theirs.len() {
                return Some(format!(
                    "{at}: {} items, fixture has {}\n  ours:    {}\n  fixture: {}",
                    ours.len(),
                    theirs.len(),
                    brief(&Value::Array(ours.clone())),
                    brief(&Value::Array(theirs.clone()))
                ));
            }
            None
        }
        (Value::Object(ours), Value::Object(theirs)) => {
            // Keys in FIXTURE order first, so a missing key reads as missing
            // rather than as the next surviving key's mismatch.
            for (key, b) in theirs {
                match ours.get(key) {
                    Some(a) => {
                        if let Some(found) = first_divergence(&format!("{at}.{key}"), a, b) {
                            return Some(found);
                        }
                    }
                    None => return Some(format!("{at}.{key}: MISSING (fixture has {})", brief(b))),
                }
            }
            for (key, a) in ours {
                if !theirs.contains_key(key) {
                    return Some(format!("{at}.{key}: EXTRA (ours has {})", brief(a)));
                }
            }
            None
        }
        _ if ours == theirs => None,
        _ => Some(format!(
            "{at}\n  ours:    {}\n  fixture: {}",
            brief(ours),
            brief(theirs)
        )),
    }
}

/// A value, truncated — a whole subtree in a failure line hides the finding.
fn brief(value: &Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(160) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}
