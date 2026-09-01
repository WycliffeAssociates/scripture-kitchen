//! Language-agnostic scripture-content analysis.
//!
//! This crate owns evidence and judgment, not USFM parsing, source mapping,
//! I/O, or resident caches. Producers enter through [`ProjectedBook`].

mod input;

pub use input::{
    BookIndex, BookKey, Chapter, Corpus, InputError, ProjectedBook, TextRange, Verse, VerseKey,
    validate,
};
