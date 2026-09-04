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

/// One pattern-table row: the glyph, the channel and its key, the band, and
/// the fraction behind the claim. Layout: codec/README.md.
pub(super) fn encode_pattern(pattern: &Pattern) -> [u8; PATTERN_ROW_LEN] {
    let mut row = [0u8; PATTERN_ROW_LEN];
    row[PATTERN_GLYPH_OFFSET..PATTERN_NEIGHBOR_OFFSET]
        .copy_from_slice(&pattern.glyph.raw().to_le_bytes());
    let (neighbor, key) = match pattern.key {
        PatternKey::ExactNeighbor(neighbor) => (neighbor.raw(), 0),
        PatternKey::PooledNeighbor(pool) => (0, pool as u8),
        PatternKey::RunShape { pure, bucket } => (0, (u8::from(pure) << 4) | bucket),
        PatternKey::Placement { side, class } => (0, ((side as u8) << 4) | class as u8),
        PatternKey::Rarity => (0, 0),
    };
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
    let glyph = ScalarKey::from_raw(read_u32(bytes, PATTERN_GLYPH_OFFSET)).ok_or(bad("glyph"))?;
    let neighbor_raw = read_u32(bytes, PATTERN_NEIGHBOR_OFFSET);
    let channel = *Channel::ALL
        .get(usize::from(bytes[PATTERN_CHANNEL_OFFSET]))
        .ok_or(bad("channel"))?;
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
    };
    if !matches!(channel, Channel::ExactNeighbor) && neighbor_raw != 0 {
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
