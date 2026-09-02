//! mise — the prep both engines share, and nothing else.
//!
//! ```text
//! mise::books   BOOK_CODES[0] == b"GEN"          the spec's 116 identifiers
//!               BookKey::new(*b"MRK")            three bytes of book identity
//!               canonical_rank(key) == 40        its place in spec order
//!
//! mise::utf16   utf16_index(bytes).to_byte(11)   byte ↔ utf16, string present
//!               utf16_table(bytes).to_utf16(16)  byte → utf16, string gone
//! ```
//!
//! Zero dependencies, in or out of the workspace: this is the leaf
//! `usfm_onion` and `sous-core` may both reach for without reaching for each
//! other. Only two kinds of thing belong — spec-derived data tables and
//! borrow-free data structures — and only when more than one crate needs them.
//! The scope rule and what it excludes: `mise/README.md`.

pub mod books;
pub mod utf16;
