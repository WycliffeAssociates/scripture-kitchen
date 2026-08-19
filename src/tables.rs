//! The marker data table: schema, authored rows, and generated artifacts.
//!
//! Flow (planning/NEXT-STEPS.md step 3):
//!
//! - [`schema`] — the named-field row types and enums. Humans author and
//!   audit against THESE; nobody ever hand-maintains a bit mask.
//! - [`rows`] — the authored data: one `MarkerRow` per canonical marker.
//!   Spellings collapse by stripping `-s`/`-e` first, then digits [G], so
//!   `q1..q4` are one row `q` and `qt3-s` is one row `qt`. Index 0 is the
//!   generic EMPTY row.
//! - [`books`] — the books AUXILIARY table: the spec's 116 `\id` book
//!   identifiers, membership only. Authored, not codegen, and not a marker
//!   fact — lint's `book-code-*` rules are its only consumer.
//! - [`emit`] — the generator: `rows::ROWS` in, the text of `generated.rs`
//!   out. A pure `String`-returning function so the freshness test can call it
//!   (`tests/codegen_output_matches_input.rs`); `src/bin/codegen.rs` is a thin main over it.
//! - [`generated`] — codegen OUTPUT, checked in: the packed runtime table with
//!   the derived facts baked (effective context mask,
//!   `schema::contributes_context`), the name→idx matcher, and the side arrays.
//!   Checked in so builds never need the codegen step and diffs are reviewable.
//!
//! The PRECEDENCE data (what an incoming scope displaces) deliberately does NOT
//! live on marker rows: it is keyed on `schema::ScopeKind`, so it belongs in a
//! ~13-entry auxiliary table [A]. See NEXT-STEPS §5 (walker design) for the
//! proposed encoding; it is not code yet.
//!
//! Division of labor: the TABLE stores facts, CODEGEN turns facts into
//! instructions (the u64 match, `common_marker_checks`), the LEXER reads
//! columns. One authored source, several generated projections.
//!
//! ## `unaudited` is gone (2026-08-12)
//!
//! The staging module that held the mechanical translation was promoted whole
//! into [`rows`] rather than being drained row-by-row — Will's call, recorded in
//! that module's header. Only [`rows::ROWS`] exists now, and it is the table.

pub mod books;
pub mod emit;
pub mod generated;
pub mod rows;
pub mod schema;
