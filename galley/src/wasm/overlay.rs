//! The overlay's doorway: options in as a JS object, rows out as JSON.
//!
//! ```text
//! galley.skeleton("ref/GEN.usfm")            // the whole truth, for drawing
//! galley.overlay("books/GEN.usfm", "ref/GEN.usfm")   // → Edits, onion-wasm's own class
//! galley.overlayReport("books/GEN.usfm", "ref/GEN.usfm")
//! ```
//!
//! Serde lives here and nowhere else: `galley::overlay` computes plain Rust
//! structs, this module serializes them, and the native crate links no
//! serializer at all. Every offset is a UTF-8 byte offset in the named book
//! unless the caller asks for `utf16`, the same opt-in `parse` and `find`
//! take.

use js_sys::{Array, Reflect};
use onion_wasm::Edits;
use wasm_bindgen::prelude::*;

use crate::overlay::{
    BlockAddress, Equivalent, Overlay, OverlayOptions, OverlayReport, Placement, Scope, Skeleton,
};
use mise::utf16::Utf16Table;

/// The transaction as the wire shape, offsets converted if asked.
pub(super) fn edits(overlay: &Overlay, table: Option<&Utf16Table>) -> Edits {
    let mut spans = Vec::with_capacity(overlay.edits.len() * 2);
    let mut lens = Vec::with_capacity(overlay.edits.len());
    let mut text = String::new();
    for edit in &overlay.edits {
        spans.extend_from_slice(&[at(edit.from, table), at(edit.to, table)]);
        // `lens` slices `text`, so it counts in whatever unit the spans do.
        lens.push(match table {
            None => edit.insert.as_bytes().len() as u32,
            Some(_) => mise::utf16::utf16_len(edit.insert.as_bytes()),
        });
        text.push_str(edit.insert.as_str());
    }
    Edits::from_parts(spans, lens, text)
}

/// `{ markers?, scope?, utf16? }` off a JS object, with a misspelled key
/// reading as absent and a wrong TYPE reading as an error.
pub(super) fn options(value: &JsValue) -> Result<(OverlayOptions, bool), JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok((OverlayOptions::default(), false));
    }
    if !value.is_object() {
        return Err(JsError::new("options must be an object"));
    }
    let markers = match get(value, "markers") {
        Some(list) => {
            let list = Array::from(&list);
            let mut names = Vec::with_capacity(list.length() as usize);
            for name in list.iter() {
                names.push(
                    name.as_string()
                        .ok_or_else(|| JsError::new("every entry of markers must be a string"))?,
                );
            }
            Some(names)
        }
        None => None,
    };
    let scope = match get(value, "scope") {
        None => None,
        Some(scope) => match (get(&scope, "sid"), get(&scope, "chapter")) {
            (Some(sid), _) => Some(Scope::Sid(
                sid.as_string()
                    .ok_or_else(|| JsError::new("scope.sid must be a string"))?,
            )),
            (None, Some(chapter)) => Some(Scope::Chapter(
                chapter
                    .as_f64()
                    .filter(|n| *n >= 1.0 && *n <= f64::from(u16::MAX))
                    .ok_or_else(|| JsError::new("scope.chapter must be a chapter number"))?
                    as u16,
            )),
            (None, None) => {
                return Err(JsError::new("scope must name a chapter or a sid"));
            }
        },
    };
    let utf16 = get(value, "utf16")
        .and_then(|flag| flag.as_bool())
        .is_some_and(|flag| flag);
    Ok((OverlayOptions { markers, scope }, utf16))
}

/// One property, or `None` for absent, `undefined` and `null` alike.
///
/// `pub(super)`: the find doors' own options parser (`wasm/find.rs`) shares
/// it rather than reimplementing the same `Reflect::get` dance.
pub(super) fn get(value: &JsValue, key: &str) -> Option<JsValue> {
    Reflect::get(value, &JsValue::from_str(key))
        .ok()
        .filter(|found| !found.is_undefined() && !found.is_null())
}

/// `{ sid, where, ordinal, marker }` off a JS object. Every field is
/// required: the marker is the check that says the address is not stale.
pub(super) fn address(value: &JsValue) -> Result<BlockAddress, JsError> {
    let sid = get(value, "sid")
        .and_then(|sid| sid.as_string())
        .ok_or_else(|| JsError::new("an address needs a sid"))?;
    let placement = match get(value, "where").and_then(|at| at.as_string()).as_deref() {
        Some("leading") => Placement::Leading,
        Some("inside") => Placement::Inside,
        _ => return Err(JsError::new("where must be \"leading\" or \"inside\"")),
    };
    let ordinal = get(value, "ordinal")
        .and_then(|n| n.as_f64())
        .filter(|n| *n >= 1.0)
        .ok_or_else(|| JsError::new("ordinal counts from one"))? as u32;
    let marker = get(value, "marker")
        .and_then(|name| name.as_string())
        .ok_or_else(|| JsError::new("an address names the marker it was taken from"))?;
    Ok(BlockAddress {
        sid,
        placement,
        ordinal,
        marker,
    })
}

/// One offset in the coordinate space the caller asked for.
fn at(byte: u32, table: Option<&Utf16Table>) -> u32 {
    table.map_or(byte, |table| table.to_utf16(byte))
}

pub(super) fn rebase_skeleton(skeleton: &mut Skeleton, table: Option<&Utf16Table>) {
    let Some(table) = table else { return };
    for verse in &mut skeleton.verses {
        verse.from = table.to_utf16(verse.from);
        verse.to = table.to_utf16(verse.to);
        verse.text_from = table.to_utf16(verse.text_from);
        verse.text_to = table.to_utf16(verse.text_to);
    }
    for block in &mut skeleton.blocks {
        block.from = table.to_utf16(block.from);
        block.to = table.to_utf16(block.to);
        block.end = table.to_utf16(block.end);
    }
}

pub(super) fn rebase_report(report: &mut OverlayReport, table: Option<&Utf16Table>) {
    let Some(table) = table else { return };
    for row in &mut report.inserted {
        row.at = table.to_utf16(row.at);
    }
    for row in &mut report.removed {
        row.from = table.to_utf16(row.from);
        row.to = table.to_utf16(row.to);
    }
}

pub(super) fn rebase_equivalent(answer: &mut Equivalent, table: Option<&Utf16Table>) {
    let Some(table) = table else { return };
    match answer {
        Equivalent::Found { found } => {
            found.from = table.to_utf16(found.from);
            found.to = table.to_utf16(found.to);
            found.end = table.to_utf16(found.end);
        }
        Equivalent::Absent { insert_at, .. } => *insert_at = table.to_utf16(*insert_at),
        Equivalent::Unpaired { .. } => {}
    }
}

/// The one serialization, so every door fails the same way.
pub(super) fn json<T: serde::Serialize>(value: &T) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|error| JsError::new(&error.to_string()))
}
