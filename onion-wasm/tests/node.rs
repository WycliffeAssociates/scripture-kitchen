//! The ONE boundary test: a real book in, a readable dish out, in Node.
//!
//! Run it with `wasm-pack test --node` from `onion-wasm/`.
//!
//! Everything about what the dish SAYS is tested natively — `onion`'s
//! `tests/wire_roundtrip.rs` reads every field back against the engine's own
//! values, and `tests/codegen_output_matches_input.rs` proves the reader and
//! the writer came from one schema. What is left, and all this checks, is that
//! the WALL works: a string crosses, bytes come back, and the header is intact.
//!
//! There are no mirrored strides here any more. The old version pinned them by
//! hand so "a change to either has to be made twice, deliberately"; the wire
//! reads them from `wire::schema`, which is the same declaration the reader was
//! generated from, so there is nothing left to keep in step.
//!
//! The book is an EXCERPT of `testData/samples-from-wild/hindi-IRV1` (MAT 1–2,
//! dense Devanagari at 3 bytes per character) inlined rather than
//! `include_str!`d: the corpus trees are gitignored, and a missing file is a
//! COMPILE error where a missing corpus should be a skip.

use onion_wasm::{FormatOpts, format_edits, format_edits_in, parse};
use usfm_onion::wire;
use wasm_bindgen_test::*;

/// MAT 1:1-2 and a synthetic chapter 2 carrying a footnote, verbatim Hindi.
const BOOK: &str = "\\id MAT\n\\ide UTF-8\n\\rem Copyright Information: Creative Commons Attribution-ShareAlike 4.0 License\n\\h मत्ती\n\\toc1 मत्ती\n\\toc2 मत्ती\n\\mt1 Matthew\n\\mt1 मत्ती\n\\c 1\n\\s यीशु मसीह की वंशावली\n\\p\n\\v 1 अब्राहम की सन्तान, दाऊद की सन्तान, यीशु मसीह* की वंशावली*।\n\\v 2 अब्राहम से इसहाक उत्‍पन्‍न हुआ, इसहाक से याकूब उत्‍पन्‍न हुआ, और याकूब से यहूदा और उसके भाई उत्‍पन्‍न हुए।\n\\c 2\n\\s चरवाहों\n\\p\n\\v 1 दूसरा अध्याय।\n\\v 2 \\f + \\ft एक टिप्पणी\\f* अन्तिम।\n";

fn word(dish: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(dish[at..at + 4].try_into().expect("four bytes"))
}

/// A section's `(offset, byte length)` by NAME — the reader's own lookup,
/// through the schema rather than a number written here.
fn section(dish: &[u8], name: &str) -> (usize, usize) {
    let n = wire::schema::SECTIONS
        .iter()
        .position(|s| s.name == name)
        .expect("a declared section");
    (
        word(dish, wire::HEADER_BYTES + n * wire::DIRECTORY_ENTRY_BYTES) as usize,
        word(
            dish,
            wire::HEADER_BYTES + n * wire::DIRECTORY_ENTRY_BYTES + 4,
        ) as usize,
    )
}

fn rows(dish: &[u8], name: &str, record: &wire::schema::Record) -> usize {
    section(dish, name).1 / record.stride()
}

#[wasm_bindgen_test]
fn the_wall_hands_over_an_intact_dish() {
    let dish = parse(BOOK, true, true, true);
    assert_eq!(word(&dish, 0), wire::MAGIC, "magic survived the crossing");
    assert_eq!(word(&dish, 4), wire::FORMAT_VERSION);
    assert_eq!(word(&dish, 8) as usize, wire::schema::SECTIONS.len());
    assert_ne!(word(&dish, 12) & wire::FLAG_UTF16, 0, "utf16 was asked for");

    // Every section lies inside the buffer and starts 4-aligned, so a reader
    // may take a typed-array view over any of them.
    for s in wire::schema::SECTIONS {
        let (at, len) = section(&dish, s.name);
        assert_eq!(at % 4, 0, "{} is not 4-aligned", s.name);
        assert!(at + len <= dish.len(), "{} runs past the buffer", s.name);
    }
}

#[wasm_bindgen_test]
fn the_hand_count_holds() {
    let dish = parse(BOOK, false, true, false);
    // Front matter, `\c 1`, `\c 2`.
    assert_eq!(rows(&dish, "chapters", &wire::schema::CHAPTER), 3);
    // Four `\v` across the two chapters.
    assert_eq!(rows(&dish, "verses", &wire::schema::VERSE), 4);
    assert!(rows(&dish, "tokens", &wire::schema::TOKEN) > 0);
    assert!(rows(&dish, "nodes", &wire::schema::NODE) > 0);
}

/// Devanagari is three bytes per character and one UTF-16 code unit, so the
/// two addressings cannot agree past the first Hindi byte — which is what
/// makes this book the right one to ask the question with.
#[wasm_bindgen_test]
fn utf16_is_opt_in_and_actually_converts() {
    let bytes = parse(BOOK, false, false, false);
    let units = parse(BOOK, false, false, true);
    assert_eq!(word(&bytes, 12) & wire::FLAG_UTF16, 0);
    assert_ne!(word(&units, 12) & wire::FLAG_UTF16, 0);

    let (b_at, _) = section(&bytes, "tokens");
    let (u_at, _) = section(&units, "tokens");
    let stride = wire::schema::TOKEN.stride();
    let last = rows(&bytes, "tokens", &wire::schema::TOKEN) - 1;
    let b_start = word(&bytes, b_at + last * stride);
    let u_start = word(&units, u_at + last * stride);
    assert!(
        u_start < b_start,
        "UTF-16 offsets fall behind byte ones here"
    );

    // Indices are indices in both: the arena is identical.
    let (ba, bl) = section(&bytes, "childIds");
    let (ua, ul) = section(&units, "childIds");
    assert_eq!(&bytes[ba..ba + bl], &units[ua..ua + ul]);
}

#[wasm_bindgen_test]
fn the_write_path_still_crosses() {
    let opts = FormatOpts::new();
    let edits = format_edits(BOOK, &opts);
    assert_eq!(edits.lens().len() * 2, edits.spans().len());
    let scoped = format_edits_in(BOOK, 0, u32::MAX, &opts);
    assert_eq!(scoped.spans(), edits.spans(), "the full range is the book");
}
