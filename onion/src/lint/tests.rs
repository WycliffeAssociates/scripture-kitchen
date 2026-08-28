//! The cross-cutting lint tests, and the helpers every machine's test module
//! shares. Each machine's own tests live beside it.

use super::*;
use crate::cst::build;
use crate::lex;

/// Every test states its snippet and the exact findings it expects — codes AND
/// anchors, so a rule firing on the right marker for the wrong reason still
/// fails. A snippet that does not open with `\id` gets one, because a file with
/// markers and no `\id` line is itself a finding; anchors are resolved by name
/// or kind, never by index, so that prefix never leaks into an assertion.
pub(crate) fn findings(usfm: &str) -> (Vec<Token>, Vec<Observation>) {
    let usfm = if usfm.starts_with("\\id") {
        usfm.to_string()
    } else {
        format!("\\id GEN\n{usfm}")
    };
    let tokens = lex(&usfm);
    let cst = build(&tokens);
    let report = lint(usfm.as_bytes(), &tokens, &cst);
    // Half of the row/fix agreement, checked on EVERY snippet rather than in one
    // test: a fix may only appear where the row declared one, and it must carry
    // that row's own label.
    for (index, obs) in report.observations.iter().enumerate() {
        if let Some(fix) = report.fix(index) {
            assert_eq!(
                Some(fix.label),
                obs.code.row().fix_label,
                "{} emitted a fix its row does not declare",
                obs.code.row().name
            );
        }
    }
    (tokens, report.observations)
}

/// The index of the nth token whose marker ROW has this name (`qt` has
/// two rows, so the row name — not a spelling lookup — is the key).
pub(crate) fn token_named(tokens: &[Token], name: &str, nth: usize) -> u32 {
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
            generated::name(token.marker_idx) == name
                && matches!(
                    token.kind(),
                    TokenKind::Marker { .. } | TokenKind::Milestone { .. }
                )
        })
        .map(|(idx, _)| idx as u32)
        .nth(nth)
        .unwrap_or_else(|| panic!("no \\{name} token #{nth}"))
}

pub(crate) fn codes(observations: &[Observation]) -> Vec<Code> {
    observations.iter().map(|obs| obs.code).collect()
}

#[test]
fn observation_is_four_words_and_carries_no_strings() {
    assert_eq!(core::mem::size_of::<Observation>(), 16);
}

#[test]
fn a_clean_book_yields_nothing() {
    let (_, obs) = findings("\\id GEN\n\\c 1\n\\p \\v 1 In the beginning\\f + \\ft note\\f*\n");
    assert_eq!(obs, vec![]);
}

#[test]
fn the_book_code_is_reported_without_an_observation() {
    let usfm = "\\id GEN\n\\c 1\n\\p \\v 1 text\n";
    let tokens = lex(usfm);
    let cst = build(&tokens);
    let report = lint(usfm.as_bytes(), &tokens, &cst);
    let book = report.book.expect("\\id GEN has a book code");
    assert_eq!(tokens[book as usize].kind(), TokenKind::BookCode);

    // No `\id`: the `None` state is KEPT (it is the fact the consumer
    // reads) and `missing-id` is raised alongside it, anchored at token 0.
    let usfm = "\\c 1\n\\p \\v 1 text\n";
    let tokens = lex(usfm);
    let cst = build(&tokens);
    let report = lint(usfm.as_bytes(), &tokens, &cst);
    assert_eq!(report.book, None);
    assert_eq!(
        report.observations,
        vec![Observation::one(Code::MissingId, 0)]
    );
}

#[test]
fn missing_id_is_anchored_at_the_top_of_the_file() {
    let (_, obs) = findings("\\id GEN\n\\c 1\n\\p \\v 1 a");
    assert_eq!(obs, vec![]);

    let usfm = "\\c 1\n\\p \\v 1 a";
    let tokens = lex(usfm);
    let cst = build(&tokens);
    let report = lint(usfm.as_bytes(), &tokens, &cst);
    assert_eq!(
        report.observations,
        vec![Observation::one(Code::MissingId, 0)]
    );
    // The state is kept as well as reported.
    assert_eq!(report.book, None);

    // A file with no markers at all is not a book and owes no `\id`.
    let usfm = "just prose\n";
    let tokens = lex(usfm);
    let cst = build(&tokens);
    assert_eq!(lint(usfm.as_bytes(), &tokens, &cst).observations, vec![]);
}

#[test]
fn the_declared_version_is_reported() {
    let version = |usfm: &str| {
        let tokens = lex(usfm);
        let cst = build(&tokens);
        lint(usfm.as_bytes(), &tokens, &cst).declared_version
    };
    assert_eq!(
        version("\\id GEN\n\\usfm 3.0\n\\p a"),
        Some(UsfmVersion::V3_0)
    );
    assert_eq!(
        version("\\id GEN\n\\usfm 3.2\n\\p a"),
        Some(UsfmVersion::V3_2)
    );
    assert_eq!(
        version("\\id GEN\n\\usfm 4\n\\p a"),
        Some(UsfmVersion::V4_0)
    );
    assert_eq!(
        version("\\id GEN\n\\usfm 3.2.1\n\\p a"),
        Some(UsfmVersion::V3_2)
    );
    // No declaration, and a declaration that is not a version, are the
    // same state: unknown, never assumed.
    assert_eq!(version("\\id GEN\n\\p a"), None);
    assert_eq!(version("\\id GEN\n\\usfm three\n\\p a"), None);
    // The header scan stops at the first `\c`, so a `\usfm` line written
    // below one is not a declaration this report will claim.
    assert_eq!(version("\\id GEN\n\\c 1\n\\usfm 3.2\n\\p a"), None);
}

#[test]
fn observations_come_back_in_document_order() {
    let (_, obs) = findings("\\p \\add a\\w* \\zfoo \\p \\nd b");
    let anchors: Vec<u32> = obs.iter().map(|o| o.anchor).collect();
    let mut sorted = anchors.clone();
    sorted.sort_unstable();
    assert_eq!(anchors, sorted);
    assert!(anchors.len() >= 3);
}
