//! Creating a Bible project's manifests: right from the start.
//!
//! ```text
//! create_bible(&NewBibleProject { books: [MAT, ROM], language: tpi, repository: "Tech_Advance/tpi_reg", … })
//!   → [("metadata.json", …), ("manifest.yaml", …)]
//!
//!   order, sort, file names, categories   from canon: MAT is 41-MAT.usfm, sort 40
//!   identification.primary                {"wacs": {"Tech_Advance/tpi_reg": {"revision": "latest", "timestamp": "2026-11-05"}}}
//!   relationships                         wacs::WA-Catalog/en_ulb
//!   checksums                             md5 and size of each book's bytes
//! ```
//!
//! A caller names books by code and hands over their bytes; it can't get an
//! order, a `sort` or a path wrong. What each app decides (publisher, creator,
//! rights, checking level, copyright) is a required argument, with no
//! defaults here. Everything written passes WA's fork of the Burrito schema
//! (`tests/schema.rs`).

use canon::Canon;
use mise::books::BookKey;

use crate::tree::Node;
use crate::{Direction, Format, Related};

/// What a new Bible project is. Every field is required: each app states its
/// own policy.
#[derive(Debug, Clone)]
pub struct NewBibleProject {
    /// The project's name, in its language: `Baibel`.
    pub title: String,
    /// The resource's code: `reg`, `ulb`.
    pub abbreviation: String,
    pub language: NewLanguage,
    /// In any order; written in the canon's.
    pub books: Vec<NewBook>,
    /// Which books may be listed, and their order.
    pub canon: Canon,
    pub sources: Vec<Related>,
    /// Kept in order, each once: `Mary_Kila` and ` mary kila ` are one.
    pub contributors: Vec<String>,
    pub publisher: String,
    pub creator: String,
    /// `CC BY-SA 4.0`
    pub rights: String,
    /// `1`, `2`, `3`
    pub checking_level: String,
    /// The Burrito's copyright statement: `Licensed under CC BY-SA 4.0.`
    pub copyright: String,
    /// The app writing the files: `Sefer`, `1.4.0`.
    pub generator: (String, String),
    /// An ISO 8601 date-time, by the caller's clock: `2026-11-05T14:30:00Z`.
    pub created_at: String,
    /// The project's WACS repo, `owner/repo`: the Burrito's primary id.
    pub repository: String,
}

#[derive(Debug, Clone)]
pub struct NewLanguage {
    pub tag: String,
    pub name: String,
    pub direction: Direction,
}

#[derive(Debug, Clone)]
pub struct NewBook {
    pub book: BookKey,
    /// The book's name in the project's language.
    pub title: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateError {
    /// A book that isn't in the chosen canon.
    NotInCanon(BookKey),
    /// The same book twice.
    Twice(BookKey),
    NoBooks,
    /// `repository` isn't `owner/repo`.
    Repository(String),
    /// `created_at` isn't a date-time.
    CreatedAt(String),
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateError::NotInCanon(book) => write!(f, "{book} isn't a book of the chosen canon"),
            CreateError::Twice(book) => write!(f, "{book} is listed twice"),
            CreateError::NoBooks => write!(f, "a Bible project needs at least one book"),
            CreateError::Repository(r) => write!(f, "\"{r}\" isn't an owner/repo path"),
            CreateError::CreatedAt(t) => {
                write!(f, "\"{t}\" isn't an ISO date-time (2026-11-05T14:30:00Z)")
            }
        }
    }
}

impl std::error::Error for CreateError {}

/// A book's MD5 and size: what a Burrito ingredient records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub md5: String,
    pub size: u64,
}

pub fn fingerprint(bytes: &[u8]) -> Fingerprint {
    Fingerprint {
        md5: crate::md5::md5_hex(bytes),
        size: bytes.len() as u64,
    }
}

/// One book, placed by canon.
pub(crate) struct Placed<'a> {
    pub book: &'a NewBook,
    /// From 1, in the canon's order: RC's `sort`.
    pub sort: usize,
    /// `41-MAT.usfm`
    pub file: String,
    pub category: &'static str,
}

pub(crate) fn place(books: &[NewBook], canon: Canon) -> Result<Vec<Placed<'_>>, CreateError> {
    let order: Vec<BookKey> = canon::books(canon).map(|b| b.code).collect();
    let mut placed = Vec::with_capacity(books.len());
    for (i, book) in books.iter().enumerate() {
        if books[..i].iter().any(|b| b.book == book.book) {
            return Err(CreateError::Twice(book.book));
        }
        let at = order
            .iter()
            .position(|&k| k == book.book)
            .ok_or(CreateError::NotInCanon(book.book))?;
        let row = canon::book(book.book).ok_or(CreateError::NotInCanon(book.book))?;
        placed.push(Placed {
            book,
            sort: at + 1,
            file: canon::usfm_file_name(book.book).unwrap_or_else(|| format!("{}.usfm", book.book)),
            category: match row.division {
                canon::Division::Nt => "bible-nt",
                _ => "bible-ot",
            },
        });
    }
    placed.sort_by_key(|p| p.sort);
    Ok(placed)
}

/// The manifests for a new Bible project, by file name: `metadata.json` and
/// `manifest.yaml`, or only the `formats` asked for (BTT Writer's isn't one
/// we write).
pub fn create_bible(
    project: &NewBibleProject,
    formats: &[Format],
) -> Result<Vec<(String, Vec<u8>)>, CreateError> {
    if project.books.is_empty() {
        return Err(CreateError::NoBooks);
    }
    let placed = place(&project.books, project.canon)?;
    if !crate::burrito::is_repository_path(&project.repository) {
        return Err(CreateError::Repository(project.repository.clone()));
    }
    let date = project
        .created_at
        .get(..10)
        .filter(|_| crate::burrito::is_date_time(&project.created_at));
    let Some(date) = date else {
        return Err(CreateError::CreatedAt(project.created_at.clone()));
    };
    let mut out = Vec::new();
    if formats.contains(&Format::Burrito) {
        out.push((
            "metadata.json".to_owned(),
            burrito(project, &placed, date).to_json().into_bytes(),
        ));
    }
    if formats.contains(&Format::Rc) {
        out.push((
            "manifest.yaml".to_owned(),
            to_yaml(&rc(project, &placed, date)).into_bytes(),
        ));
    }
    Ok(out)
}

/// The id a WACS catalog source has as a Burrito relationship.
pub(crate) fn source_id(source: &Related) -> String {
    format!("wacs::WA-Catalog/{}_{}", source.language, source.resource)
}

fn text(s: &str) -> Node {
    Node::text(s)
}

fn localized(tag: &str, s: &str) -> Node {
    Node::map([(tag, text(s))])
}

fn direction(d: Direction) -> &'static str {
    match d {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// An ingredient for one book.
pub(crate) fn ingredient(book: &NewBook) -> Node {
    let print = fingerprint(&book.bytes);
    Node::map([
        ("checksum", Node::map([("md5", text(&print.md5))])),
        ("mimeType", text("text/x-usfm")),
        ("size", Node::Number(print.size.to_string())),
        (
            "scope",
            Node::map([(book.book.to_string(), Node::List(Vec::new()))]),
        ),
    ])
}

fn burrito(p: &NewBibleProject, placed: &[Placed<'_>], date: &str) -> Node {
    let tag = p.language.tag.as_str();
    Node::map([
        ("format", text("scripture burrito")),
        (
            "meta",
            Node::map([
                ("version", text("1.0.0")),
                ("category", text("source")),
                (
                    "generator",
                    Node::map([
                        ("softwareName", text(&p.generator.0)),
                        ("softwareVersion", text(&p.generator.1)),
                    ]),
                ),
                ("defaultLocale", text(tag)),
                ("dateCreated", text(&p.created_at)),
            ]),
        ),
        (
            "idAuthorities",
            Node::map([(
                "wacs",
                Node::map([
                    ("id", text("https://content.bibletranslationtools.org")),
                    (
                        "name",
                        localized("en", "Wycliffe Associates Content Services"),
                    ),
                ]),
            )]),
        ),
        (
            "identification",
            Node::map([
                ("name", localized(tag, &p.title)),
                ("abbreviation", localized(tag, &p.abbreviation)),
                (
                    "primary",
                    Node::map([(
                        "wacs",
                        Node::map([(
                            p.repository.as_str(),
                            Node::map([("revision", text("latest")), ("timestamp", text(date))]),
                        )]),
                    )]),
                ),
            ]),
        ),
        ("confidential", Node::Bool(false)),
        (
            "type",
            Node::map([(
                "flavorType",
                Node::map([
                    ("name", text("scripture")),
                    (
                        "flavor",
                        Node::map([
                            ("name", text("textTranslation")),
                            ("projectType", text("standard")),
                            ("translationType", text("newTranslation")),
                            ("audience", text("common")),
                            ("usfmVersion", text("3.0")),
                        ]),
                    ),
                    (
                        "currentScope",
                        Node::map(
                            placed
                                .iter()
                                .map(|b| (b.book.book.to_string(), Node::List(Vec::new()))),
                        ),
                    ),
                ]),
            )]),
        ),
        (
            "copyright",
            Node::map([(
                "shortStatements",
                Node::List(vec![Node::map([
                    ("statement", text(&p.copyright)),
                    ("mimetype", text("text/plain")),
                ])]),
            )]),
        ),
        (
            "languages",
            Node::List(vec![Node::map([
                ("tag", text(tag)),
                // The fork requires an `en` name; Sefer writes the same name under both.
                (
                    "name",
                    Node::map(
                        [("en", text(&p.language.name))]
                            .into_iter()
                            .chain((tag != "en").then(|| (tag, text(&p.language.name)))),
                    ),
                ),
                ("scriptDirection", text(direction(p.language.direction))),
            ])]),
        ),
        (
            "ingredients",
            Node::map(placed.iter().map(|b| (b.file.clone(), ingredient(b.book)))),
        ),
        (
            "localizedNames",
            Node::map(placed.iter().map(|b| {
                (
                    b.book.book.to_string(),
                    Node::map([("short", localized(tag, &b.book.title))]),
                )
            })),
        ),
        (
            "relationships",
            Node::List(
                p.sources
                    .iter()
                    .map(|s| {
                        let mut r = vec![
                            ("relationType", text("source")),
                            ("flavor", text("textTranslation")),
                            ("id", text(&source_id(s))),
                        ];
                        if let Some(v) = &s.version {
                            r.push(("revision", text(v)));
                        }
                        Node::map(r)
                    })
                    .collect(),
            ),
        ),
    ])
}

/// An RC project entry for one book.
pub(crate) fn rc_project(b: &Placed<'_>) -> Node {
    Node::map([
        ("title", text(&b.book.title)),
        (
            "identifier",
            text(&b.book.book.to_string().to_ascii_lowercase()),
        ),
        ("path", text(&format!("./{}", b.file))),
        ("sort", Node::Number(b.sort.to_string())),
        ("categories", Node::List(vec![text(b.category)])),
        ("versification", text("ufw")),
    ])
}

/// Contributors in order, each once: case, underscores and runs of spaces
/// don't make a second one (Sefer's `dedupeContributors`).
pub(crate) fn contributors(names: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for name in names {
        let key = name
            .replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(name.trim().to_owned());
    }
    out
}

fn rc(p: &NewBibleProject, placed: &[Placed<'_>], date: &str) -> Node {
    let tag = p.language.tag.as_str();
    Node::map([
        (
            "dublin_core",
            Node::map([
                ("conformsto", text("rc0.2")),
                (
                    "contributor",
                    Node::List(
                        contributors(&p.contributors)
                            .iter()
                            .map(|c| text(c))
                            .collect(),
                    ),
                ),
                ("creator", text(&p.creator)),
                ("format", text("text/usfm")),
                ("identifier", text(&p.abbreviation)),
                ("issued", text(date)),
                (
                    "language",
                    Node::map([
                        ("direction", text(direction(p.language.direction))),
                        ("identifier", text(tag)),
                        ("title", text(&p.language.name)),
                    ]),
                ),
                ("modified", text(date)),
                ("publisher", text(&p.publisher)),
                (
                    "relation",
                    Node::List(vec![text(&format!("{tag}/{}", p.abbreviation))]),
                ),
                ("rights", text(&p.rights)),
                (
                    "source",
                    Node::List(
                        p.sources
                            .iter()
                            .map(|s| {
                                let mut e = vec![
                                    ("identifier", text(&s.resource)),
                                    ("language", text(&s.language)),
                                ];
                                if let Some(v) = &s.version {
                                    e.push(("version", text(v)));
                                }
                                Node::map(e)
                            })
                            .collect(),
                    ),
                ),
                ("subject", text("Bible")),
                ("title", text(&p.title)),
                ("type", text("bundle")),
                ("version", text("1")),
            ]),
        ),
        (
            "checking",
            Node::map([
                ("checking_entity", Node::List(Vec::new())),
                ("checking_level", text(&p.checking_level)),
            ]),
        ),
        (
            "projects",
            Node::List(placed.iter().map(rc_project).collect()),
        ),
    ])
}

/// Block YAML, two spaces a level, a list's items indented under their key.
/// A string is written plain when it would read back as the same string,
/// else double-quoted.
pub(crate) fn to_yaml(node: &Node) -> String {
    let mut out = String::new();
    yaml_map_body(node, 0, true, &mut out);
    out
}

fn yaml_map_body(node: &Node, depth: usize, indent_lists: bool, out: &mut String) {
    for (key, value) in node.entries() {
        out.push_str(&"  ".repeat(depth));
        out.push_str(key);
        out.push(':');
        yaml_after_key(value, depth, indent_lists, out);
    }
}

fn yaml_after_key(value: &Node, depth: usize, indent_lists: bool, out: &mut String) {
    match value {
        Node::Map(entries) if !entries.is_empty() => {
            out.push('\n');
            yaml_map_body(value, depth + 1, indent_lists, out);
        }
        Node::List(items) if !items.is_empty() => {
            out.push('\n');
            yaml_list(
                items,
                if indent_lists { depth + 1 } else { depth },
                indent_lists,
                out,
            );
        }
        scalar => {
            out.push(' ');
            out.push_str(&yaml_scalar(scalar));
            out.push('\n');
        }
    }
}

/// A block list at `depth`. With `indent_lists` off, a list under a key sits
/// at the key's own indent (`categories:` then `- bible-nt`), as the catalog
/// writes it.
pub(crate) fn yaml_list(items: &[Node], depth: usize, indent_lists: bool, out: &mut String) {
    for item in items {
        out.push_str(&"  ".repeat(depth));
        out.push('-');
        match item {
            Node::Map(_) if !item.entries().is_empty() => {
                // The first entry on the dash's line, the rest under it.
                let mut first = true;
                for (key, value) in item.entries() {
                    if first {
                        out.push(' ');
                        first = false;
                    } else {
                        out.push_str(&"  ".repeat(depth + 1));
                    }
                    out.push_str(key);
                    out.push(':');
                    yaml_after_key(value, depth + 1, indent_lists, out);
                }
            }
            Node::List(inner) if !inner.is_empty() => {
                out.push('\n');
                yaml_list(inner, depth + 1, indent_lists, out);
            }
            scalar => {
                out.push(' ');
                out.push_str(&yaml_scalar(scalar));
                out.push('\n');
            }
        }
    }
}

fn yaml_scalar(node: &Node) -> String {
    match node {
        Node::Null => "null".into(),
        Node::Bool(b) => b.to_string(),
        Node::Number(n) => n.clone(),
        Node::String(s) => yaml_string(s),
        Node::List(_) => "[]".into(),
        Node::Map(_) => "{}".into(),
    }
}

/// Plain when `key: <s>` reads back as the string `s`; else double-quoted,
/// with JSON's escapes (which YAML's double quotes share).
pub(crate) fn yaml_string(s: &str) -> String {
    let plain_ok = !s.is_empty()
        && !s.contains([
            '\n', '\r', '\t', '"', '\'', '#', '{', '}', '[', ']', ',', '&', '*', '!', '|', '>',
            '%', '@', '`',
        ])
        && !s.starts_with(['-', '?', ':', ' '])
        && !s.ends_with([' ', ':'])
        && !s.contains(": ")
        && crate::tree::from_yaml(&format!("k: {s}\n"))
            .is_ok_and(|r| r.node.get("k") == Some(&Node::String(s.to_owned())));
    if plain_ok {
        s.to_owned()
    } else {
        let mut out = String::new();
        crate::tree::json_string(s, &mut out);
        out
    }
}

#[cfg(test)]
mod tests;
