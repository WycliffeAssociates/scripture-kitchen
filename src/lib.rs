//! Parser combinator + memchr USFM lexer experiment.
//!
//! Testbed for hand-driving a lexer implementation. Not connected to
//! usfm_onion; this is a standalone playground.
//!
//! Layout follows the glossary (planning/GLOSSARY.md):
//!
//! - [`token`] — the row format: `TokenKind`, the packed-byte mapping, and
//!   the 8-byte `Token` row. What a binary codec or JS twin cares about.
//! - [`scanner`] — the Scanner: the only code that owns position. `lex`,
//!   the arms, the boundary finders, `classify_marker`, and the (stub)
//!   `Header` a scan discovers.
//!
//! The load-bearing contract (see scanner.rs module doc): boundary finding
//! and classification are kept strictly apart, and payload interiors belong
//! to interpreters, on demand, later.

pub mod experiments;
mod scanner;
pub mod tables;
mod token;

pub use scanner::{ChapterRun, Header, lex, lex_general_path_only};
pub use token::{Token, TokenKind};
