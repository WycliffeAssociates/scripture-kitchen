//! Editing existing manifests: one call, every changed file, nothing else touched.
//!
//! ```text
//! edit(&[("metadata.json", …), ("manifest.yaml", …)], &Change { add_books: [MRK], checksums: [("41-MAT.usfm", bytes)], .. })
//!   → Edited { files: [("metadata.json", …), ("manifest.yaml", …)], findings: [] }
//!
//!   metadata.json   MRK's ingredient, scope and name, placed by canon; MAT's md5 and size, if they changed
//!   manifest.yaml   MRK's project entry between MAT and LUK; every other line as it was
//! ```
//!
//! The caller commits the books and the manifests together: one commit, not
//! two. A file comes back only when it changed. An edit changes only what it
//! was asked to: adding MRK touches MRK's entries, and an old manifest's
//! `sort` values stay as they were (`check` reports them). A YAML file keeps
//! its quoting, flow lists and comments elsewhere (yaml-edit edits the file's
//! syntax tree); a JSON file keeps its key order and is written as Sefer
//! writes it, `JSON.stringify(value, null, 2)`.

use std::str::FromStr;

use canon::Canon;
use mise::books::BookKey;

use crate::tree::{Node, pointer};
use crate::write::{NewBook, Placed, fingerprint, ingredient, place, rc_project, yaml_list};
use crate::{Check, Format, ManifestFinding, Severity, book_of, format_of, without_bom};

/// What to change.
#[derive(Debug, Clone)]
pub struct Change {
    /// Books to list; refused if one is listed already.
    pub add_books: Vec<NewBook>,
    /// Which canon places them (`sort`, order). The project's own.
    pub canon: Canon,
    /// Ingredient files as they'll be committed, by path (`41-MAT.usfm`):
    /// their Burrito md5 and size are brought up to date.
    pub checksums: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    /// Only the files that changed, by name.
    pub files: Vec<(String, Vec<u8>)>,
    pub findings: Vec<ManifestFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The book is listed already.
    Listed(BookKey),
    /// A book that isn't in the chosen canon, or given twice.
    Book(crate::CreateError),
    /// A manifest that can't be read or edited: its name and why.
    Unreadable(String, String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Listed(book) => write!(f, "{book} is listed already"),
            EditError::Book(e) => write!(f, "{e}"),
            EditError::Unreadable(name, why) => write!(f, "{name} can't be edited: {why}"),
        }
    }
}

impl std::error::Error for EditError {}

/// Apply `change` to each manifest in `existing` (by file name: Burrito and
/// RC; a BTT Writer manifest is never edited and comes back unchanged).
pub fn edit(existing: &[(String, Vec<u8>)], change: &Change) -> Result<Edited, EditError> {
    let placed = place(&change.add_books, change.canon).map_err(EditError::Book)?;
    let mut out = Edited {
        files: Vec::new(),
        findings: Vec::new(),
    };
    for (name, bytes) in existing {
        let edited = match format_of(name) {
            Some(Format::Burrito) => edit_burrito(name, bytes, &placed, change, &mut out.findings)?,
            Some(Format::Rc) => edit_rc(name, bytes, &placed, change, &mut out.findings)?,
            _ => None,
        };
        if let Some(bytes) = edited {
            out.files.push((name.clone(), bytes));
        }
    }
    Ok(out)
}

/// Where a book goes among keys or items that are books: before the first
/// that comes after it in the canon, else last.
fn position(
    order: &[BookKey],
    book: BookKey,
    existing: impl Iterator<Item = Option<BookKey>>,
) -> Option<usize> {
    let place = |b: BookKey| order.iter().position(|&k| k == b);
    let mine = place(book)?;
    existing
        .enumerate()
        .find(|(_, b)| b.and_then(place).is_some_and(|theirs| theirs > mine))
        .map(|(i, _)| i)
}

fn insert_at(node: &mut Node, at: Option<usize>, key: String, value: Node) {
    if let Node::Map(entries) = node {
        let i = at.unwrap_or(entries.len());
        entries.insert(i, (key, value));
    }
}

fn edit_burrito(
    name: &str,
    bytes: &[u8],
    placed: &[Placed<'_>],
    change: &Change,
    findings: &mut Vec<ManifestFinding>,
) -> Result<Option<Vec<u8>>, EditError> {
    let unreadable = |why: String| EditError::Unreadable(name.to_owned(), why);
    let mut root = crate::tree::from_json(without_bom(bytes)).map_err(unreadable)?;
    if !matches!(root, Node::Map(_)) {
        return Err(unreadable("not an object".into()));
    }
    let before = root.clone();
    let listed = crate::read_as(Format::Burrito, bytes)
        .manifest
        .map(|m| m.books.iter().map(|b| b.book).collect::<Vec<_>>())
        .unwrap_or_default();
    let tag = root
        .get("languages")
        .and_then(|l| match l {
            Node::List(items) => items.first(),
            _ => None,
        })
        .and_then(|l| l.get("tag"))
        .and_then(|t| match t {
            Node::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "und".into());
    let order: Vec<BookKey> = canon::books(change.canon).map(|b| b.code).collect();

    for p in placed {
        let book = p.book.book;
        if listed.contains(&book) {
            return Err(EditError::Listed(book));
        }
        let code = book.to_string();
        // Ingredients: keyed by path; a key's book is its scope's.
        if let Some(ingredients) = root.entry("ingredients") {
            if !matches!(ingredients, Node::Map(_)) {
                *ingredients = Node::Map(Vec::new());
            }
            let books: Vec<Option<BookKey>> = ingredients
                .entries()
                .iter()
                .map(|(_, i)| {
                    i.get("scope")
                        .and_then(|s| s.entries().first().and_then(|(k, _)| book_of(k)))
                })
                .collect();
            let at = position(&order, book, books.into_iter());
            insert_at(ingredients, at, p.file.clone(), ingredient(p.book));
        }
        if let Some(scope) = root
            .get_mut("type")
            .and_then(|t| t.get_mut("flavorType"))
            .and_then(|f| f.entry("currentScope"))
        {
            if !matches!(scope, Node::Map(_)) {
                *scope = Node::Map(Vec::new());
            }
            let at = position(
                &order,
                book,
                scope
                    .entries()
                    .iter()
                    .map(|(k, _)| book_of(k))
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
            insert_at(scope, at, code.clone(), Node::List(Vec::new()));
        }
        if let Some(names) = root.entry("localizedNames") {
            if !matches!(names, Node::Map(_)) {
                *names = Node::Map(Vec::new());
            }
            let at = position(
                &order,
                book,
                names
                    .entries()
                    .iter()
                    .map(|(k, _)| book_of(k))
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
            let name = Node::map([(
                "short",
                Node::map([(tag.as_str(), Node::text(&p.book.title))]),
            )]);
            insert_at(names, at, code, name);
        }
    }

    for (path, bytes) in &change.checksums {
        let print = fingerprint(bytes);
        let Some(Node::Map(ingredients)) = root.get_mut("ingredients") else {
            break;
        };
        let wanted = path.trim_start_matches("./");
        let Some((_, ingredient)) = ingredients
            .iter_mut()
            .rev()
            .find(|(k, _)| k.trim_start_matches("./") == wanted)
        else {
            findings.push(ManifestFinding {
                check: Check::ManifestValid,
                severity: Severity::Warning,
                at: Some(pointer("/ingredients", path)),
                book: None,
                message: format!("{path} has no ingredient to update."),
            });
            continue;
        };
        let fresh = (Node::text(&print.md5), Node::Number(print.size.to_string()));
        if ingredient.get("checksum").and_then(|c| c.get("md5")) != Some(&fresh.0)
            && let Some(checksum) = ingredient.entry("checksum")
        {
            if !matches!(checksum, Node::Map(_)) {
                *checksum = Node::Map(Vec::new());
            }
            if let Some(md5) = checksum.entry("md5") {
                *md5 = fresh.0;
            }
        }
        // A size written `9` and one written `9.0` are the same size.
        let size_now = ingredient.get("size").and_then(|s| match s {
            Node::Number(n) => n.parse::<f64>().ok(),
            _ => None,
        });
        if size_now != Some(print.size as f64)
            && let Some(size) = ingredient.entry("size")
        {
            *size = fresh.1;
        }
    }

    Ok((root != before).then(|| root.to_json().into_bytes()))
}

fn edit_rc(
    name: &str,
    bytes: &[u8],
    placed: &[Placed<'_>],
    change: &Change,
    findings: &mut Vec<ManifestFinding>,
) -> Result<Option<Vec<u8>>, EditError> {
    if placed.is_empty() {
        return Ok(None); // an RC lists no checksums
    }
    let unreadable = |why: String| EditError::Unreadable(name.to_owned(), why);
    let had_bom = bytes.starts_with(b"\xEF\xBB\xBF");
    let text = std::str::from_utf8(without_bom(bytes)).map_err(|e| unreadable(e.to_string()))?;
    let read = crate::read_as(Format::Rc, bytes);
    let Some(crate::Detail::Rc(doc)) = &read.detail else {
        return Err(unreadable("not a Resource Container".into()));
    };
    let file = yaml_edit::YamlFile::from_str(text).map_err(|e| unreadable(e.to_string()))?;
    let document = file
        .documents()
        .find(|d| d.as_mapping().is_some())
        .ok_or_else(|| unreadable("no mapping".into()))?;
    let mapping = document
        .as_mapping()
        .ok_or_else(|| unreadable("no mapping".into()))?;
    let order: Vec<BookKey> = canon::books(change.canon).map(|b| b.code).collect();
    let indent_lists = indents_lists(text);
    let mut books: Vec<Option<BookKey>> = doc
        .projects
        .iter()
        .map(|p| p.identifier.as_deref().and_then(book_of))
        .collect();

    for p in placed {
        let book = p.book.book;
        if books.contains(&Some(book)) {
            return Err(EditError::Listed(book));
        }
        // A manifest whose sorts count from 0 (Sefer's) can tie with the new one.
        if let Some(tie) = doc.projects.iter().find(|q| q.sort == Some(p.sort as i64)) {
            findings.push(ManifestFinding {
                check: Check::SortCanon,
                severity: Severity::Warning,
                at: None,
                book: Some(book),
                message: format!(
                    "{book}'s sort, {}, ties with {}'s; order by sort, then by canon, is still right.",
                    p.sort,
                    tie.identifier.as_deref().unwrap_or("another project")
                ),
            });
        }
        let at = position(&order, book, books.iter().copied()).unwrap_or(books.len());
        let mut entry = String::new();
        yaml_list(&[rc_project(p)], 0, indent_lists, &mut entry);
        let entry = entry.strip_prefix("- ").unwrap_or(&entry);
        let entry = entry
            .lines()
            .map(|l| l.strip_prefix("  ").unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n");
        let new = yaml_edit::Document::from_str(&entry).map_err(|e| unreadable(e.to_string()))?;
        let new = new
            .as_mapping()
            .ok_or_else(|| unreadable("a project entry".into()))?;
        match mapping.get_sequence("projects") {
            Some(projects) => projects.insert(at, new),
            None => {
                mapping.set("projects", yaml_edit::YamlValue::Sequence(Vec::new()));
                mapping
                    .get_sequence("projects")
                    .ok_or_else(|| unreadable("projects".into()))?
                    .push(new);
            }
        }
        books.insert(at, Some(book));
    }

    let mut out = if had_bom {
        "\u{FEFF}".to_owned()
    } else {
        String::new()
    };
    out.push_str(&file.to_string());
    Ok(Some(out.into_bytes()))
}

/// Whether the file indents a list under its key (`projects:` then
/// `  - title: …`), or writes it at the key's own indent (`- title: …`), as
/// the catalog does. Judged by the first list under a key; indented when
/// there's none to judge by.
fn indents_lists(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .collect();
    for pair in lines.windows(2) {
        let (key, item) = (pair[0], pair[1]);
        if key.trim_end().ends_with(':') && item.trim_start().starts_with("- ") {
            let indent = |l: &str| l.len() - l.trim_start().len();
            return indent(item) > indent(key);
        }
    }
    true
}
