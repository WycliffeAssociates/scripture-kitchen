//! `fixtures/books.json`, `fixtures/schemes.json` and `fixtures/names.json`, the cases the TypeScript
//! side runs too (`ts/canon.test.mjs`). Every row is checked; a failure names
//! the row.

use canon::{
    BookKey, Canon, Form, book, books, fold, locales, name, parse_book, parse_book_in, scheme,
    schemes, usfm_file_name,
};
use serde_json::Value;

fn fixtures() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/books.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixtures exist"))
        .expect("fixtures parse")
}

fn rows<'a>(all: &'a Value, section: &str) -> &'a [Value] {
    all[section]
        .as_array()
        .unwrap_or_else(|| panic!("no {section} section"))
}

fn key(code: &str) -> BookKey {
    let bytes: [u8; 3] = code.as_bytes().try_into().expect("a three-byte code");
    BookKey::new(bytes)
}

#[test]
fn parse_book_cases() {
    for row in rows(&fixtures(), "parse_book") {
        let text = row["text"].as_str().unwrap();
        let want = row["book"].as_str().map(key);
        assert_eq!(parse_book(text), want, "parse_book({text:?})");
    }
}

#[test]
fn fold_cases() {
    for row in rows(&fixtures(), "fold") {
        let text = row["text"].as_str().unwrap();
        assert_eq!(
            fold(text),
            row["folded"].as_str().unwrap(),
            "fold({text:?})"
        );
    }
}

#[test]
fn book_cases() {
    for row in rows(&fixtures(), "book") {
        let code = row["code"].as_str().unwrap();
        let found = book(key(code));
        if row["missing"].as_bool() == Some(true) {
            assert!(found.is_none(), "book({code}) should be missing");
            continue;
        }
        let found = found.unwrap_or_else(|| panic!("book({code}) is missing"));
        assert_eq!(
            found.usfm_number,
            row["usfm_number"].as_str().unwrap(),
            "{code} usfm_number"
        );
        assert_eq!(
            found.division.as_str(),
            row["division"].as_str().unwrap(),
            "{code} division"
        );
        assert_eq!(
            found.canonical_index.map(u64::from),
            row["canonical_index"].as_u64(),
            "{code} canonical_index"
        );
        assert_eq!(
            found.english,
            row["english"].as_str().unwrap(),
            "{code} english"
        );
    }
}

#[test]
fn usfm_file_name_cases() {
    for row in rows(&fixtures(), "usfm_file_name") {
        let code = row["code"].as_str().unwrap();
        assert_eq!(
            usfm_file_name(key(code)).as_deref(),
            row["name"].as_str(),
            "usfm_file_name({code})"
        );
    }
}

#[test]
fn books_cases() {
    for row in rows(&fixtures(), "books") {
        let canon = match row["canon"].as_str().unwrap() {
            "protestant66" => Canon::Protestant66,
            "with_deuterocanon" => Canon::WithDeuterocanon,
            "all" => Canon::All,
            other => panic!("unknown canon {other}"),
        };
        let list: Vec<String> = books(canon).map(|b| b.code.to_string()).collect();
        assert_eq!(
            list.len() as u64,
            row["count"].as_u64().unwrap(),
            "{canon:?} count"
        );
        assert_eq!(
            list.first().map(String::as_str),
            row["first"].as_str(),
            "{canon:?} first"
        );
        assert_eq!(
            list.last().map(String::as_str),
            row["last"].as_str(),
            "{canon:?} last"
        );
    }
}

fn schemes_fixtures() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/schemes.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixtures exist"))
        .expect("fixtures parse")
}

fn named(row: &Value) -> &'static canon::Scheme {
    let name = row["scheme"].as_str().unwrap();
    scheme(name).unwrap_or_else(|| panic!("no scheme {name}"))
}

#[test]
fn scheme_names() {
    let all = schemes_fixtures();
    let want: Vec<&str> = rows(&all, "schemes")
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    let have: Vec<&str> = schemes().iter().map(canon::Scheme::name).collect();
    assert_eq!(have, want, "schemes(), in order");
    for name in rows(&all, "not_schemes") {
        let name = name.as_str().unwrap();
        assert!(scheme(name).is_none(), "scheme({name:?}) should be None");
    }
}

#[test]
fn chapters_cases() {
    for row in rows(&schemes_fixtures(), "chapters") {
        let code = row["book"].as_str().unwrap();
        assert_eq!(
            named(row).chapters(key(code)).map(u64::from),
            row["chapters"].as_u64(),
            "{}.chapters({code})",
            row["scheme"]
        );
    }
}

#[test]
fn last_verse_cases() {
    for row in rows(&schemes_fixtures(), "last_verse") {
        let code = row["book"].as_str().unwrap();
        let chapter = u16::try_from(row["chapter"].as_u64().unwrap()).unwrap();
        assert_eq!(
            named(row).last_verse(key(code), chapter).map(u64::from),
            row["last_verse"].as_u64(),
            "{}.last_verse({code}, {chapter})",
            row["scheme"]
        );
    }
}

#[test]
fn excluded_cases() {
    for row in rows(&schemes_fixtures(), "excluded") {
        let code = row["book"].as_str().unwrap();
        let chapter = u16::try_from(row["chapter"].as_u64().unwrap()).unwrap();
        let verse = u16::try_from(row["verse"].as_u64().unwrap()).unwrap();
        assert_eq!(
            named(row).excluded(key(code), chapter, verse),
            row["excluded"].as_bool().unwrap(),
            "{}.excluded({code} {chapter}:{verse})",
            row["scheme"]
        );
    }
}

fn names_fixtures() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/names.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixtures exist"))
        .expect("fixtures parse")
}

fn locales_of(row: &Value) -> Vec<&str> {
    row["locales"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tag| tag.as_str().unwrap())
        .collect()
}

fn form_name(form: Form) -> &'static str {
    match form {
        Form::Long => "long",
        Form::Short => "short",
        Form::Abbrev => "abbrev",
    }
}

#[test]
fn name_cases() {
    for row in rows(&names_fixtures(), "name") {
        let code = row["code"].as_str().unwrap();
        let locales = locales_of(row);
        let form = [Form::Long, Form::Short, Form::Abbrev]
            .into_iter()
            .find(|f| form_name(*f) == row["form"].as_str().unwrap())
            .expect("a form");
        let found = name(key(code), &locales, form);
        let what = format!("name({code}, {locales:?}, {form:?})");
        let Some(want) = row["text"].as_str() else {
            assert!(found.is_none(), "{what} should be None, is {found:?}");
            continue;
        };
        let found = found.unwrap_or_else(|| panic!("{what} is None"));
        assert_eq!(found.text, want, "{what} text");
        assert_eq!(
            found.locale,
            row["locale"].as_str().unwrap(),
            "{what} locale"
        );
        assert_eq!(
            form_name(found.form),
            row["answered"].as_str().unwrap(),
            "{what} form"
        );
    }
}

#[test]
fn parse_book_in_cases() {
    for row in rows(&names_fixtures(), "parse_book") {
        let text = row["text"].as_str().unwrap();
        let locales = locales_of(row);
        assert_eq!(
            parse_book_in(text, &locales),
            row["book"].as_str().map(key),
            "parse_book_in({text:?}, {locales:?})"
        );
    }
}

/// The generator folded every name with this same `fold`, so each locale's
/// every name parses back to its own book, except one that is a book code,
/// which the code wins.
#[test]
fn every_locales_names_parse_back_to_their_book() {
    for tag in locales() {
        for book in books(Canon::All) {
            for form in [Form::Long, Form::Short, Form::Abbrev] {
                let Some(named) = name(book.code, &[tag], form) else {
                    continue;
                };
                if named.locale != tag {
                    continue;
                }
                let parsed = parse_book_in(named.text, &[tag]);
                let a_code = named.text.trim().len() == 3
                    && parse_book(named.text).is_some_and(|code| {
                        code.to_string().eq_ignore_ascii_case(named.text.trim())
                    });
                assert!(
                    parsed == Some(book.code) || a_code,
                    "{tag} {}: {:?} parses as {parsed:?}",
                    book.code,
                    named.text
                );
            }
        }
    }
}
