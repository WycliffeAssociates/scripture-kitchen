//! The same three publications, through the JS handle, in Node.
//!
//! ```text
//! cd galley
//! wasm-pack test --node --features wasm --test wasm_wall
//! ```
//!
//! The bytes are `galley/tests/goldens/sous/*.bin`, pinned natively by
//! `sous_goldens.rs`. What this adds is the WALL: a string crosses, an
//! `Expediter` runs inside linear memory, and the buffer that comes back is
//! the byte-identical publication — no `f32` drift in the length lane, no
//! hasher divergence in the snapshot identity, no reordering.
//!
//! Instrument: SHAPES — the committed fixtures, `include_str!`d, so a missing
//! one is a compile error here as much as it is natively.

#![cfg(target_arch = "wasm32")]

use usfm_galley::wasm::{Galley, Knobs};
use wasm_bindgen_test::*;

const GEN: &str = include_str!("fixtures/sous/GEN.usfm");
const GEN_EDITED: &str = include_str!("fixtures/sous/GEN-edited.usfm");
const RUT: &str = include_str!("fixtures/sous/RUT.usfm");
const JON: &str = include_str!("fixtures/sous/JON.usfm");
const RUT_REF: &str = include_str!("fixtures/sous/ref/RUT.usfm");
const JON_REF: &str = include_str!("fixtures/sous/ref/JON.usfm");

const COLD: &[u8] = include_bytes!("goldens/sous/cold.bin");
const EDIT: &[u8] = include_bytes!("goldens/sous/edit.bin");
const KNOBS: &[u8] = include_bytes!("goldens/sous/knobs.bin");

/// The corpus registered, and nothing published yet.
fn loaded() -> Galley {
    let mut galley = Galley::new(None);
    assert_eq!(galley.update("books/GEN.usfm", GEN).unwrap(), "GEN");
    galley.update("books/RUT.usfm", RUT).unwrap();
    galley.update("books/JON.usfm", JON).unwrap();
    galley.update_reference("ref/RUT.usfm", RUT_REF).unwrap();
    galley.update_reference("ref/JON.usfm", JON_REF).unwrap();
    galley
}

#[wasm_bindgen_test]
fn the_three_publications_cross_unchanged() {
    let mut galley = loaded();
    assert_eq!(galley.publish().unwrap(), COLD, "the cold publication");

    galley.update("books/GEN.usfm", GEN_EDITED).unwrap();
    assert_eq!(galley.publish().unwrap(), EDIT, "one word changed");

    let mut knobs = galley.config();
    knobs.casing = false;
    knobs.sentence_start_upper_bp = 9_990;
    knobs.z_short = 2.0;
    // The lane the defaults leave off: the knobs golden is where wire code 3
    // crosses the wall.
    assert!(!knobs.source_copy, "the source-copy lane ships off");
    knobs.source_copy = true;
    galley.set_config(knobs);
    // A reference registered before the lane was on kept no word lane; the
    // host re-sends its text, and the publication says how many needed it.
    galley.publish().unwrap();
    assert_eq!(galley.last_wordless_references(), 2.0);
    galley.update_reference("ref/RUT.usfm", RUT_REF).unwrap();
    galley.update_reference("ref/JON.usfm", JON_REF).unwrap();
    assert_eq!(galley.publish().unwrap(), KNOBS, "the knobs publication");
}

#[wasm_bindgen_test]
fn a_removed_book_leaves_the_publication() {
    let mut galley = loaded();
    assert!(galley.remove("books/RUT.usfm"));
    assert!(!galley.remove("books/RUT.usfm"), "removed once");
    assert_ne!(
        galley.publish().unwrap(),
        COLD,
        "a corpus one book short publishes different bytes"
    );
}

#[wasm_bindgen_test]
fn the_handle_reports_what_it_holds() {
    let mut galley = loaded();
    galley.publish().unwrap();
    assert!(galley.resident_bytes() > 0.0, "the Pantry holds the corpus");
    assert!(galley.last_mapped() > 0.0, "the first publication maps");
    assert_eq!(galley.last_remapped(), 0.0, "nothing was half-cached");
    assert!(galley.last_located() > 0.0, "every target is scanned once");
    assert!(galley.last_paired() > 0.0, "two targets declare a source");

    galley.publish().unwrap();
    assert_eq!(galley.last_mapped(), 0.0, "an idempotent republication");
}

#[wasm_bindgen_test]
fn the_knobs_round_trip_through_js() {
    let galley = Galley::new(None);
    let defaults = galley.config();
    assert_eq!(defaults, Knobs::default());
    assert!(defaults.casing, "casing ships on");
    assert_eq!(defaults.support_floor, 5);
    assert_eq!(defaults.word_support_floor, 20);
    assert_eq!(defaults.terminal_upper_share_bp, 8_000);
    assert_eq!(defaults.min_verses, 50);
}

/// The onion door still answers off the Pantry's own Warmer.
#[wasm_bindgen_test]
fn the_onion_products_share_the_corpus_cache() {
    let mut galley = loaded();
    let dish = galley.parse(GEN, true, true, true);
    assert!(!dish.is_empty(), "a parse buffer came back");
    assert!(
        galley.verse_text(GEN).contains("beside the"),
        "the verse-text projection reads"
    );
    assert!(galley.entry_count() > 0.0, "chapters were cached");
}
