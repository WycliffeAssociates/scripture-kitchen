//! USFM in Word and LibreOffice: one book as a .docx whose named styles ARE
//! its markers, and any .docx read back into USFM.
//!
//! ```text
//! let out = docx::export(usfm, &ExportOptions::default());
//! out.docx                 // the file: "USFM p", "USFM q1", "USFM v", real footnotes
//! out.unsupported          // what has nowhere to live in Word (attributes, figures)
//!
//! let back = docx::import(&out.docx)?;
//! back.books[0].usfm       // the same book, through onion's USJ, when nothing was unsupported
//! back.books[0].source     // the git blob id it was exported from, if the file says
//! back.report              // paragraphs and runs in styles we did not write
//! ```
//!
//! Strict where it can be: a paragraph or run in a `USFM …` style is read as
//! that marker, with no guessing. Anything else is reported: an unknown
//! paragraph style becomes `\p`, an unknown character style is unwrapped.

mod comments;
mod dom;
mod export;
mod import;
mod props;
pub mod styles;
#[cfg(feature = "wasm")]
mod wasm;
mod xml;
mod zip;

pub use comments::{Comment, CommentThread, VersePoint};
pub use zip::ZipError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// Right-to-left paragraphs and runs, for Arabic or Hebrew script.
    pub rtl: bool,
    /// The document's default font.
    pub font: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    /// The `\id` code, when the USFM has one.
    pub book: Option<String>,
    pub docx: Vec<u8>,
    /// Everything the document could not carry, in source order.
    pub unsupported: Vec<Unsupported>,
}

impl Exported {
    /// `GEN.docx`, or `book.docx` without an `\id`.
    pub fn file_name(&self) -> String {
        format!("{}.docx", self.book.as_deref().unwrap_or("book"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    /// Where: the verse or chapter sid it follows, or the book code.
    pub at: String,
    pub what: String,
}

/// The exact USFM a document was exported from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Git's object id for the bytes: findable in any repository holding them.
    pub git_blob_sha1: String,
    /// onion's dish-header hash of the same bytes, 16 hex digits.
    pub xxh3_64: String,
}

impl Source {
    pub fn of(usfm: &[u8]) -> Self {
        let mut sha = sha1_smol::Sha1::new();
        sha.update(format!("blob {}\0", usfm.len()).as_bytes());
        sha.update(usfm);
        Self {
            git_blob_sha1: sha.digest().to_string(),
            xxh3_64: format!("{:016x}", xxhash_rust::xxh3::xxh3_64(usfm)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// One per `USFM id` paragraph; text before the first is a book of its own.
    pub books: Vec<ImportedBook>,
    pub report: ImportReport,
    /// Every custom document property, as written.
    pub properties: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedBook {
    pub code: Option<String>,
    pub usfm: String,
    pub source: Option<Source>,
    /// Comment threads anchored in this book (and, on the first book, any
    /// anchored nowhere).
    pub comments: Vec<CommentThread>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Paragraph styles that are not ours, by name, and how many paragraphs
    /// used each. Each paragraph became `\p`.
    pub unknown_paragraph_styles: std::collections::BTreeMap<String, usize>,
    /// Character styles that are not ours. Their text was kept, unwrapped.
    pub unknown_character_styles: std::collections::BTreeMap<String, usize>,
    /// Insertions, deletions and moves still tracked; all taken as accepted.
    pub tracked_changes: usize,
    /// Note references whose note was not in the file.
    pub missing_notes: usize,
}

impl ImportReport {
    /// Nothing was guessed: every paragraph and run was in one of our styles.
    pub fn is_exact(&self) -> bool {
        self.unknown_paragraph_styles.is_empty()
            && self.unknown_character_styles.is_empty()
            && self.missing_notes == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Zip(ZipError),
    Xml(dom::XmlError),
    /// A part the package names is not in it.
    Missing(String),
}

impl core::fmt::Display for ImportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Zip(e) => write!(f, "{e}"),
            Self::Xml(e) => write!(f, "{}: {}", e.part, e.message),
            Self::Missing(p) => write!(f, "the document has no {p}"),
        }
    }
}

impl std::error::Error for ImportError {}

pub fn export(usfm: &str, options: &ExportOptions) -> Exported {
    export::export(usfm, options)
}

pub fn import(docx: &[u8]) -> Result<Imported, ImportError> {
    import::import(docx)
}
