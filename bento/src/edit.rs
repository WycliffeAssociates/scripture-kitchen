//! Editing existing manifests: one handle over every manifest a project has,
//! every changed file at the end, nothing else touched.
//!
//! ```text
//! let mut m = Manifests::open(&[("metadata.json", …), ("manifest.yaml", …)], Canon::Protestant66)?;
//! m.add_book(MRK "Mak")?;                          MRK's ingredient, scope and name; its project entry
//! m.rename_book(JHN short "Yohani" long …)?;       localizedNames.JHN under the language's tag; the RC's title
//! m.update_checksum("41-MAT.usfm", bytes);         MAT's md5 and size, if they changed
//! m.names()                                        [("metadata.json", [JHN {short, long, abbr}, …]), …]
//! m.finish()                                       → Edited { files: only those that changed, findings }
//!
//! edit(existing, &Change { add_books, checksums, .. })    the same, in one call
//! ```
//!
//! Calls apply in the order they're made; one that fails changes nothing. The
//! caller commits the books and the manifests together: one commit, not two.
//! An edit changes only what it was asked to: an old manifest's `sort` values
//! stay as they were (`check` reports them). A YAML file keeps every byte it
//! isn't asked to change (bento splices its text); a JSON file keeps its key
//! order and is written as Sefer writes it, `JSON.stringify(value, null, 2)`.
//!
//! Book names: a Burrito's `en` is canon's English name and the language's
//! tag holds the language's own; an RC's one `title` is the language's own,
//! its short name.

use std::str::FromStr;

use canon::Canon;
use mise::books::BookKey;

use crate::tree::{Node, pointer};
use crate::write::{
    NewBook, Placed, book_name, fingerprint, ingredient, place, rc_project, trimmed_books,
    yaml_list, yaml_string,
};
use crate::{Check, Format, ManifestFinding, Severity, book_of, format_of, without_bom};

/// What to change, in one call. `Manifests` is the same, a call at a time.
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

/// A book's names in the project's language. `None`, an empty name or one of
/// only spaces leaves that name as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookNames {
    pub book: BookKey,
    /// `\toc2`: a Burrito's `short`, an RC's `title`.
    pub short: Option<String>,
    /// `\toc1`: a Burrito's `long`. An RC has none.
    pub long: Option<String>,
    /// `\toc3`: a Burrito's `abbr`. An RC has none.
    pub abbr: Option<String>,
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
    let books = trimmed_books(&change.add_books);
    let placed = place(&books, change.canon).map_err(EditError::Book)?;
    let mut m = Manifests::open(existing, change.canon)?;
    for p in &placed {
        m.add_book(p.book.clone())?;
    }
    for (path, bytes) in &change.checksums {
        m.update_checksum(path, bytes);
    }
    Ok(m.finish())
}

/// A project's manifests, open for editing.
#[derive(Debug, Clone)]
pub struct Manifests {
    canon: Canon,
    files: Vec<File>,
    findings: Vec<ManifestFinding>,
}

#[derive(Debug, Clone)]
struct File {
    name: String,
    body: Body,
}

#[derive(Debug, Clone)]
enum Body {
    Burrito {
        root: Node,
        before: Node,
    },
    Rc {
        text: String,
        before: String,
        bom: bool,
    },
    /// An RC that isn't UTF-8: an error only for a call that must edit it.
    Broken(String),
    /// Not a format bento edits (BTT Writer's).
    Untouched,
}

impl Manifests {
    /// The manifests in `existing`, by file name. An unreadable Burrito is
    /// refused here; an RC only when a call has to change it.
    pub fn open(existing: &[(String, Vec<u8>)], canon: Canon) -> Result<Manifests, EditError> {
        let mut files = Vec::with_capacity(existing.len());
        for (name, bytes) in existing {
            let unreadable = |why: String| EditError::Unreadable(name.clone(), why);
            let body = match format_of(name) {
                Some(Format::Burrito) => {
                    let root = crate::tree::from_json(without_bom(bytes)).map_err(unreadable)?;
                    if !matches!(root, Node::Map(_)) {
                        return Err(unreadable("not an object".into()));
                    }
                    Body::Burrito {
                        before: root.clone(),
                        root,
                    }
                }
                Some(Format::Rc) => match std::str::from_utf8(without_bom(bytes)) {
                    Ok(text) => Body::Rc {
                        text: text.to_owned(),
                        before: text.to_owned(),
                        bom: bytes.starts_with(b"\xEF\xBB\xBF"),
                    },
                    Err(e) => Body::Broken(e.to_string()),
                },
                _ => Body::Untouched,
            };
            files.push(File {
                name: name.clone(),
                body,
            });
        }
        Ok(Manifests {
            canon,
            files,
            findings: Vec::new(),
        })
    }

    /// List a book: refused if it's listed already or isn't in the canon.
    pub fn add_book(&mut self, book: NewBook) -> Result<Vec<ManifestFinding>, EditError> {
        let books = trimmed_books(std::slice::from_ref(&book));
        let placed = place(&books, self.canon).map_err(EditError::Book)?;
        let p = &placed[0];
        let order = order(self.canon);
        let mut files = self.files.clone();
        let mut found = Vec::new();
        for file in &mut files {
            let unreadable = |why: String| EditError::Unreadable(file.name.clone(), why);
            match &mut file.body {
                Body::Burrito { root, .. } => add_burrito(root, p, &order, &file.name, &mut found)?,
                Body::Rc { text, .. } => add_rc(text, p, &order, &file.name, &mut found)?,
                Body::Broken(why) => return Err(unreadable(why.clone())),
                Body::Untouched => {}
            }
        }
        Ok(self.commit(files, found))
    }

    /// Set a listed book's names. A book a manifest doesn't list is a
    /// warning, and that manifest is left as it was.
    pub fn rename_book(&mut self, names: BookNames) -> Result<Vec<ManifestFinding>, EditError> {
        let names = BookNames {
            book: names.book,
            short: clean(names.short),
            long: clean(names.long),
            abbr: clean(names.abbr),
        };
        let order = order(self.canon);
        let mut files = self.files.clone();
        let mut found = Vec::new();
        for file in &mut files {
            let unreadable = |why: String| EditError::Unreadable(file.name.clone(), why);
            match &mut file.body {
                Body::Burrito { root, .. } => {
                    rename_burrito(root, &names, &order, &file.name, &mut found)
                }
                Body::Rc { text, .. } if names.short.is_some() => {
                    rename_rc(text, &names, &file.name, &mut found).map_err(unreadable)?
                }
                Body::Broken(why) if names.short.is_some() => {
                    return Err(unreadable(why.clone()));
                }
                _ => {}
            }
        }
        Ok(self.commit(files, found))
    }

    /// Bring a file's Burrito md5 and size up to date. An RC lists no
    /// checksums.
    pub fn update_checksum(&mut self, path: &str, bytes: &[u8]) -> Vec<ManifestFinding> {
        let mut files = self.files.clone();
        let mut found = Vec::new();
        for file in &mut files {
            if let Body::Burrito { root, .. } = &mut file.body {
                checksum_burrito(root, path, bytes, &mut found);
            }
        }
        self.commit(files, found)
    }

    /// Each manifest's names for the books it names, in the project's
    /// language: a Burrito's under its language's tag, an RC's `title` as
    /// `short`. A Burrito with no language tag names none.
    pub fn names(&self) -> Vec<(String, Vec<BookNames>)> {
        self.files
            .iter()
            .filter_map(|file| match &file.body {
                Body::Burrito { root, .. } => Some((file.name.clone(), burrito_names(root))),
                Body::Rc { text, .. } => Some((file.name.clone(), rc_names(text))),
                _ => None,
            })
            .collect()
    }

    /// The files that changed, and every call's findings in order.
    pub fn finish(self) -> Edited {
        let files = self
            .files
            .into_iter()
            .filter_map(|file| {
                let bytes = match file.body {
                    Body::Burrito { root, before } => {
                        (root != before).then(|| root.to_json().into_bytes())
                    }
                    Body::Rc { text, before, bom } => (text != before).then(|| {
                        let mut out = if bom {
                            "\u{FEFF}".to_owned()
                        } else {
                            String::new()
                        };
                        out.push_str(&text);
                        out.into_bytes()
                    }),
                    _ => None,
                };
                bytes.map(|b| (file.name, b))
            })
            .collect();
        Edited {
            files,
            findings: self.findings,
        }
    }

    fn commit(&mut self, files: Vec<File>, found: Vec<ManifestFinding>) -> Vec<ManifestFinding> {
        self.files = files;
        self.findings.extend(found.iter().cloned());
        found
    }
}

fn order(canon: Canon) -> Vec<BookKey> {
    canon::books(canon).map(|b| b.code).collect()
}

fn clean(name: Option<String>) -> Option<String> {
    name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty())
}

fn warning(at: Option<String>, book: BookKey, message: String) -> ManifestFinding {
    ManifestFinding {
        check: Check::ManifestValid,
        severity: Severity::Warning,
        at,
        book: Some(book),
        message,
    }
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

/// `node` as a map, made one if it isn't.
fn as_map(node: &mut Node) -> &mut Node {
    if !matches!(node, Node::Map(_)) {
        *node = Node::Map(Vec::new());
    }
    node
}

fn text_of(node: Option<&Node>) -> Option<&str> {
    match node {
        Some(Node::String(s)) => Some(s),
        _ => None,
    }
}

// --- Burrito -----------------------------------------------------------------

/// The language's tag, `languages[0].tag`, when it is one.
fn burrito_tag(root: &Node) -> Option<String> {
    let tag = match root.get("languages") {
        Some(Node::List(items)) => text_of(items.first().and_then(|l| l.get("tag")))?,
        _ => return None,
    };
    is_language_tag(tag).then(|| tag.to_owned())
}

/// A BCP 47 tag's shape: subtags of letters and digits, the first two to
/// eight letters (or `x`, `i` before a private or grandfathered tag).
fn is_language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let first = parts.next().unwrap_or_default();
    let first_ok = (2..=8).contains(&first.len()) && first.bytes().all(|b| b.is_ascii_alphabetic())
        || matches!(first, "x" | "i");
    first_ok
        && parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn burrito_listed(root: &Node) -> Vec<BookKey> {
    crate::read_as(Format::Burrito, root.to_json().as_bytes())
        .manifest
        .map(|m| m.books.iter().map(|b| b.book).collect())
        .unwrap_or_default()
}

fn names_key(names: &Node, book: BookKey) -> Option<String> {
    names
        .entries()
        .iter()
        .find(|(k, _)| book_of(k) == Some(book))
        .map(|(k, _)| (*k).to_owned())
}

fn add_burrito(
    root: &mut Node,
    p: &Placed<'_>,
    order: &[BookKey],
    file: &str,
    found: &mut Vec<ManifestFinding>,
) -> Result<(), EditError> {
    let book = p.book.book;
    if burrito_listed(root).contains(&book) {
        return Err(EditError::Listed(book));
    }
    let tag = burrito_tag(root);
    let code = book.to_string();
    // Ingredients: keyed by path; a key's book is its scope's.
    if let Some(ingredients) = root.entry("ingredients") {
        let ingredients = as_map(ingredients);
        let books: Vec<Option<BookKey>> = ingredients
            .entries()
            .iter()
            .map(|(_, i)| {
                i.get("scope")
                    .and_then(|s| s.entries().first().and_then(|(k, _)| book_of(k)))
            })
            .collect();
        let at = position(order, book, books.into_iter());
        insert_at(ingredients, at, p.file.clone(), ingredient(p.book));
    }
    if let Some(scope) = root
        .get_mut("type")
        .and_then(|t| t.get_mut("flavorType"))
        .and_then(|f| f.entry("currentScope"))
    {
        let scope = as_map(scope);
        let books: Vec<Option<BookKey>> = scope.entries().iter().map(|(k, _)| book_of(k)).collect();
        let at = position(order, book, books.into_iter());
        insert_at(scope, at, code.clone(), Node::List(Vec::new()));
    }
    if tag.is_none() {
        found.push(warning(
            Some("/languages/0/tag".into()),
            book,
            format!(
                "{file} names no language tag, so {book}'s name, {}, isn't written in it; only its English name is.",
                p.book.title
            ),
        ));
    }
    if let Some(names) = root.entry("localizedNames") {
        let names = as_map(names);
        let books: Vec<Option<BookKey>> = names.entries().iter().map(|(k, _)| book_of(k)).collect();
        let at = position(order, book, books.into_iter());
        let short = match &tag {
            Some(tag) => book_name(tag, book, &p.book.title),
            None => book_name(
                "en",
                book,
                canon::book(book).map_or(&p.book.title, |b| b.english),
            ),
        };
        insert_at(names, at, code, Node::map([("short", short)]));
    }
    Ok(())
}

fn rename_burrito(
    root: &mut Node,
    names: &BookNames,
    order: &[BookKey],
    file: &str,
    found: &mut Vec<ManifestFinding>,
) {
    let book = names.book;
    let fields = [
        ("short", &names.short),
        ("long", &names.long),
        ("abbr", &names.abbr),
    ];
    if fields.iter().all(|(_, v)| v.is_none()) {
        return;
    }
    let Some(tag) = burrito_tag(root) else {
        found.push(warning(
            Some("/languages/0/tag".into()),
            book,
            format!("{file} names no language tag, so {book}'s names weren't changed."),
        ));
        return;
    };
    if !burrito_listed(root).contains(&book) {
        found.push(warning(
            Some("/ingredients".into()),
            book,
            format!("{file} doesn't list {book}, so its names weren't changed."),
        ));
        return;
    }
    let Some(all) = root.entry("localizedNames") else {
        return;
    };
    let all = as_map(all);
    let key = match names_key(all, book) {
        Some(key) => key,
        None if names.short.is_none() => {
            found.push(warning(
                Some(pointer("/localizedNames", &book.to_string())),
                book,
                format!(
                    "{file} has no name for {book}, and a name needs a short one; its long name and abbreviation weren't written."
                ),
            ));
            return;
        }
        None => {
            let books: Vec<Option<BookKey>> =
                all.entries().iter().map(|(k, _)| book_of(k)).collect();
            let at = position(order, book, books.into_iter());
            insert_at(all, at, book.to_string(), Node::Map(Vec::new()));
            book.to_string()
        }
    };
    let Some(entry) = all.get_mut(&key) else {
        return;
    };
    let entry = as_map(entry);
    for (field, value) in fields {
        let Some(value) = value else { continue };
        let Some(sub) = entry.entry(field) else {
            continue;
        };
        let sub = as_map(sub);
        if field == "short" && tag != "en" {
            // `en` is canon's English: written when missing, and replacing
            // a copy of the old name (what bento wrote before 0.1.24). A
            // real English name stays.
            let old = text_of(sub.get(&tag)).map(str::to_owned);
            let en = text_of(sub.get("en")).map(str::to_owned);
            if en.is_none() || (old.is_some() && en == old) {
                let english = canon::book(book).map_or(value.as_str(), |b| b.english);
                if let Some(slot) = sub.entry("en") {
                    *slot = Node::text(english);
                }
            }
        }
        if let Some(slot) = sub.entry(&tag) {
            *slot = Node::text(value);
        }
    }
}

fn checksum_burrito(root: &mut Node, path: &str, bytes: &[u8], found: &mut Vec<ManifestFinding>) {
    let print = fingerprint(bytes);
    let Some(Node::Map(ingredients)) = root.get_mut("ingredients") else {
        return;
    };
    let wanted = path.trim_start_matches("./");
    let Some((_, ingredient)) = ingredients
        .iter_mut()
        .rev()
        .find(|(k, _)| k.trim_start_matches("./") == wanted)
    else {
        found.push(ManifestFinding {
            check: Check::ManifestValid,
            severity: Severity::Warning,
            at: Some(pointer("/ingredients", path)),
            book: None,
            message: format!("{path} has no ingredient to update."),
        });
        return;
    };
    let fresh = (Node::text(&print.md5), Node::Number(print.size.to_string()));
    if ingredient.get("checksum").and_then(|c| c.get("md5")) != Some(&fresh.0)
        && let Some(checksum) = ingredient.entry("checksum")
        && let Some(md5) = as_map(checksum).entry("md5")
    {
        *md5 = fresh.0;
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

fn burrito_names(root: &Node) -> Vec<BookNames> {
    let Some(tag) = burrito_tag(root) else {
        return Vec::new();
    };
    let Some(all) = root.get("localizedNames") else {
        return Vec::new();
    };
    all.entries()
        .iter()
        .filter_map(|(k, v)| {
            let name =
                |field: &str| text_of(v.get(field).and_then(|f| f.get(&tag))).map(str::to_owned);
            Some(BookNames {
                book: book_of(k)?,
                short: name("short"),
                long: name("long"),
                abbr: name("abbr"),
            })
        })
        .collect()
}

// --- RC ----------------------------------------------------------------------

fn add_rc(
    text: &mut String,
    p: &Placed<'_>,
    order: &[BookKey],
    file: &str,
    found: &mut Vec<ManifestFinding>,
) -> Result<(), EditError> {
    let unreadable = |why: String| EditError::Unreadable(file.to_owned(), why);
    let read = crate::read_as(Format::Rc, text.as_bytes());
    let Some(crate::Detail::Rc(doc)) = &read.detail else {
        return Err(unreadable("not a Resource Container".into()));
    };
    let book = p.book.book;
    let projects = projects(text).map_err(unreadable)?;
    if projects.iter().any(|(b, _)| *b == Some(book)) {
        return Err(EditError::Listed(book));
    }
    // A manifest whose sorts count from 0 (Sefer's) can tie with the new one.
    if let Some(tie) = doc.projects.iter().find(|q| q.sort == Some(p.sort as i64)) {
        found.push(ManifestFinding {
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
    let at = position(order, book, projects.iter().map(|(b, _)| *b));
    splice_project(text, &projects, at, &rc_project(p), indents_lists(text)).map_err(unreadable)
}

/// The `projects` list's items as yaml-edit mappings, flow or block.
fn project_items(text: &str) -> Result<Vec<yaml_edit::Mapping>, String> {
    let file = yaml_edit::YamlFile::from_str(text).map_err(|e| e.to_string())?;
    let Some(mapping) = file.documents().find_map(|d| d.as_mapping()) else {
        return Ok(Vec::new());
    };
    let Some(yaml_edit::YamlNode::Sequence(list)) = mapping.get("projects") else {
        return Ok(Vec::new());
    };
    Ok(list
        .values()
        .filter_map(|item| match item {
            yaml_edit::YamlNode::Mapping(m) => Some(m),
            _ => None,
        })
        .collect())
}

fn item_book(m: &yaml_edit::Mapping) -> Option<BookKey> {
    match m.get("identifier") {
        Some(yaml_edit::YamlNode::Scalar(s)) => book_of(&s.as_string()),
        _ => None,
    }
}

/// Rename every item listing the book: its `title` scalar's bytes replaced in
/// the quoting it had, or a `title` added last when it has none.
fn rename_rc(
    text: &mut String,
    names: &BookNames,
    file: &str,
    found: &mut Vec<ManifestFinding>,
) -> Result<(), String> {
    let (book, Some(short)) = (names.book, names.short.as_deref()) else {
        return Ok(());
    };
    let items: Vec<_> = project_items(text)?
        .into_iter()
        .filter(|m| item_book(m) == Some(book))
        .collect();
    match items.len() {
        0 => {
            found.push(warning(
                Some("/projects".into()),
                book,
                format!("{file} doesn't list {book}, so its title wasn't changed."),
            ));
            return Ok(());
        }
        1 => {}
        n => found.push(warning(
            Some("/projects".into()),
            book,
            format!("{file} lists {book} {n} times; each one's title was changed."),
        )),
    }
    let mut splices: Vec<(usize, usize, String)> = Vec::new();
    let mut untitled = false;
    for m in &items {
        match m.get("title") {
            Some(title) => {
                let range = title.byte_range();
                let (start, end) = (range.start as usize, range.end as usize);
                // An empty `title:` is a null with no bytes, right after the colon.
                let space = if text[..start].ends_with([' ', '\t']) {
                    ""
                } else {
                    " "
                };
                splices.push((
                    start,
                    end,
                    format!("{space}{}", requoted(&text[start..end], short)),
                ));
            }
            None => untitled = true,
        }
    }
    splices.sort_by_key(|s| std::cmp::Reverse(s.0));
    for (start, end, value) in splices {
        text.replace_range(start..end, &value);
    }
    if untitled {
        let file = yaml_edit::YamlFile::from_str(text).map_err(|e| e.to_string())?;
        if let Some(mapping) = file.documents().find_map(|d| d.as_mapping())
            && let Some(yaml_edit::YamlNode::Sequence(list)) = mapping.get("projects")
        {
            for item in list.values() {
                if let yaml_edit::YamlNode::Mapping(m) = item
                    && item_book(&m) == Some(book)
                    && m.get("title").is_none()
                {
                    m.set("title", short);
                }
            }
        }
        *text = file.to_string();
    }
    Ok(())
}

/// `name` written as `was` was: single-quoted, double-quoted, or plain where
/// plain reads back as the same text.
fn requoted(was: &str, name: &str) -> String {
    let one_line = !name.chars().any(char::is_control);
    if was.starts_with('\'') && one_line {
        format!("'{}'", name.replace('\'', "''"))
    } else if was.starts_with('"') {
        let mut out = String::new();
        crate::tree::json_string(name, &mut out);
        out
    } else {
        yaml_string(name)
    }
}

fn rc_names(text: &str) -> Vec<BookNames> {
    project_items(text)
        .unwrap_or_default()
        .iter()
        .filter_map(|m| {
            let short = match m.get("title") {
                Some(yaml_edit::YamlNode::Scalar(s)) => {
                    Some(s.as_string()).filter(|t| !t.is_empty())
                }
                _ => None,
            };
            Some(BookNames {
                book: item_book(m)?,
                short,
                long: None,
                abbr: None,
            })
        })
        .collect()
}

/// The `projects` list's items: each one's book, and the byte offset its
/// line starts at. An error for a `projects` that isn't a block list.
fn projects(text: &str) -> Result<Vec<(Option<BookKey>, usize)>, String> {
    let file = yaml_edit::YamlFile::from_str(text).map_err(|e| e.to_string())?;
    let mapping = file
        .documents()
        .find_map(|d| d.as_mapping())
        .ok_or("the manifest isn't a mapping")?;
    let Some(list) = mapping.get("projects") else {
        return Ok(Vec::new());
    };
    let yaml_edit::YamlNode::Sequence(list) = list else {
        return Err("projects isn't a list".into());
    };
    if list.is_flow_style() {
        return Err("projects is a flow list ([…]); bento adds to block lists".into());
    }
    Ok(list
        .values()
        .map(|item| {
            let book = match &item {
                yaml_edit::YamlNode::Mapping(m) => item_book(m),
                _ => None,
            };
            // The item's line is its dash's, which may stand alone above
            // the item's first key (`-` then `    identifier: gen`).
            let start = item.byte_range().start as usize;
            let before = text[..start].trim_end_matches([' ', '\t', '\r', '\n']);
            let dash = if before.ends_with('-') {
                before.len() - 1
            } else {
                start
            };
            (book, text[..dash].rfind('\n').map_or(0, |n| n + 1))
        })
        .collect())
}

/// Insert `entry` as item `at` of the `projects` list (last when `None`), in
/// the list's own indentation, as text: every other byte stays as it was.
fn splice_project(
    text: &mut String,
    projects: &[(Option<BookKey>, usize)],
    at: Option<usize>,
    entry: &Node,
    indent_lists: bool,
) -> Result<(), String> {
    let mut rendered = String::new();
    yaml_list(std::slice::from_ref(entry), 0, indent_lists, &mut rendered);
    let Some(&(_, first)) = projects.first() else {
        // No list yet: a new `projects:` key at the end of the file.
        if text
            .lines()
            .any(|l| l.trim_end() == "projects:" || l.starts_with("projects:"))
        {
            return Err("projects is there but empty or not a list".into());
        }
        // At the end of the first document: before a `...` or `---` that
        // closes it, else the file's end.
        let mut end = text.len();
        let mut seen = false;
        let mut cursor = 0;
        for line in text.split_inclusive('\n') {
            let bare = line.trim_end();
            let marker = bare == "..." || bare == "---" || bare.starts_with("--- ");
            if marker && (seen || bare == "...") {
                end = cursor;
                break;
            }
            seen |= !marker && !bare.trim_start().is_empty() && !bare.trim_start().starts_with('#');
            cursor += line.len();
        }
        let mut block = String::new();
        if end == text.len() && !text.is_empty() && !text.ends_with('\n') {
            block.push('\n');
        }
        block.push_str("projects:\n");
        let pad = if indent_lists { "  " } else { "" };
        for line in rendered.lines() {
            block.push_str(pad);
            block.push_str(line);
            block.push('\n');
        }
        text.insert_str(end, &block);
        return Ok(());
    };
    let pad: String = text[first..].chars().take_while(|c| *c == ' ').collect();
    let indented: String = rendered.lines().map(|l| format!("{pad}{l}\n")).collect();
    let offset = match at {
        Some(i) => projects[i].1,
        // After the last item: the end of its last line, which is where the
        // next line indented no deeper than the list starts (or the file
        // ends). Blank and comment lines don't end the item, since a comment
        // may sit at any indent inside it; a run of them just before the end
        // stays after the new item.
        None => {
            let last = projects[projects.len() - 1].1;
            let mut offset = text.len();
            let mut cursor = text[last..].find('\n').map_or(text.len(), |n| last + n + 1);
            let mut quiet: Option<usize> = None;
            while cursor < text.len() {
                let line = &text[cursor..];
                let line_end = line.find('\n').map_or(text.len(), |n| cursor + n + 1);
                let content = &text[cursor..line_end];
                let depth = content.chars().take_while(|c| *c == ' ').count();
                let trimmed = content.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    quiet.get_or_insert(cursor);
                } else if depth <= pad.len() {
                    offset = quiet.unwrap_or(cursor);
                    break;
                } else {
                    quiet = None;
                }
                cursor = line_end;
            }
            if offset == text.len() {
                offset = quiet.unwrap_or(offset);
            }
            if offset == text.len() && !text.ends_with('\n') {
                text.push('\n');
                offset = text.len();
            }
            offset
        }
    };
    text.insert_str(offset, &indented);
    Ok(())
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
        let item_line = item.trim_start();
        if key.trim_end().ends_with(':')
            && (item_line.starts_with("- ") || item_line.trim_end() == "-")
        {
            let indent = |l: &str| l.len() - l.trim_start().len();
            return indent(item) > indent(key);
        }
    }
    true
}
