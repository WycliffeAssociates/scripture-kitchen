//! The marker data table: schema, authored rows, and generated artifacts.
//!
//! Flow (planning/NEXT-STEPS.md step 3):
//!
//! - [`schema`] — the named-field row types and enums. Humans author and
//!   audit against THESE; nobody ever hand-maintains a bit mask.
//! - [`rows`] — the authored data: one `MarkerRow` per canonical marker
//!   (numbered spellings collapsed — `q1..q4` are one row `q`).
//!   Mechanically translated from onion's marker_defs_data, then audited
//!   category-by-category.
//! - `generated` (not yet emitted) — created by `cargo run --bin codegen`:
//!   the packed runtime table (u128-ish; codegen owns the bit layout), the
//!   strip-digits-then-match name→idx function, and later the JS/TS
//!   registry and USJ type projections. CHECKED IN so builds never need
//!   the codegen step and diffs are reviewable.
//!
//! Division of labor: the TABLE stores facts, CODEGEN turns facts into
//! instructions (the u64 match, the fast-path prelude), the LEXER reads
//! columns. One authored source, several generated projections.

pub mod rows;
pub mod schema;

// pub mod generated;   // emitted by `cargo run --bin codegen`, then uncomment.
