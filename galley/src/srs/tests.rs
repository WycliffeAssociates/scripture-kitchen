use super::*;

fn ext() -> Extensions {
    Extensions::default()
}

fn jonah() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../testData/exampleCorpora/examples.bsb/32JONBSB.usfm"
    ))
    .expect("usfmtc's Jonah")
}

fn found(source: &str, reference: &str) -> Result<String, LocateError> {
    let range = srs::parse(reference).unwrap_or_else(|e| panic!("{reference}: {e}"));
    let span = locate(source, &ext(), &range[0])?;
    Ok(source[span.start as usize..span.end as usize].to_string())
}

fn text(source: &str, reference: &str) -> String {
    found(source, reference).unwrap_or_else(|e| panic!("{reference}: {e:?}"))
}

/// usfmtc's `tests/test_refs.py` cases that read text out of Jonah.
#[test]
fn usfmtcs_jonah_cases() {
    let jon = jonah();
    assert_eq!(text(&jon, "JON 2:8!2-4"), "who cling to");
    assert_eq!(text(&jon, "JON 3:10!1-2"), "When God");
    assert!(text(&jon, "JON 3:10!1-4").contains("God"));
    assert_eq!(text(&jon, "JON 3:6!3-4"), "reached the");
    assert_eq!(text(&jon, "JON 1:4!s1!2"), "Great");
    assert_eq!(text(&jon, "JON 1:13!f!5"), "dug");
    assert_eq!(text(&jon, "JON 1:13!5!f!5"), "dug");
}

/// The standard's Appendix 3, Psalm 78.
#[test]
fn the_standards_psalm_78() {
    let psa = "\\id PSA LEB\n\\c 78\n\\cl Psalm 78\n\\s1 God’s Faithfulness in Israel’s History\n\
               \\d A maskil of Asaph.\\f + \\fr Ps 78 \\ft The Hebrew Bible counts the superscription as the first verse of the psalm \\f*\n\
               \\q1\n\\v 1 Listen, O my people, to my teaching.\n\\q1 Incline your ears to the words of my mouth.\n";
    assert_eq!(text(psa, "PSA 78!d!2"), "maskil");
    assert_eq!(text(psa, "PSA 78:0!d!2"), "maskil");
    assert_eq!(text(psa, "PSA 78!d!f!2"), "78");
    assert_eq!(text(psa, "PSA 78:1!s1!5"), "History");
    assert_eq!(text(psa, "PSA 78:1!1"), "Listen,");
    assert_eq!(text(psa, "PSA 78:1!q1!3"), "ears");
    // Verse 0 is the title's own words.
    assert_eq!(text(psa, "PSA 78:0!2"), "maskil");
}

/// The standard's own examples in its sections on notes, attributes and
/// paragraphs.
#[test]
fn the_standards_section_examples() {
    let hello = "\\id GEN\n\\c 1\n\\p\n\\v 2 Hello\\f + \\fr 1:2 \\ft A greeting\\f* everyone\n";
    assert_eq!(text(hello, "GEN 1:2!1!f!3"), "greeting");
    assert_eq!(text(hello, "GEN 1:2!f!3"), "greeting");
    assert_eq!(text(hello, "GEN 1:2!2"), "everyone");
    assert_eq!(text(hello, "GEN 1:2!f@caller"), "+");
    assert_eq!(text(hello, "GEN 1:2@number"), "2");

    let anthro = "\\id GEN\n\\c 1\n\\p\n\\v 1 anthro\\f + \\ft human\\f*pomorphic\n";
    assert_eq!(text(anthro, "GEN 1:1!2"), "pomorphic", "a note ends a word");

    // The standard wraps `dit`'s attribute list across a line for display;
    // here it is one line, as a file holds it.
    let abram = "\\id GEN\n\\c 1\n\\p\n\\v 1 L'\\w Éternel|strong=\"H3068\"\\w* \\w dit|strong=\"H559\" \
                 x-morph=\"strongMorph:TH8799\"\\w* à \\w Abram|strong=\"H87\"\\w*:\n\
                 Va-t-\\w en|strong=\"H3212\" x-morph=\"strongMorph:TH8798\"\\w* de ton.\n";
    assert_eq!(text(abram, "GEN 1:1!8"), "en");
    assert_eq!(text(abram, "GEN 1:1!8@strong"), "H3212");
    assert_eq!(text(abram, "GEN 1:1!w[4]@strong"), "H3212");
    assert_eq!(text(abram, "GEN 1:1!w[4]"), "en");

    let luke = "\\id LUK\n\\c 8\n\\p\n\\v 9 Then the \\w disciples\\w* said, “Teacher!”\n\
                \\v 10 So Jesus said, “God gave you the knowledge. Therefore\n\
                \\q1 ‘They will look but not see [it],\n\\q2 they will listen but they don't understand’\n\
                \\m he told like that the disciples.\n";
    assert_eq!(text(luke, "LUK 8:10!q1!1"), "‘They");
    assert!(text(luke, "LUK 8:10!p").starts_with("So Jesus said"));
    assert!(text(luke, "LUK 8:10!p").ends_with("Therefore\n"));
    assert_eq!(text(luke, "LUK 8:9!w!1"), "disciples");
}

#[test]
fn characters_are_counted_in_nfd() {
    // `é` is one source character and two NFD ones.
    let source = "\\id GEN\n\\c 1\n\\p\n\\v 1 Éternel dit\n";
    assert_eq!(text(source, "GEN 1:1!1+1"), "É");
    assert_eq!(
        text(source, "GEN 1:1!1+2"),
        "É",
        "its accent is the same source character"
    );
    assert_eq!(text(source, "GEN 1:1!1+3"), "t");
    assert_eq!(text(source, "GEN 1:1!1+end"), "l");
    assert_eq!(text(source, "GEN 1:1!1+0"), "", "before the word");
    assert_eq!(text(source, "GEN 1:1!1+end+"), " ", "the gap after it");
    assert_eq!(
        found(source, "GEN 1:1!1+9"),
        Err(LocateError::Missing("character"))
    );
    assert_eq!(text(source, "GEN 1:1!end"), "dit");
}

#[test]
fn what_isnt_there_says_so() {
    let jon = jonah();
    assert_eq!(
        found(&jon, "JON 9:1!1"),
        Err(LocateError::Missing("chapter"))
    );
    assert_eq!(found(&jon, "JON 1:99"), Err(LocateError::Missing("verse")));
    assert_eq!(found(&jon, "JON 1:1!99"), Err(LocateError::Missing("word")));
    assert_eq!(
        found(&jon, "JON 1:1!x"),
        Err(LocateError::Missing("element"))
    );
    assert_eq!(
        found(&jon, "JON 1:1@strong"),
        Err(LocateError::Missing("attribute"))
    );
    assert_eq!(
        found(&jon, "GEN 1:1"),
        Err(LocateError::Unsupported("book"))
    );
    assert_eq!(
        found(&jon, "JON k:storm"),
        Err(LocateError::Unsupported("named reference"))
    );
}

/// A span spelled, then located again, is the same span.
#[test]
fn spell_is_locates_inverse() {
    let jon = jonah();
    for reference in [
        "JON 2:8!2-4",
        "JON 3:6!3",
        "JON 1:13!f!5",
        "JON 1:13!f!2-4",
        "JON 3:10!1+2-3",
        "JON 1:1",
        "JON 1:1-2",
        "JON 1:4!s1!2-3",
        "JON 1:2!3-3!4",
        "JON 1:13!f!5+2-13!f!6",
    ] {
        let range = srs::parse(reference).unwrap();
        let span = locate(&jon, &ext(), &range[0]).unwrap();
        let spelled =
            spell(&jon, &ext(), span.clone()).unwrap_or_else(|| panic!("{reference} spells"));
        assert_eq!(spelled.to_string(), reference, "{reference}");
        assert_eq!(
            locate(&jon, &ext(), &spelled).unwrap(),
            span,
            "{reference} round trips"
        );
    }
}

#[test]
fn spell_names_a_caret_and_note_markers_end_at_the_next() {
    let jon = jonah();
    let range = srs::parse("JON 2:8!3").unwrap();
    let word = locate(&jon, &ext(), &range[0]).unwrap();
    let caret = spell(&jon, &ext(), word.start..word.start).unwrap();
    assert_eq!(caret.to_string(), "JON 2:8!3");
    // `\fr` ends where `\ft` starts.
    let note = "\\id MAT\n\\c 1\n\\p\n\\v 1 a\\f + \\fr 1:1 \\ft one two\\f*\n";
    assert_eq!(text(note, "MAT 1:1!f!fr"), "1:1 ");
    assert_eq!(text(note, "MAT 1:1!f!ft!2"), "two");
    assert_eq!(
        found(note, "MAT 1:1!s[-2147483648]"),
        Err(LocateError::Missing("element"))
    );
}
