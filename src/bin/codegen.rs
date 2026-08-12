//! Generator: authored rows → generated artifacts. Run with
//! `cargo run --bin codegen`; output is written into src/tables/ and CHECKED IN
//! (reviewable diffs; consumers never run this).
//!
//! **The table conforms to USFM 3.2** (<https://docs.usfm.bible/usfm/3.2/>) and
//! nothing else: no version column, no version-tracking, no multi-version
//! emissions. Everything generated here — the packed table, the JS registry, the
//! USJ projection — therefore describes 3.2 only. The 3.1 grammar in
//! tcdocs/usx.rng is fallback evidence for the AUDIT, never an input to codegen.
//!
//! Planned emissions (planning/NEXT-STEPS.md step 3):
//! 1. `src/tables/generated.rs` —
//!    - packed row table (codegen owns the bit layout; u128 rows or u64+u32
//!      lanes, whichever it picks), with the derived facts baked in: the
//!      20-bit effective-context mask from `allowed_contexts`, and
//!      `schema::contributes_context(kind, category)`
//!    - side arrays: marker names, attribute-name strings (the per-attribute
//!      `AttrStatus` packs alongside each index — required/optional/deprecated
//!      is 2 bits)
//!    - `marker_idx(name: &[u8]) -> u8` — **[G] the strip order is
//!      `-s`/`-e` FIRST, then trailing ascii digits**: `qt3-s` → `qt3` → `qt`.
//!      Then load ≤8 name bytes into a u64 and integer-match over the
//!      canonical constants (the compiler emits the decision tree). The digits
//!      are validated against `numbered_max`; the bare form is ALWAYS legal
//!      [D]; `Numbering::TableColumns` means "match the alpha stem and stop,
//!      the rest of the lexeme is payload" [O]. The lookup key is
//!      (name, `SpellingShape`), so for the handful of overloaded names (`qt`)
//!      codegen emits one extra compare against the shape the lexer already
//!      classified; every other name resolves on the u64 match alone.
//!    - **Index 0 is the generic EMPTY row**, not a bare sentinel: opens
//!      nothing, closes nothing, contributes no context, no payload, no
//!      attributes. A first-byte-`z` test bails to it without matching a name
//!      at all [F], which is why a `\zaln-s` fast check needs no row — the
//!      token kind still comes from the lexical shape.
//! 2. Later, same source: `common_marker_checks` — the hot-marker fast path,
//!    priority-ordered u64/u16 compares, built one pattern at a time and
//!    measured (the `priority` column is now MEASURED, not guessed: en_ulb +
//!    examples.bsb, 2026-08-10). Then the JS/TS registry and USJ projection.
//!
//! A test will assert freshness: regenerate to a temp buffer, compare with the
//! checked-in file, fail if stale.
//!
//! Today it emits nothing and instead reports AUDIT READINESS: the state of the
//! two row sources — the AUDITED table (`tables::rows`) and the UNAUDITED
//! mechanical translation (`tables::unaudited`) — plus every recorded fact the
//! audit needs to carry across. The schema and data rounds are closed, so the
//! flags below are RECORDED FACTS AND CITATIONS, not blockers: each says what was
//! decided and why, or names something the spec itself leaves open.

use usfm_onion_2::tables::{rows, unaudited};

fn main() {
    let audited = rows::ROWS;
    let scratch = unaudited::UNAUDITED_ROWS;

    let open_categories = unaudited::CATEGORY_JUDGEMENT_CALLS
        .iter()
        .filter(|(_, reason)| !reason.starts_with("CONFIRMED") && !reason.starts_with("FIXED"))
        .count();

    println!("codegen: emits nothing yet. Schema + data rounds are CLOSED; what remains");
    println!("is the row-by-row audit (planning/NEXT-STEPS.md step 3), then step 3's");
    println!("emissions. Conformance target: USFM 3.2, no version column.");
    println!();
    println!("  AUDIT PROGRESS");
    println!(
        "    audited rows      (tables::rows — the real table)  {:>4}",
        audited.len()
    );
    println!(
        "    awaiting audit    (tables::unaudited)              {:>4}",
        scratch.len()
    );
    println!(
        "    collapsed families (numbered + milestone spellings) {:>3}",
        unaudited::COLLAPSED_FAMILIES
    );
    println!();
    println!("  RECORDED STATE — decisions, citations, and spec-side unknowns");
    println!(
        "    category rulings                                   {:>4}  ({} open)",
        unaudited::CATEGORY_JUDGEMENT_CALLS.len(),
        open_categories
    );
    if open_categories == 0 {
        println!();
        println!("    Every category ruling is settled. Nothing blocks the audit; spec-side");
        println!("    unknowns are tracked in planning/attributes-3.2.md.");
    }
    println!();
    println!("  Category rulings — where the spec group was ambiguous, contradictory, or");
    println!("  absent. All 20 reviewed and CONFIRMED CORRECT by Will 2026-08-10; kept as");
    println!("  the audit record of how each was decided:");
    for (marker, reason) in unaudited::CATEGORY_JUDGEMENT_CALLS {
        let tag = if reason.starts_with("CONFIRMED") || reason.starts_with("FIXED") {
            "ok     "
        } else {
            "OPEN   "
        };
        println!("  {tag}[{marker}] {reason}");
    }
}
