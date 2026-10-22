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

use std::sync::{Arc, Mutex, PoisonError};

use js_sys::{Array, Object, Reflect};
use usfm_onion::extensions::{ExtensionCategory, ExtensionOptions, Extensions};
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

/// `{ relaxZPrefix? }` — `customMarkers`.
pub fn custom_marker_options(
    value: Option<&JsValue>,
    door: &str,
) -> Result<ExtensionOptions, String> {
    let bag = bag(value, door, &["relaxZPrefix"])?;
    Ok(ExtensionOptions {
        relax_z_prefix: flag(bag.as_ref(), door, "relaxZPrefix")?,
    })
}

/// The `customMarkers` key off a checked bag: the list it declares, or `None`
/// when absent.
///
/// ```text
/// { customMarkers: customMarkers([{ name: "zmyf", category: "footnote" }]) }  →  that list
/// { customMarkers: { markers: [], relaxZPrefix: false } }                     →  the empty list
/// { customMarkers: "zmyf" }                  →  throws: mask: customMarkers must be a CustomMarkers object
/// { customMarkers: { markers: [{ name: "nope", category: "char" }], relaxZPrefix: false } }
///                                            →  throws: mask: customMarkers: "nope": name does not start with z
/// ```
///
/// The object is plain data and its brand is the compiler's, so every field is
/// checked here, and a list the registry would not keep whole is refused.
pub fn custom_markers(
    bag: Option<&JsValue>,
    door: &str,
) -> Result<Option<Arc<Extensions>>, String> {
    let Some(value) = get(bag, "customMarkers") else {
        return Ok(None);
    };
    let (list, opts) = read_custom_markers(&value, door)?;
    interned(door, list, &opts).map(Some)
}

/// A `CustomMarkers` object's `markers` and `relaxZPrefix`, shape-checked.
pub fn read_custom_markers(
    value: &JsValue,
    door: &str,
) -> Result<(Vec<(String, String)>, ExtensionOptions), String> {
    let refused = || format!("{door}: customMarkers must be a CustomMarkers object");
    if !value.is_object() || Array::is_array(value) {
        return Err(refused());
    }
    let known = ["markers", "relaxZPrefix", "malformed"];
    for key in Object::keys(value.unchecked_ref::<Object>()).iter() {
        let key = key.as_string().unwrap_or_default();
        if !known.contains(&key.as_str()) {
            return Err(format!(
                "{door}: customMarkers has unknown key {key:?}; expected {}",
                expected(&known)
            ));
        }
    }
    let markers = get(Some(value), "markers").ok_or_else(refused)?;
    let relax_z_prefix = get(Some(value), "relaxZPrefix")
        .and_then(|relax| relax.as_bool())
        .ok_or_else(|| format!("{door}: customMarkers.relaxZPrefix must be a boolean"))?;
    let list = entries(&markers, &format!("{door}: customMarkers.markers"), true)?
        .into_iter()
        .map(|entry| (entry.name, entry.category))
        .collect();
    Ok((list, ExtensionOptions { relax_z_prefix }))
}

/// One entry of a host's list. `description` and `attributes` ride along for
/// the host to show; lexing reads only the name and category.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub category: String,
    pub description: String,
    pub attributes: Vec<String>,
}

impl From<(String, String)> for Entry {
    fn from((name, category): (String, String)) -> Self {
        Self {
            name,
            category,
            ..Self::default()
        }
    }
}

/// An array of `{ name, category, description?, attributes? }`. `strict` also
/// refuses any other key and a category word outside the spec's.
pub fn entries(value: &JsValue, at: &str, strict: bool) -> Result<Vec<Entry>, String> {
    if !Array::is_array(value) {
        return Err(format!("{at} must be an array"));
    }
    let known = ["name", "category", "description", "attributes"];
    let mut out = Vec::new();
    for (i, entry) in value.unchecked_ref::<Array>().iter().enumerate() {
        if !entry.is_object() {
            return Err(format!("{at}[{i}] must be an object"));
        }
        if strict {
            for key in Object::keys(entry.unchecked_ref::<Object>()).iter() {
                let key = key.as_string().unwrap_or_default();
                if !known.contains(&key.as_str()) {
                    return Err(format!(
                        "{at}[{i}] has unknown key {key:?}; expected {}",
                        expected(&known)
                    ));
                }
            }
        }
        let read = |key: &str| {
            Reflect::get(&entry, &JsValue::from_str(key))
                .ok()
                .filter(|v| !v.is_undefined())
        };
        let field = |key: &str| {
            read(key)
                .and_then(|found| found.as_string())
                .ok_or_else(|| format!("{at}[{i}].{key} must be a string"))
        };
        let (name, category) = (field("name")?, field("category")?);
        if strict && ExtensionCategory::parse(&category).is_none() {
            return Err(format!(
                "{at}[{i}].category {category:?} is not a category word"
            ));
        }
        let description = match read("description") {
            None => String::new(),
            Some(_) => field("description")?,
        };
        let attributes = match read("attributes") {
            None => Vec::new(),
            Some(list) if Array::is_array(&list) => list
                .unchecked_ref::<Array>()
                .iter()
                .map(|attribute| attribute.as_string())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| format!("{at}[{i}].attributes must be an array of strings"))?,
            Some(_) => return Err(format!("{at}[{i}].attributes must be an array of strings")),
        };
        out.push(Entry {
            name,
            category,
            description,
            attributes,
        });
    }
    Ok(out)
}

/// The lists the doors were last handed, newest first.
static INTERNED: Mutex<Vec<(Key, Arc<Extensions>)>> = Mutex::new(Vec::new());
const INTERNED_LISTS: usize = 8;

/// A list as the registry would read it: order and duplicates aside, which a
/// clean list has none of.
type Key = (bool, Vec<(String, String)>);

/// The registry for a list that must be kept whole: one `Arc` for every equal
/// list, so a host that passes the same object call after call resolves it
/// once.
pub fn interned(
    door: &str,
    list: Vec<(String, String)>,
    opts: &ExtensionOptions,
) -> Result<Arc<Extensions>, String> {
    let mut key: Key = (opts.relax_z_prefix, list);
    key.1.sort_unstable();
    let mut table = INTERNED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(at) = table.iter().position(|(seen, _)| *seen == key) {
        let hit = table.remove(at);
        let found = Arc::clone(&hit.1);
        table.insert(0, hit);
        return Ok(found);
    }
    let (extensions, reports) =
        crate::judged(key.1.iter().cloned().map(Entry::from).collect(), opts);
    if let Some(report) = reports.first() {
        return Err(format!(
            "{door}: customMarkers: {:?}: {}",
            report.name.as_deref().unwrap_or(""),
            report.reason
        ));
    }
    let extensions = Arc::new(extensions);
    table.insert(0, (key, Arc::clone(&extensions)));
    table.truncate(INTERNED_LISTS);
    Ok(extensions)
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
