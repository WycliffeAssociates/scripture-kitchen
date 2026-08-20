//! Byte ↔ UTF-16 offset mapping — pricing the two index shapes.
//!
//! # The problem
//!
//! The editor session speaks UTF-16 (CodeMirror hands out and expects UTF-16
//! code-unit offsets); the engine speaks bytes. Something has to translate,
//! and settled-facts rules that it is a *boundary index* — never an eager
//! per-character conversion, never inside the core.
//!
//! The obvious boundary index (rust-analyzer's `line-index` shape) records an
//! anchor at every point where the byte↔UTF-16 *drift* changes — i.e. at every
//! non-ASCII character. That is excellent for ASCII-dominant English and
//! PATHOLOGICAL for a dense script. Measured on
//! `testData/samples-from-wild/hindi-IRV1/origin.usfm` (292 KiB, ~88%
//! non-ASCII Devanagari, every non-ASCII char 3 bytes / 1 unit): drift anchors
//! cost 688 KiB — **2.4× the source it indexes**. Fixed-stride anchors plus a
//! SWAR remainder count cost 4.6 KiB (1.6%) for the same answers — the premise
//! for this experiment said ~9 KiB, which is the stride-128 figure; at the
//! [`STRIDE`] chosen here it is half that. Cost is `n/64` bytes on ANY input,
//! script-independent, which is the whole point.
//!
//! This module builds both, proves them equal to a naive `char_indices()`
//! oracle, and lets `playground --utf16` measure them. It is an EXPERIMENT:
//! whether production adopts the stride shape is a separate ruled decision and
//! `planning/settled-facts.md` owns the ruling.
//!
//! # The two strategies
//!
//! - [`Anchors`] — strategy A, the honest naive baseline: `(byte, utf16)` per
//!   non-ASCII character. `partition_point` + arithmetic, both directions.
//! - [`Stride`] — strategy B: cumulative UTF-16 length at every [`STRIDE`]-byte
//!   boundary, a flat `Vec<u32>`. Direct index (no search) for byte→utf16,
//!   binary search + a ≤`STRIDE`-byte forward scan for utf16→byte.
//!
//! # The stride-boundary char-split rule (defined ONCE, both directions obey)
//!
//! A stride boundary can fall in the middle of a multi-byte character. The
//! rule: **a character contributes all of its UTF-16 units at its FIRST byte.**
//! So `anchors[k] = utf16_len(&src[0 .. k*STRIDE])`, and a character straddling
//! that boundary is counted *entirely* in `anchors[k]` if its lead byte is
//! below the boundary, and not at all otherwise.
//!
//! The pleasant part: [`utf16_len`]'s formula (`len − continuations + count(≥
//! 0xF0)`) implements that rule for free — truncate a 3-byte char after 1 byte
//! and it scores `1 − 0 = 1`; after 2 bytes, `2 − 1 = 1`. Which means the
//! remainder scan `utf16_len(&src[k*STRIDE .. b])` scores the straddling
//! character's leftover continuation bytes as **0**, exactly compensating for
//! the anchor having already counted it. The two halves compose with no
//! special case. See [`Stride::byte_to_utf16`].
//!
//! # No dependencies
//!
//! The SWAR count is hand-rolled u64 bit tricks — no `simdutf`, no
//! `str_indices`. Those crates are the graduation path if this ever needs true
//! SIMD (AVX2/NEON) throughput; at ~10 GiB/s scalar-SWAR on a 256-byte chunk
//! the remainder count is already far below noise, so the graduation is
//! unlikely to be worth a dependency.

/// Bytes between fixed-stride anchors. 256 costs `n/64` index bytes (1.6% of
/// source) and bounds the remainder scan at 256 bytes (~4 SWAR words + tail).
/// 1024 is the obvious alternative: 0.4% of source, 4× the scan. Both are
/// noise next to strategy A; 256 keeps the utf16→byte forward scan short,
/// which is the only direction with a real loop in it.
pub const STRIDE: usize = 256;

// ---------------------------------------------------------------------------
// The count
// ---------------------------------------------------------------------------

/// UTF-16 code units for `bytes`, by the "count at the lead byte" rule.
///
/// `len − continuation_bytes + bytes ≥ 0xF0`:
/// - 1-byte char: `1 − 0 = 1`
/// - 2-byte char: `2 − 1 = 1`
/// - 3-byte char: `3 − 2 = 1`
/// - 4-byte char: `4 − 3 + 1 = 2` (a surrogate pair)
///
/// `bytes` need NOT be a whole number of characters. A truncated character
/// scores its full unit count as soon as its lead byte is included, and 0 for
/// each continuation byte after that — which is precisely the stride-boundary
/// rule in the module doc.
///
/// This is the scalar spec. [`utf16_len_swar`] is the fast twin; the tests
/// assert they agree byte-for-byte on the zoo and both corpora.
pub fn utf16_len_scalar(bytes: &[u8]) -> u32 {
    let mut units = 0u32;
    for &b in bytes {
        if b & 0xC0 != 0x80 {
            units += 1;
            if b >= 0xF0 {
                units += 1;
            }
        }
    }
    units
}

const HIGH: u64 = 0x8080_8080_8080_8080;

/// Same contract as [`utf16_len_scalar`], a u64 at a time.
///
/// Per 8 bytes, two population counts:
/// - continuation `(b & 0xC0) == 0x80` → bit7 set AND bit6 clear →
///   `w & !(w << 1) & HIGH` (the shift lifts each byte's bit6 into bit7;
///   the cross-byte bit that shifts into the next byte's bit0 is masked off).
/// - `b >= 0xF0` → bits 7,6,5,4 all set → `w & (w<<1) & (w<<2) & (w<<3) & HIGH`.
///
/// Endian-agnostic: every operation is per-byte, and every shift's cross-byte
/// spill lands in bits `HIGH` discards.
pub fn utf16_len_swar(bytes: &[u8]) -> u32 {
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

/// The one the indexes call. Alias so the strategies read as intent, not as a
/// choice of implementation.
#[inline]
pub fn utf16_len(bytes: &[u8]) -> u32 {
    utf16_len_swar(bytes)
}

// ---------------------------------------------------------------------------
// Strategy A: drift anchors
// ---------------------------------------------------------------------------

/// One `(byte, utf16)` pair per non-ASCII character — the naive boundary
/// index, kept honest (no run coalescing) so its cost is the real cost of the
/// shape.
///
/// Each entry records the offsets **just past** a non-ASCII character. Between
/// consecutive entries every character is ASCII, so byte and UTF-16 offsets
/// advance 1:1 and the mapping inside a gap is pure addition.
pub struct Anchors {
    /// Sorted, strictly increasing in BOTH columns.
    points: Vec<(u32, u32)>,
    len_bytes: u32,
    len_utf16: u32,
}

impl Anchors {
    pub fn build(src: &str) -> Self {
        let mut points = Vec::new();
        let mut utf16 = 0u32;
        for (byte, ch) in src.char_indices() {
            let units = ch.len_utf16() as u32;
            utf16 += units;
            if !ch.is_ascii() {
                points.push(((byte + ch.len_utf8()) as u32, utf16));
            }
        }
        Self {
            points,
            len_bytes: src.len() as u32,
            len_utf16: utf16,
        }
    }

    /// Heap bytes the index occupies.
    pub fn index_bytes(&self) -> usize {
        self.points.len() * std::mem::size_of::<(u32, u32)>()
    }

    pub fn len_utf16(&self) -> u32 {
        self.len_utf16
    }

    /// `byte` must be a character boundary; offsets past the end clamp.
    ///
    /// Find the last anchor at or before `byte`. Everything from that anchor
    /// forward is ASCII, so the remainder is 1:1.
    pub fn byte_to_utf16(&self, byte: u32) -> u32 {
        let byte = byte.min(self.len_bytes);
        let i = self.points.partition_point(|&(b, _)| b <= byte);
        match i.checked_sub(1) {
            Some(prev) => {
                let (ab, au) = self.points[prev];
                au + (byte - ab)
            }
            // Before the first non-ASCII char: pure ASCII prefix, 1:1.
            None => byte,
        }
    }

    /// `utf16` must be a character boundary; offsets past the end clamp.
    ///
    /// ASYMMETRY WORTH KNOWING: unlike [`Stride::utf16_to_byte`], this cannot
    /// clamp an offset that lands inside a surrogate pair — it would return a
    /// byte offset in the middle of the 4-byte character, and it does not hold
    /// the source text to notice. Giving strategy A the same clamp guarantee
    /// means handing it `&str` too, at which point the shapes' cost comparison
    /// stops being apples-to-apples. Callers of A must pre-validate.
    pub fn utf16_to_byte(&self, utf16: u32) -> u32 {
        let utf16 = utf16.min(self.len_utf16);
        let i = self.points.partition_point(|&(_, u)| u <= utf16);
        match i.checked_sub(1) {
            Some(prev) => {
                let (ab, au) = self.points[prev];
                ab + (utf16 - au)
            }
            None => utf16,
        }
    }
}

// ---------------------------------------------------------------------------
// Strategy B: fixed-stride anchors
// ---------------------------------------------------------------------------

/// Cumulative UTF-16 length at every [`STRIDE`]-byte boundary.
///
/// `anchors[k] == utf16_len(&src[0 .. k*STRIDE])` under the lead-byte rule
/// (module doc). `anchors` is nondecreasing in `k`, which is what makes the
/// reverse direction a binary search.
pub struct Stride<'a> {
    src: &'a [u8],
    anchors: Vec<u32>,
}

impl<'a> Stride<'a> {
    /// One pass: SWAR-count each `STRIDE`-byte block, accumulating.
    pub fn build(src: &'a str) -> Self {
        let src = src.as_bytes();
        let mut anchors = Vec::with_capacity(src.len() / STRIDE + 1);
        let mut acc = 0u32;
        anchors.push(0);
        let mut at = 0usize;
        while at + STRIDE <= src.len() {
            acc += utf16_len(&src[at..at + STRIDE]);
            anchors.push(acc);
            at += STRIDE;
        }
        // The final partial block gets NO anchor — `byte_to_utf16` indexes
        // `byte / STRIDE`, which for any byte in that tail is the last anchor
        // pushed above, so the tail is covered by the remainder scan.
        Self { src, anchors }
    }

    pub fn index_bytes(&self) -> usize {
        self.anchors.len() * std::mem::size_of::<u32>()
    }

    pub fn len_utf16(&self) -> u32 {
        let last = self.anchors.len() - 1;
        self.anchors[last] + utf16_len(&self.src[last * STRIDE..])
    }

    /// `byte` must be a character boundary; offsets past the end clamp.
    ///
    /// No search: `anchors[byte / STRIDE]` plus a SWAR count of the ≤`STRIDE`
    /// remainder. If the chunk start splits a character, that character was
    /// already counted whole in the anchor and its leading continuation bytes
    /// score 0 in the remainder — the two halves compose exactly.
    pub fn byte_to_utf16(&self, byte: u32) -> u32 {
        let byte = (byte as usize).min(self.src.len());
        let k = byte / STRIDE;
        self.anchors[k] + utf16_len(&self.src[k * STRIDE..byte])
    }

    /// `utf16` → the byte offset of the character boundary it names.
    ///
    /// SURROGATE RULE: a `utf16` offset landing *between* the two halves of a
    /// 4-byte character's surrogate pair is not a character boundary. It clamps
    /// **down** to the start of that character (the low half is unreachable,
    /// same as CodeMirror's own behaviour when a position is corrected onto a
    /// grapheme-safe boundary). Offsets past the end clamp to `src.len()`.
    pub fn utf16_to_byte(&self, utf16: u32) -> u32 {
        // Last anchor at or before the target. Every stride block holds at
        // least STRIDE/4 characters, so `anchors` is strictly increasing and
        // `partition_point` names exactly one k. `anchors[0] == 0`, so the
        // prefix is never empty and the `- 1` cannot underflow.
        let k = self.anchors.partition_point(|&u| u <= utf16) - 1;
        let mut at = k * STRIDE;
        let mut acc = self.anchors[k];

        // Word-at-a-time skip. The lead-byte rule makes `acc == utf16_len(&src
        // [0..at])` an invariant that holds at EVERY byte position, aligned or
        // not — so we can leap 8 bytes whenever the whole word lands strictly
        // before the target. Stop at `>=`, not `>`: equality means the boundary
        // is at-or-just-past `at + 8` and only the byte walk can say which.
        // Without this the scan is ~85 steps per query on 3-byte script; with
        // it, ~4 word ops plus a ≤8-byte tail.
        while at + 8 <= self.src.len() {
            let units = utf16_len(&self.src[at..at + 8]);
            if acc + units >= utf16 {
                break;
            }
            acc += units;
            at += 8;
        }

        // The first REAL boundary at or after `at`: skip the tail of a
        // character straddling it (already counted whole, so `acc` is already
        // that boundary's UTF-16 offset).
        while at < self.src.len() && self.src[at] & 0xC0 == 0x80 {
            at += 1;
        }

        while at < self.src.len() {
            if acc == utf16 {
                return at as u32;
            }
            let lead = self.src[at];
            let (width, units) = decode_lead(lead);
            // Overshoot means `utf16` points inside this character's surrogate
            // pair — clamp down to the character's start.
            if acc + units > utf16 {
                return at as u32;
            }
            acc += units;
            at += width;
        }
        self.src.len() as u32
    }
}

/// `(utf8 width, utf16 units)` for a lead byte.
#[inline]
fn decode_lead(lead: u8) -> (usize, u32) {
    match lead {
        0x00..=0x7F => (1, 1),
        0xC0..=0xDF => (2, 1),
        0xE0..=0xEF => (3, 1),
        _ => (4, 2),
    }
}

// ---------------------------------------------------------------------------
// The oracle
// ---------------------------------------------------------------------------

/// Every character boundary as `(byte, utf16)`, from `str::char_indices()` —
/// the definition both strategies are measured against. Includes `(0, 0)` and
/// the end pair.
pub fn reference_pairs(src: &str) -> Vec<(u32, u32)> {
    let mut pairs = Vec::with_capacity(src.len() + 1);
    let mut utf16 = 0u32;
    for (byte, ch) in src.char_indices() {
        pairs.push((byte as u32, utf16));
        utf16 += ch.len_utf16() as u32;
    }
    pairs.push((src.len() as u32, utf16));
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real verse from `hindi-IRV1/origin.usfm` — 3-byte Devanagari, the
    /// case that breaks strategy A.
    const DEVANAGARI: &str = "\\v 1 अब्राहम की सन्तान, दाऊद की सन्तान, यीशु मसीह* की वंशावली।\n";

    fn zoo() -> Vec<String> {
        let mut zoo = vec![
            String::new(),
            "\\id GEN\n\\c 1\n\\v 1 In the beginning God created.\n".to_string(),
            DEVANAGARI.to_string(),
            format!("\\p\n{DEVANAGARI}\\v 2 plain ascii tail\n"),
            // Curly quotes (3-byte), NBSP + RTL mark (2/3-byte), emoji (4-byte).
            "he said \u{201c}peace\u{201d} then \u{00a0}gap\u{200f}rtl\n".to_string(),
            "emoji \u{1f600}\u{1f4d6} mixed \u{1f1ee}\u{1f1f3} tail\n".to_string(),
            "\u{1f600}".to_string(),
            "a".to_string(),
            "\u{0905}".to_string(),
        ];

        // Constructed so a multi-byte char straddles a STRIDE boundary — one
        // string per possible split offset within a 3-byte and a 4-byte char.
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

    /// Both strategies, at EVERY valid boundary offset, both directions,
    /// against the oracle — plus round-trips.
    fn check_exhaustive(src: &str) {
        let pairs = reference_pairs(src);
        let a = Anchors::build(src);
        let b = Stride::build(src);

        let total_utf16 = pairs[pairs.len() - 1].1;
        assert_eq!(a.len_utf16(), total_utf16, "A total");
        assert_eq!(b.len_utf16(), total_utf16, "B total");

        for &(byte, utf16) in &pairs {
            assert_eq!(a.byte_to_utf16(byte), utf16, "A byte→utf16 @{byte}");
            assert_eq!(b.byte_to_utf16(byte), utf16, "B byte→utf16 @{byte}");
            assert_eq!(a.utf16_to_byte(utf16), byte, "A utf16→byte @{utf16}");
            assert_eq!(b.utf16_to_byte(utf16), byte, "B utf16→byte @{utf16}");
            // Round-trip both ways.
            assert_eq!(a.utf16_to_byte(a.byte_to_utf16(byte)), byte);
            assert_eq!(b.utf16_to_byte(b.byte_to_utf16(byte)), byte);
        }
    }

    #[test]
    fn zoo_is_exact_at_every_boundary() {
        for src in zoo() {
            check_exhaustive(&src);
        }
    }

    #[test]
    fn swar_agrees_with_scalar_on_the_zoo() {
        for src in zoo() {
            let bytes = src.as_bytes();
            // Every prefix AND every suffix — the partial-char and unaligned
            // cases the strategies actually feed it.
            for cut in 0..=bytes.len() {
                assert_eq!(
                    utf16_len_scalar(&bytes[..cut]),
                    utf16_len_swar(&bytes[..cut]),
                    "prefix {cut} of {src:?}"
                );
                assert_eq!(
                    utf16_len_scalar(&bytes[cut..]),
                    utf16_len_swar(&bytes[cut..]),
                    "suffix {cut} of {src:?}"
                );
            }
        }
    }

    /// The documented clamp: a UTF-16 offset inside a surrogate pair resolves
    /// to the START of the 4-byte character. Strategy A cannot be asked (its
    /// anchors never name such an offset), so this pins strategy B.
    #[test]
    fn surrogate_interior_clamps_to_char_start() {
        let src = "ab\u{1f600}cd";
        let b = Stride::build(src);
        // "ab" = units 0,1; the emoji occupies units 2 and 3; "cd" starts at 4.
        assert_eq!(b.utf16_to_byte(2), 2);
        assert_eq!(b.utf16_to_byte(3), 2, "interior of the pair clamps down");
        assert_eq!(b.utf16_to_byte(4), 6);
    }

    #[test]
    fn strategy_a_costs_more_than_the_source_on_dense_script() {
        let src = DEVANAGARI.repeat(200);
        let a = Anchors::build(&src);
        let b = Stride::build(&src);
        assert!(
            a.index_bytes() > src.len(),
            "the whole point: A is {} bytes for {} of source",
            a.index_bytes(),
            src.len()
        );
        assert!(b.index_bytes() * 20 < src.len(), "B stays under 5%");
    }

    // -- corpus checks: skip loudly when the data is not mounted -------------

    fn corpus_files() -> Vec<std::path::PathBuf> {
        [
            "testData/samples-from-wild/hindi-IRV1/origin.usfm",
            "example-corpora/en_ulb/19-PSA.usfm",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .collect()
    }

    #[test]
    fn corpus_every_thousandth_offset_matches_the_oracle() {
        let files = corpus_files();
        if files.is_empty() {
            eprintln!("skipping: neither testData nor example-corpora is mounted");
            return;
        }
        for path in files {
            let src = std::fs::read_to_string(&path).expect("readable");
            let pairs = reference_pairs(&src);
            let a = Anchors::build(&src);
            let b = Stride::build(&src);

            assert_eq!(
                utf16_len_scalar(src.as_bytes()),
                utf16_len_swar(src.as_bytes())
            );
            assert_eq!(a.len_utf16(), pairs[pairs.len() - 1].1);
            assert_eq!(b.len_utf16(), pairs[pairs.len() - 1].1);

            // Every 1000th BYTE offset, snapped down to a char boundary (which
            // is what `pairs` gives when we search it), both ways.
            let mut probe = 0usize;
            while probe <= src.len() {
                let i = pairs.partition_point(|&(byte, _)| (byte as usize) <= probe) - 1;
                let (byte, utf16) = pairs[i];
                assert_eq!(a.byte_to_utf16(byte), utf16, "{path:?} A fwd @{byte}");
                assert_eq!(b.byte_to_utf16(byte), utf16, "{path:?} B fwd @{byte}");
                assert_eq!(a.utf16_to_byte(utf16), byte, "{path:?} A rev @{utf16}");
                assert_eq!(b.utf16_to_byte(utf16), byte, "{path:?} B rev @{utf16}");
                probe += 1000;
            }
        }
    }
}
