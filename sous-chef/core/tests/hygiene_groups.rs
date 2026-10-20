//! `testData/hygiene/groups.json`, run against `hygiene_groups` and `fix`:
//! rows in, groups and deletions out. `sous-chef/reader.test.mjs` runs the
//! same file against the generated reader, over the publication bytes this
//! test pins beside it as `<name>.hex`.

use serde_json::{Value, json};
use sous_core::hygiene::groups::{HygieneGroup, Scope, hygiene_groups};
use sous_core::{
    Anchor, BookIndex, BookKey, CoordinateSpace, CorpusSnapshot, FindingKind, HygieneClass,
    HygieneDigest, HygieneSite, PackedFinding, Placement, PresenceDigest, PresenceKind,
    PublicationBook, SnapshotId, encode_to_corpus_buffer,
};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testData/hygiene");

fn named<T: Copy>(all: &[T], name: impl Fn(T) -> &'static str, wanted: &Value) -> T {
    let wanted = wanted.as_str().expect("a name");
    *all.iter()
        .find(|item| name(**item) == wanted)
        .unwrap_or_else(|| panic!("unknown name {wanted}"))
}

fn finding(row: &Value, lengths: &[u32]) -> PackedFinding {
    let kind = if row["kind"] == "Presence" {
        FindingKind::Presence(PresenceDigest::new(PresenceKind::Missing, 1).unwrap())
    } else {
        let site = HygieneSite::new(
            named(&Placement::ALL, Placement::name, &row["placement"]),
            named(&Anchor::ALL, Anchor::name, &row["anchor"]),
            row["oneUnit"].as_bool().expect("oneUnit"),
            row["loneIsSilent"].as_bool().expect("loneIsSilent"),
        );
        let class = named(&HygieneClass::ALL, HygieneClass::name, &row["class"]);
        let run = row["run"].as_u64().expect("run") as u32;
        FindingKind::Hygiene(HygieneDigest::new(class, run, site).unwrap())
    };
    let at = |field: &str| row[field].as_u64().expect(field) as u32;
    let book = BookIndex::new(row["book"].as_u64().expect("book") as usize).unwrap();
    PackedFinding::new(at("from"), at("to"), book, kind, lengths).unwrap()
}

fn publication(case: &Value) -> Vec<u8> {
    let books = case["books"].as_array().expect("books");
    let lengths: Vec<u32> = books
        .iter()
        .map(|b| b["length"].as_u64().unwrap() as u32)
        .collect();
    let rows: Vec<PackedFinding> = case["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| finding(row, &lengths))
        .collect();
    let per_book: Vec<Vec<PackedFinding>> = (0..books.len())
        .map(|index| {
            rows.iter()
                .filter(|row| usize::from(row.book_idx().get()) == index)
                .copied()
                .collect()
        })
        .collect();
    let sections: Vec<PublicationBook<'_>> = books
        .iter()
        .zip(&per_book)
        .map(|(book, findings)| {
            let key: [u8; 3] = book["key"].as_str().unwrap().as_bytes().try_into().unwrap();
            PublicationBook::new(
                BookKey::new(key),
                book["id"].as_str().unwrap(),
                book["length"].as_u64().unwrap() as u32,
                findings,
            )
        })
        .collect();
    let space = match case["coordinates"].as_str() {
        Some("utf16") => CoordinateSpace::Utf16,
        Some("utf8") => CoordinateSpace::Utf8,
        other => panic!("coordinates {other:?}"),
    };
    encode_to_corpus_buffer(SnapshotId::new([0; 16]), space, &sections, &[], &[], &[]).unwrap()
}

fn group_json(group: &HygieneGroup) -> Value {
    let key = group.key();
    json!({
        "book": key.book.map(BookIndex::get),
        "class": key.class.name(),
        "placement": key.placement.name(),
        "count": group.count(),
        "rows": group.rows().iter().map(|row| json!([row.book.get(), row.row])).collect::<Vec<_>>(),
        "fix": group.fix().map(|all| all
            .iter()
            .map(|d| json!([d.book.get(), d.from, d.to]))
            .collect::<Vec<_>>()),
    })
}

/// The bytes the node test reads. Under `UPDATE_HEX` it rewrites the file,
/// and the test fails once every file is written: regenerating is never a
/// passing test.
fn assert_hex(encoded: &[u8], name: &str) -> bool {
    let path = format!("{DIR}/{name}.hex");
    let hex: Vec<String> = encoded.iter().map(|byte| format!("{byte:02x}")).collect();
    if std::env::var_os("UPDATE_HEX").is_some() {
        std::fs::write(&path, format!("{}\n", hex.join(" "))).unwrap();
        return true;
    }
    let pinned = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path}"));
    assert_eq!(pinned.trim_end(), hex.join(" "), "{name}.hex");
    false
}

#[test]
fn every_case_groups_and_fixes_as_recorded() {
    let text = std::fs::read_to_string(format!("{DIR}/groups.json")).expect("groups.json");
    let cases: Vec<Value> = serde_json::from_str(&text).expect("groups.json parses");
    assert_eq!(
        cases.len(),
        2,
        "groups.json holds a UTF-16 and a UTF-8 publication"
    );
    let mut failures = Vec::new();
    let mut rewrote = false;
    for case in &cases {
        let name = case["name"].as_str().expect("name");
        let buffer = publication(case);
        rewrote |= assert_hex(&buffer, name);
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        for (scope, label) in [(Scope::Book, "book"), (Scope::Project, "project")] {
            let got: Vec<Value> = hygiene_groups(&snapshot, scope)
                .unwrap()
                .iter()
                .map(group_json)
                .collect();
            let want = case["groups"][label].as_array().expect("groups");
            if got.len() != want.len() {
                failures.push(format!(
                    "{name} {label}: {} groups, want {}",
                    got.len(),
                    want.len()
                ));
            }
            for (got, want) in got.iter().zip(want) {
                if got != want {
                    failures.push(format!("{name} {label}\n  want {want}\n  got  {got}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        !rewrote,
        "UPDATE_HEX rewrote the publications; rerun without it to test them"
    );
}
