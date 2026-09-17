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

use usfm_galley::wasm::onion;
use usfm_galley::wasm::{Galley, SousSettings};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

const GEN: &str = include_str!("fixtures/sous/GEN.usfm");
const GEN_EDITED: &str = include_str!("fixtures/sous/GEN-edited.usfm");
const RUT: &str = include_str!("fixtures/sous/RUT.usfm");
const JON: &str = include_str!("fixtures/sous/JON.usfm");
const RUT_REF: &str = include_str!("fixtures/sous/ref/RUT.usfm");
const JON_REF: &str = include_str!("fixtures/sous/ref/JON.usfm");

const OVERLAY_TARGET: &str = include_str!("fixtures/overlay/gen-target.usfm");
const OVERLAY_SOURCE: &str = include_str!("fixtures/overlay/gen-source.usfm");

const COLD: &[u8] = include_bytes!("goldens/sous/cold.bin");
const EDIT: &[u8] = include_bytes!("goldens/sous/edit.bin");
const KNOBS: &[u8] = include_bytes!("goldens/sous/knobs.bin");

/// The corpus registered, and nothing published yet.
fn loaded() -> Galley {
    let mut galley = Galley::new(None);
    assert_eq!(galley.update("books/GEN.usfm", GEN).unwrap(), "GEN");
    galley.update("books/RUT.usfm", RUT).unwrap();
    galley.update("books/JON.usfm", JON).unwrap();
    galley
        .update_reference("ref/RUT.usfm", RUT_REF, None)
        .unwrap();
    galley
        .update_reference("ref/JON.usfm", JON_REF, None)
        .unwrap();
    galley
}

/// The find buffer's head, read the way a host reads it: the two header
/// words, the hit count, and the id table `bookIndex` indexes.
struct Found {
    hits: usize,
    ids: Vec<String>,
}

fn decode_find(bytes: &[u8]) -> Found {
    let word =
        |at: usize| u32::from_le_bytes(bytes[at * 4..at * 4 + 4].try_into().expect("four bytes"));
    assert_eq!(word(0), 0x444E_4946, "the buffer leads with FIND");
    assert_eq!(word(1), 1, "find wire version 1");
    let hits = word(2) as usize;
    let books = word(3) as usize;
    // One record per hit: four words, plus two per source piece.
    let mut at = 4;
    for _ in 0..hits {
        at += 4 + 2 * word(at + 3) as usize;
    }
    let id_lens: Vec<usize> = (0..books).map(|i| word(at + i) as usize).collect();
    // The preview lengths sit between the id lengths and the byte blob.
    let mut cursor = (at + books + hits) * 4;
    let ids = id_lens
        .into_iter()
        .map(|len| {
            let id = String::from_utf8(bytes[cursor..cursor + len].to_vec())
                .expect("the encoder wrote UTF-8");
            cursor += len;
            id
        })
        .collect();
    Found { hits, ids }
}

/// A needle every fixture's verse text carries.
const NEEDLE: &str = "the";

#[wasm_bindgen_test]
fn the_three_publications_cross_unchanged() {
    let mut galley = loaded();
    assert_eq!(galley.publish().unwrap(), COLD, "the cold publication");

    galley.update("books/GEN.usfm", GEN_EDITED).unwrap();
    assert_eq!(galley.publish().unwrap(), EDIT, "one word changed");

    let mut settings = galley.config();
    settings.casing = false;
    settings.sentence_start_upper_bp = 9_990;
    settings.z_short = 2.0;
    // The lane the defaults leave off: the settings golden is where wire code 3
    // crosses the wall.
    assert!(!settings.source_copy, "the source-copy lane ships off");
    settings.source_copy = true;
    galley.set_config(settings);
    // A reference registered before the lane was on kept no word lane; the
    // host re-sends its text, and the publication says how many needed it.
    galley.publish().unwrap();
    assert_eq!(galley.last_wordless_references(), 2.0);
    galley
        .update_reference("ref/RUT.usfm", RUT_REF, None)
        .unwrap();
    galley
        .update_reference("ref/JON.usfm", JON_REF, None)
        .unwrap();
    assert_eq!(galley.publish().unwrap(), KNOBS, "the settings publication");
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
    assert_eq!(defaults, SousSettings::default());
    assert!(defaults.casing, "casing ships on");
    assert_eq!(defaults.support_floor, 5);
    assert_eq!(defaults.word_support_floor, 20);
    assert_eq!(defaults.terminal_upper_share_bp, 8_000);
    assert_eq!(defaults.min_verses, 50);
}

/// An onion-wasm door, in the galley module: the shims land here because the
/// cdylib links the object they sit in. `tests/sous_conformance.mjs` pins the
/// whole export list as JavaScript sees it.
#[wasm_bindgen_test]
fn an_onion_door_answers_on_this_module() {
    let at = GEN.find("\\c 1").expect("the fixture declares a chapter") as u32;
    assert_eq!(
        onion::to_utf16(GEN, at),
        GEN[..at as usize].encode_utf16().count() as u32,
    );
}

/// The onion door still answers off the Pantry's own chunk cache.
#[wasm_bindgen_test]
fn the_onion_products_share_the_corpus_cache() {
    let mut galley = loaded();
    let dish = galley.parse_text(GEN, true, true, true);
    assert!(!dish.is_empty(), "a parse buffer came back");
    assert!(
        galley.verse_text_of(GEN).contains("beside the"),
        "the verse-text projection reads"
    );
    assert!(galley.entry_count() > 0.0, "chapters were cached");
}

/// The id doors and the text doors plate the same book.
#[wasm_bindgen_test]
fn the_retained_copy_answers_with_the_same_bytes() {
    let mut galley = loaded();
    let by_id = galley.parse("books/GEN.usfm", true, true, true).unwrap();
    assert_eq!(by_id, galley.parse_text(GEN, true, true, true));
    assert_eq!(
        galley.verse_text("books/GEN.usfm").unwrap(),
        galley.verse_text_of(GEN),
    );
    assert_eq!(
        galley.lint("books/GEN.usfm").unwrap(),
        galley.parse_text(GEN, true, false, false),
        "lint is the diagnostics section alone",
    );
    assert!(galley.parse("books/NUM.usfm", true, true, true).is_err());
}

/// A reference is searchable exactly when the host asked it to keep its text,
/// and the refusal names the argument that would fix it.
#[wasm_bindgen_test]
fn a_reference_is_findable_only_with_keep_text() {
    let mut galley = loaded();
    let refused = galley
        .find("ref/RUT.usfm", NEEDLE, case_sensitive_opts())
        .expect_err("a lengths-only reference retains nothing to search");
    assert!(format!("{refused:?}").contains("keepText"), "{refused:?}");
    assert!(
        galley
            .find("books/NUM.usfm", NEEDLE, case_sensitive_opts())
            .is_err()
    );

    galley
        .update_reference("ref/RUT.usfm", RUT_REF, Some(true))
        .unwrap();
    let found = decode_find(
        &galley
            .find("ref/RUT.usfm", NEEDLE, case_sensitive_opts())
            .unwrap(),
    );
    assert!(found.hits > 0, "the source's own verse text is searchable");
    assert_eq!(found.ids, vec!["ref/RUT.usfm".to_string()]);
    // Keeping the text buys the projection with it.
    assert!(galley.verse_text("ref/RUT.usfm").is_ok());
    // And the publication is still the one the goldens pin.
    assert_eq!(galley.publish().unwrap(), COLD, "the cold publication");
}

/// The scope decides which books are searched, and the id table names exactly
/// those.
#[wasm_bindgen_test]
fn the_find_scope_chooses_the_id_table() {
    let mut galley = loaded();
    galley
        .update_reference("ref/RUT.usfm", RUT_REF, Some(true))
        .unwrap();

    let targets = decode_find(&galley.find_all(NEEDLE, scoped_opts(None)).unwrap());
    assert_eq!(targets.ids.len(), 3, "the default scope is the targets");
    assert!(targets.ids.iter().all(|id| id.starts_with("books/")));

    let references = decode_find(
        &galley
            .find_all(NEEDLE, scoped_opts(Some("references")))
            .unwrap(),
    );
    // ref/JON kept no text, so it is neither searched nor listed.
    assert_eq!(references.ids, vec!["ref/RUT.usfm".to_string()]);
    assert!(references.hits > 0);

    let all = decode_find(&galley.find_all(NEEDLE, scoped_opts(Some("all"))).unwrap());
    assert_eq!(all.ids.len(), 4, "three targets and the one kept reference");
    assert_eq!(all.hits, targets.hits + references.hits);

    assert!(
        galley
            .find_all(NEEDLE, scoped_opts(Some("elsewhere")))
            .is_err(),
        "an unknown scope is an error, not a default"
    );
}

/// A reference keeps no text by default, so every door that needs one refuses
/// with the Pantry's own words rather than answering from nothing.
#[wasm_bindgen_test]
fn a_reference_refuses_the_text_doors() {
    let mut galley = loaded();
    assert!(galley.lint("ref/RUT.usfm").is_err(), "no text to lint");
    assert!(galley.parse("ref/RUT.usfm", true, true, true).is_err());
    assert!(galley.verse_text("ref/RUT.usfm").is_err());
    assert!(galley.lint("books/RUT.usfm").is_ok(), "the target has text");
}

/// Dirty is positional; rework is set membership. One edited verse in chapter
/// 2 makes the file differ and names that chapter alone.
#[wasm_bindgen_test]
fn a_fingerprint_separates_dirty_from_rework() {
    let galley = Galley::new(None);
    let baseline = galley.fingerprint(GEN);
    let current = galley.fingerprint(GEN_EDITED);

    assert!(
        !baseline.differs_from(&baseline),
        "the same bytes are clean"
    );
    assert!(baseline.differs_from(&current), "one word makes it dirty");
    assert!(baseline.chunk_count() > 1, "the fixture has chapters");

    let changed = baseline.changed_chunks(&current);
    assert_eq!(changed.len(), 2, "one range, as from and to");
    let (from, to) = (changed[0] as usize, changed[1] as usize);
    assert!(
        GEN_EDITED[from..to].starts_with("\\c 2"),
        "the edited chapter alone",
    );
}

/// The same question against a book the Pantry already holds.
#[wasm_bindgen_test]
fn the_retained_copy_is_its_own_baseline() {
    let galley = loaded();
    assert_eq!(
        galley.changed_since_update("books/GEN.usfm", GEN),
        Some(Vec::new()),
        "the text as last updated needs no rework",
    );
    let changed = galley
        .changed_since_update("books/GEN.usfm", GEN_EDITED)
        .expect("a registered id");
    assert_eq!(changed.len(), 2, "one chapter to re-derive");
    assert_eq!(
        galley.changed_since_update("books/NUM.usfm", GEN),
        None,
        "an unregistered id answers undefined",
    );
}

/// The overlay, end to end through the module: the skeleton JSON, the edit
/// transaction, the applied text, and the UTF-16 opt-in.
#[wasm_bindgen_test]
fn an_overlay_crosses_the_wall() {
    let mut galley = Galley::new(None);
    galley.update("books/GEN.usfm", OVERLAY_TARGET).unwrap();
    galley
        .update_reference("ref/GEN.usfm", OVERLAY_SOURCE, Some(true))
        .unwrap();

    let skeleton = galley
        .skeleton("ref/GEN.usfm", JsValue::UNDEFINED, None)
        .expect("the source has a skeleton");
    assert!(
        skeleton.contains(r#""sid":"GEN 2:23","where":"inside","ordinal":1,"marker":"q1""#),
        "{skeleton}"
    );
    assert!(
        skeleton.contains(r#""sid":"GEN 2:24","where":"leading","ordinal":1,"marker":"p""#),
        "{skeleton}"
    );

    let report = galley
        .overlay_report("books/GEN.usfm", "ref/GEN.usfm", JsValue::UNDEFINED)
        .expect("a report");
    assert!(report.contains(r#""removed":[]"#), "{report}");
    assert!(report.contains(r#""unpaired":[]"#), "{report}");
    assert!(
        report.contains(r#""empty":true"#),
        "inside blocks await text"
    );

    let applied = galley
        .overlay_text("books/GEN.usfm", "ref/GEN.usfm", JsValue::UNDEFINED)
        .expect("the overlay applies");
    assert!(
        applied.contains("\\q1\n\\q2\n\\q1\n\\q2\n\\p\n\\v 24"),
        "{applied}"
    );
    assert!(!applied.contains("\\f "), "the source's notes stay home");

    // Both coordinate spaces, from one transaction: the UTF-16 spans are the
    // byte spans through the module's own `toUtf16`.
    let bytes = galley
        .overlay("books/GEN.usfm", "ref/GEN.usfm", JsValue::UNDEFINED)
        .expect("a transaction");
    let units = galley
        .overlay("books/GEN.usfm", "ref/GEN.usfm", utf16_opts())
        .expect("the same, in UTF-16");
    assert_eq!(bytes.spans().len(), units.spans().len());
    assert_eq!(bytes.text(), units.text(), "the inserted text is the same");
    for (byte, unit) in bytes.spans().into_iter().zip(units.spans()) {
        assert_eq!(unit, onion::to_utf16(OVERLAY_TARGET, byte), "span {byte}");
    }
}

/// `{ utf16: true }`, built the way a host would.
fn utf16_opts() -> JsValue {
    let opts = js_sys::Object::new();
    js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("utf16"),
        &JsValue::from_bool(true),
    )
    .expect("a plain object takes a property");
    opts.into()
}

/// `{ caseSensitive: true }`, `find`'s options built the way a host would.
fn case_sensitive_opts() -> JsValue {
    scoped_opts(None)
}

/// `{ caseSensitive: true, scope? }`, `findAll`'s options built the way a
/// host would; `None` omits `scope` for the default.
fn scoped_opts(scope: Option<&str>) -> JsValue {
    let opts = js_sys::Object::new();
    js_sys::Reflect::set(
        &opts,
        &JsValue::from_str("caseSensitive"),
        &JsValue::from_bool(true),
    )
    .expect("a plain object takes a property");
    if let Some(scope) = scope {
        js_sys::Reflect::set(
            &opts,
            &JsValue::from_str("scope"),
            &JsValue::from_str(scope),
        )
        .expect("a plain object takes a property");
    }
    opts.into()
}
