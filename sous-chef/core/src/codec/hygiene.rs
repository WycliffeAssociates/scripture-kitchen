//! Lanes for `RuleCode::Hygiene`: the class discriminant, then the run
//! length. The scan itself lives in `crate::hygiene`; this is only its wire
//! shape.
//!
//! ```text
//! lanes  [0, 223]              →  C0Control, a 223-code-point run
//! lanes  [1, i16::MAX] + SATURATED  →  Delete, a run longer than 32767
//! ```

use super::{CodecError, FindingFlags};

/// One deterministic hygiene class. The wire carries the discriminant in the
/// first payload lane, so values are fixed and append-only like rule codes.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HygieneClass {
    /// C0 control other than tab, LF, or the CR of a CRLF pair.
    C0Control = 0,
    Delete = 1,
    /// U+0080..=U+009F.
    C1Control = 2,
    /// U+FFFD.
    ReplacementChar = 3,
    /// CR not followed by LF.
    StrayCarriageReturn = 4,
    /// A backslash inside content, where Onion left no marker.
    StrandedBackslash = 5,
    /// A line-initial `<<<<<<< `, `=======`, or `>>>>>>> `.
    ConflictMarker = 6,
}

impl HygieneClass {
    pub const ALL: [Self; 7] = [
        Self::C0Control,
        Self::Delete,
        Self::C1Control,
        Self::ReplacementChar,
        Self::StrayCarriageReturn,
        Self::StrandedBackslash,
        Self::ConflictMarker,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::C0Control => "C0Control",
            Self::Delete => "Delete",
            Self::C1Control => "C1Control",
            Self::ReplacementChar => "ReplacementChar",
            Self::StrayCarriageReturn => "StrayCarriageReturn",
            Self::StrandedBackslash => "StrandedBackslash",
            Self::ConflictMarker => "ConflictMarker",
        }
    }
}

impl TryFrom<i16> for HygieneClass {
    type Error = CodecError;

    fn try_from(raw: i16) -> Result<Self, Self::Error> {
        usize::try_from(raw)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .ok_or(CodecError::UnknownHygieneClass(raw))
    }
}

/// The compact hygiene payload: the class and how many code points the
/// maximal run holds. `run` saturates at `i16::MAX` and sets `SATURATED`;
/// the span itself stays exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HygieneDigest {
    class: HygieneClass,
    run: u32,
}

impl HygieneDigest {
    pub fn new(class: HygieneClass, run: u32) -> Result<Self, CodecError> {
        if run == 0 {
            return Err(CodecError::EmptyHygieneRun);
        }
        Ok(Self { class, run })
    }

    pub const fn class(self) -> HygieneClass {
        self.class
    }

    /// The run length as it will be read back: exact below `i16::MAX`.
    pub const fn run(self) -> u32 {
        if self.run > i16::MAX as u32 {
            i16::MAX as u32
        } else {
            self.run
        }
    }

    pub const fn saturated(self) -> bool {
        self.run > i16::MAX as u32
    }
}

impl HygieneDigest {
    pub(super) fn lanes(self) -> [i16; 2] {
        [self.class as i16, self.run() as i16]
    }

    pub(super) fn from_lanes(lanes: [i16; 2], flags: FindingFlags) -> Result<Self, CodecError> {
        let class = HygieneClass::try_from(lanes[0])?;
        let run = u32::try_from(lanes[1]).map_err(|_| CodecError::EmptyHygieneRun)?;
        // A saturated row reads back exactly i16::MAX; anything else claiming
        // saturation is malformed.
        let saturated = flags.contains(FindingFlags::SATURATED);
        if saturated && run != i16::MAX as u32 {
            return Err(CodecError::UnknownFlags(flags.bits()));
        }
        Self::new(class, if saturated { run + 1 } else { run })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BookIndex, FindingKind, PackedFinding};

    static BOOK_LENGTHS: [u32; 1] = [u32::MAX];

    fn hygiene(from: u32, to: u32, class: HygieneClass, run: u32) -> PackedFinding {
        PackedFinding::new(
            from,
            to,
            BookIndex::new(0).unwrap(),
            FindingKind::Hygiene(HygieneDigest::new(class, run).unwrap()),
            &BOOK_LENGTHS,
        )
        .unwrap()
    }

    #[test]
    fn hygiene_golden_vector_carries_class_and_run_in_the_lanes() {
        let record = hygiene(7, 230, HygieneClass::C0Control, 223);
        assert_eq!(
            record.encode(),
            [
                0x07, 0x00, 0x00, 0x00, 0xe6, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
                0xdf, 0x00,
            ]
        );
        assert_eq!(PackedFinding::decode(&record.encode(), &[230]), Ok(record));
        let FindingKind::Hygiene(digest) = record.kind() else {
            panic!("hygiene kind")
        };
        assert_eq!(digest.class(), HygieneClass::C0Control);
        assert_eq!(digest.run(), 223);
        assert!(!digest.saturated());
    }

    #[test]
    fn hygiene_run_saturates_with_the_flag_and_round_trips_as_saturated() {
        let record = hygiene(0, 40_000, HygieneClass::Delete, 40_000);
        assert!(record.flags().contains(FindingFlags::SATURATED));
        let bytes = record.encode();
        assert_eq!(&bytes[14..16], &i16::MAX.to_le_bytes());
        let FindingKind::Hygiene(digest) = PackedFinding::decode(&bytes, &[40_000]).unwrap().kind()
        else {
            panic!("hygiene kind")
        };
        assert_eq!(digest.run(), i16::MAX as u32);
        assert!(digest.saturated());
    }

    #[test]
    fn hygiene_wire_fails_closed_on_class_run_and_flag_misuse() {
        let record = hygiene(0, 1, HygieneClass::ConflictMarker, 1);
        let mut bad_class = record.encode();
        bad_class[12] = 7;
        assert_eq!(
            PackedFinding::decode(&bad_class, &[1]),
            Err(CodecError::UnknownHygieneClass(7))
        );
        let mut zero_run = record.encode();
        zero_run[14..16].copy_from_slice(&0i16.to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&zero_run, &[1]),
            Err(CodecError::EmptyHygieneRun)
        );
        let mut negative_run = record.encode();
        negative_run[14..16].copy_from_slice(&(-5i16).to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&negative_run, &[1]),
            Err(CodecError::EmptyHygieneRun)
        );
        let mut false_saturation = record.encode();
        false_saturation[11] = 1;
        assert_eq!(
            PackedFinding::decode(&false_saturation, &[1]),
            Err(CodecError::UnknownFlags(1))
        );
        assert_eq!(
            HygieneDigest::new(HygieneClass::Delete, 0),
            Err(CodecError::EmptyHygieneRun)
        );
    }
}
