//! Creating a Bible project's manifests: right from the start.
//!
//! ```text
//! create_bible(&NewBibleProject { books: [MAT, ROM], language: tpi, repository: "Tech_Advance/tpi_reg", … })
//!   → [("metadata.json", …), ("manifest.yaml", …)]
//!
//!   order, sort, file names, categories   from canon: MAT is 41-MAT.usfm, sort 40
//!   names                                 under "en" and the project's tag; defaultLocale "en"
//!   book names                            MAT: {"en": "Matthew", "tpi": "Matyu"}: canon's English, the language's own
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
    let project = &trimmed(project);
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

/// The project with its names trimmed: the fork's localized text can't
/// start or end with space (`trimmedText`), and nor should a name anywhere.
fn trimmed(p: &NewBibleProject) -> NewBibleProject {
    NewBibleProject {
        title: p.title.trim().to_owned(),
        abbreviation: p.abbreviation.trim().to_owned(),
        language: NewLanguage {
            tag: p.language.tag.trim().to_owned(),
            name: p.language.name.trim().to_owned(),
            direction: p.language.direction,
        },
        books: trimmed_books(&p.books),
        ..p.clone()
    }
}

pub(crate) fn trimmed_books(books: &[NewBook]) -> Vec<NewBook> {
    books
        .iter()
        .map(|b| NewBook {
            title: b.title.trim().to_owned(),
            ..b.clone()
        })
        .collect()
}

/// The id a WACS catalog source has as a Burrito relationship.
pub(crate) fn source_id(source: &Related) -> String {
    format!("wacs::WA-Catalog/{}_{}", source.language, source.resource)
}

fn text(s: &str) -> Node {
    Node::text(s)
}

/// A name under `en` and under the project's own tag (once, when the tag is
/// `en`). WA's fork requires an `en` name for a language, and `defaultLocale`
/// is `en`, which upstream says every name field must carry (Will,
/// 2026-11-05: `en` stays the default locale, the project's own tag beside it).
pub(crate) fn named(tag: &str, s: &str) -> Node {
    Node::map(
        [("en", text(s))]
            .into_iter()
            .chain((tag != "en").then(|| (tag, text(s)))),
    )
}

/// A book's short name: canon's English under `en`, the language's own under
/// its tag (Will, 2026-11-12). An `en` project's own name is its English one.
pub(crate) fn book_name(tag: &str, book: BookKey, title: &str) -> Node {
    let english = canon::book(book).map_or(title, |b| b.english);
    if tag == "en" || tag.is_empty() {
        return Node::map([("en", text(if tag.is_empty() { english } else { title }))]);
    }
    Node::map([("en", text(english)), (tag, text(title))])
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
                ("defaultLocale", text("en")),
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
                ("name", named(tag, &p.title)),
                ("abbreviation", named(tag, &p.abbreviation)),
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
                ("name", named(tag, &p.language.name)),
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
                    Node::map([("short", book_name(tag, b.book.book, &b.book.title))]),
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

/// Plain when no YAML reader would take it for anything but this text, else
/// double-quoted with JSON's escapes (which YAML's double quotes share).
///
/// Quoted: YAML 1.2's null, booleans and numbers (`null`, `true`, `12`,
/// `1.10`, `0x1F`, `.inf`), YAML 1.1's booleans too (`yes`, `off`: older
/// readers still take them so), a leading indicator (`-`, `[`, `&`, …), a `: `
/// or ` #`, quotes, braces, brackets, commas, control characters, and space
/// at either end. Decided by rule, not by parsing, so a module that writes a
/// manifest needn't carry a YAML parser; `tests` checks the rule against one.
pub(crate) fn yaml_string(s: &str) -> String {
    if is_plain_safe(s) {
        s.to_owned()
    } else {
        let mut out = String::new();
        crate::tree::json_string(s, &mut out);
        out
    }
}

fn is_plain_safe(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    if s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) || s.ends_with(':') {
        return false;
    }
    if "-?:,[]{}#&*!|>'\"%@`~".contains(first) {
        return false;
    }
    if s.chars()
        .any(|c| c.is_control() || "\"'{}[],#`".contains(c))
        || s.contains(": ")
    {
        return false;
    }
    !looks_typed(s)
}

/// What a YAML reader (1.2's core schema, or 1.1's) would read as other than text.
fn looks_typed(s: &str) -> bool {
    const WORDS: &[&str] = &[
        "null", "Null", "NULL", "~", "true", "True", "TRUE", "false", "False", "FALSE", "y", "Y",
        "yes", "Yes", "YES", "n", "N", "no", "No", "NO", "on", "On", "ON", "off", "Off", "OFF",
        ".inf", ".Inf", ".INF", "+.inf", "-.inf", ".nan", ".NaN", ".NAN",
    ];
    if WORDS.contains(&s) {
        return true;
    }
    let digits =
        |t: &str, radix: u32| !t.is_empty() && t.chars().all(|c| c.is_digit(radix) || c == '_');
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    if let Some(hex) = unsigned.strip_prefix("0x") {
        return digits(hex, 16);
    }
    if let Some(oct) = unsigned.strip_prefix("0o") {
        return digits(oct, 8);
    }
    // Decimal, with an optional fraction and exponent: `12`, `1.10`, `.5`, `1e3`, `1_000`.
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
        None => (unsigned, None),
    };
    let mantissa_ok = match mantissa.split_once('.') {
        Some((whole, frac)) => {
            (whole.is_empty() || digits(whole, 10))
                && (frac.is_empty() || digits(frac, 10))
                && !(whole.is_empty() && frac.is_empty())
        }
        None => digits(mantissa, 10),
    };
    let exponent_ok = exponent.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e), 10));
    // YAML 1.1 sexagesimal (`1:30`) has a colon; dates (`2026-11-05`) read as
    // text in 1.2 and are left plain, as Sefer's and the catalog's writers do.
    mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests;
