//! A lossless USFM lexer: memchr-driven scan into compact token rows.
//!
//! - [`token`] — the row format: `TokenKind`, its packed-byte mapping, the
//!   8-byte `Token`. What a binary codec or JS twin cares about.
//! - [`designator`] — reads a `Designator` span's interior; nothing else does.
//! - [`attributes`] — reads an `AttrList` span's interior, and matches
//!   attribute NAMES against the marker table.
//! - [`scanner`] — the only code that owns position.
//! - [`toc`](crate::toc) — the first token CONSUMER: indexes the book code, the chapter
//!   table and the verse anchors out of an already-lexed stream, and answers
//!   "what reference is this byte".
//! - [`utf16`](crate::utf16) — the editor-wire translation layer: `byte ↔ utf16`
//!   over any one string, a stride index costing 1.6% of it.
//! - [`mask`](crate::mask) — WHICH bytes survive a `Filter`, as a range set that
//!   doubles as the offset map back to the source. Walks the CST, because
//!   "text is not verse text" is a scope fact.
//!
//! The load-bearing contract (see scanner.rs): boundary finding and
//! classification stay strictly apart, and payload interiors belong to
//! interpreters, on demand, later.

pub mod attributes;
pub mod cst;
pub mod designator;
pub mod edit;
pub mod experiments;
#[cfg(any(feature = "usj", feature = "usx", feature = "html"))]
mod export;
#[cfg(feature = "html")]
pub mod html;
pub mod lint;
pub mod mask;
mod scanner;
pub mod tables;
pub mod toc;
mod token;
#[cfg(feature = "usj")]
pub mod usj;
#[cfg(feature = "usx")]
pub mod usx;
pub mod utf16;

pub use mask::{Action, Filter, Mask, TextRule, mask};
pub use scanner::{lex, lex_general_path_only};
pub use toc::{ChapterRow, Sid, Toc, VerseAnchor, toc};
pub use token::{Token, TokenKind};
pub use utf16::{Utf16Index, utf16_index};
