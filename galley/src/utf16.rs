//! `Utf16Table`: byte → UTF-16 with the source bytes GONE.
//!
//! ```text
//! let t = utf16_table("\\v 1 λόγος ἦν".as_bytes());   // then drop the string
//! t.to_utf16(16) == 11        // the ἦ's first byte is the 11th unit
//! t.len_utf16()  == 13        // …and no byte of the source is read again
//! ```
//!
//! One bit per source byte marks where a UTF-16 unit STARTS — a character's
//! lead byte, plus the byte after it for an astral character's second surrogate
//! — so a query is a cumulative read plus population counts and needs no bytes
//! to scan. Onion's [`Utf16Index`](crate::onion::Utf16Index) borrows its source and
//! stays the tool for a call that has the string; this is the form the
//! [`Pantry`](crate::Pantry) retains. Layout and size: `galley/src/pantry.md`.

/// Bytes per cumulative entry, matching [`crate::onion::utf16::STRIDE`]. One `u32`
/// per stride plus one bit per byte is 36 bytes per 256 of source, 14.1%.
pub const STRIDE: usize = 256;

/// Bytes per mark word.
const WORD: usize = 64;

/// Where every UTF-16 unit of one string starts, without the string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf16Table {
    /// Bit `i % 64` of word `i / 64` is set when source byte `i` starts a
    /// UTF-16 unit, so the units in any byte range are its population count.
    marks: Vec<u64>,
    /// `totals[k]` is the unit count of `source[..k * STRIDE]`, which bounds
    /// a query's popcounts at `STRIDE / WORD`.
    totals: Vec<u32>,
    len_utf16: u32,
    source_len: u32,
}

/// Builds the table for `source`, which must be valid UTF-8.
///
/// One linear pass, one bit per byte.
pub fn utf16_table(source: &[u8]) -> Utf16Table {
    let mut marks = vec![0u64; source.len().div_ceil(WORD)];
    // A unit starts at every non-continuation byte, and again at the
    // continuation after a 4-byte lead: that second mark IS the low surrogate,
    // which is what lets one mask count units instead of two masks counting
    // characters and astral characters apart.
    let mut prev = 0u8;
    for (at, &byte) in source.iter().enumerate() {
        if byte & 0xC0 != 0x80 || prev >= 0xF0 {
            marks[at / WORD] |= 1 << (at % WORD);
        }
        prev = byte;
    }

    let mut totals = Vec::with_capacity(source.len() / STRIDE + 1);
    let mut units = 0u32;
    for (word, bits) in marks.iter().enumerate() {
        if word % (STRIDE / WORD) == 0 {
            totals.push(units);
        }
        units += bits.count_ones();
    }
    // A source that ends exactly on a stride boundary has one more boundary
    // than it has stride-leading words.
    while totals.len() < source.len() / STRIDE + 1 {
        totals.push(units);
    }

    Utf16Table {
        marks,
        totals,
        len_utf16: units,
        source_len: source.len() as u32,
    }
}

impl Utf16Table {
    /// [`utf16_table`], for callers who prefer the associated form.
    pub fn new(source: &[u8]) -> Self {
        utf16_table(source)
    }

    /// The whole string's UTF-16 length.
    pub fn len_utf16(&self) -> u32 {
        self.len_utf16
    }

    /// The source byte length the table was built from.
    pub fn source_len(&self) -> u32 {
        self.source_len
    }

    /// Heap bytes this table occupies — the number the Pantry's retention
    /// budget counts.
    pub fn index_bytes(&self) -> usize {
        self.marks.len() * size_of::<u64>() + self.totals.len() * size_of::<u32>()
    }

    /// The UTF-16 offset of a CHARACTER-BOUNDARY source byte offset; past the
    /// end clamps to [`len_utf16`](Self::len_utf16).
    ///
    /// PRECONDITION: `byte` starts a character. Onion's index snaps an interior
    /// byte down to its character by reading the source; with no source present
    /// there is nothing to snap with, and every offset the Pantry rebases —
    /// mask ranges, TOC anchors, finding spans — is already boundary-legal.
    pub fn to_utf16(&self, byte: u32) -> u32 {
        let byte = (byte as usize).min(self.source_len as usize);
        let mut units = self.totals[byte / STRIDE];
        let last = byte / WORD;
        for word in byte / STRIDE * (STRIDE / WORD)..last {
            units += self.marks[word].count_ones();
        }
        let into = byte % WORD;
        if into > 0 {
            units += (self.marks[last] & ((1 << into) - 1)).count_ones();
        }
        units
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onion::utf16::utf16_index;

    /// A real verse from `hindi-IRV1/origin.usfm` — 3-byte Devanagari.
    const DEVANAGARI: &str = "\\v 1 अब्राहम की सन्तान, दाऊद की सन्तान, यीशु मसीह* की वंशावली।\n";

    fn zoo() -> Vec<String> {
        let mut zoo = vec![
            String::new(),
            "a".to_string(),
            "\\id GEN\r\n\\c 1\r\n\\v 1 In the beginning God created.\r\n".to_string(),
            "\\v 1 λόγος ἦν".to_string(),
            DEVANAGARI.to_string(),
            "emoji \u{1f600}\u{1f4d6} flag \u{1f1ee}\u{1f1f3} gothic \u{10330}\u{10331} tail\n"
                .to_string(),
            "\u{1f600}".to_string(),
            // Exactly one stride, and exactly two.
            "x".repeat(STRIDE),
            "x".repeat(STRIDE * 2),
        ];
        // A multi-byte character straddling a stride boundary, one string per
        // split offset within a 3-byte and a 4-byte character.
        for split in 1..4 {
            zoo.push(format!(
                "{}\u{0905}{DEVANAGARI}",
                "x".repeat(STRIDE - split)
            ));
        }
        for split in 1..5 {
            zoo.push(format!(
                "{}\u{1f600}tail\u{1f600}",
                "x".repeat(STRIDE - split)
            ));
        }
        // Straddle a later boundary, with dense text before it.
        zoo.push(format!("{}\u{1f600}rest", DEVANAGARI.repeat(20)));
        zoo
    }

    #[test]
    fn every_character_boundary_equals_onions_borrowing_index() {
        for source in zoo() {
            let table = utf16_table(source.as_bytes());
            let index = utf16_index(source.as_bytes());
            assert_eq!(table.len_utf16(), index.len_utf16(), "{source:?} total");
            for (byte, _) in source.char_indices() {
                assert_eq!(
                    table.to_utf16(byte as u32),
                    index.to_utf16(byte as u32),
                    "{source:?} @{byte}"
                );
            }
            let end = source.len() as u32;
            assert_eq!(table.to_utf16(end), index.to_utf16(end), "{source:?} @end");
            assert_eq!(table.to_utf16(u32::MAX), table.len_utf16(), "past the end");
        }
    }

    #[test]
    fn the_greek_module_doc_example() {
        let table = utf16_table("\\v 1 λόγος ἦν".as_bytes());
        assert_eq!(table.to_utf16(16), 11);
        assert_eq!(table.len_utf16(), 13);
    }

    /// The size claim: one bit per byte plus one `u32` per stride.
    #[test]
    fn the_table_is_one_bit_per_byte_plus_a_stride_total() {
        let source = DEVANAGARI.repeat(200);
        let table = utf16_table(source.as_bytes());
        assert_eq!(
            table.index_bytes(),
            source.len().div_ceil(WORD) * 8 + (source.len() / STRIDE + 1) * 4
        );
        assert!(
            table.index_bytes() * 4 <= source.len(),
            "{} table bytes for {} of source",
            table.index_bytes(),
            source.len()
        );
    }
}
