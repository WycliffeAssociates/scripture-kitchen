//! The doors `docx.ts` stands on. Use `docx.ts`.
//!
//! ```text
//! exportBook(usfm, rtl, font?)     → { docx: Uint8Array, json: '{"book", "unsupported"}' }
//! template()                       → Uint8Array, the .dotx
//! readDocument(bytes)              → DocumentReading, or throws when not a .docx/.odt
//! reading.render(settingsJson)     → JSON Imported
//! ```
//!
//! Structured values cross as JSON in the shapes `docx.ts` declares: the
//! report and comment threads are `@sefer/document-import`'s, with our own
//! fields beside them.

use js_sys::{Object, Reflect, Uint8Array};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::{
    CommentThread, ExportOptions, Front, Headings, ImportReport, Imported, Keep, KeepDrop, Reading,
    Settings, SkipReason, Source, VerseMode, VersePoint,
};

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

#[wasm_bindgen]
pub fn template() -> Vec<u8> {
    crate::template()
}

/// A document read once, rendered as often as its settings change.
#[wasm_bindgen]
pub struct DocumentReading {
    inner: Reading,
}

#[wasm_bindgen(js_name = readDocument)]
pub fn read_document(bytes: &[u8]) -> Result<DocumentReading, JsError> {
    let inner = crate::read(bytes).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(DocumentReading { inner })
}

#[wasm_bindgen]
impl DocumentReading {
    pub fn render(&self, settings: &str) -> Result<String, JsError> {
        let settings = settings_of(settings).map_err(|e| JsError::new(&e))?;
        Ok(imported_json(&crate::render(&self.inner, &settings)).to_string())
    }
}

fn settings_of(json: &str) -> Result<Settings, String> {
    let v: Value = serde_json::from_str(if json.is_empty() { "{}" } else { json })
        .map_err(|e| e.to_string())?;
    let text = |k: &str| v.get(k).and_then(Value::as_str);
    let bad = |k: &str, got: &str| format!("settings.{k}: unknown value \"{got}\"");
    let keep = |k: &str| match text(k) {
        None | Some("keep") => Ok(Keep::Keep),
        Some("unwrap") => Ok(Keep::Unwrap),
        Some(other) => Err(bad(k, other)),
    };
    let keep_drop = |k: &str| match text(k) {
        None | Some("keep") => Ok(KeepDrop::Keep),
        Some("drop") => Ok(KeepDrop::Drop),
        Some(other) => Err(bad(k, other)),
    };
    let mut s = Settings {
        book: text("book").map(str::to_owned),
        verses: match text("verses") {
            None | Some("auto") => None,
            Some(m) => Some(VerseMode::parse(m).ok_or_else(|| bad("verses", m))?),
        },
        versification: text("versification").map(str::to_owned),
        poetry: keep("poetry")?,
        headings: match text("headings") {
            None | Some("keep") => Headings::Keep,
            Some("rem") => Headings::Rem,
            Some("drop") => Headings::Drop,
            Some(other) => return Err(bad("headings", other)),
        },
        front: match text("front") {
            None | Some("rem") => Front::Rem,
            Some("mt") => Front::Mt,
            Some("drop") => Front::Drop,
            Some(other) => return Err(bad("front", other)),
        },
        notes: keep_drop("notes")?,
        italic: keep("italic")?,
        bold: keep("bold")?,
        lists: keep("lists")?,
        style_markers: keep("styleMarkers")?,
        comments: keep_drop("comments")?,
        ..Settings::default()
    };
    if let Some(words) = v.get("chapterWords").and_then(Value::as_array) {
        s.chapter_words = words
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    if let Some(markers) = v.get("markers").and_then(Value::as_object) {
        s.markers = markers
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
            .collect();
    }
    Ok(s)
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
    let f = &r.found;
    let c = &r.candidates;
    json!({
        "source": if r.passthrough { "usfm" } else { "document" },
        "verses": r.verses.name(),
        "detected": r.detected.name(),
        "candidates": {
            "superscript": c.superscript, "alone": c.alone, "marked": c.marked,
            "lineStart": c.line_start, "inline": c.inline,
        },
        "chapters": r.chapters.iter().map(|ch| json!({
            "chapter": ch.chapter, "verses": ch.verses, "last": ch.last, "missing": ch.missing,
        })).collect::<Vec<_>>(),
        "found": {
            "poetry": f.poetry, "headings": f.headings, "front": f.front, "notes": f.notes,
            "italic": f.italic, "bold": f.bold, "lists": f.lists, "tables": f.tables,
            "styleMarkers": f.style_markers, "comments": f.comments, "trackedChanges": f.tracked_changes,
            "lineBreaks": f.line_breaks, "blankParagraphs": f.blank_paragraphs,
        },
        "skipped": r.skipped.iter().map(|s| json!({
            "ref": s.r#ref,
            "number": s.number,
            "reason": match s.reason {
                SkipReason::Sequence => "sequence",
                SkipReason::Versification => "versification",
                SkipReason::Thousands => "thousands",
            },
        })).collect::<Vec<_>>(),
        "guesses": r.guesses.iter().map(|g| json!({
            "ref": g.r#ref, "marker": g.marker, "reason": g.reason, "excerpt": g.excerpt,
        })).collect::<Vec<_>>(),
        "exact": r.is_exact(),
        "guessedParagraphs": r.guessed_paragraphs,
        "unknownCharacterStyles": r.unknown_character_styles,
        "unknownNoteStyles": r.unknown_note_styles,
        "trackedChanges": r.tracked_changes,
        "missingNotes": r.missing_notes,
    })
}

fn point_json(p: &Option<VersePoint>) -> Value {
    p.as_ref().map_or(
        Value::Null,
        |p| json!({ "ref": p.r#ref, "offset": p.offset }),
    )
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
