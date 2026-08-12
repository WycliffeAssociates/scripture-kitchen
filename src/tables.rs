//! The marker data table: schema, authored rows, and generated artifacts.
//!
//! Flow (planning/NEXT-STEPS.md step 3):
//!
//! - [`schema`] — the named-field row types and enums. Humans author and
//!   audit against THESE; nobody ever hand-maintains a bit mask.
//! - [`rows`] — the authored data: one `MarkerRow` per canonical marker.
//!   Spellings collapse by stripping `-s`/`-e` first, then digits [G], so
//!   `q1..q4` are one row `q` and `qt3-s` is one row `qt`. Mechanically
//!   translated from onion's marker_defs_data, then audited category-by-category
//!   against tcdocs/ + planning/scratch.md.
//! - `generated` (not yet emitted) — created by `cargo run --bin codegen`:
//!   the packed runtime table (u128-ish; codegen owns the bit layout, and bakes
//!   the derived facts — the context mask and
//!   `schema::contributes_context`), the name→idx matcher, and later the JS/TS
//!   registry and USJ type projections. CHECKED IN so builds never need
//!   the codegen step and diffs are reviewable.
//!
//! The PRECEDENCE data (what an incoming scope displaces) deliberately does NOT
//! live on marker rows: it is keyed on `schema::ScopeKind`, so it belongs in a
//! ~13-entry auxiliary table [A]. See planning/TRANSITIONS.md §3 for the
//! proposed encoding; it is not code yet.
//!
//! Division of labor: the TABLE stores facts, CODEGEN turns facts into
//! instructions (the u64 match, `common_marker_checks`), the LEXER reads
//! columns. One authored source, several generated projections.

//! ## A word on [`unaudited`]
//!
//! **`unaudited` IS NOT THE TABLE.** It is the mechanical translation of
//! onion's data into the new schema — machine output, reviewed by nobody, kept
//! only so the audit has something concrete to argue with and so the schema is
//! exercised by real data. Nothing downstream may read it: the scanner, lint,
//! codegen's emissions, and every test that asserts a marker FACT read
//! [`rows::ROWS`], which is empty until rows are moved across by hand,
//! category-by-category, verified against tcdocs/. A row that has not made
//! that trip is not a fact. Delete the module when the last row has moved.

pub mod rows;
pub mod schema;
pub mod unaudited;

// pub mod generated;   // emitted by `cargo run --bin codegen`, then uncomment.
