//! The ONE boundary test: a real book in, known values out, in Node.
//!
//! Run it with `wasm-pack test --node` from `onion-wasm/`. Everything else about
//! `analyze` is tested NATIVELY (tests/analyze_corpus.rs re-derives every
//! read over the whole corpus) — with no mirror types there is nothing
//! boundary-specific left to drift, so this checks only that the wall itself
//! works: a string goes in, typed arrays come out, and the numbers in them are
//! the ones a hand count says they should be.
//!
//! The book is an EXCERPT of `testData/samples-from-wild/hindi-IRV1` (MAT 1–2,
//! dense Devanagari at 3 bytes per character) inlined rather than
//! `include_str!`d: the corpus trees are gitignored, and a missing file is a
//! COMPILE error where a missing corpus should be a skip.

use js_sys::{Object, Reflect, Uint32Array};
use onion_wasm::{FormatOpts, analyze, format_edits, format_edits_in, wants_all};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

/// MAT 1:1-2 and a synthetic chapter 2 carrying a footnote, verbatim Hindi.
const BOOK: &str = "\\id MAT\n\\ide UTF-8\n\\rem Copyright Information: Creative Commons Attribution-ShareAlike 4.0 License\n\\h मत्ती\n\\toc1 मत्ती\n\\toc2 मत्ती\n\\mt1 Matthew\n\\mt1 मत्ती\n\\c 1\n\\s यीशु मसीह की वंशावली\n\\p\n\\v 1 अब्राहम की सन्तान, दाऊद की सन्तान, यीशु मसीह* की वंशावली*।\n\\v 2 अब्राहम से इसहाक उत्‍पन्‍न हुआ, इसहाक से याकूब उत्‍पन्‍न हुआ, और याकूब से यहूदा और उसके भाई उत्‍पन्‍न हुए।\n\\c 2\n\\s चरवाहों\n\\p\n\\v 1 दूसरा अध्याय।\n\\v 2 \\f + \\ft एक टिप्पणी\\f* अन्तिम।\n";

/// One read off the plain object `analyze` returns. Reading it by NAME here is
/// half the point of the boundary test: a renamed key is a broken consumer.
fn read(a: &Object, key: &str) -> Vec<u32> {
    Uint32Array::from(Reflect::get(a, &JsValue::from_str(key)).expect("the key exists")).to_vec()
}

fn number(a: &Object, key: &str) -> u32 {
    Reflect::get(a, &JsValue::from_str(key))
        .expect("the key exists")
        .as_f64()
        .expect("a number") as u32
}

/// Strides, mirrored from `onion-wasm.ts` — the point of pinning them here is that
/// a change to either has to be made twice, deliberately.
const CHAPTERS: usize = 7;
const LINES: usize = 4;
const VERSE_ANCHORS: usize = 5;
const NOTE_EXTENTS: usize = 3;
const NOTE_PARTS: usize = 4;
const DIAGNOSTICS: usize = 7;

/// The number-shaped flag, mirrored from `onion-wasm.ts` for the same reason.
const NUMBER_SHAPED: u32 = 1 << 31;

#[wasm_bindgen_test]
fn a_real_book_crosses_the_wall_with_the_right_numbers() {
    let a = analyze(BOOK, wants_all(), None, None);
    let len = number(&a, "lenUtf16");

    // Hand count: 438 UTF-16 code units for 844 bytes — dense 3-byte script.
    assert_eq!(len, 438);

    // Front matter, chapter 1, chapter 2.
    let chapters = read(&a, "chapters");
    assert_eq!(chapters.len(), 3 * CHAPTERS);
    // `\c 1` starts at byte 196, which is UTF-16 156; `\c 2` at byte 700 = 364.
    assert_eq!(chapters[CHAPTERS + 5], 156, "chapter 1 starts");
    assert_eq!(chapters[2 * CHAPTERS + 5], 364, "chapter 2 starts");
    assert_eq!(
        chapters[CHAPTERS],
        1 | NUMBER_SHAPED,
        "its number, and it IS one"
    );
    assert_eq!(chapters[2 * CHAPTERS], 2 | NUMBER_SHAPED);
    // The `\c` marker is where the chapter starts, and content is past `\c 1`.
    assert_eq!(chapters[CHAPTERS + 1], 156, "the `\\c` marker");
    assert_eq!(chapters[CHAPTERS + 4], 160, "past `\\c 1`");
    // Row 0 has no marker at all.
    assert_eq!(chapters[1], u32::MAX);
    assert_eq!(chapters[4], u32::MAX);
    // The rows tile: the last one reaches the end of the document.
    assert_eq!(chapters[2 * CHAPTERS + 6], len);

    // Four verses, and the last one's `\v` sits at byte 776 = UTF-16 402, so
    // its designator (`2`, three bytes further into ASCII) is at 405.
    let verses = read(&a, "verseAnchors");
    assert_eq!(verses.len(), 4 * VERSE_ANCHORS);
    assert_eq!(
        verses[3 * VERSE_ANCHORS],
        2 | NUMBER_SHAPED,
        "it is in chapter 2, and its designator is a number"
    );
    assert_eq!(
        verses[3 * VERSE_ANCHORS + 1],
        402,
        "the `\\v` marker starts"
    );
    assert_eq!(verses[3 * VERSE_ANCHORS + 2], 405, "its designator starts");
    assert_eq!(verses[3 * VERSE_ANCHORS + 3], 406, "and is one unit long");
    assert_eq!(
        verses[3 * VERSE_ANCHORS + 4],
        407,
        "content is past the delimiter"
    );

    // One footnote, inside chapter 2 and inside the document.
    let notes = read(&a, "noteExtents");
    assert_eq!(notes.len(), NOTE_EXTENTS);
    assert_eq!(notes[0], 0, "the \\f family");
    assert!(notes[1] > 364 && notes[2] <= len);

    // The `\mt1` repeated on two lines is legal; the book is clean apart from
    // the `*` footnote-caller shapes the wild file carries. Whatever fires,
    // every finding must land inside the document and name a real code.
    for finding in read(&a, "diagnostics").chunks_exact(DIAGNOSTICS) {
        assert!(finding[1] <= finding[2]);
        assert!(finding[2] <= len);
    }

    // The `\id` line is a marked line, and it is METADATA rather than prose:
    // FRONT | META, the split the class word carries (bit 8).
    let lines = read(&a, "lines");
    assert!(lines.len() >= LINES);
    assert_eq!(lines[1], 0, "the first marked line starts at byte 0");
    assert_eq!(lines[0] & (1 << 4), 1 << 4, "FRONT");
    assert_eq!(lines[0] & (1 << 8), 1 << 8, "META");

    // The footnote's interior: a caller, then markup and body.
    let parts = read(&a, "noteParts");
    assert!(parts.len() >= 3 * NOTE_PARTS);
    assert_eq!(parts[0], 0, "they belong to note 0");
    assert_eq!(parts[1], 0, "and the first part is its CALLER");
    for part in parts.chunks_exact(NOTE_PARTS) {
        assert!(
            part[2] >= notes[1] && part[3] <= notes[2],
            "inside the extent"
        );
    }

    // The `\usfm` line is absent from this book, and that is not 3.0.
    assert_eq!(number(&a, "usfmVersion"), u32::MAX);

    // Token spans reach both ends of the document. (They no longer tile it:
    // a folded delimiter run's remainder belongs to no span — the
    // one-delimiter rule; this book neither starts nor ends inside one.)
    let tokens = read(&a, "tokenSpans");
    assert!(!tokens.is_empty());
    assert_eq!(tokens[1], 0);
    assert_eq!(tokens[tokens.len() - 1], len);
}

/// A clip bounds the token reads and nothing else.
#[wasm_bindgen_test]
fn a_clip_reaches_the_wall() {
    let whole = analyze(BOOK, wants_all(), None, None);
    let clipped = analyze(BOOK, wants_all(), Some(0), Some(156));
    assert_eq!(read(&whole, "chapters"), read(&clipped, "chapters"));
    assert!(read(&clipped, "tokenSpans").len() < read(&whole, "tokenSpans").len());
}

/// The ranged write path across the wall: a chapter window given in UTF-16 over
/// 3-byte-per-character Devanagari, and every edit it yields inside it.
#[wasm_bindgen_test]
fn a_ranged_format_takes_its_window_in_utf16() {
    let opts = FormatOpts::new();
    let whole = format_edits(BOOK, &opts).spans();
    // Chapter 2 starts at UTF-16 364 (the byte offset is 700 — the wall matters).
    let ranged = format_edits_in(BOOK, 364, u32::MAX, &opts).spans();
    assert!(!whole.is_empty());
    assert!(ranged.len() < whole.len());
    assert!(ranged.chunks_exact(2).all(|span| span[0] >= 364));
    // Every ranged edit is one of the whole-book edits — a subset, never a
    // different proposal.
    assert!(
        ranged
            .chunks_exact(2)
            .all(|span| whole.chunks_exact(2).any(|other| other == span))
    );
}
