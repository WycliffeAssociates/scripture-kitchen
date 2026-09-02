// EXPERIMENT — NOT COMPILED. See README.md.
//
// Question: the eight-byte SWAR ASCII chunk pays for itself on Latin text.
//   Should it sit over the decoding lookup (`chars()` plus `class_of`, which
//   is what v1 did), or over the byte trie?
//
// Date: Stage 1, 2026. Retired from the library 2026-09-02.
//
// Numbers: `cargo bench -p sous-core`, Apple Silicon, one core, 8-corpus test
//   tier, medians in ns/scalar.
//
//     byte trie, no chunk    1.31 en/nya/swh · 1.51 spa · 1.65 fra · 3.23 amh · 3.46 hin · 3.89 grc
//     byte trie + SWAR       0.22 en/nya · 0.36 swh · 1.49 spa · 1.76 fra · 3.40 amh · 3.62 hin · 4.01 grc
//     decoding walk + SWAR   taxes amh/hin 24% and grc 13% against the plain lookup
//
// Verdict: rejected. Over the trie the chunk costs non-Latin only 3-7% and no
//   corpus in the tier ends up slower than the plain lookup. Over the decoding
//   walk it reproduces v1's failure mode exactly: it buys English speed with an
//   Indic and Greek tax, which is the wrong trade for this product.
//
// Winner: `sous-core/src/unicode/lookup.rs::walk_trie_swar`, and the same lane
//   inline in `sous-core/src/hygiene.rs::scan_scalars`. Row: evidence.md,
//   "roofline, Stage 1 classifier walk".

use super::{class_of, table};

/// Re-arm the SWAR lane only after this many consecutive ASCII bytes, so
/// non-Latin text pays the eight-byte test once and then stays on the scalar
/// lane instead of re-attempting it on every chunk.
const REARM_AFTER: u32 = 32;
const HIGH_BITS: u64 = 0x8080_8080_8080_8080;

/// Eight-byte ASCII lane with hysteresis over the *decoding* lookup: every
/// scalar that misses the chunk is decoded by `chars()` before it is classed.
/// That decode is the tax the byte trie removes.
pub fn walk_swar_ascii(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut acc = 0u64;
    let mut armed = true;
    let mut ascii_run = 0u32;
    while at < bytes.len() {
        if armed && at + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"));
            if word & HIGH_BITS == 0 {
                for byte in &bytes[at..at + 8] {
                    acc = acc.wrapping_add(u64::from(table::ASCII[*byte as usize]));
                }
                at += 8;
                continue;
            }
            armed = false;
            ascii_run = 0;
        }
        let c = text[at..].chars().next().expect("char boundary");
        acc = acc.wrapping_add(u64::from(class_of(c).bits()));
        let width = c.len_utf8();
        if width == 1 {
            ascii_run += 1;
            armed |= ascii_run >= REARM_AFTER;
        } else {
            ascii_run = 0;
        }
        at += width;
    }
    acc
}
