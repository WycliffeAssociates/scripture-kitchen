//! `Utf16Index`: byte ↔ UTF-16 offsets over one string, both directions O(1).
//!
//! The engine counts bytes; a JS editor counts UTF-16 code units. Over
//! `\v 1 λόγος ἦν` the two spaces drift apart:
//!
//! ```text
//! bytes   \  v  ␠  1  ␠  CE BB CF 8C CE B3 CE BF CF 82 ␠  E1 BC A6 CE BD
//! byte#   0  1  2  3  4  5     7     9     11    13    15 16       19
//! utf16#  0  1  2  3  4  5     6     7     8     9     10 11       12
//!                        λ     ό     γ     ο     ς        ἦ        ν
//!
//! let ix = utf16_index("\\v 1 λόγος ἦν".as_bytes());
//! ix.to_utf16(16) == 11        // the ἦ's first byte is the 11th unit
//! ix.to_byte(11)  == 16
//! ix.len_utf16()  == 13
//! ```
//!
//! One `u32` per [`STRIDE`] bytes holds the unit count at that boundary
//! (4 bytes per 256 = 1.6% of the string), so a query is a table read plus a
//! SWAR count over ≤`STRIDE` bytes. Nothing decodes; the count needs only byte
//! CLASSES, because UTF-16 units are
//! `bytes − continuations + (leads ≥ 0xF0)` for every character:
//!
//! ```text
//! char      utf8 bytes         utf16   formula
//! e         1  (65)            1       1 − 0 + 0
//! λ         2  (CE BB)         1       2 − 1 + 0
//! ἦ         3  (E1 BC A6)      1       3 − 2 + 0
//! 𝕽 U+1D57D 4  (F0 9D 95 BD)   2       4 − 3 + 1   (a surrogate pair)
//! ```
//!
//! An index belongs to exactly ONE string — it borrows the bytes it indexes, so
//! pointing a source index at a masked view is unrepresentable. Nothing is
//! cached: a whole book builds in microseconds from bytes the caller already
//! holds, so no invalidation story exists to get wrong.

/// Bytes between stride boundaries. 256 costs `n/64` index bytes (1.6% of the
/// source) and bounds every query's scan at 256 bytes — a handful of SWAR
/// words. 1024 would cost 0.4% for a 4× longer scan.
pub const STRIDE: usize = 256;

/// UTF-16 code units for `bytes`, counting each character at its LEAD byte.
///
/// ```text
/// utf16_len("λόγος".as_bytes()) == 5     // 10 bytes − 5 continuations
/// utf16_len("😀".as_bytes())    == 2     // 4 − 3 + 1
/// utf16_len(&"😀".as_bytes()[..2]) == 2  // a truncated char still scores whole
/// utf16_len(&"😀".as_bytes()[2..]) == 0  // …and its orphaned tail scores 0
/// ```
///
/// The last two lines are load-bearing: `bytes` need not be a whole number of
/// characters, and the "whole at the lead, 0 per continuation" rule is what
/// lets a stride boundary fall mid-character with no special case — see
/// [`Utf16Index::to_utf16`].
///
/// Two population counts per 8 bytes, endian-agnostic (every operation is
/// per-byte and every shift's cross-byte spill lands in bits `HIGH` discards):
/// a continuation is bit7 set with bit6 clear, a `≥ 0xF0` lead is bits 7-4 all
/// set.
pub fn utf16_len(bytes: &[u8]) -> u32 {
    const HIGH: u64 = 0x8080_8080_8080_8080;

    let mut continuations = 0u32;
    let mut four_byte = 0u32;

    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let w = u64::from_ne_bytes(chunk.try_into().expect("chunks_exact(8)"));
        continuations += (w & !(w << 1) & HIGH).count_ones();
        four_byte += (w & (w << 1) & (w << 2) & (w << 3) & HIGH).count_ones();
    }
    for &b in chunks.remainder() {
        if b & 0xC0 == 0x80 {
            continuations += 1;
        } else if b >= 0xF0 {
            four_byte += 1;
        }
    }

    bytes.len() as u32 - continuations + four_byte
}

/// Cumulative UTF-16 length at every [`STRIDE`]-byte boundary of one string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf16Index<'s> {
    source: &'s [u8],
    /// `strides[k] == utf16_len(&source[..k * STRIDE])`, therefore strictly
    /// increasing (a full 256-byte block is worth at least 64 units), which is
    /// what makes [`Utf16Index::to_byte`] a binary search. `strides[0] == 0`,
    /// and the trailing partial block gets no entry — it is the scan's job.
    strides: Vec<u32>,
}

/// Builds the index for `source`, which must be valid UTF-8.
///
/// One linear SWAR pass, ~µs per book. `source.len() / STRIDE + 1` u32s.
pub fn utf16_index(source: &[u8]) -> Utf16Index<'_> {
    let mut strides = Vec::with_capacity(source.len() / STRIDE + 1);
    strides.push(0);
    let mut units = 0u32;
    let mut at = 0usize;
    while at + STRIDE <= source.len() {
        units += utf16_len(&source[at..at + STRIDE]);
        strides.push(units);
        at += STRIDE;
    }
    Utf16Index { source, strides }
}

impl<'s> Utf16Index<'s> {
    /// [`utf16_index`], for callers who prefer the associated form.
    pub fn new(source: &'s [u8]) -> Self {
        utf16_index(source)
    }

    /// The whole string's UTF-16 length.
    pub fn len_utf16(&self) -> u32 {
        let last = self.strides.len() - 1;
        self.strides[last] + utf16_len(&self.source[last * STRIDE..])
    }

    /// Heap bytes this index occupies.
    pub fn index_bytes(&self) -> usize {
        self.strides.len() * size_of::<u32>()
    }

    /// The UTF-16 offset of a source byte offset. TOTAL, like `Toc::locate`:
    ///
    /// - a byte INSIDE a multi-byte character reports that CHARACTER's offset
    ///   (snap down) — the ἦ's second and third bytes answer 11, same as its
    ///   first. UTF-16 has no name for a position inside a character, and
    ///   snapping down is the direction [`to_byte`](Self::to_byte) snaps too,
    ///   so a garbage offset stays inside the character it came from instead of
    ///   sliding onto the next one.
    /// - past the end clamps to [`len_utf16`](Self::len_utf16).
    ///
    /// No search: read `strides[byte / STRIDE]`, then count the remainder. If
    /// the block boundary splits a character, the table already counted it
    /// whole and its leading continuation bytes score 0 — the halves compose.
    pub fn to_utf16(&self, byte: u32) -> u32 {
        let mut byte = (byte as usize).min(self.source.len());
        // Back onto a character boundary — at most 3 steps, and none at all on
        // the boundary-legal offsets every real caller passes.
        while byte > 0 && byte < self.source.len() && self.source[byte] & 0xC0 == 0x80 {
            byte -= 1;
        }
        let k = byte / STRIDE;
        self.strides[k] + utf16_len(&self.source[k * STRIDE..byte])
    }

    /// The source byte offset of a UTF-16 offset. TOTAL:
    ///
    /// - an offset landing between the halves of a surrogate pair reports the
    ///   4-byte character's FIRST byte (snap down) — the low half names no byte
    ///   position, and this matches how an editor corrects a position onto a
    ///   safe boundary.
    /// - past the end clamps to `source.len()`.
    pub fn to_byte(&self, utf16: u32) -> u32 {
        // The last boundary at or before the target. `strides[0] == 0`, so the
        // prefix is never empty and the `- 1` cannot underflow.
        let k = self.strides.partition_point(|&units| units <= utf16) - 1;
        let mut at = k * STRIDE;
        let mut units = self.strides[k];

        // Word-at-a-time skip. `units == utf16_len(&source[..at])` holds at
        // EVERY byte position, aligned or not, so a whole word can be leapt
        // whenever it lands strictly before the target. Stop at `>=`, not `>`:
        // equality means the boundary is at or just past `at + 8`, and only the
        // byte walk can say which. Without this the scan is ~85 steps per query
        // on 3-byte script; with it, ~4 word ops plus a ≤8-byte tail.
        while at + 8 <= self.source.len() {
            let step = utf16_len(&self.source[at..at + 8]);
            if units + step >= utf16 {
                break;
            }
            units += step;
            at += 8;
        }

        // Skip the tail of a character straddling `at`: it was counted whole,
        // so `units` is already the NEXT boundary's offset.
        while at < self.source.len() && self.source[at] & 0xC0 == 0x80 {
            at += 1;
        }

        while at < self.source.len() {
            if units == utf16 {
                return at as u32;
            }
            let (width, step) = lead(self.source[at]);
            // Overshoot means `utf16` points inside this character's surrogate
            // pair — snap down to the character's start.
            if units + step > utf16 {
                return at as u32;
            }
            units += step;
            at += width;
        }
        self.source.len() as u32
    }
}

/// Byte → UTF-16 with NO byte scanning: the document as runs of uniform
/// character width.
///
/// ```text
/// "\\v 1 λόγος ἦν"   →  [ (0, 0, w1), (5, 5, w2), (15, 10, w1), (16, 11, w3), (22, 13, w1) ]
///                            ASCII       λόγος        space        ἦν            (end)
/// ```
///
/// Inside a run the map is affine — `utf16 = run.utf16 + (byte − run.byte) /
/// width` — so a query is a subtract, a divide by a constant, and an add.
/// [`Cursor`] costs a SWAR scan of every byte between consecutive queries,
/// which is one pass over the document PER caller; this costs one pass to
/// build and none thereafter, so ten readers share the one build.
///
/// The size is one entry per width change, i.e. per non-ASCII run and the
/// ASCII stretch after it. MEASURED: en_ulb 0.12% non-ASCII, 1,808 runs over
/// 4.5 MB; en_ult 11.7%, 935k over 103 MB. A document with no ASCII at all
/// approaches one entry per word, which is why [`Runs::worth_it`] exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runs {
    /// Ascending; `byte[i]` is the first byte of run `i`.
    byte: Vec<u32>,
    /// UTF-16 offset of `byte[i]`.
    utf16: Vec<u32>,
    /// Bytes per character in run `i` — 1, 2, 3 or 4.
    width: Vec<u8>,
    len_utf16: u32,
}

/// What a run map may cost, as a fraction of the source it maps, before a
/// [`Cursor`] is the better tool. One entry is nine bytes.
///
/// Deliberately tight because of where this runs: wasm linear memory GROWS AND
/// NEVER SHRINKS, so a map that is transient to one call still raises the
/// page's high-water mark for its lifetime. A sixteenth caps that at ~6% of the
/// document. MEASURED: en_ulb needs 0.02% (1,808 runs over 4.5 MB), en_ult
/// 0.8% — both far inside, so the cap only ever catches a document with almost
/// no ASCII in it, which is exactly where the map stops paying anyway.
const RUN_BUDGET_NUMERATOR: usize = 1;
const RUN_BUDGET_DENOMINATOR: usize = 16;
/// `byte` + `utf16` + `width` per entry.
const ENTRY_BYTES: usize = 9;
/// Bytes of [`Runs::promising`]'s look-ahead.
const PROBE: usize = 64 << 10;

impl Runs {
    /// One pass, ASCII eight bytes at a time — `None` when the document's shape
    /// would blow the budget, ABANDONED as soon as that is known rather than
    /// finished and discarded.
    ///
    /// A word with no high bit is eight width-1 characters, so the common case
    /// never classifies a lead byte at all — which is the difference between
    /// this and a `Cursor` scan on a document that is 99.9% ASCII. Only a word
    /// carrying a high bit falls to the byte walk.
    pub fn new(source: &[u8]) -> Option<Runs> {
        const HIGH: u64 = 0x8080_8080_8080_8080;
        let budget = source.len() * RUN_BUDGET_NUMERATOR / RUN_BUDGET_DENOMINATOR / ENTRY_BYTES;
        if !Runs::promising(source, budget) {
            return None;
        }
        let mut runs = Runs {
            byte: Vec::new(),
            utf16: Vec::new(),
            width: Vec::new(),
            len_utf16: 0,
        };
        let (mut at, mut units) = (0usize, 0u32);
        let mut open: Option<u8> = None;
        while at < source.len() {
            if open == Some(1) && at + 8 <= source.len() {
                // Skip whole ASCII words without touching a lead table.
                let word = u64::from_ne_bytes(source[at..at + 8].try_into().expect("eight bytes"));
                if word & HIGH == 0 {
                    at += 8;
                    units += 8;
                    continue;
                }
            }
            let (bytes, step) = lead(source[at]);
            let width = bytes as u8;
            if open != Some(width) {
                if runs.byte.len() >= budget {
                    return None;
                }
                runs.byte.push(at as u32);
                runs.utf16.push(units);
                runs.width.push(width);
                open = Some(width);
            }
            at += bytes;
            units += step;
        }
        runs.len_utf16 = units;
        Some(runs)
    }

    pub fn len_utf16(&self) -> u32 {
        self.len_utf16
    }

    /// A cheap read on whether the whole document can fit the budget, from its
    /// first [`PROBE`] bytes.
    ///
    /// Abandoning mid-build still pays for however much was walked first — on a
    /// script-heavy book that was a third of the document, walked and thrown
    /// away. A probe costs a fixed 64 KB and answers before anything is built.
    /// Density is not uniform, so the projection is allowed to be generous:
    /// being wrong here costs a scan, never an answer.
    fn promising(source: &[u8], budget: usize) -> bool {
        const HIGH: u64 = 0x8080_8080_8080_8080;
        let probe = source.len().min(PROBE);
        if probe == 0 {
            return true;
        }
        // Words carrying a high bit bound the width changes: a run boundary
        // needs one, and eight ASCII bytes can never hold a boundary.
        let mut mixed = 0usize;
        for word in source[..probe].chunks_exact(8) {
            let w = u64::from_ne_bytes(word.try_into().expect("eight bytes"));
            mixed += usize::from(w & HIGH != 0);
        }
        // Two boundaries per mixed word is the worst shape (in, then out).
        let projected = mixed * 2 * source.len() / probe;
        projected <= budget
    }

    /// Run index for `byte`, searched from scratch. `at` from a previous call
    /// on an ASCENDING sequence makes this a step instead — see [`Runs::walk`].
    fn run_of(&self, byte: u32) -> usize {
        self.byte
            .partition_point(|start| *start <= byte)
            .saturating_sub(1)
    }

    /// The UTF-16 offset of a source byte offset, with [`Cursor::to_utf16`]'s
    /// semantics: an interior byte snaps down to its character (the division
    /// does it), past the end clamps.
    pub fn to_utf16(&self, byte: u32) -> u32 {
        if self.byte.is_empty() {
            return 0;
        }
        self.at_run(self.run_of(byte), byte)
    }

    /// `to_utf16` once the run is known.
    fn at_run(&self, run: usize, byte: u32) -> u32 {
        let base = self.byte[run];
        let into = byte.saturating_sub(base);
        let units = match self.width[run] {
            1 => into,
            2 => into / 2,
            3 => into / 3,
            _ => (into / 4) * 2,
        };
        (self.utf16[run] + units).min(self.len_utf16)
    }

    /// A cursor for one ASCENDING sequence of queries: the run index only ever
    /// moves forward, so a whole read's offsets cost one walk of the RUNS —
    /// never of the source.
    pub fn walk(&self) -> RunWalk<'_> {
        RunWalk { runs: self, at: 0 }
    }
}

/// See [`Runs::walk`].
pub struct RunWalk<'r> {
    runs: &'r Runs,
    at: usize,
}

impl RunWalk<'_> {
    /// Total, like [`Runs::to_utf16`]. A backward query is legal and rewinds.
    pub fn to_utf16(&mut self, byte: u32) -> u32 {
        if self.runs.byte.is_empty() {
            return 0;
        }
        if byte < self.runs.byte[self.at] {
            self.at = 0;
        }
        while self.at + 1 < self.runs.byte.len() && self.runs.byte[self.at + 1] <= byte {
            self.at += 1;
        }
        self.runs.at_run(self.at, byte)
    }
}

/// A STREAMING `(byte, utf16)` position over one string — the bulk-out sibling
/// of [`Utf16Index`].
///
/// ```text
/// let mut c = Cursor::new("\\v 1 λόγος ἦν".as_bytes());
/// c.to_utf16(5)  == 5      // walks 0 → 5
/// c.to_utf16(16) == 11     // walks 5 → 16, never re-counting the prefix
/// c.to_byte(13)  == 20     // the same walk, driven from the other side
/// ```
///
/// [`Utf16Index`] answers random-access queries and costs 1.6% of the source in
/// heap. A cursor answers a SORTED sequence of queries in one pass and costs
/// nothing: converting every offset an [`analyze`](mod@crate::analyze) read emits is
/// one sweep of the document, not one binary search per offset. Both directions
/// are total and snap the same way as the index; a query that goes BACKWARD is
/// legal and rewinds to the start, so a caller that sorts is fast and a caller
/// that doesn't is still correct.
#[derive(Debug, Clone)]
pub struct Cursor<'s> {
    source: &'s [u8],
    /// A char boundary of `source`, and `utf16 == utf16_len(&source[..byte])`.
    byte: usize,
    utf16: u32,
}

impl<'s> Cursor<'s> {
    pub fn new(source: &'s [u8]) -> Self {
        Self {
            source,
            byte: 0,
            utf16: 0,
        }
    }

    /// Back to the start of the string — what a backward query does implicitly,
    /// and what a caller does between two independently sorted sequences.
    pub fn reset(&mut self) {
        self.byte = 0;
        self.utf16 = 0;
    }

    /// The UTF-16 offset of a source byte offset, with [`Utf16Index::to_utf16`]'s
    /// semantics (interior bytes snap down, past the end clamps).
    pub fn to_utf16(&mut self, byte: u32) -> u32 {
        let mut byte = (byte as usize).min(self.source.len());
        while byte > 0 && byte < self.source.len() && self.source[byte] & 0xC0 == 0x80 {
            byte -= 1;
        }
        if byte < self.byte {
            self.reset();
        }
        self.utf16 += utf16_len(&self.source[self.byte..byte]);
        self.byte = byte;
        self.utf16
    }

    /// The source byte offset of a UTF-16 offset, with [`Utf16Index::to_byte`]'s
    /// semantics (a surrogate interior snaps down, past the end clamps).
    pub fn to_byte(&mut self, utf16: u32) -> u32 {
        if utf16 < self.utf16 {
            self.reset();
        }
        // Word-at-a-time skip, then the byte walk — the index's own two loops,
        // started from where this cursor already stands instead of from a
        // stride boundary.
        while self.byte + 8 <= self.source.len() {
            let step = utf16_len(&self.source[self.byte..self.byte + 8]);
            if self.utf16 + step >= utf16 {
                break;
            }
            self.utf16 += step;
            self.byte += 8;
        }
        // The word skip can stop mid-character; its tail was counted whole at
        // its lead, so stepping over it costs no units and restores the
        // boundary the byte walk below reads lead bytes from.
        while self.byte < self.source.len() && self.source[self.byte] & 0xC0 == 0x80 {
            self.byte += 1;
        }
        while self.byte < self.source.len() {
            if self.utf16 == utf16 {
                return self.byte as u32;
            }
            let (width, step) = lead(self.source[self.byte]);
            if self.utf16 + step > utf16 {
                return self.byte as u32;
            }
            self.utf16 += step;
            self.byte += width;
        }
        self.source.len() as u32
    }

    /// The whole string's UTF-16 length, leaving the cursor at the end.
    pub fn len_utf16(&mut self) -> u32 {
        self.to_utf16(self.source.len() as u32)
    }
}

/// `(utf8 width, utf16 units)` for a lead byte.
#[inline]
fn lead(byte: u8) -> (usize, u32) {
    match byte {
        0x00..=0x7F => (1, 1),
        0xC0..=0xDF => (2, 1),
        0xE0..=0xEF => (3, 1),
        _ => (4, 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character boundary as `(byte, utf16)`, walked one `char` at a
    /// time — the definition the index is measured against.
    fn char_walk(source: &str) -> Vec<(u32, u32)> {
        let mut pairs = Vec::with_capacity(source.len() + 1);
        let mut utf16 = 0u32;
        for (byte, ch) in source.char_indices() {
            pairs.push((byte as u32, utf16));
            utf16 += ch.len_utf16() as u32;
        }
        pairs.push((source.len() as u32, utf16));
        pairs
    }

    /// A real verse from `hindi-IRV1/origin.usfm` — 3-byte Devanagari.
    const DEVANAGARI: &str = "\\v 1 अब्राहम की सन्तान, दाऊद की सन्तान, यीशु मसीह* की वंशावली।\n";

    fn zoo() -> Vec<String> {
        let mut zoo = vec![
            String::new(),
            "\\id GEN\n\\c 1\n\\v 1 In the beginning God created.\n".to_string(),
            "\\v 1 λόγος ἦν".to_string(),
            DEVANAGARI.to_string(),
            format!("\\p\n{DEVANAGARI}\\v 2 plain ascii tail\n"),
            // Curly quotes (3-byte), NBSP + RTL mark (2/3-byte).
            "he said \u{201c}peace\u{201d} then \u{00a0}gap\u{200f}rtl\n".to_string(),
            // Supplementary plane: emoji, a flag pair, and Gothic — 4-byte
            // UTF-8, two UTF-16 units each, the `≥ 0xF0` correction's only
            // exercise (the corpora contain none).
            "emoji \u{1f600}\u{1f4d6} flag \u{1f1ee}\u{1f1f3} gothic \u{10330}\u{10331} tail\n"
                .to_string(),
            "\u{1f600}".to_string(),
            "a".to_string(),
            "\u{0905}".to_string(),
        ];

        // A multi-byte character straddling a stride boundary, one string per
        // possible split offset within a 3-byte and a 4-byte character.
        for split in 1..4 {
            let pad = "x".repeat(STRIDE - split);
            zoo.push(format!("{pad}\u{0905}{DEVANAGARI}"));
        }
        for split in 1..5 {
            let pad = "x".repeat(STRIDE - split);
            zoo.push(format!("{pad}\u{1f600}tail\u{1f600}"));
        }
        // Straddle the SECOND boundary too, with dense text before it.
        zoo.push(format!("{}\u{1f600}rest", DEVANAGARI.repeat(20)));
        zoo
    }

    /// Both directions, at EVERY character boundary, against the walk.
    fn check_every_boundary(source: &str) {
        let pairs = char_walk(source);
        let ix = utf16_index(source.as_bytes());

        assert_eq!(ix.len_utf16(), pairs[pairs.len() - 1].1, "total");
        assert_eq!(
            ix.index_bytes(),
            (source.len() / STRIDE + 1) * 4,
            "index size"
        );

        for &(byte, utf16) in &pairs {
            assert_eq!(ix.to_utf16(byte), utf16, "to_utf16 @{byte}");
            assert_eq!(ix.to_byte(utf16), byte, "to_byte @{utf16}");
            assert_eq!(ix.to_byte(ix.to_utf16(byte)), byte, "round trip @{byte}");
        }
    }

    #[test]
    fn exact_at_every_boundary_of_the_zoo() {
        for source in zoo() {
            check_every_boundary(&source);
        }
    }

    /// The streaming cursor answers exactly what the index answers — forward
    /// (its fast path), backward (its rewind), and at every byte offset, not
    /// only the boundaries.
    #[test]
    fn the_cursor_agrees_with_the_index() {
        for source in zoo() {
            let bytes = source.as_bytes();
            let ix = utf16_index(bytes);

            let mut forward = Cursor::new(bytes);
            for at in 0..=bytes.len() as u32 {
                assert_eq!(forward.to_utf16(at), ix.to_utf16(at), "fwd to_utf16 @{at}");
            }
            let mut backward = Cursor::new(bytes);
            for at in (0..=bytes.len() as u32).rev() {
                assert_eq!(backward.to_utf16(at), ix.to_utf16(at), "rev to_utf16 @{at}");
            }

            let units = ix.len_utf16();
            let mut forward = Cursor::new(bytes);
            for at in 0..=units {
                assert_eq!(forward.to_byte(at), ix.to_byte(at), "fwd to_byte @{at}");
            }
            let mut backward = Cursor::new(bytes);
            for at in (0..=units).rev() {
                assert_eq!(backward.to_byte(at), ix.to_byte(at), "rev to_byte @{at}");
            }

            // Interleaved directions on ONE cursor: the two walks share the
            // position, so a to_byte that landed mid-character must not confuse
            // the next to_utf16.
            let mut mixed = Cursor::new(bytes);
            for at in 0..=bytes.len() as u32 {
                let units = mixed.to_utf16(at);
                assert_eq!(mixed.to_byte(units), ix.to_byte(units), "mixed @{at}");
            }
            assert_eq!(Cursor::new(bytes).len_utf16(), units);
        }
    }

    #[test]
    fn the_greek_module_doc_example() {
        let ix = utf16_index("\\v 1 λόγος ἦν".as_bytes());
        assert_eq!(ix.to_utf16(16), 11);
        assert_eq!(ix.to_byte(11), 16);
        assert_eq!(ix.len_utf16(), 13);
    }

    /// A byte inside a multi-byte character reports that character's offset.
    #[test]
    fn interior_bytes_snap_down() {
        let source = "aἦb"; // a=0, ἦ=1..4 (unit 1), b=4 (unit 2)
        let ix = utf16_index(source.as_bytes());
        assert_eq!(ix.to_utf16(1), 1);
        assert_eq!(ix.to_utf16(2), 1, "second byte of ἦ");
        assert_eq!(ix.to_utf16(3), 1, "third byte of ἦ");
        assert_eq!(ix.to_utf16(4), 2);

        // A 4-byte character: every interior byte reports the pair's HIGH half.
        let ix = utf16_index("a\u{1f600}b".as_bytes()); // emoji = units 1,2
        for interior in 1..=4 {
            assert_eq!(ix.to_utf16(interior), 1, "byte {interior} of the emoji");
        }
        assert_eq!(ix.to_utf16(5), 3);
    }

    /// A UTF-16 offset inside a surrogate pair reports the character's start.
    #[test]
    fn surrogate_interior_snaps_down() {
        let ix = utf16_index("ab\u{1f600}cd".as_bytes());
        // "ab" = units 0,1; the emoji owns units 2 and 3; "cd" starts at 4.
        assert_eq!(ix.to_byte(2), 2);
        assert_eq!(ix.to_byte(3), 2, "the pair's low half");
        assert_eq!(ix.to_byte(4), 6);
    }

    #[test]
    fn past_the_end_clamps() {
        let source = "λόγος";
        let ix = utf16_index(source.as_bytes());
        assert_eq!(ix.to_utf16(u32::MAX), 5);
        assert_eq!(ix.to_byte(u32::MAX), 10);
        // The empty string answers both ways too.
        let empty = utf16_index(b"");
        assert_eq!(empty.len_utf16(), 0);
        assert_eq!(empty.to_utf16(7), 0);
        assert_eq!(empty.to_byte(7), 0);
    }

    /// The size claim: one u32 per stride, ~1.6% of the source.
    #[test]
    fn index_is_one_u32_per_stride() {
        let source = DEVANAGARI.repeat(200);
        let ix = utf16_index(source.as_bytes());
        assert_eq!(ix.strides.len(), source.len() / STRIDE + 1);
        assert!(
            ix.index_bytes() * 50 < source.len(),
            "{} index bytes for {} of source",
            ix.index_bytes(),
            source.len()
        );
    }

    /// `utf16_len` on every prefix AND suffix — the partial-character and
    /// unaligned slices the queries actually feed it, against a byte loop.
    #[test]
    fn utf16_len_matches_a_scalar_walk() {
        fn scalar(bytes: &[u8]) -> u32 {
            let mut units = 0;
            for &b in bytes {
                if b & 0xC0 != 0x80 {
                    units += 1 + u32::from(b >= 0xF0);
                }
            }
            units
        }
        for source in zoo() {
            let bytes = source.as_bytes();
            for cut in 0..=bytes.len() {
                assert_eq!(
                    scalar(&bytes[..cut]),
                    utf16_len(&bytes[..cut]),
                    "prefix {cut}"
                );
                assert_eq!(
                    scalar(&bytes[cut..]),
                    utf16_len(&bytes[cut..]),
                    "suffix {cut}"
                );
            }
        }
    }
}
