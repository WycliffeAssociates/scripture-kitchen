//! Lanes for `RuleCode::SourceCopy`: the run, then the words it ran through.
//!
//! ```text
//! lanes  [3, 14]     →  three consecutive target words, out of fourteen
//! lanes  [12, 12]    →  every word of the unit is in the paired source
//! ```

use super::{CodecError, FindingFlags};

/// How many consecutive target words the run covers, and how many eligible
/// words the paired unit held. Both saturate at `i16::MAX` and set
/// `SATURATED`; the flag says a lane clamped, not which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCopyDigest {
    run: u32,
    eligible: u32,
    /// Carried rather than derived: a decoded row reads its lanes already
    /// clamped, so only the flag still knows one of them was.
    saturated: bool,
}

impl SourceCopyDigest {
    /// A run covers at least one word and never more words than the unit
    /// offered.
    pub fn new(run: u32, eligible: u32) -> Result<Self, CodecError> {
        if run == 0 || run > eligible {
            return Err(CodecError::InvalidSourceCopyRun { run, eligible });
        }
        Ok(Self {
            run,
            eligible,
            saturated: run > i16::MAX as u32 || eligible > i16::MAX as u32,
        })
    }

    /// The run as it will be read back: exact below `i16::MAX`.
    pub const fn run(self) -> u32 {
        clamp(self.run)
    }

    /// The eligible word count as it will be read back.
    pub const fn eligible(self) -> u32 {
        clamp(self.eligible)
    }

    pub const fn saturated(self) -> bool {
        self.saturated
    }
}

const fn clamp(value: u32) -> u32 {
    if value > i16::MAX as u32 {
        i16::MAX as u32
    } else {
        value
    }
}

impl SourceCopyDigest {
    pub(super) fn lanes(self) -> [i16; 2] {
        [self.run() as i16, self.eligible() as i16]
    }

    pub(super) fn from_lanes(lanes: [i16; 2], flags: FindingFlags) -> Result<Self, CodecError> {
        let read = |raw: i16| u32::try_from(raw).ok();
        let (Some(run), Some(eligible)) = (read(lanes[0]), read(lanes[1])) else {
            return Err(CodecError::InvalidSourceCopyRun {
                run: lanes[0] as u32,
                eligible: lanes[1] as u32,
            });
        };
        // A saturated row clamped at least one lane, so one of them reads back
        // exactly i16::MAX.
        let saturated = flags.contains(FindingFlags::SATURATED);
        if saturated && run != i16::MAX as u32 && eligible != i16::MAX as u32 {
            return Err(CodecError::UnknownFlags(flags.bits()));
        }
        Self::new(run, eligible).map(|digest| Self {
            saturated,
            ..digest
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BookIndex, FindingKind, PackedFinding};

    static BOOK_LENGTHS: [u32; 1] = [u32::MAX];

    fn copy(from: u32, to: u32, run: u32, eligible: u32) -> PackedFinding {
        PackedFinding::new(
            from,
            to,
            BookIndex::new(0).unwrap(),
            FindingKind::SourceCopy(SourceCopyDigest::new(run, eligible).unwrap()),
            &BOOK_LENGTHS,
        )
        .unwrap()
    }

    #[test]
    fn source_copy_golden_vector_carries_the_run_and_the_denominator() {
        let record = copy(0x20, 0x2c, 3, 14);
        assert_eq!(
            record.encode(),
            [
                0x20, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x03, 0x00,
                0x0e, 0x00,
            ]
        );
        assert_eq!(PackedFinding::decode(&record.encode(), &[0x2c]), Ok(record));
        let FindingKind::SourceCopy(digest) = record.kind() else {
            panic!("source copy kind")
        };
        assert_eq!((digest.run(), digest.eligible()), (3, 14));
        assert!(!digest.saturated());
    }

    #[test]
    fn both_lanes_saturate_together_and_round_trip_as_saturated() {
        let record = copy(0, 8, 40_000, 40_000);
        assert!(record.flags().contains(FindingFlags::SATURATED));
        let bytes = record.encode();
        assert_eq!(&bytes[12..14], &i16::MAX.to_le_bytes());
        assert_eq!(&bytes[14..16], &i16::MAX.to_le_bytes());
        let FindingKind::SourceCopy(digest) = PackedFinding::decode(&bytes, &[8]).unwrap().kind()
        else {
            panic!("source copy kind")
        };
        assert_eq!(digest.run(), i16::MAX as u32);
        assert!(digest.saturated());
    }

    #[test]
    fn source_copy_wire_fails_closed_on_run_denominator_and_flag_misuse() {
        let record = copy(0, 4, 2, 5);
        let mut zero_run = record.encode();
        zero_run[12..14].copy_from_slice(&0i16.to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&zero_run, &[4]),
            Err(CodecError::InvalidSourceCopyRun {
                run: 0,
                eligible: 5
            })
        );
        let mut past_the_unit = record.encode();
        past_the_unit[14..16].copy_from_slice(&1i16.to_le_bytes());
        assert_eq!(
            PackedFinding::decode(&past_the_unit, &[4]),
            Err(CodecError::InvalidSourceCopyRun {
                run: 2,
                eligible: 1
            })
        );
        let mut negative = record.encode();
        negative[12..14].copy_from_slice(&(-3i16).to_le_bytes());
        assert!(matches!(
            PackedFinding::decode(&negative, &[4]),
            Err(CodecError::InvalidSourceCopyRun { .. })
        ));
        let mut false_saturation = record.encode();
        false_saturation[11] = 1;
        assert_eq!(
            PackedFinding::decode(&false_saturation, &[4]),
            Err(CodecError::UnknownFlags(1))
        );
        assert_eq!(
            SourceCopyDigest::new(0, 3),
            Err(CodecError::InvalidSourceCopyRun {
                run: 0,
                eligible: 3
            })
        );
    }
}
