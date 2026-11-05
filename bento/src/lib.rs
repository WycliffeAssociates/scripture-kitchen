//! Scripture containers: the manifest at a repo's root that says what the repo is.
//!
//! ```text
//! find(["README.md", "manifest.yaml"])    → Some(("manifest.yaml", Format::Rc))
//! read("manifest.yaml", bytes)            → Read {
//!                                              manifest: Manifest { language: llx, kind: Bible, books: [PSA, PHM, …], … },
//!                                              detail:   Detail::Rc(RcManifest { dublin_core, checking, projects }),
//!                                              findings: [ManifestFinding { check: SortCanon, … }, …] }
//! check(&read, &Repo { name: Some("Tech_Advance/llx_reg"), files: Some(&files) })  → findings that need the repo
//! create_bible(&NewBibleProject { books: [MAT, ROM], … }, &[Burrito, Rc])  → metadata.json, manifest.yaml
//! edit(&manifests, &Change { add_books: [MRK], checksums: [(path, bytes)], .. })  → only the files that changed
//!
//! manifest.json   BTT Writer, one book        bttwriter::read(bytes) → BttWriterManifest
//! manifest.yaml   Resource Container 0.2      rc
//! metadata.json   Scripture Burrito 1.0       burrito
//! ```
//!
//! Two rules. **Reading never changes anything**: [`read`] and [`check`]
//! report what's there and what's wrong with it, as [`ManifestFinding`]s,
//! and never "fix" a value on the way through. **Reading never fails**: a
//! number where text belongs, one value where a list belongs, a byte order
//! mark, a second YAML document are read anyway and said as findings; a file
//! that can't be read at all is an error finding with no manifest.
//!
//! [`Manifest`] is what every format has (language, books, sources), so most
//! callers never branch on the format. [`Detail`] is the format's own typed
//! document, for what only one format has.

pub mod bttwriter;
pub mod burrito;
mod check;
mod edit;
mod kind;
mod lenient;
mod md5;
pub mod rc;
mod related;
pub mod tree;
mod write;

use std::collections::BTreeMap;

use mise::books::BookKey;

pub use check::{Repo, check};
pub use edit::{Change, EditError, Edited, edit};
pub use kind::Domain;
pub use write::{
    CreateError, Fingerprint, NewBibleProject, NewBook, NewLanguage, create_bible, fingerprint,
};

/// Which container a manifest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Scripture Burrito, `metadata.json`.
    Burrito,
    /// Resource Container, `manifest.yaml`.
    Rc,
    /// BTT Writer (and translationStudio, translationCore), `manifest.json`.
    BttWriter,
}

/// Root file names a manifest has, in the order [`find`] prefers them.
///
/// Matched whole and in any case, so `org-manifest.json` and `manifest
/// (2).json` (both seen on WACS) aren't manifests. A Burrito names its
/// format; an RC is next; a Writer `manifest.json` is last, because exports
/// can leave one beside the manifest that describes the repo.
pub const MANIFEST_NAMES: &[(&str, Format)] = &[
    ("metadata.json", Format::Burrito),
    ("manifest.yaml", Format::Rc),
    ("manifest.yml", Format::Rc),
    ("manifest.json", Format::BttWriter),
];

/// The manifest among a repo's root file names, as given, and its format.
/// `None` is the [`Check::ManifestMissing`] finding ([`ManifestFinding::missing`]).
pub fn find<S: AsRef<str>>(root_names: &[S]) -> Option<(&str, Format)> {
    MANIFEST_NAMES.iter().find_map(|(want, format)| {
        root_names
            .iter()
            .map(AsRef::as_ref)
            .find(|name| name.eq_ignore_ascii_case(want))
            .map(|name| (name, *format))
    })
}

/// The format a manifest's file name says: by suffix and in any case, so
/// `en_ulb.manifest.yaml` is an RC.
pub fn format_of(name: &str) -> Option<Format> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with("metadata.json") {
        Some(Format::Burrito)
    } else if lower.ends_with("manifest.yaml") || lower.ends_with("manifest.yml") {
        Some(Format::Rc)
    } else if lower.ends_with("manifest.json") {
        Some(Format::BttWriter)
    } else {
        None
    }
}

/// What reading a manifest gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    /// `None` only when nothing could be read (an error finding says why).
    pub manifest: Option<Manifest>,
    pub detail: Option<Detail>,
    /// What reading had to forgive, couldn't read, or found wrong in the
    /// manifest alone.
    pub findings: Vec<ManifestFinding>,
}

impl Read {
    /// True when any finding is an error.
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }
}

/// The format's own document.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    Rc(rc::RcManifest),
    Burrito(burrito::BurritoMetadata),
    BttWriter(bttwriter::BttWriterManifest),
}

/// Text in several languages, by tag: `{"en": "Romans", "tpi": "Rom"}`.
pub type LocalizedText = BTreeMap<String, String>;

/// What every format says about a repo.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub format: Format,
    /// A Burrito's primary id (`Tech_Advance/cdi_reg`); `None` for RC and BTT Writer.
    pub id: Option<String>,
    /// `English Unlocked Literal Bible`
    pub name: LocalizedText,
    /// The resource's code: `ulb`, `reg`.
    pub abbreviation: LocalizedText,
    pub language: Language,
    pub kind: Kind,
    /// In the order the file lists them; only entries naming a book of canon's table.
    pub books: Vec<BookEntry>,
    /// What it was translated from.
    pub sources: Vec<Related>,
    pub version: Option<String>,
    pub issued: Option<String>,
    pub modified: Option<String>,
    pub rights: Option<String>,
    pub publisher: Option<String>,
}

/// The repo's own language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    /// As written: `en`, `tpi`. Empty when the manifest names none.
    pub tag: String,
    pub name: LocalizedText,
    pub direction: Direction,
}

/// What kind of resource a repo is, in one vocabulary for every format.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Kind {
    pub domain: Option<Domain>,
    /// Canonical: `Bible`, `Translation Notes`, `Open Bible Stories`. A
    /// subject we don't know comes back as written.
    pub subject: Option<String>,
    /// A Burrito's flavor, as written: `textTranslation`.
    pub flavor: Option<String>,
    /// What the manifest itself says.
    pub declared: Declared,
}

/// A manifest's own words for its kind, as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Declared {
    pub subject: Option<String>,
    pub r#type: Option<String>,
    /// The content's media type: `text/usfm`.
    pub format: Option<String>,
    pub identifier: Option<String>,
}

/// One book a manifest lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookEntry {
    pub book: BookKey,
    /// `./41-MAT.usfm`, as written.
    pub path: Option<String>,
    /// The name the manifest gives it.
    pub title: LocalizedText,
    /// RC's `sort`, as written; never corrected.
    pub sort: Option<i64>,
}

/// A resource a container says it was translated from: `en/ulb`, version `12`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    /// Lowercase language tag: `en`.
    pub language: String,
    /// Lowercase resource id: `ulb`.
    pub resource: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

/// One thing a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestFinding {
    pub check: Check,
    pub severity: Severity,
    /// A JSON pointer into the manifest: `/dublin_core/language/identifier`,
    /// `/ingredients/.~141-MAT.usfm/size`.
    pub at: Option<String>,
    pub book: Option<BookKey>,
    /// A sentence a person can act on.
    pub message: String,
}

impl ManifestFinding {
    /// A repo with no manifest at its root.
    pub fn missing() -> ManifestFinding {
        ManifestFinding {
            check: Check::ManifestMissing,
            severity: Severity::Error,
            at: None,
            book: None,
            message: "The repo has no metadata.json, manifest.yaml or manifest.json at its root."
                .into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// The checks, each with a stable id, so a page can list them as passed or
/// failed, as Door43's health check does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Check {
    /// No manifest at the repo's root.
    ManifestMissing,
    /// Not JSON or YAML, or not an object: nothing could be read.
    ManifestUnreadable,
    /// A field its format requires is missing, or of the wrong shape.
    ManifestValid,
    /// A value read leniently: a number written for text, one value for a list.
    LenientValue,
    /// The resource has a name of its own (not empty, not its code).
    TitleSet,
    /// The manifest names its language.
    LanguageSet,
    /// A non-English project's book titles aren't canon's English names.
    BookTitleTranslated,
    /// An RC project's `sort` is canon's place for the book.
    SortCanon,
    /// The repo's name starts with the manifest's language (`tpi_reg` for `tpi`).
    RepoNameLanguage,
    /// Each file the manifest lists is in the repo.
    IngredientExists,
    /// Each file the manifest lists has something in it.
    IngredientEmpty,
    /// Each Burrito ingredient's size and md5 are its file's.
    IngredientChecksum,
}

impl Check {
    pub const ALL: &[Check] = &[
        Check::ManifestMissing,
        Check::ManifestUnreadable,
        Check::ManifestValid,
        Check::LenientValue,
        Check::TitleSet,
        Check::LanguageSet,
        Check::BookTitleTranslated,
        Check::SortCanon,
        Check::RepoNameLanguage,
        Check::IngredientExists,
        Check::IngredientEmpty,
        Check::IngredientChecksum,
    ];

    /// The stable id: `title-set`.
    pub fn id(self) -> &'static str {
        match self {
            Check::ManifestMissing => "manifest-missing",
            Check::ManifestUnreadable => "manifest-unreadable",
            Check::ManifestValid => "manifest-valid",
            Check::LenientValue => "lenient-value",
            Check::TitleSet => "title-set",
            Check::LanguageSet => "language-set",
            Check::BookTitleTranslated => "book-title-translated",
            Check::SortCanon => "sort-canon",
            Check::RepoNameLanguage => "repo-name-language",
            Check::IngredientExists => "ingredient-exists",
            Check::IngredientEmpty => "ingredient-empty",
            Check::IngredientChecksum => "ingredient-checksum",
        }
    }
}

/// Read a manifest's bytes, by its file name's format ([`format_of`]).
pub fn read(name: &str, bytes: &[u8]) -> Read {
    match format_of(name) {
        Some(format) => read_as(format, bytes),
        None => unreadable(format!(
            "{name} isn't a manifest's name (metadata.json, manifest.yaml, manifest.json)."
        )),
    }
}

/// Read a manifest's bytes as `format`. A leading byte order mark is skipped.
pub fn read_as(format: Format, bytes: &[u8]) -> Read {
    let bytes = without_bom(bytes);
    let mut findings = Vec::new();
    let node = match format {
        Format::Rc => {
            let Ok(text) = std::str::from_utf8(bytes) else {
                return unreadable("The manifest isn't UTF-8 text.".into());
            };
            match tree::from_yaml(text) {
                Ok(read) => {
                    if read.documents > 1 {
                        findings.push(lenient::finding(
                            Check::LenientValue,
                            Severity::Info,
                            None,
                            format!("The file holds {} YAML documents; the first that says anything was read.", read.documents),
                        ));
                    }
                    read.node
                }
                Err(why) => return unreadable(format!("The manifest isn't YAML: {why}")),
            }
        }
        Format::Burrito | Format::BttWriter => match tree::from_json(bytes) {
            Ok(node) => node,
            Err(why) => return unreadable(format!("The manifest isn't JSON: {why}")),
        },
    };
    if !matches!(node, tree::Node::Map(_)) {
        return unreadable(format!("The manifest is {}, not an object.", node.kind()));
    }
    let mut reader = lenient::Reader::new(findings);
    let (manifest, detail) = match format {
        Format::Rc => {
            let doc = rc::from_node(&node, &mut reader);
            (doc.manifest(&mut reader), Detail::Rc(doc))
        }
        Format::Burrito => {
            let doc = burrito::from_node(&node, &mut reader);
            (doc.manifest(&mut reader), Detail::Burrito(doc))
        }
        Format::BttWriter => {
            let doc = bttwriter::from_node(&node, &mut reader);
            (doc.manifest(), Detail::BttWriter(doc))
        }
    };
    let mut findings = reader.into_findings();
    check::manifest_only(&manifest, &detail, &mut findings);
    Read {
        manifest: Some(manifest),
        detail: Some(detail),
        findings,
    }
}

fn unreadable(message: String) -> Read {
    Read {
        manifest: None,
        detail: None,
        findings: vec![lenient::finding(
            Check::ManifestUnreadable,
            Severity::Error,
            None,
            message,
        )],
    }
}

/// Why a manifest couldn't be read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Not JSON, or JSON of a shape no leniency covers (`"project": 3`).
    Json(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Json(why) => write!(f, "not a readable manifest: {why}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// The bytes after a leading UTF-8 byte order mark, if there is one.
fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// `{tag: text}`, as written, or nothing when the text is empty; an empty tag is `und`.
fn one(tag: &str, text: Option<&str>) -> LocalizedText {
    match text.filter(|t| !t.is_empty()) {
        Some(text) => {
            let tag = if tag.is_empty() { "und" } else { tag };
            LocalizedText::from([(tag.to_owned(), text.to_owned())])
        }
        None => LocalizedText::new(),
    }
}

/// A book code as a manifest writes it (`mat`, `MAT`, ` Mat `), if it's in canon's table.
fn book_of(code: &str) -> Option<BookKey> {
    let upper: [u8; 3] = code
        .trim()
        .to_ascii_uppercase()
        .as_bytes()
        .try_into()
        .ok()?;
    let key = BookKey::new(upper);
    canon::book(key).map(|b| b.code)
}
