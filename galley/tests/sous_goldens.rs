//! The three publications the wasm wall and the JS reader are held to.
//!
//! ```text
//! cold   GEN RUT JON + ref/RUT ref/JON  →  goldens/sous/cold.bin
//! edit   GEN replaced by GEN-edited     →  goldens/sous/edit.bin
//! knobs  casing off, the two dials moved →  goldens/sous/knobs.bin
//! ```
//!
//! Native is the pin. `galley/tests/wasm_wall.rs` drives the same three steps
//! through the `Galley` handle and `galley/tests/sous_conformance.mjs` drives
//! them through `pkg-node`; all three compare against these bytes, so the
//! claim "the wall changes nothing" is one comparison and not three.
//!
//! Instrument: SHAPES — five hand-written fixtures, `include_str!`d, chosen so
//! the published bytes hold a row of every wire code. Absent bytes are a
//! COMPILE error, never a silent skip.
//!
//! `UPDATE_GOLDENS=1 cargo test -p usfm_galley --test sous_goldens` rewrites
//! the three files and fails, saying so: regenerating is never a passing test.

use std::path::PathBuf;

use sous_core::{Brigade, CorpusSnapshot, FindingKind, JudgingConfig};
use usfm_galley::sous::Expediter;
use usfm_galley::{Retain, Role};

/// The Warmer LRU ceiling; the whole fixture corpus is 15 KB, so it never bites.
const BUDGET: usize = 1 << 20;

pub const GEN: &str = include_str!("fixtures/sous/GEN.usfm");
pub const GEN_EDITED: &str = include_str!("fixtures/sous/GEN-edited.usfm");
pub const RUT: &str = include_str!("fixtures/sous/RUT.usfm");
pub const JON: &str = include_str!("fixtures/sous/JON.usfm");
pub const RUT_REF: &str = include_str!("fixtures/sous/ref/RUT.usfm");
pub const JON_REF: &str = include_str!("fixtures/sous/ref/JON.usfm");

pub const COLD: &[u8] = include_bytes!("goldens/sous/cold.bin");
pub const EDIT: &[u8] = include_bytes!("goldens/sous/edit.bin");
pub const KNOBS: &[u8] = include_bytes!("goldens/sous/knobs.bin");

/// The knobs `knobs.bin` is published under, in one place so the wasm wall and
/// the JS script move with it.
pub fn moved_knobs() -> JudgingConfig {
    let mut config = JudgingConfig::default();
    config.channels.casing = false;
    config.sentence_start_upper_bp = 9_990;
    config.lengths.z_short = 2.0;
    config
}

/// The three publications, in order, from one resident coordinator.
fn publications() -> [Vec<u8>; 3] {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    for (id, text) in [
        ("books/GEN.usfm", GEN),
        ("books/RUT.usfm", RUT),
        ("books/JON.usfm", JON),
    ] {
        sous.update(id, Role::Target, text).expect("a target");
    }
    for (id, text) in [("ref/RUT.usfm", RUT_REF), ("ref/JON.usfm", JON_REF)] {
        sous.update_with(id, Role::Reference, Retain::ProductsOnly, text)
            .expect("a reference");
    }
    let cold = sous.publish().expect("the cold publication");

    sous.update("books/GEN.usfm", Role::Target, GEN_EDITED)
        .expect("the keystroke");
    let edit = sous.publish().expect("the edit publication");

    let moved = moved_knobs();
    sous.set_config(((), moved, moved));
    let knobs = sous.publish().expect("the knobs publication");

    [cold, edit, knobs]
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/sous")
}

#[test]
fn the_three_publications_equal_their_goldens() {
    let published = publications();
    let named: [(&str, &[u8]); 3] = [("cold", COLD), ("edit", EDIT), ("knobs", KNOBS)];

    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        for ((name, _), bytes) in named.iter().zip(&published) {
            let path = goldens_dir().join(format!("{name}.bin"));
            std::fs::write(&path, bytes).expect("the goldens directory is writable");
        }
        panic!("UPDATE_GOLDENS rewrote the three goldens; rerun without it to test them");
    }

    for ((name, golden), bytes) in named.iter().zip(&published) {
        assert_eq!(
            bytes.as_slice(),
            *golden,
            "{name}.bin: the publication differs from its golden"
        );
    }
    assert_ne!(COLD, EDIT, "one changed word moves the publication");
    assert_ne!(EDIT, KNOBS, "moving a knob moves the publication");
}

#[test]
fn the_cold_golden_holds_a_row_of_every_wire_code() {
    let snapshot = CorpusSnapshot::open(COLD).expect("cold.bin is a corpus buffer");
    let (mut lengths, mut hygiene, mut conventions, mut presence) = (0, 0, 0, 0);
    for index in 0..snapshot.len() {
        let book = snapshot
            .book(sous_core::BookIndex::new(index).expect("a listed book"))
            .expect("a listed book");
        for row in 0..book.len() {
            match book.at(row).expect("a readable row").kind() {
                FindingKind::LengthProportionality(_) => lengths += 1,
                FindingKind::Hygiene(_) => hygiene += 1,
                FindingKind::Convention(_) => conventions += 1,
                FindingKind::Presence(_) => presence += 1,
            }
        }
    }
    assert!(lengths > 0, "no length row: the RUT pairing went quiet");
    assert!(
        hygiene > 0,
        "no hygiene row: the NBSP in GEN 1:3 went quiet"
    );
    assert!(
        conventions > 0,
        "no convention row: the glyph lanes went quiet"
    );
    assert!(
        presence > 0,
        "no presence row: ref/RUT 2:11 found a target counterpart"
    );
}

/// The knobs golden earns its name: the casing channel is off in it, so no row
/// may resolve to a `Casing` pattern.
#[test]
fn the_knobs_golden_publishes_no_casing_row() {
    let cold = CorpusSnapshot::open(COLD).expect("cold.bin is a corpus buffer");
    let knobs = CorpusSnapshot::open(KNOBS).expect("knobs.bin is a corpus buffer");
    let casing = |snapshot: &CorpusSnapshot<'_>| {
        snapshot
            .patterns()
            .expect("a readable pattern table")
            .iter()
            .filter(|pattern| pattern.channel == sous_core::Channel::Casing)
            .count()
    };
    assert_eq!(casing(&cold), 1, "the fixture's one casing pattern");
    assert_eq!(casing(&knobs), 0, "casing is off in the knobs publication");
}
