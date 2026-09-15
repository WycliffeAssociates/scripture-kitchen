//! One pool of generated bits, two index paths into it.
//!
//! ```text
//! class_of('é')             // BLOCK_INDEX[cp >> 6] → BLOCKS[..][cp & 63]
//! trie_at("é".as_bytes())   // the same blocks, indexed by raw UTF-8
//! ```
//!
//! Both read `table::BLOCKS`, so they cannot disagree by construction; the
//! agreement test pins it anyway. The walk helpers are bench subjects.
//! Index arithmetic and the rejected shapes:
//! `sous-chef/core/src/unicode/README.md`.

use super::{Class, table};

/// The class of one scalar, through `BLOCK_INDEX` into `BLOCKS`.
#[inline]
pub(super) fn class_of(c: char) -> Class {
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

/// Above the BMP the ranges are long and hits are rare, so a search over the
/// committed runs beats any table.
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

/// The class of one ASCII byte, for a caller that already proved the byte is
/// ASCII — a SWAR chunk test, say.
#[inline]
pub fn ascii_class(byte: u8) -> Class {
    debug_assert!(byte < 0x80, "the caller proves the byte is ASCII");
    Class::from_bits(table::ASCII[usize::from(byte & 0x7f)])
}

/// The class and UTF-8 width of the scalar at `bytes[0]`, read off the
/// encoding without reassembling a scalar. `bytes` must start on a char
/// boundary of well-formed UTF-8.
#[inline]
pub fn trie_at(bytes: &[u8]) -> (Class, usize) {
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

/// Baseline: decode every scalar, then one [`class_of`] per scalar.
pub fn walk(text: &str) -> u64 {
    text.chars().fold(0u64, |acc, c| {
        acc.wrapping_add(u64::from(class_of(c).bits()))
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

/// Consecutive ASCII bytes before the SWAR lane re-arms. Without hysteresis
/// non-Latin text re-attempts the eight-byte test on every chunk.
const REARM_AFTER: u32 = 32;
const HIGH_BITS: u64 = 0x8080_8080_8080_8080;

/// The shipped lane: an eight-byte ASCII chunk over the byte trie, which
/// needs no decode when the chunk drops out.
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
