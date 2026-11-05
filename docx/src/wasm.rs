//! The doors `docx.ts` stands on. Use `docx.ts`.
//!
//! ```text
//! exportBook(usfm, rtl, font?)  → { docx: Uint8Array, json: '{"book", "unsupported"}' }
//! importDocx(bytes)             → JSON Imported, or throws when the file is not a .docx
//! ```
//!
//! Structured values cross as JSON in the shapes `docx.ts` declares; comment
//! threads are exactly `@sefer/document-import`'s.

use js_sys::{Object, Reflect, Uint8Array};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::{CommentThread, ExportOptions, ImportReport, Imported, Source, VersePoint};

#[wasm_bindgen(js_name = exportBook)]
pub fn export_book(usfm: &str, rtl: bool, font: Option<String>) -> Result<Object, JsError> {
    let out = crate::export(usfm, &ExportOptions { rtl, font });
    let meta = json!({
        "book": out.book,
        "unsupported": out.unsupported.iter().map(|u| json!({"at": u.at, "what": u.what})).collect::<Vec<_>>(),
    });
    let result = Object::new();
    let set = |k: &str, v: &JsValue| Reflect::set(&result, &JsValue::from_str(k), v).map(|_| ());
    set("docx", &Uint8Array::from(out.docx.as_slice())).map_err(|_| JsError::new("set docx"))?;
    set("json", &JsValue::from_str(&meta.to_string())).map_err(|_| JsError::new("set json"))?;
    Ok(result)
}

#[wasm_bindgen(js_name = importDocx)]
pub fn import_docx(bytes: &[u8]) -> Result<String, JsError> {
    let imported = crate::import(bytes).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(imported_json(&imported).to_string())
}

fn imported_json(i: &Imported) -> Value {
    json!({
        "books": i.books.iter().map(|b| json!({
            "code": b.code,
            "usfm": b.usfm,
            "source": b.source.as_ref().map(source_json),
            "comments": b.comments.iter().map(thread_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "report": report_json(&i.report),
        "properties": i.properties.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
    })
}

fn source_json(s: &Source) -> Value {
    json!({ "gitBlobSha1": s.git_blob_sha1, "xxh3_64": s.xxh3_64 })
}

fn report_json(r: &ImportReport) -> Value {
    json!({
        "unknownParagraphStyles": r.unknown_paragraph_styles,
        "unknownCharacterStyles": r.unknown_character_styles,
        "trackedChanges": r.tracked_changes,
        "missingNotes": r.missing_notes,
    })
}

fn point_json(p: &Option<VersePoint>) -> Value {
    p.as_ref().map_or(Value::Null, |p| json!({ "ref": p.r#ref, "offset": p.offset }))
}

fn thread_json(t: &CommentThread) -> Value {
    json!({
        "ref": t.r#ref,
        "anchorText": t.anchor_text,
        "start": point_json(&t.start),
        "end": point_json(&t.end),
        "comments": t.comments.iter().map(|c| json!({
            "id": c.id,
            "author": c.author,
            "date": c.date,
            "text": c.text,
            "parentId": c.parent_id,
            "resolved": c.resolved,
        })).collect::<Vec<_>>(),
    })
}
