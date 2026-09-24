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

use js_sys::{Object, Reflect};
use onion_wasm::{FormatOpts, attr_resolve, attrs, format_edits, format_edits_in, parse, to_utf16};
use usfm_onion::wire;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

/// MAT 1:1-2 and a synthetic chapter 2 carrying a footnote, verbatim Hindi.
const BOOK: &str = "\\id MAT\n\\ide UTF-8\n\\rem Copyright Information: Creative Commons Attribution-ShareAlike 4.0 License\n\\h मत्ती\n\\toc1 मत्ती\n\\toc2 मत्ती\n\\mt1 Matthew\n\\mt1 मत्ती\n\\c 1\n\\s यीशु मसीह की वंशावली\n\\p\n\\v 1 अब्राहम की सन्तान, दाऊद की सन्तान, यीशु मसीह* की वंशावली*।\n\\v 2 अब्राहम से इसहाक उत्‍पन्‍न हुआ, इसहाक से याकूब उत्‍पन्‍न हुआ, और याकूब से यहूदा और उसके भाई उत्‍पन्‍न हुए।\n\\c 2\n\\s चरवाहों\n\\p\n\\v 1 दूसरा अध्याय।\n\\v 2 \\f + \\ft एक टिप्पणी\\f* अन्तिम।\n";

/// An options object as a JS caller writes one.
fn opts(pairs: &[(&str, bool)]) -> Option<JsValue> {
    let bag = Object::new();
    for (key, value) in pairs {
        Reflect::set(&bag, &JsValue::from_str(key), &JsValue::from_bool(*value))
            .expect("a plain object");
    }
    Some(bag.into())
}

/// `parse` with the three flags spelled as a caller spells them.
fn parsed(text: &str, diagnostics: bool, toc: bool, utf16: bool) -> Vec<u8> {
    parse(
        text,
        opts(&[("diagnostics", diagnostics), ("toc", toc), ("utf16", utf16)]),
    )
    .expect("known options")
}

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
    let dish = parsed(BOOK, true, true, true);
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

/// The header's last three words: the source's length in the dish's own offset
/// space, and its hash over the bytes whichever space that is.
#[wasm_bindgen_test]
fn the_header_names_the_source_it_came_from() {
    let bytes = parsed(BOOK, false, false, false);
    let units = parsed(BOOK, false, false, true);
    let hash = |d: &[u8]| u64::from_le_bytes(d[24..32].try_into().expect("eight bytes"));

    assert_eq!(word(&bytes, 20) as usize, BOOK.len());
    assert_eq!(
        word(&units, 20),
        BOOK.encode_utf16().count() as u32,
        "Devanagari is three bytes and one code unit"
    );
    assert!(word(&units, 20) < word(&bytes, 20));
    assert_eq!(hash(&bytes), hash(&units), "always over the bytes");
}

#[wasm_bindgen_test]
fn the_hand_count_holds() {
    let dish = parsed(BOOK, false, true, false);
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
    let bytes = parsed(BOOK, false, false, false);
    let units = parsed(BOOK, false, false, true);
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

/// The attribute interpreter's four shapes, across the wall. The span the
/// caller passes is the LIST TOKEN's, found here the way an editor would find
/// it — off a parse it already holds.
#[wasm_bindgen_test]
fn the_attribute_view_crosses() {
    // (four words per attribute, then the malformed code and its offset)
    let read = |text: &str, utf16: bool| -> Vec<u32> {
        let list = usfm_onion::lex(text)
            .into_iter()
            .find(|t| t.kind() == usfm_onion::TokenKind::AttrList)
            .expect("an attribute list");
        let (from, to) = if utf16 {
            (to_utf16(text, list.start), to_utf16(text, list.end()))
        } else {
            (list.start, list.end())
        };
        attrs(text, from, to, opts(&[("utf16", utf16)])).expect("known options")
    };
    fn slice(text: &str, from: u32, to: u32) -> &str {
        &text[from as usize..to as usize]
    }

    // Pairs.
    let text = "\\w grace|lemma=\"grace\" x-y=\"z\"\\w*";
    let flat = read(text, false);
    assert_eq!(flat.len(), 2 * 4 + 2);
    assert_eq!(slice(text, flat[0], flat[1]), "lemma");
    assert_eq!(slice(text, flat[2], flat[3]), "grace");
    assert_eq!(slice(text, flat[4], flat[5]), "x-y");
    assert_eq!(slice(text, flat[6], flat[7]), "z");
    assert_eq!(&flat[8..], [wire::NONE, wire::NONE], "nothing malformed");

    // Bare: an EMPTY name span sitting at the value's start.
    let text = "\\w In|in\\w*";
    let flat = read(text, false);
    assert_eq!(flat.len(), 4 + 2);
    assert_eq!(flat[0], flat[1], "a bare name span is empty");
    assert_eq!(flat[1], flat[2], "…and sits at the value");
    assert_eq!(slice(text, flat[2], flat[3]), "in");

    // Node-initial: the closing pipe and the HS it absorbed are delimiters.
    let text = "\\p|cat=\"emphasised\"| text";
    let flat = read(text, false);
    assert_eq!(flat.len(), 4 + 2);
    assert_eq!(slice(text, flat[0], flat[1]), "cat");
    assert_eq!(slice(text, flat[2], flat[3]), "emphasised");

    // Unterminated quote: no attribute, and the OPENING quote is the offset.
    let text = "\\w x|lemma=\"grace\\w*";
    let flat = read(text, false);
    assert_eq!(flat.len(), 2, "a malformed tail ends the walk");
    assert_eq!(flat[0], 0, "UnterminatedQuote");
    assert_eq!(&text[flat[1] as usize..flat[1] as usize + 1], "\"");

    // UTF-16, with a non-BMP character ahead of the list: two code units for
    // one character, so every word must come back short of its byte offset.
    let text = "\\p \u{1d11e}\n\\w grace|lemma=\"grace\"\\w*";
    let bytes = read(text, false);
    let units = read(text, true);
    assert_eq!(units.len(), bytes.len());
    let doc: Vec<u16> = text.encode_utf16().collect();
    assert!(units[0] < bytes[0], "the offsets have drifted apart");
    assert_eq!(
        String::from_utf16(&doc[units[0] as usize..units[1] as usize]).expect("a name"),
        "lemma"
    );
    assert_eq!(
        String::from_utf16(&doc[units[2] as usize..units[3] as usize]).expect("a value"),
        "grace"
    );
}

/// A name against a row: the resolution code, and nothing that is a string.
#[wasm_bindgen_test]
fn attribute_names_resolve_against_the_table() {
    let w = usfm_onion::tables::generated::marker_idx(
        b"w",
        usfm_onion::tables::schema::SpellingShape::PlainOnly,
    );
    assert_eq!(attr_resolve("lemma", u32::from(w)), 0, "Defined");
    assert_eq!(attr_resolve("", u32::from(w)), 0, "the bare default is one");
    assert_eq!(attr_resolve("x-strong", u32::from(w)), 1, "UserNamespace");
    assert_eq!(attr_resolve("nonesuch", u32::from(w)), 2, "Unknown");
}

#[wasm_bindgen_test]
fn the_write_path_still_crosses() {
    let opts = FormatOpts::new();
    let edits = format_edits(BOOK, &opts);
    assert_eq!(edits.lens().len() * 2, edits.spans().len());
    let scoped = format_edits_in(BOOK, 0, u32::MAX, &opts);
    assert_eq!(scoped.spans(), edits.spans(), "the full range is the book");
}

/// An options object is checked at the wall: an unknown key is refused by
/// name, never read as `false`, and absent is every default.
#[wasm_bindgen_test]
fn an_unknown_option_is_refused_by_name() {
    assert!(parse(BOOK, None).is_ok(), "no options is every default");
    assert!(parse(BOOK, Some(JsValue::NULL)).is_ok());
    let misspelled = onion_wasm::options::parse_options(opts(&[("tco", true)]).as_ref(), "parse")
        .expect_err("a misspelling");
    assert!(misspelled.contains("\"tco\""), "{misspelled}");
    let wrong_type = onion_wasm::options::parse_options(Some(&JsValue::from_f64(1.0)), "parse")
        .expect_err("not an object");
    assert!(wrong_type.contains("must be an object"), "{wrong_type}");
}
