//! `fixtures/map.json`: where a verse of one scheme is under another. The
//! expected answers were written by an independent prototype of the same
//! rules (no TypeScript runs these yet: `map` reaches TypeScript through
//! galley's wasm, plan step 7).

use canon::{BookKey, Mapped, Verse, map, scheme};
use serde_json::Value;

fn verse(text: &str) -> Verse {
    let (book, rest) = text.split_once(' ').expect("BBB c:v");
    let (chapter, number) = rest.split_once(':').expect("c:v");
    let (number, part) = match number.strip_suffix(|c: char| c.is_ascii_lowercase()) {
        Some(digits) => (digits, number.chars().last()),
        None => (number, None),
    };
    let bytes: [u8; 3] = book.as_bytes().try_into().expect("a three-byte code");
    Verse {
        part,
        ..Verse::new(
            BookKey::new(bytes),
            chapter.parse().unwrap(),
            number.parse().unwrap(),
        )
    }
}

fn expected(value: &Value) -> Mapped {
    match value {
        Value::String(s) if s == "same" => Mapped::Same,
        Value::String(s) if s == "absent" => Mapped::Absent,
        Value::Object(o) => {
            let (kind, target) = o.iter().next().expect("one key");
            match kind.as_str() {
                "moved" => Mapped::Moved(verse(target.as_str().unwrap())),
                "merged" => Mapped::Merged(verse(target.as_str().unwrap())),
                "split" => Mapped::Split(
                    target
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| verse(t.as_str().unwrap()))
                        .collect(),
                ),
                other => panic!("unknown kind {other}"),
            }
        }
        other => panic!("unknown mapped {other}"),
    }
}

#[test]
fn map_cases() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/map.json");
    let all: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixtures exist")).unwrap();
    for row in all["map"].as_array().unwrap() {
        let from = scheme(row["from"].as_str().unwrap()).unwrap();
        let to = scheme(row["to"].as_str().unwrap()).unwrap();
        let text = row["verse"].as_str().unwrap();
        assert_eq!(
            map(from, to, verse(text)),
            expected(&row["mapped"]),
            "map({}, {}, {text})",
            from.name(),
            to.name()
        );
    }
}

#[test]
fn to_org_and_from_org_are_map_through_org() {
    let eng = scheme("eng").unwrap();
    let org = scheme("org").unwrap();
    let v = verse("GEN 31:55");
    assert_eq!(eng.to_org(v), map(eng, org, v));
    assert_eq!(eng.from_org(verse("GEN 32:1")), Mapped::Moved(v));
}

#[test]
fn moves_are_sparse_and_name_real_rows() {
    let eng = scheme("eng").unwrap();
    let moves: Vec<_> = eng.moves().collect();
    assert!(moves.contains(&(verse("GEN 31:55"), verse("GEN 32:1"))));
    assert!(moves.contains(&(verse("PSA 3:0"), verse("PSA 3:1"))));
    assert!(
        !moves
            .iter()
            .any(|(from, _)| from.book == BookKey::new(*b"MAT"))
    );
    assert_eq!(scheme("org").unwrap().moves().count(), 0);
}

#[test]
fn a_verse_shows_with_its_part() {
    assert_eq!(verse("ESG 4:17c").to_string(), "ESG 4:17c");
    assert_eq!(verse("GEN 1:1").to_string(), "GEN 1:1");
}
