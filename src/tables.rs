//! The marker data table: schema, authored rows, and generated artifacts.
//!
//! - [`schema`] — the named-field row types and enums. Humans author and audit
//!   against THESE; nobody ever hand-maintains a bit mask.
//! - [`rows`] — the authored data, one `MarkerRow` per canonical marker.
//!   Spellings collapse by stripping `-s`/`-e` first, then digits, so `q1..q4`
//!   are one row `q` and `qt3-s` is one row `qt`. Index 0 is the EMPTY row.
//! - [`books`] — the `\id` book identifiers, membership only; authored, and
//!   lint's `book-code-*` rules are its only consumer.
//! - [`emit`] — the generator: `rows::ROWS` in, the text of `generated.rs` out.
//!   A pure `String`-returning function so the freshness test can call it;
//!   `src/bin/codegen.rs` is a thin main over it.
//! - [`generated`] — codegen OUTPUT, checked in so builds never need the
//!   codegen step and diffs are reviewable: the packed runtime table with
//!   derived facts baked in, the name→idx matcher, and the side arrays.
//!
//! PRECEDENCE data (what an incoming scope displaces) deliberately does not
//! live on marker rows: it is keyed on `schema::ScopeKind`, so it belongs in a
//! ~13-entry auxiliary table. Not code yet.
//!
//! Division of labor: the TABLE stores facts, CODEGEN turns facts into
//! instructions (the u64 match, `common_marker_checks`), the LEXER reads
//! columns. One authored source, several generated projections.

pub mod books;
pub mod emit;
pub mod generated;
pub mod rows;
pub mod schema;
