//! Lanes for `RuleCode::Presence`: the kind, then the keys the row covers.
//!
//! ```text
//! lanes  [0, 30]              →  Missing, thirty consecutive source keys
//! lanes  [1, 2]               →  Extra, two consecutive target keys
//! lanes  [2, 1] + span        →  Empty, one paired target verse with no content
//! ```

use super::{CodecError, FindingFlags};

/// Which side holds the verse. The discriminant rides the first payload lane,
/// so values are fixed and append-only like rule codes.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresenceKind {
    /// A source verse key with no target unit in that book.
    Missing = 0,
    /// A target verse key with no source unit in that book.
    Extra = 1,
    /// A paired unit whose target side has no graphemes and whose source has
    /// some.
    Empty = 2,
}

impl PresenceKind {
    pub const ALL: [Self; 3] = [Self::Missing, Self::Extra, Self::Empty];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Missing => "Missing",
            Self::Extra => "Extra",
            Self::Empty => "Empty",
        }
    }
}

impl TryFrom<i16> for PresenceKind {
    type Error = CodecError;

    fn try_from(raw: i16) -> Result<Self, Self::Error> {
        usize::try_from(raw)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .ok_or(CodecError::UnknownPresenceKind(raw))
    }
}

/// The kind and the consecutive verse keys one coalesced row covers. `keys`
/// saturates at `i16::MAX` and sets `SATURATED`.
///
/// The row says how many keys, never which: a consumer reads those from the
/// gap in its own table of contents around the published span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresenceDigest {
    kind: PresenceKind,
    keys: u32,
}

impl PresenceDigest {
    pub fn new(kind: PresenceKind, keys: u32) -> Result<Self, CodecError> {
        if keys == 0 {
            return Err(CodecError::EmptyPresenceRun);
        }
        Ok(Self { kind, keys })
    }

    pub const fn kind(self) -> PresenceKind {
        self.kind
    }

    /// The key count as it will be read back: exact below `i16::MAX`.
    pub const fn keys(self) -> u32 {
        if self.keys > i16::MAX as u32 {
            i16::MAX as u32
        } else {
            self.keys
        }
    }

    pub const fn saturated(self) -> bool {
        self.keys > i16::MAX as u32
    }
}

impl PresenceDigest {
    pub(super) fn lanes(self) -> [i16; 2] {
        [self.kind as i16, self.keys() as i16]
    }

    pub(super) fn from_lanes(lanes: [i16; 2], flags: FindingFlags) -> Result<Self, CodecError> {
        let kind = PresenceKind::try_from(lanes[0])?;
        let keys = u32::try_from(lanes[1]).map_err(|_| CodecError::EmptyPresenceRun)?;
        // A saturated row reads back exactly i16::MAX.
        let saturated = flags.contains(FindingFlags::SATURATED);
        if saturated && keys != i16::MAX as u32 {
            return Err(CodecError::UnknownFlags(flags.bits()));
        }
        Self::new(kind, if saturated { keys + 1 } else { keys })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BookIndex, FindingKind, PackedFinding};

    static BOOK_LENGTHS: [u32; 1] = [u32::MAX];

    fn presence(from: u32, to: u32, kind: PresenceKind, keys: u32) -> PackedFinding {
        PackedFinding::new(
            from,
            to,
            BookIndex::new(0).unwrap(),
            FindingKind::Presence(PresenceDigest::new(kind, keys).unwrap()),
            &BOOK_LENGTHS,
        )
        .unwrap()
    }

    #[test]
    fn presence_golden_vector_carries_kind_and_key_count_in_the_lanes() {
        let record = presence(0x40, 0x40, PresenceKind::Missing, 30);
        assert_eq!(
            record.encode(),
            [
                0x40, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x1e, 0x00,
            ]
        );
        assert_eq!(PackedFinding::decode(&record.encode(), &[0x40]), Ok(record));
        let FindingKind::Presence(digest) = record.kind() else {
            panic!("presence kind")
        };
        assert_eq!(digest.kind(), PresenceKind::Missing);
        assert_eq!(digest.keys(), 30);
        assert!(!digest.saturated());
    }

    #[test]
    fn presence_key_count_saturates_with_the_flag_and_round_trips_as_saturated() {
        let record = presence(0, 8, PresenceKind::Extra, 40_000);
        assert!(record.flags().contains(FindingFlags::SATURATED));
        let bytes = record.encode();
        assert_eq!(&bytes[14..16], &i16::MAX.to_le_bytes());
        let FindingKind::Presence(digest) = PackedFinding::decode(&bytes, &[8]).unwrap().kind()
        else {
            panic!("presence kind")
        };
        assert_eq!(digest.keys(), i16::MAX as u32);
        assert!(digest.saturated());
    }

    #[test]
    fn presence_wire_fails_closed_on_kind_count_and_flag_misuse() {
        let record = presence(0, 1, PresenceKind::Empty, 1);
        let mut bad_kind = record.encode();
        bad_kind[12] = 3;
        assert_eq!(
            PackedFinding::decode(&bad_kind, &[1]),
            Err(CodecError::UnknownPresenceKind(3))
        );
        let mut zero_keys = record.encode();
        zero_keys[14..16].copy_from_slice(&0i16.to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&zero_keys, &[1]),
            Err(CodecError::EmptyPresenceRun)
        );
        let mut negative_keys = record.encode();
        negative_keys[14..16].copy_from_slice(&(-5i16).to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&negative_keys, &[1]),
            Err(CodecError::EmptyPresenceRun)
        );
        let mut false_saturation = record.encode();
        false_saturation[11] = 1;
        assert_eq!(
            PackedFinding::decode(&false_saturation, &[1]),
            Err(CodecError::UnknownFlags(1))
        );
        assert_eq!(
            PresenceDigest::new(PresenceKind::Missing, 0),
            Err(CodecError::EmptyPresenceRun)
        );
    }
}
