//! `testData/resolve/cases.json`, run against `Toc::resolve`: each case a USFM
//! snippet, an address in Sefer's shape, and the answer in byte offsets.

use serde_json::{Value, json};
use usfm_onion::resolve::{Address, Missing, Point, Resolution};
use usfm_onion::{lex, toc};

fn point(value: &Value) -> Point {
    Point {
        chapter: value["chapter"].as_u64().expect("chapter") as u16,
        verse: value["verse"].as_u64().expect("verse") as u16,
        segment: value
            .get("segment")
            .map(|s| s.as_str().expect("segment").to_string()),
    }
}

fn address(value: &Value) -> Address {
    match value["kind"].as_str().expect("kind") {
        "book" => Address::Book,
        "intro" => Address::Intro,
        "chapters" => Address::Chapters {
            from: value["from"].as_u64().expect("from") as u16,
            to: value["to"].as_u64().expect("to") as u16,
        },
        "verses" => Address::Verses {
            from: point(&value["from"]),
            to: point(&value["to"]),
        },
        other => panic!("unknown address kind {other}"),
    }
}

fn point_json(point: &Point) -> Value {
    let mut out = json!({ "chapter": point.chapter, "verse": point.verse });
    if let Some(segment) = &point.segment {
        out["segment"] = json!(segment);
    }
    out
}

fn address_json(address: &Address) -> Value {
    match address {
        Address::Book => json!({ "kind": "book" }),
        Address::Intro => json!({ "kind": "intro" }),
        Address::Chapters { from, to } => json!({ "kind": "chapters", "from": from, "to": to }),
        Address::Verses { from, to } => {
            json!({ "kind": "verses", "from": point_json(from), "to": point_json(to) })
        }
    }
}

fn answer_json(answer: &Resolution) -> Value {
    match answer {
        Resolution::Found {
            span,
            opening,
            covers,
            coarser,
        } => json!({ "found": {
            "from": span.start, "to": span.end, "opening": opening,
            "covers": address_json(covers), "coarser": coarser,
        }}),
        Resolution::Missing { missing, within } => {
            let what = match missing {
                Missing::Intro => "intro",
                Missing::Chapter => "chapter",
                Missing::Verse => "verse",
            };
            let mut out = json!({ "missing": what });
            if let Some(within) = within {
                out["within"] = json!({ "from": within.start, "to": within.end });
            }
            out
        }
        Resolution::Ambiguous(candidates) => json!({ "ambiguous": candidates
            .iter()
            .map(|c| json!({ "from": c.span.start, "to": c.span.end, "opening": c.opening }))
            .collect::<Vec<_>>() }),
    }
}

#[test]
fn every_case_resolves_as_recorded() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../testData/resolve/cases.json"
    );
    let text = std::fs::read_to_string(path).expect("testData/resolve/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&text).expect("cases.json parses");
    assert!(cases.len() > 20, "cases.json holds the rule cases");
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().expect("name");
        let usfm = case["usfm"].as_str().expect("usfm");
        let book = toc(usfm.as_bytes(), &lex(usfm));
        let got = answer_json(&book.resolve(usfm, &address(&case["address"])));
        if got != case["expect"] {
            failures.push(format!("{name}\n  want {}\n  got  {got}", case["expect"]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
