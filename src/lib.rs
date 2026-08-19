//! Parser combinator + memchr USFM lexer experiment.
//!
//! Testbed for hand-driving a lexer implementation. Not connected to
//! usfm_onion; this is a standalone playground.
//!
//! Layout follows the glossary (planning/GLOSSARY.md):
//!
//! - [`token`] — the row format: `TokenKind`, the packed-byte mapping, and
//!   the 8-byte `Token` row. What a binary codec or JS twin cares about.
//! - [`designator`] — the chapter/verse designator interpreter: the one
//!   reader of a `Designator` span's interior (ordering lint today, vref
//!   exports later).
//! - [`scanner`] — the Scanner: the only code that owns position. `lex`,
//!   the arms, the boundary finders, `classify_marker`.
//! - [`parse_header`] — the first CONSUMER of tokens: a second pass that indexes
//!   the book code and the chapter runs out of an already-lexed stream.
//!
//! The load-bearing contract (see scanner.rs module doc): boundary finding
//! and classification are kept strictly apart, and payload interiors belong
//! to interpreters, on demand, later.

pub mod cst;
pub mod designator;
pub mod edit;
pub mod experiments;
pub mod lint;
mod parse_header;
mod scanner;
pub mod tables;
mod token;

pub use parse_header::{ChapterRun, ParseHeader};
pub use scanner::{lex, lex_general_path_only};
pub use token::{Token, TokenKind};
