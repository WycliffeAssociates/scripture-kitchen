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

use std::sync::Arc;

use js_sys::{Object, Reflect};
use usfm_onion::extensions::{ExtensionOptions, Extensions};
use usfm_onion::wire;
use wasm_bindgen::convert::TryFromJsValue;
use wasm_bindgen::{JsCast, JsValue};

use crate::CustomMarkers;

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

/// `{ diagnostics?, toc?, utf16?, customMarkers? }` — `parse`, and galley's
/// `parseText`.
pub fn parse_text_options(
    value: Option<&JsValue>,
    door: &str,
) -> Result<(wire::ParseOptions, Option<Arc<Extensions>>), String> {
    let bag = bag(
        value,
        door,
        &["diagnostics", "toc", "utf16", "customMarkers"],
    )?;
    Ok((
        wire::ParseOptions {
            diagnostics: flag(bag.as_ref(), door, "diagnostics")?,
            toc: flag(bag.as_ref(), door, "toc")?,
            utf16: flag(bag.as_ref(), door, "utf16")?,
        },
        custom_markers(bag.as_ref(), door)?,
    ))
}

/// `{ utf16?, customMarkers? }` — `resolve`, and galley's `resolveOf`.
pub fn resolve_text_options(
    value: Option<&JsValue>,
    door: &str,
) -> Result<(bool, Option<Arc<Extensions>>), String> {
    let bag = bag(value, door, &["utf16", "customMarkers"])?;
    Ok((
        flag(bag.as_ref(), door, "utf16")?,
        custom_markers(bag.as_ref(), door)?,
    ))
}

/// `{ customMarkers? }` — the doors whose only option is the list they lex by.
pub fn lex_options(value: Option<&JsValue>, door: &str) -> Result<Option<Arc<Extensions>>, String> {
    let bag = bag(value, door, &["customMarkers"])?;
    custom_markers(bag.as_ref(), door)
}

/// `{ relaxZPrefix? }` — `setCustomMarkers` and `new CustomMarkers`.
pub fn custom_marker_options(
    value: Option<&JsValue>,
    door: &str,
) -> Result<ExtensionOptions, String> {
    let bag = bag(value, door, &["relaxZPrefix"])?;
    Ok(ExtensionOptions {
        relax_z_prefix: flag(bag.as_ref(), door, "relaxZPrefix")?,
    })
}

/// The `customMarkers` key off a checked bag: the list its handle holds, or
/// `None` when absent.
///
/// Read through the handle's `__handoff` getter, which answers a second
/// handle on the same list: converting the caller's own would free it.
pub fn custom_markers(
    bag: Option<&JsValue>,
    door: &str,
) -> Result<Option<Arc<Extensions>>, String> {
    let Some(value) = get(bag, "customMarkers") else {
        return Ok(None);
    };
    let refused = || format!("{door}: customMarkers must be a live CustomMarkers");
    let handed = Reflect::get(&value, &JsValue::from_str("__handoff")).map_err(|_| refused())?;
    CustomMarkers::try_from_js_value(handed)
        .map(|markers| Some(Arc::clone(markers.list())))
        .map_err(|_| refused())
}

/// The list a door lexes by: the one it was given, or the installed one.
pub fn or_installed(markers: Option<Arc<Extensions>>) -> Arc<Extensions> {
    markers.unwrap_or_else(usfm_onion::extensions::current)
}

/// `a`, `a or b`, `a, b or c` — the keys a refusal lists.
fn expected(known: &[&str]) -> String {
    match known {
        [] => "no options".to_string(),
        [only] => (*only).to_string(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}
