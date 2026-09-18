//! The mask doors' shared options object.
//!
//! ```text
//! galley.mask("books/MRK.usfm");                          // verse text, bytes
//! galley.mask("books/MRK.usfm", { utf16: true });         // the editor's unit
//! galley.maskOf(text, { recipe: "structure" });           // over loose text
//! ```
//!
//! One parser for both doors ([`Galley::mask`](super::Galley::mask) and
//! [`Galley::mask_of`](super::Galley::mask_of)): an absent key reads as its
//! default, a wrong TYPE names the key, and an unknown recipe names the three
//! that exist rather than falling back to any of them.

use wasm_bindgen::prelude::*;

use super::overlay::get;
use crate::mask::Recipe;

/// `mask` and `maskOf`'s options, defaulted and type-checked.
#[derive(Default)]
pub(super) struct MaskOptions {
    pub recipe: Recipe,
    pub utf16: bool,
}

/// `{ recipe?, utf16? }` off a JS object.
pub(super) fn options(value: &JsValue) -> Result<MaskOptions, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(MaskOptions::default());
    }
    if !value.is_object() {
        return Err(JsError::new("options must be an object"));
    }
    let recipe = match get(value, "recipe") {
        None => Recipe::default(),
        Some(name) => {
            let name = name
                .as_string()
                .ok_or_else(|| JsError::new("recipe must be a string"))?;
            match name.as_str() {
                "verseText" => Recipe::VerseText,
                "structure" => Recipe::Structure,
                "text" => Recipe::Text,
                other => {
                    return Err(JsError::new(&format!(
                        "unknown mask recipe {other:?}; \
                         expected \"verseText\", \"structure\" or \"text\""
                    )));
                }
            }
        }
    };
    let utf16 = match get(value, "utf16") {
        None => false,
        Some(flag) => flag
            .as_bool()
            .ok_or_else(|| JsError::new("utf16 must be a boolean"))?,
    };
    Ok(MaskOptions { recipe, utf16 })
}
