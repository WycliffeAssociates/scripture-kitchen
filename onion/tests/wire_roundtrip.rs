//! The dish says what the engine said.
//!
//! Every value the writer emits is read back out of the bytes and compared to
//! the engine's own — the Rust half of the conformance the JS reader is held
//! to. If these disagree, no reader can be right.

use usfm_onion::wire::{self, ParseOptions, schema};

/// A reader over a plated parse, in Rust — deliberately independent of the
/// writer, so agreement means something.
struct Dish {
    bytes: Vec<u8>,
    sections: Vec<(usize, usize)>,
    utf16: bool,
    usfm_version: u32,
}

impl Dish {
    fn read(bytes: Vec<u8>) -> Dish {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!(word(0), schema::MAGIC, "magic");
        assert_eq!(word(4), schema::FORMAT_VERSION, "version");
        let count = word(8) as usize;
        assert_eq!(count, schema::SECTIONS.len(), "section count");
        let utf16 = word(12) & schema::FLAG_UTF16 != 0;
        let usfm_version = word(16);
        let sections = (0..count)
            .map(|n| (word(32 + n * 8) as usize, word(32 + n * 8 + 4) as usize))
            .collect();
        Dish {
            bytes,
            sections,
            utf16,
            usfm_version,
        }
    }

    fn section(&self, n: usize) -> &[u8] {
        let (at, len) = self.sections[n];
        &self.bytes[at..at + len]
    }

    /// One field of one row, by name — the reader's job, done the slow way.
    fn field(&self, section: usize, record: &schema::Record, row: usize, name: &str) -> u32 {
        let bytes = self.section(section);
        let at = row * record.stride() + record.offset_of(name);
        let width = record
            .fields
            .iter()
            .find(|f| f.name == name)
            .expect("a declared field")
            .width;
        match width {
            schema::Width::U8 => u32::from(bytes[at]),
            schema::Width::U16 => {
                u32::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()))
            }
            schema::Width::U32 => u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
        }
    }

    fn rows(&self, section: usize, record: &schema::Record) -> usize {
        self.section(section).len() / record.stride()
    }
}

const SAMPLE: &str = "\\id GEN\n\\usfm 3.0\n\\c 1\n\\p \\v 1 In the beginning\\f + \\ft note\\f* .\n\\q1 \\v 2-3 More text\n\\c 2\n\\p \\v 1 Second chapter\n";

fn dish(text: &str, opts: ParseOptions) -> Dish {
    let parsed = wire::parse(text, opts);
    Dish::read(wire::plate(&parsed))
}

#[test]
fn header_is_well_formed() {
    let d = dish(SAMPLE, ParseOptions::default());
    assert!(!d.utf16);
    // The `\usfm 3.0` line, read whether or not anything else was asked for.
    assert_eq!(d.usfm_version, 0, "3.0 is ladder index 0");
    // Every section is 4-aligned so a reader may take a typed-array view.
    for (at, _) in &d.sections {
        assert_eq!(at % 4, 0, "section at {at} is not 4-aligned");
    }
}

#[test]
fn tokens_round_trip() {
    let parsed = wire::parse(SAMPLE, ParseOptions::default());
    let d = Dish::read(wire::plate(&parsed));
    assert_eq!(d.rows(0, &schema::TOKEN), parsed.tokens.len());
    for (i, token) in parsed.tokens.iter().enumerate() {
        assert_eq!(
            d.field(0, &schema::TOKEN, i, "start"),
            token.start,
            "start {i}"
        );
        assert_eq!(d.field(0, &schema::TOKEN, i, "end"), token.end(), "end {i}");
        assert_eq!(
            d.field(0, &schema::TOKEN, i, "kind"),
            u32::from(token.kind_bits),
            "kind {i}"
        );
        assert_eq!(
            d.field(0, &schema::TOKEN, i, "marker"),
            u32::from(token.marker_idx),
            "marker {i}"
        );
        assert_eq!(
            d.field(0, &schema::TOKEN, i, "level"),
            u32::from(token.level),
            "level {i}"
        );
        assert_eq!(d.field(0, &schema::TOKEN, i, "reserved"), 0, "reserved {i}");
    }
}

#[test]
fn nodes_and_arena_round_trip() {
    let parsed = wire::parse(SAMPLE, ParseOptions::default());
    let d = Dish::read(wire::plate(&parsed));
    assert_eq!(d.rows(1, &schema::NODE), parsed.cst.nodes.len());
    for (i, node) in parsed.cst.nodes.iter().enumerate() {
        assert_eq!(
            d.field(1, &schema::NODE, i, "token"),
            node.token,
            "token {i}"
        );
        assert_eq!(
            d.field(1, &schema::NODE, i, "childFrom"),
            node.children.start,
            "childFrom {i}"
        );
        assert_eq!(
            d.field(1, &schema::NODE, i, "childTo"),
            node.children.end,
            "childTo {i}"
        );
        assert_eq!(
            d.field(1, &schema::NODE, i, "reason"),
            u32::from(node.reason)
        );
        assert_eq!(d.field(1, &schema::NODE, i, "ctx"), u32::from(node.ctx));
    }
    let arena = d.section(2);
    assert_eq!(arena.len() / 4, parsed.cst.child_ids.len());
    for (i, id) in parsed.cst.child_ids.iter().enumerate() {
        let got = u32::from_le_bytes(arena[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(got, *id, "child id {i}");
    }
}

#[test]
fn toc_is_absent_unless_asked_for() {
    let plain = dish(SAMPLE, ParseOptions::default());
    assert_eq!(plain.rows(7, &schema::CHAPTER), 0);
    assert_eq!(plain.rows(8, &schema::VERSE), 0);

    let asked = dish(
        SAMPLE,
        ParseOptions {
            toc: true,
            ..Default::default()
        },
    );
    // `\c 1`, `\c 2`, and the front-matter row.
    assert_eq!(asked.rows(7, &schema::CHAPTER), 3);
    assert_eq!(asked.field(7, &schema::CHAPTER, 1, "number"), 1);
    assert_eq!(asked.field(7, &schema::CHAPTER, 2, "number"), 2);
    // Three `\v`, one of them a bridge.
    assert_eq!(asked.rows(8, &schema::VERSE), 3);
    assert_eq!(asked.field(8, &schema::VERSE, 1, "first"), 2);
    assert_eq!(asked.field(8, &schema::VERSE, 1, "last"), 3);
}

#[test]
fn diagnostics_are_absent_unless_asked_for() {
    let plain = dish(SAMPLE, ParseOptions::default());
    assert_eq!(plain.rows(3, &schema::DIAGNOSTIC), 0);

    let asked = dish(
        "\\id GEN\n\\c 1\n\\p \\v 1 unclosed \\add here\n",
        ParseOptions {
            diagnostics: true,
            ..Default::default()
        },
    );
    assert!(
        asked.rows(3, &schema::DIAGNOSTIC) > 0,
        "an unclosed \\add reports"
    );
}

#[test]
fn utf16_converts_offsets_and_nothing_else() {
    // Devanagari: three bytes per character, one UTF-16 code unit.
    let text = "\\id MAT\n\\c 1\n\\p \\v 1 अब्राहम की सन्तान\n";
    let opts = |utf16| ParseOptions {
        toc: true,
        utf16,
        ..Default::default()
    };
    let bytes = dish(text, opts(false));
    let units = dish(text, opts(true));
    assert!(units.utf16 && !bytes.utf16);

    assert_eq!(
        bytes.rows(0, &schema::TOKEN),
        units.rows(0, &schema::TOKEN),
        "the same tokens either way"
    );
    let mut differed = false;
    for i in 0..bytes.rows(0, &schema::TOKEN) {
        let b = bytes.field(0, &schema::TOKEN, i, "start");
        let u = units.field(0, &schema::TOKEN, i, "start");
        assert!(u <= b, "a UTF-16 offset never exceeds its byte offset");
        differed |= u != b;
        // Indices are indices in both spaces.
        assert_eq!(
            bytes.field(0, &schema::TOKEN, i, "marker"),
            units.field(0, &schema::TOKEN, i, "marker"),
        );
    }
    assert!(differed, "non-ASCII text must move some offset");

    // The arena and every node field are indices, so they are byte-identical.
    assert_eq!(bytes.section(1), units.section(1), "nodes never convert");
    assert_eq!(
        bytes.section(2),
        units.section(2),
        "child ids never convert"
    );
}

#[test]
fn utf16_offsets_match_the_engines_own_conversion() {
    let text = "\\id MAT\n\\c 1\n\\p \\v 1 अब्राहम की सन्तान — dash\n\\q1 \\v 2 more\n";
    let units = dish(
        text,
        ParseOptions {
            utf16: true,
            ..Default::default()
        },
    );
    let parsed = wire::parse(text, ParseOptions::default());
    let mut cursor = usfm_onion::utf16::Cursor::new(text.as_bytes());
    for (i, token) in parsed.tokens.iter().enumerate() {
        let expect = cursor.to_utf16(token.start);
        assert_eq!(
            units.field(0, &schema::TOKEN, i, "start"),
            expect,
            "token {i} start"
        );
    }
}

#[test]
fn empty_document_plates() {
    let d = dish("", ParseOptions::default());
    assert_eq!(d.rows(0, &schema::TOKEN), 0);
    // The root exists even with nothing under it.
    assert_eq!(d.rows(1, &schema::NODE), 1);
}

/// The `\usfm` version crosses even when nothing else was asked for — it gates
/// diagnostics, so a consumer needs it before it can show one.
#[test]
fn the_declared_version_is_unconditional() {
    for opts in [
        ParseOptions::default(),
        ParseOptions {
            diagnostics: true,
            toc: true,
            utf16: true,
        },
    ] {
        assert_eq!(
            dish(SAMPLE, opts).usfm_version,
            0,
            "3.0, whatever was asked"
        );
    }
    let silent = dish("\\id GEN\n\\c 1\n", ParseOptions::default());
    assert_eq!(
        silent.usfm_version,
        schema::NONE,
        "a document declaring none"
    );
}

/// A chapter and a verse each name their designator's token, so no consumer
/// re-implements `toc::designator_row` to find the number.
#[test]
fn the_designator_crosses() {
    let parsed = wire::parse(
        SAMPLE,
        ParseOptions {
            toc: true,
            ..Default::default()
        },
    );
    let d = Dish::read(wire::plate(&parsed));
    for row in 0..d.rows(7, &schema::CHAPTER) {
        let token = d.field(7, &schema::CHAPTER, row, "token");
        let designator = d.field(7, &schema::CHAPTER, row, "designator");
        if token == schema::NONE {
            assert_eq!(designator, schema::NONE, "front matter has neither");
            continue;
        }
        assert!(designator > token, "the designator follows its marker");
        assert_eq!(
            parsed.tokens[designator as usize].kind(),
            usfm_onion::TokenKind::Designator,
        );
    }
    for row in 0..d.rows(8, &schema::VERSE) {
        let designator = d.field(8, &schema::VERSE, row, "designator");
        assert_eq!(
            parsed.tokens[designator as usize].kind(),
            usfm_onion::TokenKind::Designator,
        );
    }
}
