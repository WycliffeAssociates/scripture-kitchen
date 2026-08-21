//! Byte ↔ UTF-16 offset mapping — the measurement that priced the two index
//! shapes. Strategy B WON and lives in [`crate::utf16`]; what remains here is
//! the loser plus the oracle, so `playground --utf16` can still price them
//! against each other.
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
//! SWAR remainder count cost 4.6 KiB (1.6%) for the same answers, on ANY input,
//! script-independent, which is the whole point.
//!
//! # The two strategies
//!
//! - [`Anchors`] — strategy A, the honest naive baseline: `(byte, utf16)` per
//!   non-ASCII character. `partition_point` + arithmetic, both directions.
//! - [`crate::utf16::Utf16Index`] — strategy B: cumulative UTF-16 length at
//!   every `STRIDE`-byte boundary, a flat `Vec<u32>`. Direct index (no search)
//!   for byte→utf16, binary search + a ≤`STRIDE`-byte forward scan back.
//!
//! # No dependencies
//!
//! The SWAR count is hand-rolled u64 bit tricks — no `simdutf`, no
//! `str_indices`. Those crates are the graduation path if this ever needs true
//! SIMD (AVX2/NEON) throughput; at ~10 GiB/s scalar-SWAR on a 256-byte chunk
//! the remainder count is already far below noise, so the graduation is
//! unlikely to be worth a dependency.

/// The scalar spec for [`crate::utf16::utf16_len`]'s formula — the baseline the
/// SWAR count is timed and checked against.
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
    /// ASYMMETRY WORTH KNOWING: unlike [`crate::utf16::Utf16Index::to_byte`],
    /// this cannot snap an offset that lands inside a surrogate pair — it would
    /// return a byte offset in the middle of the 4-byte character, and it does
    /// not hold the source text to notice. Giving strategy A the same guarantee
    /// means handing it the text too, at which point the shapes' cost
    /// comparison stops being apples-to-apples. Callers of A must pre-validate.
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
    use crate::utf16::{self, Utf16Index};

    /// A real verse from `hindi-IRV1/origin.usfm` — 3-byte Devanagari, the
    /// case that breaks strategy A.
    const DEVANAGARI: &str = "\\v 1 अब्राहम की सन्तान, दाऊद की सन्तान, यीशु मसीह* की वंशावली।\n";

    /// Strategy A agrees with the oracle — and therefore with strategy B,
    /// which src/utf16.rs pins boundary-by-boundary against the same walk.
    #[test]
    fn strategy_a_is_exact_at_every_boundary() {
        let sources = [
            String::new(),
            "\\id GEN\n\\c 1\n\\v 1 In the beginning God created.\n".to_string(),
            DEVANAGARI.to_string(),
            format!("\\p\n{DEVANAGARI}\\v 2 plain ascii tail\n"),
            "he said \u{201c}peace\u{201d} then \u{00a0}gap\u{200f}rtl\n".to_string(),
        ];
        for src in sources {
            let pairs = reference_pairs(&src);
            let a = Anchors::build(&src);
            let b = Utf16Index::new(src.as_bytes());
            assert_eq!(a.len_utf16(), pairs[pairs.len() - 1].1);
            for &(byte, utf16) in &pairs {
                assert_eq!(a.byte_to_utf16(byte), utf16, "A byte→utf16 @{byte}");
                assert_eq!(a.utf16_to_byte(utf16), byte, "A utf16→byte @{utf16}");
                assert_eq!(b.to_utf16(byte), utf16, "B byte→utf16 @{byte}");
            }
        }
    }

    #[test]
    fn scalar_and_swar_agree_on_every_slice() {
        let src = format!("{DEVANAGARI}\u{1f600}ascii tail \u{201c}q\u{201d}");
        let bytes = src.as_bytes();
        for cut in 0..=bytes.len() {
            assert_eq!(
                utf16_len_scalar(&bytes[..cut]),
                utf16::utf16_len(&bytes[..cut])
            );
            assert_eq!(
                utf16_len_scalar(&bytes[cut..]),
                utf16::utf16_len(&bytes[cut..])
            );
        }
    }

    /// The finding this experiment exists for: A costs more than the source it
    /// indexes on dense script, B stays under 5%.
    #[test]
    fn strategy_a_costs_more_than_the_source_on_dense_script() {
        let src = DEVANAGARI.repeat(200);
        let a = Anchors::build(&src);
        let b = Utf16Index::new(src.as_bytes());
        assert!(
            a.index_bytes() > src.len(),
            "the whole point: A is {} bytes for {} of source",
            a.index_bytes(),
            src.len()
        );
        assert!(b.index_bytes() * 20 < src.len(), "B stays under 5%");
    }
}
