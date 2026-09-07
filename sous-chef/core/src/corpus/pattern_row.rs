//! Encodes and decodes one pattern-table row (24 bytes). Layout:
//! codec/README.md.
//!
//! ```text
//! decode_pattern(encode_pattern(&pattern), 0) == Ok(pattern)
//! ```

use super::*;
use crate::codec::PackedFinding;
use crate::judge::{Channel, Pattern, PatternKey, Side, Staircase};
use crate::substrate::{OuterClass, RUN_BUCKETS, ScalarKey};
use crate::unicode::Pool;
use crate::words::{Form, LETTER_RUN_MAX, LETTER_RUN_MIN};

/// One pattern-table row: the glyph, the channel and its key, the band, and
/// the fraction behind the claim. Layout: codec/README.md.
pub(super) fn encode_pattern(pattern: &Pattern) -> [u8; PATTERN_ROW_LEN] {
    let mut row = [0u8; PATTERN_ROW_LEN];
    // On a word channel the first eight bytes are the u64 word hash,
    // little-endian: low half where a glyph would be, high half where a
    // neighbor would be.
    let (glyph, neighbor, key) = match pattern.key {
        PatternKey::ExactNeighbor(neighbor) => (pattern.glyph.raw(), neighbor.raw(), 0),
        PatternKey::PooledNeighbor(pool) => (pattern.glyph.raw(), 0, pool as u8),
        PatternKey::RunShape { pure, bucket } => {
            (pattern.glyph.raw(), 0, (u8::from(pure) << 4) | bucket)
        }
        PatternKey::Placement { side, class } => {
            (pattern.glyph.raw(), 0, ((side as u8) << 4) | class as u8)
        }
        PatternKey::Rarity => (pattern.glyph.raw(), 0, 0),
        PatternKey::Casing { hash, form } => (hash as u32, (hash >> 32) as u32, form as u8),
        PatternKey::WordLength { hash, sigma } => (hash as u32, (hash >> 32) as u32, sigma),
        PatternKey::Doubled { hash, separated } => {
            (hash as u32, (hash >> 32) as u32, u8::from(separated))
        }
        // A real scalar in the glyph field: the letter is what was repeated.
        PatternKey::LetterRun { length } => (pattern.glyph.raw(), 0, length),
    };
    row[PATTERN_GLYPH_OFFSET..PATTERN_NEIGHBOR_OFFSET].copy_from_slice(&glyph.to_le_bytes());
    row[PATTERN_NEIGHBOR_OFFSET..PATTERN_CHANNEL_OFFSET].copy_from_slice(&neighbor.to_le_bytes());
    row[PATTERN_CHANNEL_OFFSET] = pattern.channel as u8;
    row[PATTERN_KEY_OFFSET] = key;
    row[PATTERN_BAND_OFFSET] = pattern.band.unwrap_or(PATTERN_BAND_NONE);
    row[PATTERN_NUMERATOR_OFFSET..PATTERN_DENOMINATOR_OFFSET]
        .copy_from_slice(&pattern.numerator.to_le_bytes());
    row[PATTERN_DENOMINATOR_OFFSET..PATTERN_SHARE_OFFSET]
        .copy_from_slice(&pattern.denominator.to_le_bytes());
    row[PATTERN_SHARE_OFFSET..PATTERN_BOOKS_OFFSET]
        .copy_from_slice(&pattern.share_bp.to_le_bytes());
    row[PATTERN_BOOKS_OFFSET] = pattern.books;
    row
}

/// Refuses every row the encoder cannot have written: a reserved byte set, a
/// channel or key outside its table, a band past the staircase, a share over
/// 10,000 basis points, a dispersion outside `1..=book_count`.
pub(super) fn decode_pattern(
    bytes: &[u8],
    row: usize,
    book_count: usize,
) -> Result<Pattern, CorpusWireError> {
    let bad = |field: &'static str| CorpusWireError::InvalidPattern { row, field };
    if bytes[PATTERN_FLAGS_OFFSET] != 0 {
        return Err(bad("flags"));
    }
    if bytes[PATTERN_RESERVED_OFFSET..PATTERN_ROW_LEN] != [0] {
        return Err(bad("reserved"));
    }
    let glyph_raw = read_u32(bytes, PATTERN_GLYPH_OFFSET);
    let neighbor_raw = read_u32(bytes, PATTERN_NEIGHBOR_OFFSET);
    let channel = *Channel::ALL
        .get(usize::from(bytes[PATTERN_CHANNEL_OFFSET]))
        .ok_or(bad("channel"))?;
    // The glyph field carries no scalar on a word channel, so
    // `ScalarKey::from_raw` is not applied to it there.
    let glyph = if channel.is_word() {
        ScalarKey::NONE
    } else {
        ScalarKey::from_raw(glyph_raw).ok_or(bad("glyph"))?
    };
    let word_hash = u64::from(glyph_raw) | (u64::from(neighbor_raw) << 32);
    let raw_key = bytes[PATTERN_KEY_OFFSET];
    let (high, low) = (raw_key >> 4, raw_key & 0x0f);
    let key = match channel {
        Channel::ExactNeighbor => {
            if raw_key != 0 {
                return Err(bad("key"));
            }
            PatternKey::ExactNeighbor(ScalarKey::from_raw(neighbor_raw).ok_or(bad("neighbor"))?)
        }
        Channel::RunShape => {
            if high > 1 || low == 0 || usize::from(low) > RUN_BUCKETS {
                return Err(bad("key"));
            }
            PatternKey::RunShape {
                pure: high == 1,
                bucket: low,
            }
        }
        Channel::Placement => PatternKey::Placement {
            side: match high {
                0 => Side::Prev,
                1 => Side::Next,
                _ => return Err(bad("key")),
            },
            class: OuterClass::from_raw(low).ok_or(bad("key"))?,
        },
        Channel::Rarity => {
            if raw_key != 0 {
                return Err(bad("key"));
            }
            PatternKey::Rarity
        }
        Channel::PooledNeighbor => {
            PatternKey::PooledNeighbor(Pool::from_raw(raw_key).ok_or(bad("key"))?)
        }
        Channel::Casing => {
            let form = Form::from_raw(raw_key).ok_or(bad("key"))?;
            if form == Form::Uncased {
                return Err(bad("key"));
            }
            PatternKey::Casing {
                hash: word_hash,
                form,
            }
        }
        Channel::WordLength => PatternKey::WordLength {
            hash: word_hash,
            sigma: raw_key,
        },
        Channel::Doubled => PatternKey::Doubled {
            hash: word_hash,
            separated: match raw_key {
                0 => false,
                1 => true,
                _ => return Err(bad("key")),
            },
        },
        Channel::LetterRun => {
            if !(LETTER_RUN_MIN..=LETTER_RUN_MAX).contains(&raw_key) {
                return Err(bad("key"));
            }
            PatternKey::LetterRun { length: raw_key }
        }
    };
    if channel != Channel::ExactNeighbor && !channel.is_word() && neighbor_raw != 0 {
        return Err(bad("neighbor"));
    }
    let band = match (bytes[PATTERN_BAND_OFFSET], channel) {
        (PATTERN_BAND_NONE, Channel::Rarity) => None,
        (step, channel) if channel != Channel::Rarity && usize::from(step) < Staircase::STEPS => {
            Some(step)
        }
        _ => return Err(bad("band")),
    };
    let share_bp = u16::from_le_bytes(
        bytes[PATTERN_SHARE_OFFSET..PATTERN_BOOKS_OFFSET]
            .try_into()
            .expect("two bytes"),
    );
    if share_bp > 10_000 {
        return Err(bad("share"));
    }
    let books = bytes[PATTERN_BOOKS_OFFSET];
    if usize::from(books) > book_count {
        return Err(bad("books"));
    }
    let pattern = Pattern {
        glyph,
        channel,
        key,
        band,
        numerator: read_u32(bytes, PATTERN_NUMERATOR_OFFSET),
        denominator: read_u32(bytes, PATTERN_DENOMINATOR_OFFSET),
        share_bp,
        books,
    };
    pattern.validate().map_err(bad)?;
    Ok(pattern)
}

/// A `Convention` row names a table position; past the table it is refused.
pub(super) fn validate_pattern_ref(
    finding: PackedFinding,
    pattern_count: usize,
    book: usize,
    row: usize,
) -> Result<(), CorpusWireError> {
    let crate::FindingKind::Convention(digest) = finding.kind() else {
        return Ok(());
    };
    let index = usize::from(digest.pattern().get());
    if index >= pattern_count {
        return Err(CorpusWireError::PatternIndexPastTable {
            index,
            count: pattern_count,
            at: Some((book, row)),
        });
    }
    Ok(())
}
