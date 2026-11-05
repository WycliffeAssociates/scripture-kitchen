//! The JSON the TypeScript layer reads and writes: `bento.ts`'s types, by hand.
//!
//! ```text
//! read_json("manifest.yaml", bytes)  → {"manifest": {"format": "rc", "language": {"tag": "en", …}, …},
//!                                       "detail": {"format": "rc", "document": {…the file's own value…}},
//!                                       "findings": [{"check": "sort-canon", "severity": "info", …}]}
//! project_from_json(text, bytes)     → NewBibleProject (createBible's argument, books' bytes beside it)
//! ```
//!
//! Field names are camelCase, as `bento.ts` declares them; an absent value is
//! an absent key. In TypeScript `detail.document` is the file's own value as
//! written (YAML as its JSON equivalent), not the lenient typed read the Rust
//! `Detail` holds: it's the format's own shape, which its TS interface names.

use crate::tree::Node;
use crate::write::{NewBibleProject, NewBook, NewLanguage};
use crate::{
    Direction, Domain, Format, LocalizedText, Manifest, ManifestFinding, Read, Related, Severity,
};

fn text(s: &str) -> Node {
    Node::text(s)
}

fn opt(entries: &mut Vec<(String, Node)>, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        entries.push((key.to_owned(), text(v)));
    }
}

pub fn format_name(f: Format) -> &'static str {
    match f {
        Format::Burrito => "burrito",
        Format::Rc => "rc",
        Format::BttWriter => "bttwriter",
    }
}

fn localized(t: &LocalizedText) -> Node {
    Node::map(t.iter().map(|(k, v)| (k.clone(), text(v))))
}

fn direction(d: Direction) -> Node {
    text(match d {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    })
}

pub fn related(r: &Related) -> Node {
    let mut e = vec![
        ("relation".to_owned(), text("source")),
        ("language".to_owned(), text(&r.language)),
        ("resource".to_owned(), text(&r.resource)),
    ];
    opt(&mut e, "version", r.version.as_deref());
    Node::Map(e)
}

pub fn manifest(m: &Manifest) -> Node {
    let mut e: Vec<(String, Node)> = vec![("format".into(), text(format_name(m.format)))];
    opt(&mut e, "id", m.id.as_deref());
    e.push(("name".into(), localized(&m.name)));
    e.push(("abbreviation".into(), localized(&m.abbreviation)));
    e.push((
        "language".into(),
        Node::map([
            ("tag", text(&m.language.tag)),
            ("name", localized(&m.language.name)),
            ("direction", direction(m.language.direction)),
        ]),
    ));
    let mut kind = Vec::new();
    if let Some(d) = m.kind.domain {
        let d = match d {
            Domain::Scripture => "scripture",
            Domain::Gloss => "gloss",
            Domain::Parascriptural => "parascriptural",
            Domain::Peripheral => "peripheral",
        };
        kind.push(("domain".to_owned(), text(d)));
    }
    opt(&mut kind, "subject", m.kind.subject.as_deref());
    opt(&mut kind, "flavor", m.kind.flavor.as_deref());
    let mut declared = Vec::new();
    let d = &m.kind.declared;
    opt(&mut declared, "subject", d.subject.as_deref());
    opt(&mut declared, "type", d.r#type.as_deref());
    opt(&mut declared, "format", d.format.as_deref());
    opt(&mut declared, "identifier", d.identifier.as_deref());
    kind.push(("declared".into(), Node::Map(declared)));
    e.push(("kind".into(), Node::Map(kind)));
    e.push((
        "books".into(),
        Node::List(
            m.books
                .iter()
                .map(|b| {
                    let mut be = vec![("book".to_owned(), text(&b.book.to_string()))];
                    opt(&mut be, "path", b.path.as_deref());
                    be.push(("title".into(), localized(&b.title)));
                    if let Some(s) = b.sort {
                        be.push(("sort".into(), Node::Number(s.to_string())));
                    }
                    Node::Map(be)
                })
                .collect(),
        ),
    ));
    e.push((
        "sources".into(),
        Node::List(m.sources.iter().map(related).collect()),
    ));
    opt(&mut e, "version", m.version.as_deref());
    opt(&mut e, "issued", m.issued.as_deref());
    opt(&mut e, "modified", m.modified.as_deref());
    opt(&mut e, "rights", m.rights.as_deref());
    opt(&mut e, "publisher", m.publisher.as_deref());
    Node::Map(e)
}

pub fn finding(f: &ManifestFinding) -> Node {
    let mut e = vec![
        ("check".to_owned(), text(f.check.id())),
        (
            "severity".to_owned(),
            text(match f.severity {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            }),
        ),
    ];
    opt(&mut e, "at", f.at.as_deref());
    if let Some(b) = f.book {
        e.push(("book".into(), text(&b.to_string())));
    }
    e.push(("message".into(), text(&f.message)));
    Node::Map(e)
}

pub fn findings(fs: &[ManifestFinding]) -> Node {
    Node::List(fs.iter().map(finding).collect())
}

/// A [`Read`] as `bento.ts`'s `Read`, with the file's own value as the detail's document.
pub fn read(read: &Read, format: Option<Format>, document: Option<Node>) -> Node {
    let mut e = Vec::new();
    if let Some(m) = &read.manifest {
        e.push(("manifest".to_owned(), manifest(m)));
    }
    if let (Some(f), Some(doc), Some(_)) = (format, document, &read.detail) {
        e.push((
            "detail".into(),
            Node::map([("format", text(format_name(f))), ("document", doc)]),
        ));
    }
    e.push(("findings".into(), findings(&read.findings)));
    Node::Map(e)
}

/// Why a JSON argument from the TS layer was refused.
pub fn want<'n>(node: &'n Node, key: &str) -> Result<&'n Node, String> {
    node.get(key).ok_or_else(|| format!("{key} is required"))
}

pub fn want_text(node: &Node, key: &str) -> Result<String, String> {
    match want(node, key)? {
        Node::String(s) => Ok(s.clone()),
        other => Err(format!("{key} must be text, not {}", other.kind())),
    }
}

fn want_list<'n>(node: &'n Node, key: &str) -> Result<&'n [Node], String> {
    match node.get(key) {
        None | Some(Node::Null) => Ok(&[]),
        Some(Node::List(items)) => Ok(items),
        Some(other) => Err(format!("{key} must be a list, not {}", other.kind())),
    }
}

pub fn canon_of(node: &Node) -> Result<canon::Canon, String> {
    match node.get("canon") {
        None | Some(Node::Null) => Ok(canon::Canon::Protestant66),
        Some(Node::String(s)) if s == "protestant66" => Ok(canon::Canon::Protestant66),
        Some(Node::String(s)) if s == "withDeuterocanon" => Ok(canon::Canon::WithDeuterocanon),
        Some(other) => Err(format!(
            "canon must be \"protestant66\" or \"withDeuterocanon\", not {other:?}"
        )),
    }
}

/// `[{book: "MAT", title: "Matyu"}, …]` with each one's bytes, in order.
pub fn books(node: &Node, key: &str, bytes: Vec<Vec<u8>>) -> Result<Vec<NewBook>, String> {
    let list = want_list(node, key)?;
    if list.len() != bytes.len() {
        return Err(format!(
            "{key}: {} books but {} byte arrays",
            list.len(),
            bytes.len()
        ));
    }
    list.iter()
        .zip(bytes)
        .map(|(b, bytes)| {
            let code = want_text(b, "book")?;
            let book = crate::book_of(&code)
                .ok_or_else(|| format!("{code} isn't a book of canon's table"))?;
            Ok(NewBook {
                book,
                title: want_text(b, "title")?,
                bytes,
            })
        })
        .collect()
}

/// `createBible`'s argument as JSON text, with its books' bytes in order:
/// for another wasm module that takes a project in `bento.ts`'s shape.
pub fn project_from_json(text: &str, book_bytes: Vec<Vec<u8>>) -> Result<NewBibleProject, String> {
    let node = crate::tree::from_json(text.as_bytes())?;
    let books = books(&node, "books", book_bytes)?;
    project(&node, books)
}

/// `createBible`'s argument: every `NewBibleProject` field, camelCase.
pub fn project(node: &Node, books: Vec<NewBook>) -> Result<NewBibleProject, String> {
    let language = want(node, "language")?;
    let generator = want(node, "generator")?;
    let sources = want_list(node, "sources")?
        .iter()
        .map(|s| {
            Ok(Related {
                language: want_text(s, "language")?,
                resource: want_text(s, "resource")?,
                version: match s.get("version") {
                    Some(Node::String(v)) => Some(v.clone()),
                    _ => None,
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let contributors = want_list(node, "contributors")?
        .iter()
        .map(|c| match c {
            Node::String(s) => Ok(s.clone()),
            other => Err(format!("a contributor must be text, not {}", other.kind())),
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(NewBibleProject {
        title: want_text(node, "title")?,
        abbreviation: want_text(node, "abbreviation")?,
        language: NewLanguage {
            tag: want_text(language, "tag")?,
            name: want_text(language, "name")?,
            direction: match want_text(language, "direction")?.as_str() {
                "rtl" => Direction::Rtl,
                "ltr" => Direction::Ltr,
                other => return Err(format!("direction must be ltr or rtl, not {other}")),
            },
        },
        books,
        canon: canon_of(node)?,
        sources,
        contributors,
        publisher: want_text(node, "publisher")?,
        creator: want_text(node, "creator")?,
        rights: want_text(node, "rights")?,
        checking_level: want_text(node, "checkingLevel")?,
        copyright: want_text(node, "copyright")?,
        generator: (
            want_text(generator, "name")?,
            want_text(generator, "version")?,
        ),
        created_at: want_text(node, "createdAt")?,
        repository: want_text(node, "repository")?,
    })
}

/// The formats `createBible` writes: `["burrito", "rc"]` by default.
pub fn formats(names: &[String]) -> Result<Vec<Format>, String> {
    if names.is_empty() {
        return Ok(vec![Format::Burrito, Format::Rc]);
    }
    names
        .iter()
        .map(|n| match n.as_str() {
            "burrito" => Ok(Format::Burrito),
            "rc" => Ok(Format::Rc),
            other => Err(format!("{other} isn't a format bento writes (burrito, rc)")),
        })
        .collect()
}

#[cfg(test)]
mod tests;
