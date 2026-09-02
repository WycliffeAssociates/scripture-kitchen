// EXPERIMENT — NOT COMPILED. See README.md.
//
// Question: should the BMP lookup be a lazy 128 KiB flat array indexed by
//   code point (v1's shape), or the static two-level table in `.rodata`?
//
// Date: Stage 1, 2026. Retired from the library 2026-09-02.
//
// Numbers: `cargo bench -p sous-core`, Apple Silicon, one core, 8-corpus test
//   tier, medians in ns/scalar.
//
//     static two-level  1.64 en/nya/swh · 1.83 spa · 2.00 fra · 3.66 amh · 3.90 hin · 4.17 grc
//     lazy flat BMP     1.97            · 2.14     · 2.26     · 3.36     · 3.57     · 3.79
//
//   Flat BMP is 8-10% faster on non-Latin and 17% slower on Latin.
//
// Verdict: rejected. Speed is a near tie, so size decides: the two-level table
//   is 28.7 KiB of `.rodata` — no heap, no `OnceLock`, nothing extra in the
//   `.wasm` — against 128 KiB built at first use, on a per-call branch every
//   lookup pays. The flat array also cannot be shared with the byte trie,
//   which reads the same 64-scalar blocks straight off UTF-8.
//
// Winner: `sous-core/src/unicode/lookup.rs` — `class_of` over
//   `table::BLOCK_INDEX` / `table::BLOCKS`, with `trie_at` indexing the same
//   pool. Row: evidence.md, "roofline, Stage 1 classifier walk".

use std::sync::OnceLock;

use super::{Class, table};

/// 128 KiB flat BMP array built from `CLASS_RANGES` at first use. Astral
/// scalars fall through to the shared range search.
fn flat_bmp(c: char) -> Class {
    static BMP: OnceLock<Box<[u16]>> = OnceLock::new();
    let cp = c as u32;
    if cp >= 0x1_0000 {
        return astral(cp);
    }
    let table = BMP.get_or_init(|| {
        let mut flat = vec![0u16; 0x1_0000].into_boxed_slice();
        for &(lo, hi, bits) in table::CLASS_RANGES {
            if lo >= 0x1_0000 {
                break;
            }
            for cp in lo..=hi.min(0xFFFF) {
                flat[cp as usize] = bits;
            }
        }
        flat
    });
    Class::from_bits(table[cp as usize])
}

// The bench drove this through a `Lookup { TwoLevel, FlatBmp, Trie }` enum and
// a `class_of_with(lookup, c)` dispatcher, both retired with the candidate:
//
//     #[divan::bench(args = FILES)]
//     fn class_flat_bmp(bencher: Bencher, name: &str) {
//         classified(bencher, name, |text| walk(Lookup::FlatBmp, text));
//     }
