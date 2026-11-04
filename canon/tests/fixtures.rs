//! `fixtures/books.json`, the cases the TypeScript side runs too
//! (`ts/canon.test.mjs`). Every row is checked; a failure names the row.

use canon::{BookKey, Canon, book, books, fold, parse_book, usfm_file_name};
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
