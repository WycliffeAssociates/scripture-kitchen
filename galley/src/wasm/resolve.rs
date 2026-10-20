//! The resolve door: an address in as a JS object, the answer out as JSON.
//!
//! ```text
//! galley.resolve("books/LUK.usfm", { kind: "verses", from: { chapter: 1, verse: 2 }, to: { chapter: 1, verse: 2 } })
//!   → {"found":{"from":8,"to":19,"opening":5,"covers":{…},"coarser":false}}
//! ```
//!
//! The JSON is `testData/resolve/cases.json`'s `expect`, in bytes unless the
//! caller asks for `utf16`.

use crate::onion::resolve::{Address, Missing, Point, Resolution};
use mise::utf16::Utf16Table;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use super::overlay::get;

/// Sefer's `Address` off a JS object. A `book` key is ignored: the book is the
/// caller's to match.
pub(super) fn address(value: &JsValue) -> Result<Address, JsError> {
    let kind = get(value, "kind").and_then(|kind| kind.as_string());
    match kind.as_deref() {
        Some("book") => Ok(Address::Book),
        Some("intro") => Ok(Address::Intro),
        Some("chapters") => Ok(Address::Chapters {
            from: number(value, "from", "chapter")?,
            to: number(value, "to", "chapter")?,
        }),
        Some("verses") => Ok(Address::Verses {
            from: point(value, "from")?,
            to: point(value, "to")?,
        }),
        _ => Err(JsError::new(
            "resolve: an address's kind is \"book\", \"intro\", \"chapters\" or \"verses\"",
        )),
    }
}

fn point(value: &JsValue, end: &str) -> Result<Point, JsError> {
    let point = get(value, end)
        .ok_or_else(|| JsError::new(&format!("resolve: a verses address needs {end}")))?;
    let segment = match get(&point, "segment") {
        None => None,
        Some(segment) => Some(
            segment
                .as_string()
                .filter(|segment| !segment.is_empty())
                .ok_or_else(|| JsError::new("resolve: a segment is a non-empty string"))?,
        ),
    };
    Ok(Point {
        chapter: number(&point, "chapter", "chapter")?,
        verse: number(&point, "verse", "verse")?,
        segment,
    })
}

/// A chapter or verse number: a whole number from 1, as the `Toc`'s u16 holds.
fn number(value: &JsValue, key: &str, what: &str) -> Result<u16, JsError> {
    get(value, key)
        .and_then(|n| n.as_f64())
        .filter(|n| n.fract() == 0.0 && *n >= 1.0 && *n <= f64::from(u16::MAX))
        .map(|n| n as u16)
        .ok_or_else(|| JsError::new(&format!("resolve: {key} must be a {what} number from 1")))
}

pub(super) fn json(answer: &Resolution, table: Option<&Utf16Table>) -> String {
    let at = |byte: u32| table.map_or(byte, |table| table.to_utf16(byte));
    let value = match answer {
        Resolution::Found {
            span,
            opening,
            covers,
            coarser,
        } => json!({ "found": {
            "from": at(span.start), "to": at(span.end), "opening": at(*opening),
            "covers": address_json(covers), "coarser": coarser,
        }}),
        Resolution::Missing { missing, within } => {
            let mut out = json!({ "missing": match missing {
                Missing::Intro => "intro",
                Missing::Chapter => "chapter",
                Missing::Verse => "verse",
            }});
            if let Some(within) = within {
                out["within"] = json!({ "from": at(within.start), "to": at(within.end) });
            }
            out
        }
        Resolution::Ambiguous(candidates) => json!({ "ambiguous": candidates
            .iter()
            .map(|c| json!({ "from": at(c.span.start), "to": at(c.span.end), "opening": at(c.opening) }))
            .collect::<Vec<_>>() }),
    };
    value.to_string()
}

fn address_json(address: &Address) -> Value {
    let point = |point: &Point| {
        let mut out = json!({ "chapter": point.chapter, "verse": point.verse });
        if let Some(segment) = &point.segment {
            out["segment"] = json!(segment);
        }
        out
    };
    match address {
        Address::Book => json!({ "kind": "book" }),
        Address::Intro => json!({ "kind": "intro" }),
        Address::Chapters { from, to } => json!({ "kind": "chapters", "from": from, "to": to }),
        Address::Verses { from, to } => {
            json!({ "kind": "verses", "from": point(from), "to": point(to) })
        }
    }
}
