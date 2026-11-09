//! The versification doors' JS shapes: a declaration off an options bag, a
//! Copenhagen mapping object walked into canon's builder, and the JSON that
//! `sniff`, `mapVerse` and `pairs` answer with.
//!
//! JS already has `JSON.parse`, so a scheme arrives as the parsed object and
//! nothing here reads JSON text: canon checks every entry as it would its
//! own data.

use canon::{Confidence, Mapped, Paired, Scheme, SchemeBuilder, Sniff};
use js_sys::{Array, Object, Reflect};
use serde::Serialize;
use wasm_bindgen::{JsCast, JsError, JsValue};

use crate::versification::Declared;

/// The `versification` key off a checked bag: `"followSource"`, a scheme's
/// name, or absent.
pub(super) fn declaration(bag: Option<&JsValue>, door: &str) -> Result<Option<Declared>, JsError> {
    let Some(value) = onion_wasm::options::get(bag, "versification") else {
        return Ok(None);
    };
    let name = value.as_string().ok_or_else(|| {
        JsError::new(&format!(
            "{door}: versification must be a scheme's name or \"followSource\""
        ))
    })?;
    Ok(Some(match name.as_str() {
        "followSource" => Declared::FollowSource,
        _ => Declared::Scheme(name),
    }))
}

/// A Copenhagen mapping object (`maxVerses`, `mappedVerses`,
/// `excludedVerses`; `partialVerses` is accepted and not read) as a scheme.
pub(super) fn scheme(name: &str, mapping: &JsValue) -> Result<Scheme, JsError> {
    let fail = |what: String| JsError::new(&format!("defineScheme {name:?}: {what}"));
    if !mapping.is_object() || Array::is_array(mapping) {
        return Err(fail("the mapping must be an object".into()));
    }
    for key in Object::keys(mapping.unchecked_ref::<Object>()).iter() {
        let key = key.as_string().unwrap_or_default();
        if ![
            "maxVerses",
            "mappedVerses",
            "excludedVerses",
            "partialVerses",
        ]
        .contains(&key.as_str())
        {
            return Err(fail(format!(
                "unknown key {key:?}; expected maxVerses, mappedVerses, excludedVerses or partialVerses"
            )));
        }
    }
    let mut builder = SchemeBuilder::new(name).map_err(|e| fail(e.to_string()))?;

    let max = field(mapping, "maxVerses").ok_or_else(|| fail("no maxVerses".into()))?;
    for (code, chapters) in entries(&max).map_err(&fail)? {
        let book = book_key(&code).ok_or_else(|| fail(format!("{code:?} is not a book code")))?;
        let chapters: Array = chapters
            .dyn_into()
            .map_err(|_| fail(format!("maxVerses.{code} must be an array")))?;
        let counts = chapters
            .iter()
            .enumerate()
            .map(|(at, n)| {
                count(&n)
                    .ok_or_else(|| fail(format!("maxVerses.{code}[{at}] is not a verse count")))
            })
            .collect::<Result<Vec<u16>, JsError>>()?;
        builder
            .book(book, counts)
            .map_err(|e| fail(e.to_string()))?;
    }
    if let Some(mapped) = field(mapping, "mappedVerses") {
        for (from, to) in entries(&mapped).map_err(&fail)? {
            let to = to
                .as_string()
                .ok_or_else(|| fail(format!("mappedVerses[{from:?}] must be a string")))?;
            builder.map(&from, &to).map_err(|e| fail(e.to_string()))?;
        }
    }
    if let Some(excluded) = field(mapping, "excludedVerses") {
        let list: Array = excluded
            .dyn_into()
            .map_err(|_| fail("excludedVerses must be an array".into()))?;
        for reference in list.iter() {
            let reference = reference
                .as_string()
                .ok_or_else(|| fail("excludedVerses holds a non-string".into()))?;
            builder
                .exclude(&reference)
                .map_err(|e| fail(e.to_string()))?;
        }
    }
    builder.build().map_err(|e| fail(e.to_string()))
}

fn field(object: &JsValue, key: &str) -> Option<JsValue> {
    Reflect::get(object, &JsValue::from_str(key))
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
}

fn entries(object: &JsValue) -> Result<Vec<(String, JsValue)>, String> {
    if !object.is_object() || Array::is_array(object) {
        return Err("expected an object".into());
    }
    Ok(Object::entries(object.unchecked_ref::<Object>())
        .iter()
        .map(|pair| {
            let pair: Array = pair.unchecked_into();
            (pair.get(0).as_string().unwrap_or_default(), pair.get(1))
        })
        .collect())
}

/// A count as a number or, as the published files write it, a decimal string.
fn count(value: &JsValue) -> Option<u16> {
    let n = match value.as_f64() {
        Some(n) if n.fract() == 0.0 && (0.0..=f64::from(u16::MAX)).contains(&n) => n as u16,
        Some(_) => return None,
        None => value.as_string()?.parse().ok()?,
    };
    Some(n)
}

pub(super) fn book_key(code: &str) -> Option<canon::BookKey> {
    let key = canon::BookKey::new(code.as_bytes().try_into().ok()?);
    canon::book(key).map(|_| key)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SniffJson<'a> {
    /// The winner, unless undetermined.
    best: Option<&'a str>,
    confidence: &'static str,
    voted: u32,
    ranked: Vec<StandingJson<'a>>,
    dissent: Vec<DissentJson<'a>>,
    unmatched: &'a [&'static str],
}

#[derive(Serialize)]
struct StandingJson<'a> {
    scheme: &'a str,
    agree: u32,
    disagree: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DissentJson<'a> {
    rule: &'a str,
    text_follows: &'a [&'static str],
}

pub(super) fn sniff_json(sniff: &Sniff) -> Result<JsValue, JsError> {
    super::overlay::json(&SniffJson {
        best: sniff.best().map(Scheme::name),
        confidence: match sniff.confidence {
            Confidence::Decided => "decided",
            Confidence::Leaning => "leaning",
            Confidence::Undetermined => "undetermined",
        },
        voted: sniff.voted,
        ranked: sniff
            .ranked
            .iter()
            .map(|s| StandingJson {
                scheme: s.scheme.name(),
                agree: s.agree,
                disagree: s.disagree,
            })
            .collect(),
        dissent: sniff
            .dissent
            .iter()
            .map(|d| DissentJson {
                rule: d.rule,
                text_follows: &d.text_follows,
            })
            .collect(),
        unmatched: &sniff.unmatched,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum MappedJson {
    Same(bool),
    Moved(String),
    Merged(String),
    Split(Vec<String>),
    Absent(bool),
}

pub(super) fn mapped_json(mapped: &Mapped) -> Result<JsValue, JsError> {
    super::overlay::json(&match mapped {
        Mapped::Same => MappedJson::Same(true),
        Mapped::Moved(v) => MappedJson::Moved(v.to_string()),
        Mapped::Merged(v) => MappedJson::Merged(v.to_string()),
        Mapped::Split(vs) => MappedJson::Split(vs.iter().map(ToString::to_string).collect()),
        Mapped::Absent => MappedJson::Absent(true),
    })
}

#[derive(Serialize)]
struct PairedJson {
    here: Vec<String>,
    there: Vec<String>,
}

pub(super) fn pairs_json(units: &[Paired]) -> Result<JsValue, JsError> {
    super::overlay::json(
        &units
            .iter()
            .map(|u| PairedJson {
                here: u.here.iter().map(ToString::to_string).collect(),
                there: u.there.iter().map(ToString::to_string).collect(),
            })
            .collect::<Vec<_>>(),
    )
}
