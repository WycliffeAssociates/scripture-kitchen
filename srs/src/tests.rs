use super::*;

fn spelled(text: &str) -> String {
    format(&parse(text).unwrap_or_else(|e| panic!("{text}: {e}")))
}

fn refused(text: &str) -> (ErrorKind, usize) {
    let e = parse(text).expect_err(text);
    (e.kind, e.at)
}

fn word(n: u32) -> Step {
    Step::Word {
        word: Index::At(n),
        char: None,
    }
}

fn element(marker: &str) -> Step {
    Step::Element {
        marker: marker.to_string(),
        index: None,
    }
}

/// Every example the standard gives, and usfmtc's grammar cases, written back
/// as themselves.
#[test]
fn the_standards_examples_read_and_write_back() {
    for text in [
        // Introduction.
        "MAT 2:1!3",
        "MAT 2:1!3-4",
        "MAT 2:1!3-2!4",
        "MAT 2:1!3-2:4",
        "MAT 2:1-4",
        "MAT 2-4",
        "en-t-wsg+sil.MAT 1:1!f!5+6",
        // Basic, words and characters.
        "LUK 11:2b",
        "PSA 7:0",
        "GEN 7:8!2-6",
        "GEN 7:8!2-12!3",
        "wsg-gong.JHN 3:16",
        "en-t-wsg.MAT 1:1!5+6",
        "GEN 2:3!4+0",
        "GEN 2:3!4+2+",
        "MAT 1:1!5+2-6",
        "ROM 9:end",
        "ROM 9:5-end",
        "en+niv.MRK 1:14!s",
        "en+niv+text.GEN 1:1",
        // Non-scriptural text.
        "MRK 1:14!s",
        "MRK 1:14!s[2]",
        "MRK 1:14!s[-1]",
        "MRK 1:14!s!f",
        "LUK 8:10!q1",
        "LUK 8:10!p",
        "MAT 6:8!f[3]",
        "GEN 1:2!1!f!3",
        "GEN 1:2!f!3",
        "MRK 0!ip!53",
        "MRK 0!ip!w!1",
        "GEN 1:1!8@strong",
        "GEN 1:1!w[4]@strong",
        "GEN 1:2!f@category",
        "GEN 2:1@number",
        "NEH t:Israel/6/1",
        "MRK k:baptism",
        // Appendix 3.
        "PSA 78!d!2",
        "PSA 78:0!d!2",
        "PSA 78!d!f!2",
        "PSA 78:1!s1!5",
        "PSA 78:1!1",
        "PSA 78:1!q1!3",
        // usfmtc.
        "JON 3:6!3",
        "JON 2:8!2-4",
        "JON 1:4!s1!2",
        "JON 1:13!f!5",
        "JON 1:13!5!f!5",
        "1CO 6:5a-10",
        "LUK 3:35-end",
        "JON 1:1-4:11",
    ] {
        assert_eq!(spelled(text), text, "{text}");
    }
}

#[test]
fn spellings_the_standard_allows_are_written_one_way() {
    for (text, written) in [
        ("en-t-wsg:JHN 3:16", "en-t-wsg.JHN 3:16"),
        ("ROM 9.1", "ROM 9:1"),
        ("GEN 1:1-GEN 1:31", "GEN 1:1-31"),
        ("GEN 1:22\u{200F}-23", "GEN 1:22-23"),
        ("GEN 1\u{200F}:23 \u{200F}- 24", "GEN 1:23-24"),
        ("JUD 1,2,4", "JUD 1:1; JUD 1:2; JUD 1:4"),
        ("3JN 3", "3JN 1:3"),
        ("MAT 7:1,2; 8:6b-9:4", "MAT 7:1; MAT 7:2; MAT 8:6b-9:4"),
        (
            "GEN 1:1-3; 3:2-11; LUK 4:5",
            "GEN 1:1-3; GEN 3:2-11; LUK 4:5",
        ),
        ("JON 3:10 - MIC 1:2", "JON 3:10-MIC 1:2"),
    ] {
        assert_eq!(spelled(text), written, "{text}");
        assert_eq!(spelled(written), written, "{written} reads back as itself");
    }
}

#[test]
fn refinements_chain_in_order() {
    let one = |text| parse(text).unwrap().remove(0);
    assert_eq!(
        one("JON 1:13!5!f!5").first.steps,
        [word(5), element("f"), word(5)]
    );
    assert_eq!(one("JON 1:4!s1!2").first.steps, [element("s1"), word(2)]);
    assert_eq!(
        one("GEN 1:1!8@strong").first.steps,
        [word(8), Step::Attribute("strong".into())]
    );
    let mrk = one("MRK 1:14!s[-1]").first;
    assert_eq!(
        mrk.steps,
        [Step::Element {
            marker: "s".into(),
            index: Some(-1)
        }]
    );
    let chars = one("MAT 1:1!5+2-6");
    assert_eq!(
        chars.last.unwrap().steps,
        [Step::Word {
            word: Index::At(5),
            char: Some(CharStep {
                at: Index::At(6),
                gap: false
            })
        }]
    );
    // In context: a number after a word is a word; with `!` after, a verse.
    let words = one("MAT 2:1!3-5").last.unwrap();
    assert_eq!(words.steps, [word(5)]);
    let verse = one("MAT 2:1!3-2!4").last.unwrap();
    assert_eq!(
        (verse.place, verse.steps),
        (
            Place::Verse {
                chapter: 2,
                verse: Index::At(2),
                segment: None
            },
            vec![word(4)]
        )
    );
    let t = one("en+ulb+text.ROM 1:1").first.translation.unwrap();
    assert_eq!(
        (
            t.language.as_str(),
            t.translation.as_deref(),
            t.product.as_deref()
        ),
        ("en", Some("ulb"), Some("text"))
    );
}

#[test]
fn anything_else_is_refused_with_where() {
    for (text, want) in [
        ("", (ErrorKind::Empty, 0)),
        ("JON 3:10abc", (ErrorKind::Unexpected, 8)),
        ("  JON   2 :  3 ", (ErrorKind::Unexpected, 10)),
        ("JON :5", (ErrorKind::Unexpected, 4)),
        ("INVALID 1:1", (ErrorKind::ExpectedBook, 0)),
        ("XYZ 1:1", (ErrorKind::UnknownBook, 0)),
        ("JON 3:5 - JON 2:1", (ErrorKind::Backwards, 10)),
        ("MAT 2:1!5-3", (ErrorKind::Backwards, 10)),
        ("MAT 2:1!", (ErrorKind::ExpectedStep, 8)),
        ("MAT 2:1!f[0]", (ErrorKind::Unexpected, 9)),
        ("MAT 2:1-", (ErrorKind::RangeEnd, 8)),
        ("24", (ErrorKind::NoReferenceBefore, 0)),
        ("en+ulb.", (ErrorKind::ExpectedBook, 0)),
        ("ROM 3:1;", (ErrorKind::Empty, 8)),
    ] {
        assert_eq!(refused(text), want, "{text:?}");
    }
}

#[test]
fn a_range_lies_within_a_basic_reference() {
    let within = |text: &str| parse(text).unwrap()[0].reference().map(|r| r.to_string());
    assert_eq!(within("MAT 2:1!3-5").as_deref(), Some("MAT 2:1"));
    assert_eq!(within("MAT 2:1!3-2!4").as_deref(), Some("MAT 2:1-2"));
    assert_eq!(within("en+ulb.ROM 1:1-3").as_deref(), Some("ROM 1:1-3"));
    assert_eq!(within("PSA 78!d!2").as_deref(), Some("PSA 78"));
    assert_eq!(within("MRK 0!ip!53").as_deref(), Some("MRK 0"));
    assert_eq!(within("LUK 11:2b").as_deref(), Some("LUK 11:2b"));
    assert_eq!(within("NEH t:Israel/6/1"), None);
    assert_eq!(within("ROM 9:5-end"), None);
    assert_eq!(within("JON 3:10-MIC 1:2"), None);
}

/// The Kotlin hub's cases (reference/fixtures/hub-srs-cases.json) that
/// reach past the basic reference, which the `reference` crate refuses.
#[test]
fn the_hubs_srs_cases() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../reference/fixtures/hub-srs-cases.json")).unwrap();
    let mut checked = 0;
    for case in cases["parse"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let below = case["belowVerse"].as_bool() == Some(true) || case.get("language").is_some();
        if !below {
            continue;
        }
        assert_eq!(
            spelled(input),
            case["canonical"].as_str().unwrap(),
            "{input}"
        );
        checked += 1;
    }
    assert_eq!(checked, 9);
}
