//! The doors `bento.ts` stands on: raw, JSON and bytes. Use `bento.ts`.
//!
//! ```text
//! find(names)                                   → JSON {name, format} | undefined
//! read(name, bytes)                             → JSON Read
//! check(name, bytes, repoName?, files?)         → JSON ManifestFinding[]
//! createBible(projectJson, bookBytes[], formats) → Map<name, bytes>
//! edit(existing, changeJson, addBytes[], checksumBytes[]) → {files: Map<name, bytes>, findings: JSON}
//! fingerprint(bytes)                            → JSON {md5, size}
//! ```
//!
//! Structured values cross as JSON (`js.rs` writes `bento.ts`'s types), and
//! files as `Uint8Array`s, in a `Map` by name or an array beside a JSON list.

use js_sys::{Array, Map, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::tree::{Node, from_json};
use crate::{Change, Format, Repo, js};

fn bytes_of(array: &Array) -> Vec<Vec<u8>> {
    array.iter().map(|v| Uint8Array::new(&v).to_vec()).collect()
}

fn files_of(map: &Map) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    map.for_each(&mut |value, key| {
        out.push((
            key.as_string().unwrap_or_default(),
            Uint8Array::new(&value).to_vec(),
        ));
    });
    out
}

fn to_map(files: &[(String, Vec<u8>)]) -> Map {
    let map = Map::new();
    for (name, bytes) in files {
        map.set(
            &JsValue::from_str(name),
            &Uint8Array::from(bytes.as_slice()),
        );
    }
    map
}

fn parse(json: &str) -> Result<Node, JsError> {
    from_json(json.as_bytes()).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen(js_name = find)]
pub fn find_js(names: Vec<String>) -> Option<String> {
    crate::find(&names).map(|(name, format)| {
        Node::map([
            ("name", Node::text(name)),
            ("format", Node::text(js::format_name(format))),
        ])
        .to_json()
    })
}

#[wasm_bindgen(js_name = read)]
pub fn read_js(name: &str, bytes: &[u8]) -> String {
    match crate::format_of(name) {
        Some(format) => {
            let (read, document) = crate::read_with_document(format, bytes);
            js::read(&read, Some(format), document).to_json()
        }
        None => js::read(&crate::read(name, bytes), None, None).to_json(),
    }
}

#[wasm_bindgen(js_name = check)]
pub fn check_js(name: &str, bytes: &[u8], repo_name: Option<String>, files: Option<Map>) -> String {
    let read = crate::read(name, bytes);
    let files = files.as_ref().map(files_of);
    let repo = Repo {
        name: repo_name.as_deref(),
        files: files.as_deref(),
    };
    js::findings(&crate::check(&read, &repo)).to_json()
}

#[wasm_bindgen(js_name = createBible)]
pub fn create_bible_js(
    project: &str,
    book_bytes: Array,
    formats: Vec<String>,
) -> Result<Map, JsError> {
    let node = parse(project)?;
    let books = js::books(&node, "books", bytes_of(&book_bytes)).map_err(|e| JsError::new(&e))?;
    let project = js::project(&node, books).map_err(|e| JsError::new(&e))?;
    let formats: Vec<Format> = js::formats(&formats).map_err(|e| JsError::new(&e))?;
    let files =
        crate::create_bible(&project, &formats).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(to_map(&files))
}

#[wasm_bindgen(js_name = edit)]
pub fn edit_js(
    existing: Map,
    change: &str,
    add_bytes: Array,
    checksum_bytes: Array,
) -> Result<Object, JsError> {
    let node = parse(change)?;
    let add_books =
        js::books(&node, "addBooks", bytes_of(&add_bytes)).map_err(|e| JsError::new(&e))?;
    let paths: Vec<String> = match node.get("checksums") {
        Some(Node::List(items)) => items
            .iter()
            .map(|p| match p {
                Node::String(s) => Ok(s.clone()),
                other => Err(JsError::new(&format!(
                    "a checksum path must be text, not {}",
                    other.kind()
                ))),
            })
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    let checksum_bytes = bytes_of(&checksum_bytes);
    if paths.len() != checksum_bytes.len() {
        return Err(JsError::new(
            "checksums: paths and byte arrays differ in number",
        ));
    }
    let change = Change {
        add_books,
        canon: js::canon_of(&node).map_err(|e| JsError::new(&e))?,
        checksums: paths.into_iter().zip(checksum_bytes).collect(),
    };
    let edited =
        crate::edit(&files_of(&existing), &change).map_err(|e| JsError::new(&e.to_string()))?;
    let out = Object::new();
    Reflect::set(&out, &"files".into(), &to_map(&edited.files))
        .map_err(|_| JsError::new("set files"))?;
    Reflect::set(
        &out,
        &"findings".into(),
        &js::findings(&edited.findings).to_json().into(),
    )
    .map_err(|_| JsError::new("set findings"))?;
    Ok(out)
}

#[wasm_bindgen(js_name = fingerprint)]
pub fn fingerprint_js(bytes: &[u8]) -> String {
    let print = crate::fingerprint(bytes);
    Node::map([
        ("md5", Node::text(print.md5)),
        ("size", Node::Number(print.size.to_string())),
    ])
    .to_json()
}
