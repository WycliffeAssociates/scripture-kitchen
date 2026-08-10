//! Generator: authored rows → generated artifacts. Run with
//! `cargo run --bin codegen`; output is written into src/tables/ and
//! CHECKED IN (reviewable diffs; consumers never run this).
//!
//! Planned emissions (planning/NEXT-STEPS.md step 3):
//! 1. `src/tables/generated.rs` —
//!    - packed row table (codegen owns the bit layout; u128 rows or
//!      u64+u32 lanes, whichever it picks)
//!    - side arrays: names, default-attribute strings
//!    - `marker_idx(name: &[u8]) -> u8`: strip trailing ascii digits, load
//!      ≤8 name bytes into a u64, integer match over canonical constants;
//!      digits validated against numbered_max; 0 = unresolved/custom
//! 2. Later, same source: fast-path prelude (priority-ordered u64/u16
//!    compares for hot markers), JS/TS registry, USJ type projection.
//!
//! A test will assert freshness: regenerate to a temp buffer, compare with
//! the checked-in file, fail if stale.

fn main() {
    let rows = usfm_onion_2::tables::rows::ROWS;
    println!(
        "codegen: {} authored row(s); nothing to emit yet — translation + audit first \
         (planning/NEXT-STEPS.md step 3)",
        rows.len()
    );
}
