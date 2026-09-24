//! The find doors' shared options object.
//!
//! ```text
//! galley.find("books/MRK.usfm", "wept", { caseSensitive: true });
//! galley.findAll("God", { wholeWord: true, limit: 200, scope: "all" });
//! ```
//!
//! One parser for both doors ([`Galley::find`](super::Galley::find) and
//! [`Galley::find_all`](super::Galley::find_all)); `scope` is read here too,
//! and it is `find`'s job — not this module's — to refuse it when present,
//! since only `findAll` takes more than one book.

use wasm_bindgen::prelude::*;

use super::overlay::get;

/// `find` and `findAll`'s options, defaulted and type-checked.
#[derive(Default)]
pub(super) struct FindOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub limit: u32,
    pub scope: Option<String>,
}

/// `{ caseSensitive?, wholeWord?, limit?, scope? }` off a JS object, with an
/// absent key reading as its default and a wrong TYPE or an unknown key
/// reading as an error naming the key.
pub(super) fn options(value: Option<&JsValue>, door: &str) -> Result<FindOptions, JsError> {
    let bag = onion_wasm::options::bag(
        value,
        door,
        &["caseSensitive", "wholeWord", "limit", "scope"],
    )
    .map_err(|e| JsError::new(&e))?;
    let Some(value) = bag.as_ref() else {
        return Ok(FindOptions::default());
    };
    let case_sensitive = match get(value, "caseSensitive") {
        None => false,
        Some(flag) => flag
            .as_bool()
            .ok_or_else(|| JsError::new("caseSensitive must be a boolean"))?,
    };
    let whole_word = match get(value, "wholeWord") {
        None => false,
        Some(flag) => flag
            .as_bool()
            .ok_or_else(|| JsError::new("wholeWord must be a boolean"))?,
    };
    let limit = match get(value, "limit") {
        None => 0,
        Some(n) => n
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= f64::from(u32::MAX))
            .ok_or_else(|| JsError::new("limit must be a non-negative integer"))?
            as u32,
    };
    let scope = match get(value, "scope") {
        None => None,
        Some(scope) => Some(
            scope
                .as_string()
                .ok_or_else(|| JsError::new("scope must be a string"))?,
        ),
    };
    Ok(FindOptions {
        case_sensitive,
        whole_word,
        limit,
        scope,
    })
}
