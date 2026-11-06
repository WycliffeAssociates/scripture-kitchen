use super::*;

fn key(code: &str) -> BookKey {
    BookKey::new(code.as_bytes().try_into().expect("three bytes"))
}

fn problem_name(problem: Problem) -> &'static str {
    match problem {
        Problem::Empty => "empty",
        Problem::UnknownBook => "unknown-book",
        Problem::NoBook => "no-book",
        Problem::Malformed => "malformed",
        Problem::Zero => "zero",
        Problem::Backwards => "backwards",
    }
}

/// `fixtures/cite.json`: Sefer's cases, and ours where we depart.
#[test]
fn the_shared_cases() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../fixtures/cite.json")).expect("json");
    let mut catalogue = Catalogue::new(&[]);
    for (code, names) in fixture["catalogue"]["names"].as_object().expect("names") {
        for name in names.as_array().expect("list") {
            catalogue.name(key(code), name.as_str().expect("name"));
        }
    }
    for word in fixture["catalogue"]["intro"].as_array().expect("intro") {
        catalogue.intro_word(word.as_str().expect("word"));
    }
    let cases = fixture["cases"].as_array().expect("cases");
    for case in cases {
        let text = case["text"].as_str().expect("text");
        let grammar = match case["grammar"].as_str() {
            Some("prose") => Grammar::Prose,
            _ => Grammar::Navigation,
        };
        let held: Vec<BookKey> = case["held"]
            .as_array()
            .map(|held| {
                held.iter()
                    .map(|b| key(b.as_str().expect("code")))
                    .collect()
            })
            .unwrap_or_default();
        let got = cite(text, &catalogue, grammar, &held);
        match (case["refs"].as_array(), got) {
            (Some(want), Ok(cited)) => {
                let want: Vec<&str> = want.iter().map(|r| r.as_str().expect("ref")).collect();
                let got: Vec<String> = cited.refs.iter().map(ToString::to_string).collect();
                assert_eq!(got, want, "{text:?}");
                if let Some(assumed) = case["assumed"].as_array() {
                    let want: Vec<&str> = assumed.iter().map(|a| a.as_str().expect("a")).collect();
                    let got: Vec<String> = cited
                        .assumed
                        .iter()
                        .map(|a| match a {
                            Assumption::Prefix { book, .. } => format!("prefix:{book}"),
                            Assumption::CommaIsColon => "comma".to_string(),
                        })
                        .collect();
                    assert_eq!(got, want, "{text:?} assumed");
                }
            }
            (None, Err(error)) => {
                let want = case["problem"].as_str().expect("problem");
                assert_eq!(problem_name(error.problem), want, "{text:?}");
                if let Some(word) = case["word"].as_str() {
                    assert_eq!(error.word.as_deref(), Some(word), "{text:?} word");
                }
            }
            (want, got) => panic!("{text:?}: wanted {want:?}, got {got:?}"),
        }
    }
    assert_eq!(cases.len(), 67);
}

#[test]
fn a_gateway_language_names_a_book() {
    let catalogue = Catalogue::new(&["es-419", "fr"]);
    let read = |text| {
        cite(text, &catalogue, Grammar::Prose, &[])
            .map(|cited| crate::format(&cited.refs))
            .map_err(|e| e.problem)
    };
    assert_eq!(
        read("Mateo 5:3-7; 6:1"),
        Ok("MAT 5:3-7; MAT 6:1".to_string())
    );
    assert_eq!(read("Matthieu 5:3"), Ok("MAT 5:3".to_string()));
    assert_eq!(read("Éxodo 20"), Ok("EXO 20".to_string()));
    // Without the locale, the Spanish name is no book.
    let english = Catalogue::new(&[]);
    assert_eq!(
        cite("Mateo 5:3", &english, Grammar::Prose, &[]).map_err(|e| e.problem),
        Err(Problem::UnknownBook)
    );
}

#[test]
fn a_filter_lists_every_book_a_word_could_mean() {
    let catalogue = Catalogue::new(&[]);
    let john = catalogue.books_matching(citation_words("john 3:16"));
    for code in ["JHN", "1JN", "2JN", "3JN"] {
        assert!(john.contains(&key(code)), "{code} in {john:?}");
    }
    assert_eq!(citation_words("1 John 3:2; 4:1"), "1 John");
    assert!(catalogue.books_matching("").is_empty());
}

#[test]
fn show_writes_for_a_person() {
    let refs = crate::parse("MAT 5:3-7; MAT 0; MAT; LUK 11:2a-3; MAT 2-4").unwrap();
    let names = ["Mateo", "Mateo", "Mateo", "Lucas", "Mateo"];
    let shown: Vec<String> = refs
        .iter()
        .zip(names)
        .map(|(r, name)| show(r, name, "introducción"))
        .collect();
    assert_eq!(
        shown,
        [
            "Mateo 5:3-7",
            "Mateo introducción",
            "Mateo",
            "Lucas 11:2a-3",
            "Mateo 2-4"
        ]
    );
    let name = canon::name(key("MAT"), &["fr"], canon::Form::Short)
        .unwrap()
        .text;
    assert_eq!(show(&refs[0], name, "introduction"), "Matthieu 5:3-7");
}

#[test]
fn a_citation_reads_back_as_a_reference() {
    let catalogue = Catalogue::new(&[]);
    for text in [
        "Romans 9:1-18; 11:13-36",
        "Jude 3",
        "Psalm 7:0",
        "Luke intro",
        "Genesis",
    ] {
        let cited = cite(text, &catalogue, Grammar::Prose, &[]).unwrap();
        let spelled = crate::format(&cited.refs);
        assert_eq!(
            crate::parse(&spelled).unwrap(),
            cited.refs,
            "{text} as {spelled}"
        );
    }
}
