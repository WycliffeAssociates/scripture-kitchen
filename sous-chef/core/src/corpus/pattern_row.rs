//! Encodes and decodes one pattern-table row (36 bytes). Layout:
//! codec/README.md.
//!
//! ```text
//! decode_pattern(encode_pattern(&pattern), 0) == Ok(pattern)
//! ```

use super::*;
use crate::codec::PackedFinding;
use crate::judge::{Channel, Pattern, PatternKey, Side, Staircase, Usual};
use crate::substrate::{OuterClass, RUN_BUCKETS, ScalarKey};
use crate::unicode::Pool;
use crate::words::{Form, LETTER_RUN_MAX, LETTER_RUN_MIN};

/// One pattern-table row: the glyph, the channel and its key, the band, the
/// fraction behind the claim, and what is usual instead. Layout:
/// codec/README.md.
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
        PatternKey::SentenceStart => (pattern.glyph.raw(), 0, 0),
        PatternKey::BookRate { side, class, .. } => {
            (pattern.glyph.raw(), 0, ((side as u8) << 4) | class as u8)
        }
    };
    row[PATTERN_GLYPH_OFFSET..PATTERN_NEIGHBOR_OFFSET].copy_from_slice(&glyph.to_le_bytes());
    row[PATTERN_NEIGHBOR_OFFSET..PATTERN_CHANNEL_OFFSET].copy_from_slice(&neighbor.to_le_bytes());
    row[PATTERN_CHANNEL_OFFSET] = pattern.channel as u8;
    row[PATTERN_KEY_OFFSET] = key;
    row[PATTERN_BAND_OFFSET] = pattern.band.unwrap_or(PATTERN_BAND_NONE);
    if let Usual::Rarity {
        lookalike: true, ..
    } = pattern.usual
    {
        row[PATTERN_FLAGS_OFFSET] = PATTERN_LOOKALIKE;
    }
    row[PATTERN_NUMERATOR_OFFSET..PATTERN_DENOMINATOR_OFFSET]
        .copy_from_slice(&pattern.numerator.to_le_bytes());
    row[PATTERN_DENOMINATOR_OFFSET..PATTERN_SHARE_OFFSET]
        .copy_from_slice(&pattern.denominator.to_le_bytes());
    row[PATTERN_SHARE_OFFSET..PATTERN_BOOKS_OFFSET]
        .copy_from_slice(&pattern.share_bp.to_le_bytes());
    row[PATTERN_BOOKS_OFFSET] = pattern.books;
    let (usual, usual_count, other_count) = usual_lanes(pattern);
    row[PATTERN_USUAL_OFFSET..PATTERN_USUAL_COUNT_OFFSET].copy_from_slice(&usual.to_le_bytes());
    row[PATTERN_USUAL_COUNT_OFFSET..PATTERN_OTHER_COUNT_OFFSET]
        .copy_from_slice(&usual_count.to_le_bytes());
    row[PATTERN_OTHER_COUNT_OFFSET..PATTERN_ROW_LEN].copy_from_slice(&other_count.to_le_bytes());
    row
}

/// `(usual, usual_count, other_count)`, each channel's own meaning.
fn usual_lanes(pattern: &Pattern) -> (u32, u32, u32) {
    match pattern.usual {
        Usual::None => (0, 0, 0),
        Usual::Placement { class, count } => (class as u32, count, 0),
        Usual::ExactNeighbor {
            neighbor,
            count,
            reversed,
        } => (neighbor.raw(), count, reversed),
        Usual::RunShape {
            pure,
            bucket,
            count,
        } => ((u32::from(pure) << 4) | u32::from(bucket), count, 0),
        Usual::Rarity { glyph, count, .. } => (glyph.map_or(0, ScalarKey::raw), count, 0),
        Usual::Casing { form, count } => (form as u32, count, 0),
        Usual::BookRate { baseline_bp, books } => {
            let book = match pattern.key {
                PatternKey::BookRate { book, .. } => u32::from(book.get()),
                _ => 0,
            };
            (u32::from(baseline_bp), books, book)
        }
    }
}

/// The inverse of [`usual_lanes`]: a value outside the channel's domain is
/// `Err(field)`, and so is a lane the channel does not use holding anything,
/// or the lookalike flag on any channel but `Rarity`.
fn read_usual(
    channel: Channel,
    usual: u32,
    count: u32,
    other: u32,
    lookalike: bool,
) -> Result<Usual, &'static str> {
    let unused = |lane: u32, field| if lane == 0 { Ok(()) } else { Err(field) };
    if lookalike && channel != Channel::Rarity {
        return Err("flags");
    }
    let byte = u8::try_from(usual).map_err(|_| "usual");
    Ok(match channel {
        Channel::Placement => {
            unused(other, "other_count")?;
            let class = OuterClass::from_raw(byte?).ok_or("usual")?;
            Usual::Placement { class, count }
        }
        Channel::ExactNeighbor => Usual::ExactNeighbor {
            neighbor: ScalarKey::from_raw(usual).ok_or("usual")?,
            count,
            reversed: other,
        },
        Channel::RunShape => {
            unused(other, "other_count")?;
            let byte = byte?;
            if byte >> 4 > 1 {
                return Err("usual");
            }
            Usual::RunShape {
                pure: byte >> 4 == 1,
                bucket: byte & 0x0f,
                count,
            }
        }
        Channel::Rarity => {
            unused(other, "other_count")?;
            let glyph = match usual {
                0 => None,
                raw => Some(ScalarKey::from_raw(raw).ok_or("usual")?),
            };
            Usual::Rarity {
                glyph,
                count,
                lookalike,
            }
        }
        Channel::Casing => {
            unused(other, "other_count")?;
            Usual::Casing {
                form: Form::from_raw(byte?).ok_or("usual")?,
                count,
            }
        }
        Channel::PooledNeighbor
        | Channel::WordLength
        | Channel::Doubled
        | Channel::LetterRun
        | Channel::SentenceStart => {
            unused(usual, "usual")?;
            unused(count, "usual_count")?;
            unused(other, "other_count")?;
            Usual::None
        }
        // `other` is the book, which the key already read.
        Channel::BookRate => Usual::BookRate {
            baseline_bp: u16::try_from(usual).map_err(|_| "usual")?,
            books: count,
        },
    })
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
    let flags = bytes[PATTERN_FLAGS_OFFSET];
    if flags & !PATTERN_LOOKALIKE != 0 {
        return Err(bad("flags"));
    }
    if bytes[PATTERN_RESERVED_OFFSET] != 0 {
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
        Channel::SentenceStart => {
            if raw_key != 0 {
                return Err(bad("key"));
            }
            PatternKey::SentenceStart
        }
        Channel::BookRate => {
            let book = usize::try_from(read_u32(bytes, PATTERN_OTHER_COUNT_OFFSET))
                .ok()
                .filter(|book| *book < book_count)
                .ok_or(bad("other_count"))?;
            PatternKey::BookRate {
                side: match high {
                    0 => Side::Prev,
                    1 => Side::Next,
                    _ => return Err(bad("key")),
                },
                class: OuterClass::from_raw(low).ok_or(bad("key"))?,
                book: BookIndex::new(book).expect("under the book count"),
            }
        }
    };
    if channel != Channel::ExactNeighbor && !channel.is_word() && neighbor_raw != 0 {
        return Err(bad("neighbor"));
    }
    let band = match (bytes[PATTERN_BAND_OFFSET], channel) {
        (PATTERN_BAND_NONE, Channel::Rarity | Channel::BookRate) => None,
        (step, channel)
            if !matches!(channel, Channel::Rarity | Channel::BookRate)
                && usize::from(step) < Staircase::STEPS =>
        {
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
    let usual = read_usual(
        channel,
        read_u32(bytes, PATTERN_USUAL_OFFSET),
        read_u32(bytes, PATTERN_USUAL_COUNT_OFFSET),
        read_u32(bytes, PATTERN_OTHER_COUNT_OFFSET),
        flags & PATTERN_LOOKALIKE != 0,
    )
    .map_err(bad)?;
    let pattern = Pattern {
        glyph,
        channel,
        key,
        band,
        numerator: read_u32(bytes, PATTERN_NUMERATOR_OFFSET),
        denominator: read_u32(bytes, PATTERN_DENOMINATOR_OFFSET),
        share_bp,
        books,
        usual,
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
