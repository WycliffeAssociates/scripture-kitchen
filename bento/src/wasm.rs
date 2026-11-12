//! The doors `bento.ts` stands on: raw, JSON and bytes. Use `bento.ts`.
//!
//! ```text
//! find(names)                                   → JSON {name, format} | undefined
//! read(name, bytes)                             → JSON Read
//! check(name, bytes, repoName?, files?)         → JSON ManifestFinding[]
//! createBible(projectJson, bookBytes[], formats) → Map<name, bytes>
//! edit(existing, changeJson, addBytes[], checksumBytes[]) → {files: Map<name, bytes>, findings: JSON}
//! new Manifests(existing, canon?)               a handle; each call → JSON ManifestFinding[]
//!   .addBook(bookJson, bytes) .renameBook(namesJson) .updateChecksum(path, bytes)
//!   .names() → JSON {file: {BOOK: {short?, long?, abbr?}}}    .finish() → as edit's
//! fingerprint(bytes)                            → JSON {md5, size}
//! writerProject(bytes, repo?)                   → JSON WriterProject | undefined
//! ```
//!
//! Structured values cross as JSON (`js.rs` writes `bento.ts`'s types), and
//! files as `Uint8Array`s, in a `Map` by name or an array beside a JSON list.

use js_sys::{Array, Map, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::tree::{Node, from_json};
use crate::{BookNames, Change, Format, Repo, js};

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

/// A BTT Writer `manifest.json`'s book project, or `undefined` when it
/// doesn't name a book, a language and a resource. `repo` is the repo or
/// folder name (`tmf_3jn_text_reg`): when the manifest names no resource,
/// Writer's naming supplies it.
#[wasm_bindgen(js_name = writerProject)]
pub fn writer_project_js(bytes: &[u8], repo: Option<String>) -> Option<String> {
    let manifest = crate::bttwriter::read(bytes).ok()?;
    let project = manifest.project(repo.as_deref())?;
    Some(Node::Map(js::writer_project(&project)).to_json())
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
    edited_js(&edited)
}

fn edited_js(edited: &crate::Edited) -> Result<Object, JsError> {
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

/// A project's manifests, open for editing: `bento.ts`'s `open`.
#[wasm_bindgen(js_name = Manifests)]
pub struct ManifestsJs(crate::Manifests);

fn findings_js(found: &[crate::ManifestFinding]) -> String {
    js::findings(found).to_json()
}

fn book_key(node: &Node) -> Result<mise::books::BookKey, JsError> {
    let code = js::want_text(node, "book").map_err(|e| JsError::new(&e))?;
    crate::book_of(&code).ok_or_else(|| JsError::new(&format!("{code} isn't a book of canon's table")))
}

#[wasm_bindgen(js_class = Manifests)]
impl ManifestsJs {
    /// `canon`: `"protestant66"` (the default) or `"withDeuterocanon"`.
    #[wasm_bindgen(constructor)]
    pub fn open(existing: Map, canon: Option<String>) -> Result<ManifestsJs, JsError> {
        let canon = js::canon_of(&Node::map([(
            "canon",
            canon.map_or(Node::Null, Node::text),
        )]))
        .map_err(|e| JsError::new(&e))?;
        crate::Manifests::open(&files_of(&existing), canon)
            .map(ManifestsJs)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// `book`: `{book: "MRK", title: "Mak"}`.
    #[wasm_bindgen(js_name = addBook)]
    pub fn add_book(&mut self, book: &str, bytes: Vec<u8>) -> Result<String, JsError> {
        let node = Node::map([("books", Node::List(vec![parse(book)?]))]);
        let mut books = js::books(&node, "books", vec![bytes]).map_err(|e| JsError::new(&e))?;
        let found = self
            .0
            .add_book(books.remove(0))
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(findings_js(&found))
    }

    /// `names`: `{book: "JHN", short?, long?, abbr?}`.
    #[wasm_bindgen(js_name = renameBook)]
    pub fn rename_book(&mut self, names: &str) -> Result<String, JsError> {
        let node = parse(names)?;
        let name = |key: &str| match node.get(key) {
            Some(Node::String(s)) => Ok(Some(s.clone())),
            None | Some(Node::Null) => Ok(None),
            Some(other) => Err(JsError::new(&format!("{key} must be text, not {}", other.kind()))),
        };
        let found = self
            .0
            .rename_book(BookNames {
                book: book_key(&node)?,
                short: name("short")?,
                long: name("long")?,
                abbr: name("abbr")?,
            })
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(findings_js(&found))
    }

    #[wasm_bindgen(js_name = updateChecksum)]
    pub fn update_checksum(&mut self, path: &str, bytes: &[u8]) -> String {
        findings_js(&self.0.update_checksum(path, bytes))
    }

    pub fn names(&self) -> String {
        Node::map(self.0.names().into_iter().map(|(file, books)| {
            (
                file,
                Node::map(books.into_iter().map(|b| {
                    let fields = [("short", b.short), ("long", b.long), ("abbr", b.abbr)]
                        .into_iter()
                        .filter_map(|(k, v)| v.map(|v| (k, Node::text(v))));
                    (b.book.to_string(), Node::map(fields))
                })),
            )
        }))
        .to_json()
    }

    /// Ends the handle: `{files, findings}`, as `edit` returns.
    pub fn finish(self) -> Result<Object, JsError> {
        edited_js(&self.0.finish())
    }
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
