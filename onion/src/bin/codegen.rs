//! Generator: authored rows → generated artifacts. Run with
//! `cargo run --bin codegen`; output is written into src/tables/ and CHECKED IN
//! (reviewable diffs; consumers never run this).
//!
//! **The table conforms to USFM 3.2** (<https://docs.usfm.bible/usfm/3.2/>) and
//! nothing else: no version column, no multi-version emissions.
//!
//! A thin main: the generator itself is `tables::emit`, in the library, so
//! `tests/codegen_output_matches_input.rs` can regenerate to a buffer and fail
//! the build when the checked-in file is stale.
//!
//! TWO artifacts now: the marker table (`src/tables/generated.rs`) and the
//! diagnostics side-table the JS bundle loads (`onion-wasm/diagnostics.json` —
//! `lint::catalog`). Both are checked in and both have a staleness test.
//!
//! Deliberately NOT emitted: `common_marker_checks`, the hot-marker fast path —
//! built one pattern at a time and MEASURED, never speculated (the `priority`
//! column is measurement, not a guess) — and the JS/TS registry + USJ
//! projection, which wait on the wasm/JS boundary.

use std::path::Path;

use usfm_onion::lint;
use usfm_onion::tables::{emit, rows};
use usfm_onion::wire::emit as wire_emit;
use usfm_onion::wire::schema;

// Anchored to the crate, not the shell: `cargo run --bin codegen` writes the
// same four files from the workspace root, from onion/, or from anywhere else.
const CRATE: &str = env!("CARGO_MANIFEST_DIR");
const OUT: &str = "src/tables/generated.rs";
const DIAGNOSTICS: &str = "../onion-wasm/diagnostics.json";
const WIRE: &str = "src/wire/generated.rs";
const READER: &str = "../onion-wasm/reader.ts";

fn main() -> std::io::Result<()> {
    let text = emit::generated_rs();

    let path = &Path::new(CRATE).join(OUT);
    let previous = std::fs::read_to_string(path).unwrap_or_default();
    std::fs::write(path, &text)?;

    let attrs: usize = rows::ROWS
        .iter()
        .map(|row| row.defined_attributes.len())
        .sum();
    let overloaded = rows::ROWS
        .iter()
        .filter(|row| !matches!(row.shape, usfm_onion::tables::schema::SpellingShape::Any))
        .count();

    println!("codegen → {OUT}");
    println!(
        "  rows                {:>4}  (index 0 is the empty row)",
        rows::ROWS.len()
    );
    println!("  attribute entries   {:>4}  before run-sharing", attrs);
    println!(
        "  shape-keyed rows    {:>4}  (names needing the extra compare)",
        overloaded
    );
    println!("  bits used per row   {:>4}  of 128", emit::BITS_USED);
    println!("  bytes written     {:>6}", text.len());
    println!(
        "  {}",
        if previous == text {
            "unchanged — checked-in file was already fresh"
        } else if previous.is_empty() {
            "CREATED"
        } else {
            "CHANGED — review the diff before committing"
        }
    );

    let catalog = lint::diagnostics_json();
    let path = &Path::new(CRATE).join(DIAGNOSTICS);
    let stale = std::fs::read_to_string(path).unwrap_or_default() != catalog;
    std::fs::write(path, &catalog)?;
    println!("codegen → {DIAGNOSTICS}");
    println!("  lint codes          {:>4}", lint::LINT_ROWS.len());
    println!("  bytes written     {:>6}", catalog.len());
    println!(
        "  {}",
        if stale {
            "CHANGED — review the diff before committing"
        } else {
            "unchanged — checked-in file was already fresh"
        }
    );
    // The wire: one schema, both ends. The writer compiles into this crate;
    // the reader ships in the JS package beside the .wasm it decodes.
    let text = wire_emit::wire_generated_rs();
    let path = &Path::new(CRATE).join(WIRE);
    let stale = std::fs::read_to_string(path).unwrap_or_default() != text;
    std::fs::write(path, &text)?;
    println!("codegen \u{2192} {WIRE}");
    println!("  wire records      {:>4}", schema::RECORDS.len());
    println!("  sections          {:>4}", schema::SECTIONS.len());
    println!("  bytes written   {:>6}", text.len());
    println!(
        "  {}",
        if stale {
            "CHANGED \u{2014} review the diff before committing"
        } else {
            "unchanged \u{2014} checked-in file was already fresh"
        }
    );

    let text = wire_emit::reader_ts(
        &wire_emit::marker_table_ts(),
        &wire_emit::enums_ts(),
        &wire_emit::catalog_ts(),
    );
    let path = &Path::new(CRATE).join(READER);
    let stale = std::fs::read_to_string(path).unwrap_or_default() != text;
    std::fs::write(path, &text)?;
    println!("codegen \u{2192} {READER}");
    println!("  marker rows       {:>4}", rows::ROWS.len());
    println!("  lint codes        {:>4}", lint::LINT_ROWS.len());
    println!("  bytes written   {:>6}", text.len());
    println!(
        "  {}",
        if stale {
            "CHANGED \u{2014} review the diff before committing"
        } else {
            "unchanged \u{2014} checked-in file was already fresh"
        }
    );

    Ok(())
}
