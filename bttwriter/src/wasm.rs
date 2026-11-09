//! The doors `bttwriter.ts` stands on: raw handles, JSON and bytes. Use `bttwriter.ts`.
//!
//! ```text
//! new HistoryWalk(head, shallow)   .wants() → ids   .give(ids, kinds, lengths, bytes)   .finish(source) → WriterHistory
//! WriterHistory                    .project() JSON  .title()  .saves  .first(now)/.last(now) JSON
//!                                  .progress(now) → [at, verses, at, verses, …]  .usfm()
//! replay(histories, now, deltas)   → Replayed | undefined          (the histories are used up)
//! consolidate(histories, now, deltas, projectJson, closingJson, formats) → Replayed & { manifests }
//! stitch(paths, lengths, bytes, book, title?) → usfm   (one book's files, no history: no git)
//! ```
//!
//! The walk is sans-IO: the TS driver asks `wants()`, reads those objects
//! with whatever git it has (isomorphic-git), and `give`s them back packed:
//! ids, a kind byte each (0 commit, 1 tree, 2 blob), lengths, and one buffer.

use js_sys::{Array, Map, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

use bento::tree::Node;

use crate::pack::{Kind, ObjectId, Signature};
use crate::{Closing, ReplayError, ReplayOptions, Replayed, walk};

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn id(hex: &str) -> Result<ObjectId, JsError> {
    ObjectId::from_hex(hex).ok_or_else(|| JsError::new(&format!("{hex} isn't an object id")))
}

fn set(object: &Object, key: &str, value: &JsValue) {
    let _ = Reflect::set(object, &JsValue::from_str(key), value);
}

#[wasm_bindgen(js_name = HistoryWalk)]
pub struct Walk(walk::HistoryWalk);

#[wasm_bindgen(js_class = HistoryWalk)]
impl Walk {
    #[wasm_bindgen(constructor)]
    pub fn new(head: &str, shallow: Vec<String>) -> Result<Walk, JsError> {
        let shallow = shallow
            .iter()
            .map(|h| id(h))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Walk(walk::HistoryWalk::new(id(head)?, &shallow)))
    }

    /// The object ids to read next; empty when the walk is done.
    pub fn wants(&self) -> Vec<String> {
        self.0.wants().iter().map(ToString::to_string).collect()
    }

    /// Hand over objects: their ids, a kind each (0 commit, 1 tree, 2 blob),
    /// their lengths, and their bytes one after another.
    pub fn give(
        &mut self,
        ids: Vec<String>,
        kinds: &[u8],
        lengths: &[u32],
        bytes: &[u8],
    ) -> Result<(), JsError> {
        if ids.len() != kinds.len() || ids.len() != lengths.len() {
            return Err(JsError::new(
                "give: ids, kinds and lengths differ in number",
            ));
        }
        let mut objects = Vec::with_capacity(ids.len());
        let mut at = 0usize;
        for ((hex, &kind), &len) in ids.iter().zip(kinds).zip(lengths) {
            let end = at + len as usize;
            let body = bytes
                .get(at..end)
                .ok_or_else(|| JsError::new("give: lengths run past the bytes"))?;
            let kind = match kind {
                0 => Kind::Commit,
                1 => Kind::Tree,
                2 => Kind::Blob,
                other => {
                    return Err(JsError::new(&format!(
                        "give: kind {other} (0 commit, 1 tree, 2 blob)"
                    )));
                }
            };
            objects.push((id(hex)?, kind, body.to_vec()));
            at = end;
        }
        self.0.give(objects).map_err(err)
    }

    /// The history, once the walk wants nothing more. `source` is `owner/repo`.
    pub fn finish(self, source: &str) -> Result<History, JsError> {
        self.0.finish(source).map(History).map_err(err)
    }
}

/// One book project's saves, held in the module.
#[wasm_bindgen(js_name = WriterHistory)]
pub struct History(walk::WriterHistory);

fn signature(s: &Signature) -> Node {
    Node::map([
        ("name", Node::text(&s.name)),
        ("email", Node::text(&s.email)),
        ("at", Node::Number(s.at.to_string())),
        ("offset", Node::Number(s.offset_minutes.to_string())),
    ])
}

#[wasm_bindgen(js_class = WriterHistory)]
impl History {
    /// JSON: `{book, language: {id, name?, direction}, resource, sources, translators, name?}`.
    pub fn project(&self) -> String {
        let mut e = bento::js::writer_project(&self.0.project);
        if let Some(n) = &self.0.project_name {
            e.push(("name".into(), Node::text(n)));
        }
        Node::Map(e).to_json()
    }

    /// `owner/repo`: where `Writer-Source` says a save came from.
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String {
        self.0.source.clone()
    }

    pub fn title(&self) -> String {
        self.0.title()
    }

    #[wasm_bindgen(getter)]
    pub fn saves(&self) -> usize {
        self.0.saves.len()
    }

    /// JSON `{name, email, at, offset}` of the first save, after clock fixes.
    pub fn first(&self, now: f64) -> Option<String> {
        self.edge(now, true)
    }

    /// JSON `{name, email, at, offset}` of the last save, after clock fixes.
    pub fn last(&self, now: f64) -> Option<String> {
        self.edge(now, false)
    }

    fn edge(&self, now: f64, first: bool) -> Option<String> {
        let clocks = self.0.clocks(now as i64);
        let i = if first {
            0
        } else {
            self.0.saves.len().checked_sub(1)?
        };
        let save = self.0.saves.get(i)?;
        Some(
            signature(&Signature {
                at: clocks[i].author_at,
                ..save.author.clone()
            })
            .to_json(),
        )
    }

    /// `[at, verses, at, verses, …]`: verses present after each save.
    pub fn progress(&self, now: f64) -> Vec<f64> {
        self.0
            .progress(now as i64)
            .into_iter()
            .flat_map(|(at, v)| [at as f64, v as f64])
            .collect()
    }

    /// The book at its last save, as replay renders it.
    pub fn usfm(&self) -> String {
        self.0.usfm()
    }
}

fn replayed(r: Replayed) -> Object {
    let out = Object::new();
    set(&out, "head", &r.head.to_string().into());
    set(&out, "pack", &Uint8Array::from(r.pack.as_slice()));
    set(&out, "idx", &Uint8Array::from(r.idx.as_slice()));
    set(&out, "name", &r.name.into());
    let books = Array::new();
    for b in &r.books {
        let o = Object::new();
        set(&o, "book", &b.book.to_string().into());
        set(&o, "file", &b.file.clone().into());
        set(&o, "title", &b.title.clone().into());
        books.push(&o);
    }
    set(&out, "books", &books);
    set(&out, "commits", &(r.commits as f64).into());
    set(&out, "unchanged", &(r.unchanged as f64).into());
    set(&out, "estimated", &(r.estimated as f64).into());
    let stats = Object::new();
    set(&stats, "objects", &(r.stats.objects as f64).into());
    set(&stats, "deltas", &(r.stats.deltas as f64).into());
    set(&stats, "packBytes", &(r.stats.pack_bytes as f64).into());
    set(&out, "stats", &stats);
    out
}

/// Every history's saves as one line of commits in one pack, or `undefined`
/// when no save changed any book. Uses the histories up.
#[wasm_bindgen]
pub fn replay(histories: Vec<History>, now: f64, deltas: bool) -> Result<Option<Object>, JsError> {
    let histories: Vec<walk::WriterHistory> = histories.into_iter().map(|h| h.0).collect();
    match crate::replay(
        &histories,
        &ReplayOptions {
            now: now as i64,
            deltas,
        },
    ) {
        Ok(r) => Ok(Some(replayed(r))),
        Err(ReplayError::NothingChanged) => Ok(None),
        Err(e) => Err(err(e)),
    }
}

fn signature_from(node: &Node, key: &str) -> Result<Signature, String> {
    let s = bento::js::want(node, key)?;
    let number = |k: &str| match s.get(k) {
        Some(Node::Number(n)) => n
            .parse::<f64>()
            .map_err(|_| format!("{key}.{k} isn't a number")),
        _ => Err(format!("{key}.{k} must be a number")),
    };
    Ok(Signature {
        name: bento::js::want_text(s, "name")?,
        email: bento::js::want_text(s, "email")?,
        at: number("at")? as i64,
        offset_minutes: number("offset")? as i32,
    })
}

/// [`replay`], then a closing commit holding the project's manifests.
/// `project` is bento's `NewBibleProject` without books; `closing` is
/// `{author, committer?, message}`. Uses the histories up.
#[wasm_bindgen]
pub fn consolidate(
    histories: Vec<History>,
    now: f64,
    deltas: bool,
    project: &str,
    closing: &str,
    formats: Vec<String>,
) -> Result<Object, JsError> {
    let histories: Vec<walk::WriterHistory> = histories.into_iter().map(|h| h.0).collect();
    let project =
        bento::js::project_from_json(project, Vec::new()).map_err(|e| JsError::new(&e))?;
    let formats = bento::js::formats(&formats).map_err(|e| JsError::new(&e))?;
    let node = bento::tree::from_json(closing.as_bytes()).map_err(|e| JsError::new(&e))?;
    let author = signature_from(&node, "author").map_err(|e| JsError::new(&e))?;
    let committer = match node.get("committer") {
        Some(_) => signature_from(&node, "committer").map_err(|e| JsError::new(&e))?,
        None => author.clone(),
    };
    let closing = Closing {
        author,
        committer,
        message: bento::js::want_text(&node, "message").map_err(|e| JsError::new(&e))?,
    };
    let done = crate::consolidate(
        &histories,
        &ReplayOptions {
            now: now as i64,
            deltas,
        },
        &project,
        &formats,
        &closing,
    )
    .map_err(err)?;
    let out = replayed(done.replayed);
    let manifests = Map::new();
    for (name, bytes) in &done.manifests {
        manifests.set(
            &JsValue::from_str(name),
            &Uint8Array::from(bytes.as_slice()),
        );
    }
    set(&out, "manifests", &manifests);
    Ok(out)
}

/// One Writer book's files, with no history, → its USFM: the template
/// replay renders each save with, over the files as they are. `paths[i]`'s
/// bytes are the next `lengths[i]` of `bytes`, as `give` packs them; files
/// that aren't the book's text, or aren't UTF-8, are left out. `book` is the
/// code `\id` gets; `title` stands in when `front/title.txt` says nothing
/// (default: canon's English name). Needs no git.
#[wasm_bindgen]
pub fn stitch(
    paths: Vec<String>,
    lengths: &[u32],
    bytes: &[u8],
    book: &str,
    title: Option<String>,
) -> Result<String, JsError> {
    if paths.len() != lengths.len() {
        return Err(JsError::new("stitch: paths and lengths differ in number"));
    }
    let code: [u8; 3] = book
        .as_bytes()
        .try_into()
        .ok()
        .filter(|code: &[u8; 3]| {
            code.iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
        .ok_or_else(|| {
            JsError::new(&format!(
                "stitch: {book:?} is not a book code, like \"ROM\""
            ))
        })?;
    let book = mise::books::BookKey::new(code);
    let mut files = Vec::with_capacity(paths.len());
    let mut at = 0usize;
    for (path, &len) in paths.iter().zip(lengths) {
        let end = at + len as usize;
        let body = bytes
            .get(at..end)
            .ok_or_else(|| JsError::new("stitch: lengths run past the bytes"))?;
        files.push((path.as_str(), body));
        at = end;
    }
    let title = title.unwrap_or_else(|| crate::replay::fallback_title(book).to_owned());
    Ok(crate::stitch(files, book, &title))
}
