//! Three shapes for the same generated bits, plus the two walk-level fast
//! lanes the roofline bench weighs against a plain `chars()` walk.
//!
//! ```text
//! class_of_with(Lookup::TwoLevel, 'é')  // BLOCK_INDEX[cp >> 6] → BLOCKS[..][cp & 63]
//! class_of_with(Lookup::FlatBmp,  'é')  // one 128 KiB lazy array, indexed by cp
//! class_of_with(Lookup::Trie,     'é')  // the same blocks, indexed by raw UTF-8
//! ```
//!
//! All three read one table, so they cannot disagree by construction; the
//! agreement test pins that anyway. What differs is the arithmetic on the
//! way in, which is what the bench is measuring.

use std::sync::OnceLock;

use super::{Class, table};

/// Which lookup `class_of` uses. Selected by the roofline bench, not by the
/// caller: the public [`super::class_of`] always takes [`DEFAULT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// Static two-level table in `.rodata`: no heap, no `OnceLock`.
    TwoLevel,
    /// 128 KiB flat BMP array built from `CLASS_RANGES` at first use.
    FlatBmp,
    /// The two-level blocks indexed straight off UTF-8 bytes, no decode.
    Trie,
}

/// The shipped lookup. See roadmap.md's evidence table for the numbers.
pub const DEFAULT: Lookup = Lookup::TwoLevel;

/// Always [`DEFAULT`], so the shipped path and the bench's named path
/// cannot drift apart.
#[inline]
pub(super) fn class_of_default(c: char) -> Class {
    class_of_with(DEFAULT, c)
}

pub fn class_of_with(lookup: Lookup, c: char) -> Class {
    match lookup {
        Lookup::TwoLevel => two_level(c),
        Lookup::FlatBmp => flat_bmp(c),
        Lookup::Trie => {
            let mut buf = [0u8; 4];
            trie_at(c.encode_utf8(&mut buf).as_bytes()).0
        }
    }
}

#[inline]
fn two_level(c: char) -> Class {
    let cp = c as u32;
    if cp < 0x80 {
        return Class::from_bits(table::ASCII[cp as usize]);
    }
    if cp < 0x1_0000 {
        let block = table::BLOCK_INDEX[(cp >> 6) as usize] as usize;
        return Class::from_bits(table::BLOCKS[block][(cp & 63) as usize]);
    }
    astral(cp)
}

/// Above the BMP the ranges are long and the hits are rare, so a search over
/// the committed runs beats any table: the test tier holds one such scalar.
fn astral(cp: u32) -> Class {
    let ranges = &table::CLASS_RANGES[table::ASTRAL_START..];
    match ranges.binary_search_by(|&(lo, hi, _)| {
        if hi < cp {
            core::cmp::Ordering::Less
        } else if lo > cp {
            core::cmp::Ordering::Greater
        } else {
            core::cmp::Ordering::Equal
        }
    }) {
        Ok(at) => Class::from_bits(ranges[at].2),
        Err(_) => Class::default(),
    }
}

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

/// The class of the scalar starting at `bytes[0]`, with its UTF-8 width,
/// read straight off the encoding. `bytes` must start on a char boundary of
/// well-formed UTF-8.
///
/// A lead byte selects the block index directly: `cp >> 6` is `lead & 0x1F`
/// for a two-byte scalar and `((lead & 0x0F) << 6) | (next & 0x3F)` for a
/// three-byte one, so no scalar is ever reassembled.
#[inline]
pub(crate) fn trie_at(bytes: &[u8]) -> (Class, usize) {
    let lead = bytes[0];
    if lead < 0x80 {
        return (Class::from_bits(table::ASCII[lead as usize]), 1);
    }
    if lead < 0xE0 {
        let block = table::BLOCK_INDEX[(lead & 0x1F) as usize] as usize;
        return (
            Class::from_bits(table::BLOCKS[block][(bytes[1] & 0x3F) as usize]),
            2,
        );
    }
    if lead < 0xF0 {
        let index = (usize::from(lead & 0x0F) << 6) | usize::from(bytes[1] & 0x3F);
        let block = table::BLOCK_INDEX[index] as usize;
        return (
            Class::from_bits(table::BLOCKS[block][(bytes[2] & 0x3F) as usize]),
            3,
        );
    }
    let cp = (u32::from(lead & 0x07) << 18)
        | (u32::from(bytes[1] & 0x3F) << 12)
        | (u32::from(bytes[2] & 0x3F) << 6)
        | u32::from(bytes[3] & 0x3F);
    (astral(cp), 4)
}

// ── Walk lanes (bench subjects; the accumulator keeps the loop alive) ────

/// Baseline: decode every scalar, then one `class_of_with` per scalar.
pub fn walk(lookup: Lookup, text: &str) -> u64 {
    text.chars().fold(0u64, |acc, c| {
        acc.wrapping_add(u64::from(class_of_with(lookup, c).bits()))
    })
}

/// The byte trie as a walk: no scalar is ever decoded.
pub fn walk_trie(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut acc = 0u64;
    while at < bytes.len() {
        let (class, width) = trie_at(&bytes[at..]);
        acc = acc.wrapping_add(u64::from(class.bits()));
        at += width;
    }
    acc
}

/// Re-arm the SWAR lane only after this many consecutive ASCII bytes, so
/// non-Latin text pays the eight-byte test once and then stays on the
/// scalar lane instead of re-attempting it on every chunk.
const REARM_AFTER: u32 = 32;
const HIGH_BITS: u64 = 0x8080_8080_8080_8080;

/// Eight-byte ASCII lane with hysteresis over the default lookup.
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
        acc = acc.wrapping_add(u64::from(class_of_default(c).bits()));
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

/// The same lane over the byte trie, which needs no decode when it drops
/// out: the ASCII chunk is the only thing the hysteresis buys here.
pub fn walk_trie_swar(text: &str) -> u64 {
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
        let (class, width) = trie_at(&bytes[at..]);
        acc = acc.wrapping_add(u64::from(class.bits()));
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
