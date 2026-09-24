//! The one reading of an options object at the wall.
//!
//! ```text
//! parse(text)                               →  every key its default
//! parse(text, { toc: true, utf16: true })   →  those two on
//! parse(text, { tco: true })                →  throws: parse: unknown option "tco"; expected diagnostics, toc or utf16
//! parse(text, { toc: 1 })                   →  throws: parse: toc must be a boolean
//! parse(text, true)                         →  throws: parse: options must be an object
//! ```
//!
//! Every door that takes options declares them as a TypeScript interface, so a
//! misspelled key is a compile error for a typed caller. This module is the
//! same promise for an untyped one: a key the door does not know is REFUSED by
//! name, never read as `false`. Absent (`undefined`, `null`, or no argument)
//! is every default.
//!
//! Errors are `String`s, not `JsError`s: `JsError` cannot be built off a wasm
//! target, and the doors' native tests live there.

use js_sys::{Object, Reflect};
use usfm_onion::wire;
use wasm_bindgen::{JsCast, JsValue};

/// The options object, checked: `None` when absent, an error when it is not an
/// object or names a key outside `known`.
pub fn bag(value: Option<&JsValue>, door: &str, known: &[&str]) -> Result<Option<JsValue>, String> {
    let Some(value) = value.filter(|v| !v.is_undefined() && !v.is_null()) else {
        return Ok(None);
    };
    if !value.is_object() {
        return Err(format!("{door}: options must be an object"));
    }
    for key in Object::keys(value.unchecked_ref::<Object>()).iter() {
        let key = key.as_string().unwrap_or_default();
        if !known.contains(&key.as_str()) {
            return Err(format!(
                "{door}: unknown option {key:?}; expected {}",
                expected(known)
            ));
        }
    }
    Ok(Some(value.clone()))
}

/// One key off a checked bag: `None` when absent, `undefined` or `null`.
pub fn get(bag: Option<&JsValue>, key: &str) -> Option<JsValue> {
    let bag = bag?;
    Reflect::get(bag, &JsValue::from_str(key))
        .ok()
        .filter(|found| !found.is_undefined() && !found.is_null())
}

/// A boolean key, absent reading as `false` and any other type as an error.
pub fn flag(bag: Option<&JsValue>, door: &str, key: &str) -> Result<bool, String> {
    match get(bag, key) {
        None => Ok(false),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("{door}: {key} must be a boolean")),
    }
}

/// `{ diagnostics?, toc?, utf16? }` — `parse`, and galley's `parse`/`parseText`.
pub fn parse_options(value: Option<&JsValue>, door: &str) -> Result<wire::ParseOptions, String> {
    let bag = bag(value, door, &["diagnostics", "toc", "utf16"])?;
    Ok(wire::ParseOptions {
        diagnostics: flag(bag.as_ref(), door, "diagnostics")?,
        toc: flag(bag.as_ref(), door, "toc")?,
        utf16: flag(bag.as_ref(), door, "utf16")?,
    })
}

/// `{ utf16? }` — the doors whose only option is the coordinate space.
pub fn utf16_only(value: Option<&JsValue>, door: &str) -> Result<bool, String> {
    let bag = bag(value, door, &["utf16"])?;
    flag(bag.as_ref(), door, "utf16")
}

/// `a`, `a or b`, `a, b or c` — the keys a refusal lists.
fn expected(known: &[&str]) -> String {
    match known {
        [] => "no options".to_string(),
        [only] => (*only).to_string(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}
