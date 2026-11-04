//! canon — what a book is, before any text of it exists.
//!
//! ```text
//! canon::book(MAT)                 -> Book { usfm_number: "41", canonical_index: Some(40), .. }
//! canon::parse_book("1samuel")     -> Some(1SA)    code, slug, English name or alias; folded
//! canon::books(Canon::Protestant66) -> GEN .. REV  in canonical order
//! canon::usfm_file_name(MAT)       -> "41-MAT.usfm"
//! ```
//!
//! The data is `data/books.tsv`, one row per USFM book identifier. The tables
//! the code reads (`src/tables.rs`, and `ts/tables.ts` for the TypeScript side)
//! are generated from it by `cargo run -p canon --bin gen-canon`, and
//! `tests/generator.rs` fails when either is stale. No row is parsed at run
//! time and nothing here allocates except `usfm_file_name`'s string.
//!
//! Versification (schemes, mapping, coverage, sniffing) lands here next; the
//! plan is `planning/items/2026-11-04-canon-and-versification-the-book-table-schemes-map.mdx`.

pub mod books;
mod tables;

pub use books::{Book, Canon, Division, book, books, fold, parse_book, usfm_file_name};
pub use mise::books::BookKey;
