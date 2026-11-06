//! USFM in Word and LibreOffice: one book as a .docx whose named styles ARE
//! its markers, and any .docx or .odt read back into USFM.
//!
//! ```text
//! let out = docx::export(usfm, &ExportOptions::default());
//! out.docx                 // the file: "USFM p", "USFM q1", "USFM v", real footnotes
//! out.unsupported          // what has nowhere to live in Word (attributes, figures)
//!
//! let back = docx::import(&out.docx, &Settings::default())?;
//! back.books[0].usfm       // the same book, when nothing was unsupported
//! back.books[0].source     // the git blob id it was exported from, if the file says
//! back.report              // every guess, and what was not in our styles
//!
//! let reading = docx::read(bytes)?;              // once per file
//! docx::render(&reading, &settings)              // cheap: rerun on every settings change
//! ```
//!
//! Strict where it can be: a paragraph in a `USFM …` style is read as that
//! marker, with no guessing. Any other paragraph is read by
//! `@sefer/document-import`'s best-effort rules (verse numbers by how the
//! document writes them, poetry by indent, headings by look), and each guess
//! is in the report. `docx::template()` is an empty document with the styles,
//! for writing USFM in Word from scratch.

use std::collections::BTreeMap;

mod comments;
mod dom;
mod export;
mod guess;
mod import;
mod odt;
mod props;
mod reading;
mod strict;
pub mod styles;
mod verses;
#[cfg(feature = "wasm")]
mod wasm;
mod xml;
mod zip;

pub use comments::{Comment, CommentThread, VersePoint};
pub use reading::Reading;
pub use verses::{Counts, VerseMode};
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
    /// The document already held USFM markers, and was copied through as text.
    pub passthrough: bool,
    /// How verse numbers were read in guessed paragraphs.
    pub verses: VerseMode,
    pub detected: VerseMode,
    /// How many numbers of each kind the guessed paragraphs hold, with our
    /// own `USFM v` numbers elsewhere counted as superscript.
    pub candidates: Counts,
    pub chapters: Vec<ChapterSummary>,
    pub found: Found,
    /// Numbers written like verse numbers that were left as text, and why.
    pub skipped: Vec<Skip>,
    /// Every structural guess, with the reason.
    pub guesses: Vec<Guess>,
    /// Paragraphs in no style of ours, by style name: read by the best-effort rules.
    pub guessed_paragraphs: BTreeMap<String, usize>,
    /// Character styles that are not ours, inside our paragraphs. Their text
    /// was kept, unwrapped.
    pub unknown_character_styles: BTreeMap<String, usize>,
    /// Notes whose paragraph style is not ours; each became `\f` or `\fe`.
    pub unknown_note_styles: BTreeMap<String, usize>,
    /// Insertions, deletions and moves still tracked; all taken as accepted.
    pub tracked_changes: usize,
    /// Note references whose note was not in the file.
    pub missing_notes: usize,
}

impl ImportReport {
    /// Nothing was guessed: every paragraph and run was in one of our styles.
    pub fn is_exact(&self) -> bool {
        !self.passthrough
            && self.guessed_paragraphs.is_empty()
            && self.unknown_character_styles.is_empty()
            && self.unknown_note_styles.is_empty()
            && self.guesses.is_empty()
            && self.missing_notes == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterSummary {
    pub chapter: u32,
    pub verses: usize,
    pub last: u32,
    pub missing: Vec<u32>,
}

/// What the document holds, whatever the settings do with it: the switches a
/// host offers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub poetry: usize,
    pub headings: usize,
    pub front: usize,
    pub notes: usize,
    pub italic: usize,
    pub bold: usize,
    pub lists: usize,
    pub tables: usize,
    pub style_markers: usize,
    pub comments: usize,
    pub tracked_changes: usize,
    pub line_breaks: usize,
    pub blank_paragraphs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Sequence,
    Versification,
    Thousands,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    pub r#ref: Option<String>,
    pub number: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guess {
    pub r#ref: Option<String>,
    pub marker: String,
    pub reason: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Keep {
    #[default]
    Keep,
    Unwrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeepDrop {
    #[default]
    Keep,
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Headings {
    #[default]
    Keep,
    Rem,
    Drop,
}

impl Headings {
    pub fn name(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Rem => "rem",
            Self::Drop => "drop",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Front {
    #[default]
    Rem,
    Mt,
    Drop,
}

impl Front {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rem => "rem",
            Self::Mt => "mt",
            Self::Drop => "drop",
        }
    }
}

/// How the best-effort rules read a paragraph in no style of ours.
/// `@sefer/document-import`'s settings; the defaults are its defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Book code for the `\id` line when the document has none of its own.
    pub book: Option<String>,
    /// How verse numbers are written; `None` detects it.
    pub verses: Option<VerseMode>,
    /// Words that label a chapter, as in "Chapter 3".
    pub chapter_words: Vec<String>,
    /// A canon versification scheme ("eng", "org", "ulb"): a number past the
    /// chapter's last verse is never a verse. Needs a book code.
    pub versification: Option<String>,
    /// Indented, italic, Quote-styled or short-lined paragraphs: `\q1`–`\q3`, or `\p`.
    pub poetry: Keep,
    pub headings: Headings,
    /// Text before the first verse.
    pub front: Front,
    pub notes: KeepDrop,
    pub italic: Keep,
    pub bold: Keep,
    /// Numbered and bulleted paragraphs: `\li1`, or `\p`.
    pub lists: Keep,
    /// Paragraph styles named after a marker ("q2", "s1 - Heading").
    pub style_markers: Keep,
    pub comments: KeepDrop,
    /// Renames guessed paragraph markers last: `[("q2", "q1")]`.
    pub markers: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            book: None,
            verses: None,
            chapter_words: vec!["chapter".to_owned()],
            versification: None,
            poetry: Keep::Keep,
            headings: Headings::Keep,
            front: Front::Rem,
            notes: KeepDrop::Keep,
            italic: Keep::Keep,
            bold: Keep::Keep,
            lists: Keep::Keep,
            style_markers: Keep::Keep,
            comments: KeepDrop::Keep,
            markers: Vec::new(),
        }
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

/// Reads a .docx or .odt once; [`render`] it as often as the settings change.
pub fn read(bytes: &[u8]) -> Result<Reading, ImportError> {
    import::read(bytes)
}

pub fn render(reading: &Reading, settings: &Settings) -> Imported {
    import::render(reading, settings)
}

pub fn import(bytes: &[u8], settings: &Settings) -> Result<Imported, ImportError> {
    Ok(render(&read(bytes)?, settings))
}

/// An empty document with every style, for writing USFM in Word from scratch.
pub fn template() -> Vec<u8> {
    export::template()
}
