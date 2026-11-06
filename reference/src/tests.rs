use super::*;

fn key(code: &str) -> BookKey {
    BookKey::new(code.as_bytes().try_into().expect("three bytes"))
}

/// Each reference written back, `; `-joined: what a round trip must reproduce.
fn spelled(text: &str) -> String {
    format(&parse(text).unwrap_or_else(|e| panic!("{text}: {e}")))
}

fn refused(text: &str) -> ErrorKind {
    parse(text).expect_err(text).kind
}

fn eng() -> Context<'static> {
    Context {
        book: None,
        scheme: canon::scheme::scheme("eng"),
    }
}

#[test]
fn the_standards_examples_read_and_write_back() {
    for (text, written) in [
        ("GEN", "GEN"),
        ("MAT 2-4", "MAT 2-4"),
        ("MAT 2:1-4", "MAT 2:1-4"),
        ("GEN 1:1-31", "GEN 1:1-31"),
        ("GEN 1-31", "GEN 1-31"),
        ("LUK 11:2b", "LUK 11:2b"),
        ("LUK 11:34-35", "LUK 11:34-35"),
        ("PSA 68:25,28", "PSA 68:25; PSA 68:28"),
        ("GEN 1:1; 2:4", "GEN 1:1; GEN 2:4"),
        ("MAT 1:20-2:3", "MAT 1:20-2:3"),
        ("PSA 7:0", "PSA 7:0"),
        ("MRK 0", "MRK 0"),
        ("GEN 1:1-GEN 1:31", "GEN 1:1-31"),
        // `.` is a chapter separator too, and a bidi mark may come before it.
        ("ROM 9.1", "ROM 9:1"),
        ("ROM 9\u{200F}:1", "ROM 9:1"),
        // Space is allowed around `-` and the list separators.
        ("ROM 9:1 - 18 ;  MAL 3", "ROM 9:1-18; MAL 3"),
    ] {
        assert_eq!(spelled(text), written, "{text}");
        assert_eq!(spelled(written), written, "{written} reads back as itself");
    }
}

#[test]
fn context_is_the_parent_of_the_reference_before() {
    for (text, written) in [
        // After a verse, a number is a verse of its chapter.
        ("ROM 3:21, 24-26", "ROM 3:21; ROM 3:24-26"),
        // After a chapter, a number is a chapter.
        ("ROM 9-10, 12", "ROM 9-10; ROM 12"),
        // `c:v` is always a chapter and verse of the book.
        ("ROM 3:21; 4:1; GEN", "ROM 3:21; ROM 4:1; GEN"),
        ("MAL 3:1-18; 4:1-6", "MAL 3:1-18; MAL 4:1-6"),
        // A bare part is a part of the verse before it.
        ("LUK 11:2a-b", "LUK 11:2a-2b"),
        ("LUK 11:2a-3", "LUK 11:2a-3"),
    ] {
        assert_eq!(spelled(text), written, "{text}");
    }
    assert_eq!(
        refused("RUT 1:20-3"),
        ErrorKind::Backwards,
        "3 is a verse: 1:20-1:3"
    );
    assert_eq!(refused("24; ROM 3:1"), ErrorKind::NoReferenceBefore);
}

#[test]
fn a_one_chapter_books_number_is_a_verse() {
    for (text, written) in [
        ("JUD 3", "JUD 1:3"),
        ("JUD 1-4", "JUD 1:1-4"),
        ("JUD 1:4", "JUD 1:4"),
        ("JUD 0", "JUD 0"),
        (
            "ROM 3:21, 24-26; JUD 3, 5",
            "ROM 3:21; ROM 3:24-26; JUD 1:3; JUD 1:5",
        ),
        ("PHM 10", "PHM 1:10"),
    ] {
        assert_eq!(spelled(text), written, "{text}");
    }
}

#[test]
fn a_book_from_context() {
    let mat = key("MAT");
    let read = |text| format(&parse_in(mat, text).unwrap());
    assert_eq!(read("5:3-12"), "MAT 5:3-12");
    assert_eq!(read("5"), "MAT 5");
    assert_eq!(read("5:3, 7"), "MAT 5:3; MAT 5:7");
    assert_eq!(
        read("MRK 1:1"),
        "MRK 1:1",
        "a full reference keeps its own book"
    );
    assert_eq!(format(&parse_in(key("JUD"), "4").unwrap()), "JUD 1:4");
}

#[test]
fn addresses_are_the_ones_mise_names() {
    let one = |text| parse(text).unwrap()[0].address;
    assert_eq!(one("GEN"), Address::Book);
    assert_eq!(one("GEN 0"), Address::Intro);
    assert_eq!(one("GEN 2-4"), Address::Chapters { from: 2, to: 4 });
    let lk = Point {
        chapter: 11,
        verse: 2,
        segment: Segment::new(b'b'),
    };
    assert_eq!(one("LUK 11:2b"), Address::Verses { from: lk, to: lk });
    assert_eq!(parse("1CO 1:1").unwrap()[0].book, key("1CO"));
}

#[test]
fn end_needs_a_scheme_and_a_scheme_checks_numbers() {
    assert_eq!(refused("ROM 9:5-end"), ErrorKind::EndNeedsScheme);
    let with = |text| parse_with(text, eng());
    assert_eq!(format(&with("ROM 9:5-end").unwrap()), "ROM 9:5-33");
    assert_eq!(format(&with("ROM 15-end").unwrap()), "ROM 15-16");
    assert_eq!(
        with("ROM 17").unwrap_err().kind,
        ErrorKind::ChapterOutOfRange
    );
    assert_eq!(
        with("ROM 9:40").unwrap_err().kind,
        ErrorKind::VerseOutOfRange
    );
    // Without one, nothing is checked.
    assert_eq!(spelled("ROM 17"), "ROM 17");
}

#[test]
fn anything_else_is_refused_with_where() {
    for (text, kind, at) in [
        ("", ErrorKind::Empty, 0),
        ("   ", ErrorKind::Empty, 3),
        ("ROM 3:1;", ErrorKind::Empty, 8),
        ("rom 9:1", ErrorKind::ExpectedBook, 0),
        ("Romans 9:1", ErrorKind::ExpectedBook, 0),
        ("XYZ 1:1", ErrorKind::UnknownBook, 0),
        ("24", ErrorKind::NoReferenceBefore, 0),
        ("100 1:1", ErrorKind::NoReferenceBefore, 0),
        ("ROM 9:", ErrorKind::ExpectedVerse, 6),
        ("ROM 9:1:2", ErrorKind::Unexpected, 7),
        ("ROM 9 : 1", ErrorKind::Unexpected, 6),
        ("ROM 9:1-", ErrorKind::RangeEnd, 8),
        ("ROM 9:1-GEN 1:1", ErrorKind::DifferentBook, 8),
        ("ROM 9:18-1", ErrorKind::Backwards, 9),
        ("ROM 9:1-8:2", ErrorKind::Backwards, 8),
        ("ROM 9-10:4", ErrorKind::MixedRange, 6),
        ("GEN 0:3", ErrorKind::IntroHasNoVerses, 0),
        ("GEN 0-2", ErrorKind::IntroHasNoVerses, 0),
        ("en+ulb.ROM 1:1", ErrorKind::NotBasic, 0),
        ("ROM 3:21!3", ErrorKind::NotBasic, 8),
        ("MRK 1:14!s", ErrorKind::NotBasic, 8),
        ("MRK 0!ip", ErrorKind::NotBasic, 5),
        ("GEN1:1", ErrorKind::ExpectedBook, 0),
    ] {
        let error = parse(text).expect_err(text);
        assert_eq!((error.kind, error.at), (kind, at), "{text}");
    }
}

/// The Kotlin translation hub's cases (`fixtures/hub-srs-cases.json`, its
/// `fixtures/srs/cases.json` at 4355451). Its parse, list and invalid groups run
/// here; its range operations (split per chapter, overlap, verse lists) are not
/// this crate's. Where we answer differently, by design, the case is listed in
/// `ours` with why.
#[test]
fn the_hubs_cases_agree_or_differ_on_purpose() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/hub-srs-cases.json")).expect("json");
    // Input -> our answer: Ok(spelling) or Err(kind).
    let ours: &[(&str, Result<&str, ErrorKind>, &str)] = &[
        (
            "rom 9:1",
            Err(ErrorKind::ExpectedBook),
            "strict: a code is capitals; cite reads it",
        ),
        (
            "1co 1:1",
            Err(ErrorKind::ExpectedBook),
            "strict: a code is capitals",
        ),
        (
            "  rom   9 : 1 - 18 ",
            Err(ErrorKind::ExpectedBook),
            "strict; the hub ignores space",
        ),
        (
            "MRK 0!ip",
            Err(ErrorKind::NotBasic),
            "a marker reference is srs's",
        ),
        (
            "MRK 1:14!s",
            Err(ErrorKind::NotBasic),
            "a marker reference is srs's",
        ),
        (
            "ROM 3:21!f",
            Err(ErrorKind::NotBasic),
            "a marker reference is srs's",
        ),
        (
            "ROM 3:21!3",
            Err(ErrorKind::NotBasic),
            "a word reference is srs's",
        ),
        (
            "ROM 3:21!f[2]",
            Err(ErrorKind::NotBasic),
            "a marker reference is srs's",
        ),
        (
            "en+ulb.ROM 1:1",
            Err(ErrorKind::NotBasic),
            "a translation prefix is srs's",
        ),
        (
            "en+ulb+text.ROM 1:1",
            Err(ErrorKind::NotBasic),
            "a translation prefix is srs's",
        ),
        (
            "cja+wcb.ROM 9:1",
            Err(ErrorKind::NotBasic),
            "a translation prefix is srs's",
        ),
        (
            "en-t-wsg:JHN 3:16",
            Err(ErrorKind::NotBasic),
            "a translation prefix is srs's",
        ),
        (
            "JUD 1-4",
            Ok("JUD 1:1-4"),
            "written with its chapter, so it reads back the same anywhere",
        ),
        ("JUD 3", Ok("JUD 1:3"), "written with its chapter"),
        ("JUD 1:4", Ok("JUD 1:4"), "written with its chapter"),
        ("JUD 5", Ok("JUD 1:5"), "written with its chapter"),
        (
            "100 1:1",
            Err(ErrorKind::NoReferenceBefore),
            "all digits is no code: a number with nothing before",
        ),
        (
            "24",
            Err(ErrorKind::NoReferenceBefore),
            "the same as \"24; ROM 3:1\"",
        ),
    ];
    let mine = |input: &str| ours.iter().find(|(text, ..)| *text == input);
    let scheme_for = |case: &serde_json::Value| {
        let wants = case["versification"].as_bool() == Some(true)
            || case["error"]
                .as_str()
                .is_some_and(|e| e.contains("with versification"));
        if wants { eng() } else { Context::default() }
    };
    let mut checked = 0;

    for case in cases["parse"].as_array().expect("parse") {
        let input = case["input"].as_str().expect("input");
        let got = parse_with(input, scheme_for(case));
        checked += 1;
        match mine(input) {
            Some((_, Err(kind), why)) => {
                assert_eq!(got.map_err(|e| e.kind).err(), Some(*kind), "{input}: {why}")
            }
            Some((_, Ok(spelling), why)) => {
                assert_eq!(format(&got.expect(input)), *spelling, "{input}: {why}")
            }
            None => {
                let got = got.unwrap_or_else(|e| panic!("{input}: {e}"));
                assert_eq!(
                    format(&got),
                    case["canonical"].as_str().expect("canonical"),
                    "{input}"
                );
                let Address::Verses { from, to } = got[0].address else {
                    continue;
                };
                let field = |name: &str| case[name].as_u64().map(|n| n as u16);
                if let Some(v) = field("verseStart") {
                    assert_eq!(from.verse, v, "{input} verseStart");
                }
                if let Some(v) = field("verseEnd") {
                    assert_eq!(to.verse, v, "{input} verseEnd");
                }
                if let Some(c) = field("chapterStart").or(field("chapter")) {
                    assert_eq!(from.chapter, c, "{input} chapter");
                }
                if let Some(s) = case["subverse"].as_str() {
                    assert_eq!(
                        from.segment.map(|s| s.to_string()).as_deref(),
                        Some(s),
                        "{input}"
                    );
                }
            }
        }
    }

    for case in cases["parseList"].as_array().expect("parseList") {
        let input = case["input"].as_str().expect("input");
        let want = match mine(input) {
            Some((_, Ok(spelling), _)) => spelling.to_string(),
            _ => case["items"]
                .as_array()
                .expect("items")
                .iter()
                .map(|item| {
                    let item = item.as_str().expect("item");
                    mine(item)
                        .and_then(|(_, ours, _)| ours.ok())
                        .unwrap_or(item)
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("; "),
        };
        assert_eq!(spelled(input), want, "{input}");
        checked += 1;
    }

    for group in ["invalid", "parseListInvalid"] {
        for case in cases[group].as_array().expect(group) {
            let input = case["input"].as_str().expect("input");
            let hub = case["error"].as_str().expect("error");
            let want = match mine(input) {
                Some((_, Err(kind), _)) => *kind,
                _ => match hub {
                    "unknown book" => ErrorKind::UnknownBook,
                    "range end before start" => ErrorKind::Backwards,
                    "chapter out of range (with versification)" => ErrorKind::ChapterOutOfRange,
                    "verse out of range (with versification)" => ErrorKind::VerseOutOfRange,
                    "'end' needs a versification" => ErrorKind::EndNeedsScheme,
                    "empty" => ErrorKind::Empty,
                    "expected a verse" => ErrorKind::ExpectedVerse,
                    "unexpected" => ErrorKind::Unexpected,
                    "range end" => ErrorKind::RangeEnd,
                    "different book" => ErrorKind::DifferentBook,
                    "no reference before" => ErrorKind::NoReferenceBefore,
                    other => panic!("{input}: no mapping for the hub's {other:?}"),
                },
            };
            let got = parse_with(input, scheme_for(case));
            assert_eq!(
                got.map_err(|e| e.kind).err(),
                Some(want),
                "{input}: the hub says {hub}"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked,
        28 + 8 + 14 + 2,
        "every case in the four groups ran"
    );
}
