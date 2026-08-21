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
//! Deliberately NOT emitted: `common_marker_checks`, the hot-marker fast path —
//! built one pattern at a time and MEASURED, never speculated (the `priority`
//! column is measurement, not a guess) — and the JS/TS registry + USJ
//! projection, which wait on the wasm/JS boundary.

use std::path::Path;

use usfm_onion_2::tables::{emit, rows};

const OUT: &str = "src/tables/generated.rs";

fn main() -> std::io::Result<()> {
    let text = emit::generated_rs();

    let path = Path::new(OUT);
    let previous = std::fs::read_to_string(path).unwrap_or_default();
    std::fs::write(path, &text)?;

    let attrs: usize = rows::ROWS
        .iter()
        .map(|row| row.defined_attributes.len())
        .sum();
    let overloaded = rows::ROWS
        .iter()
        .filter(|row| !matches!(row.shape, usfm_onion_2::tables::schema::SpellingShape::Any))
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
    Ok(())
}
