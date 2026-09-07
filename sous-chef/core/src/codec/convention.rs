//! Lanes for `RuleCode::Convention`: the pattern table row, then every ladder
//! rung the site matched. The judging is `crate::judge`; this is only its
//! wire shape.
//!
//! ```text
//! lanes  [3, 0b0000_1010]  →  pattern 3, matched on placement-after and the
//!                             exact neighbor
//! ```

use super::{CodecError, FindingFlags};
use crate::judge::PatternIndex;

/// Every independently sufficient reason one site fired, as a bitmask.
///
/// One run yields one finding, and the finding keeps all its reasons rather
/// than the strongest one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reasons(u16);

impl Reasons {
    pub const PLACEMENT_BEFORE: Self = Self(1 << 0);
    pub const PLACEMENT_AFTER: Self = Self(1 << 1);
    pub const RUN_SHAPE: Self = Self(1 << 2);
    pub const EXACT_NEIGHBOR: Self = Self(1 << 3);
    pub const RARITY: Self = Self(1 << 4);
    pub const POOLED_NEIGHBOR: Self = Self(1 << 5);
    pub const CASING: Self = Self(1 << 6);
    pub const WORD_LENGTH: Self = Self(1 << 7);
    const KNOWN_BITS: u16 = 0b1111_1111;
    /// Bit order on the wire, low bit first.
    pub const NAMES: [&'static str; 8] = [
        "PlacementBefore",
        "PlacementAfter",
        "RunShape",
        "ExactNeighbor",
        "Rarity",
        "PooledNeighbor",
        "Casing",
        "WordLength",
    ];

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// A convention row carries at least one reason, and no bit outside the
    /// table.
    pub fn from_bits(bits: u16) -> Result<Self, CodecError> {
        if bits == 0 {
            return Err(CodecError::EmptyReasons);
        }
        if bits & !Self::KNOWN_BITS != 0 {
            return Err(CodecError::UnknownReasons(bits));
        }
        Ok(Self(bits))
    }
}

/// Which pattern of the publication's table this site matched, and on which
/// rungs.
///
/// The row carries no evidence of its own: the table is the evidence, and a
/// site names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConventionDigest {
    pattern: PatternIndex,
    reasons: Reasons,
}

impl ConventionDigest {
    pub const fn new(pattern: PatternIndex, reasons: Reasons) -> Self {
        Self { pattern, reasons }
    }

    pub const fn pattern(self) -> PatternIndex {
        self.pattern
    }

    pub const fn reasons(self) -> Reasons {
        self.reasons
    }

    /// Never: both lanes are exact.
    pub const fn saturated(self) -> bool {
        false
    }
}

impl ConventionDigest {
    pub(super) fn lanes(self) -> [i16; 2] {
        [self.pattern.get() as i16, self.reasons.bits() as i16]
    }

    pub(super) fn from_lanes(lanes: [i16; 2], flags: FindingFlags) -> Result<Self, CodecError> {
        if flags.contains(FindingFlags::SATURATED) {
            return Err(CodecError::UnknownFlags(flags.bits()));
        }
        Ok(Self {
            pattern: PatternIndex::new(lanes[0] as u16),
            reasons: Reasons::from_bits(lanes[1] as u16)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BookIndex, FindingKind, PackedFinding};

    static BOOK_LENGTHS: [u32; 1] = [u32::MAX];

    fn convention(from: u32, to: u32, pattern: u16, reasons: Reasons) -> PackedFinding {
        PackedFinding::new(
            from,
            to,
            BookIndex::new(0).unwrap(),
            FindingKind::Convention(ConventionDigest::new(PatternIndex::new(pattern), reasons)),
            &BOOK_LENGTHS,
        )
        .unwrap()
    }

    #[test]
    fn convention_golden_vector() {
        let reasons = Reasons::PLACEMENT_AFTER.union(Reasons::EXACT_NEIGHBOR);
        let record = convention(7, 11, 0x0123, reasons);
        assert_eq!(
            record.encode(),
            [
                0x07, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x23, 0x01,
                0x0a, 0x00,
            ]
        );
        assert_eq!(PackedFinding::decode(&record.encode(), &[11]), Ok(record));
        let FindingKind::Convention(digest) = record.kind() else {
            panic!("convention kind")
        };
        assert_eq!(digest.pattern().get(), 0x0123);
        assert!(digest.reasons().contains(Reasons::EXACT_NEIGHBOR));
        assert!(!digest.reasons().contains(Reasons::RARITY));
        assert!(!digest.saturated());
    }

    /// A pattern index past 32,767 rides the lane as an unsigned `u16`.
    #[test]
    fn a_high_pattern_index_round_trips_through_the_signed_lane() {
        let record = convention(0, 0, 60_000, Reasons::RARITY);
        let FindingKind::Convention(digest) = PackedFinding::decode(&record.encode(), &[0])
            .unwrap()
            .kind()
        else {
            panic!("convention kind")
        };
        assert_eq!(digest.pattern().get(), 60_000);
        assert_eq!(digest.reasons(), Reasons::RARITY);
    }

    #[test]
    fn convention_refuses_unknown_reason_bits() {
        // Bit 7 is the last the u8 lane holds; bit 8 is the first past it.
        assert_eq!(Reasons::from_bits(1 << 7), Ok(Reasons::WORD_LENGTH));
        assert_eq!(
            Reasons::from_bits(1 << 8),
            Err(CodecError::UnknownReasons(256))
        );
        let record = convention(0, 1, 0, Reasons::RUN_SHAPE);
        let mut unknown = record.encode();
        unknown[14..16].copy_from_slice(&0x0100i16.to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&unknown, &[1]),
            Err(CodecError::UnknownReasons(0x0100))
        );
        let mut negative = record.encode();
        negative[14..16].copy_from_slice(&(-1i16).to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&negative, &[1]),
            Err(CodecError::UnknownReasons(0xffff))
        );
        let mut saturated = record.encode();
        saturated[11] = 1;
        assert_eq!(
            PackedFinding::decode(&saturated, &[1]),
            Err(CodecError::UnknownFlags(1))
        );
    }

    #[test]
    fn convention_refuses_no_reason() {
        assert_eq!(Reasons::from_bits(0), Err(CodecError::EmptyReasons));
        let record = convention(0, 1, 0, Reasons::RARITY);
        let mut empty = record.encode();
        empty[14..16].copy_from_slice(&0i16.to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&empty, &[1]),
            Err(CodecError::EmptyReasons)
        );
    }
}
