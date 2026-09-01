//! Lanes for `RuleCode::LengthProportionality`: two signed Q8.8 standardized
//! deviations, book scope then project scope.
//!
//! ```text
//! lanes  [0x0180, i16::MIN]   →  book +1.5, project unavailable
//! flags  SATURATED            →  a lane was clamped; the analysis value is larger
//! ```

use super::{CodecError, FindingFlags};

/// A signed Q8.8 standardized deviation.
///
/// The wire value `i16::MIN` is reserved for an unavailable scope and is
/// represented by `Option<QuantizedDeviation>` in [`ProportionalityDigest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantizedDeviation(i16);

impl QuantizedDeviation {
    pub fn from_raw(raw: i16) -> Result<Self, CodecError> {
        if raw == i16::MIN {
            return Err(CodecError::MissingDeviationSentinel);
        }
        Ok(Self(raw))
    }

    pub const fn raw(self) -> i16 {
        self.0
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / 256.0
    }
}

/// The typed payload for the currently active proportionality rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProportionalityDigest {
    book_scope: Option<QuantizedDeviation>,
    project_scope: Option<QuantizedDeviation>,
    saturated: bool,
}

impl ProportionalityDigest {
    pub const fn new(
        book_scope: Option<QuantizedDeviation>,
        project_scope: Option<QuantizedDeviation>,
        saturated: bool,
    ) -> Self {
        Self {
            book_scope,
            project_scope,
            saturated,
        }
    }

    pub const fn book_scope(self) -> Option<QuantizedDeviation> {
        self.book_scope
    }

    pub const fn project_scope(self) -> Option<QuantizedDeviation> {
        self.project_scope
    }

    pub const fn saturated(self) -> bool {
        self.saturated
    }
}

impl ProportionalityDigest {
    pub(super) fn lanes(self) -> [i16; 2] {
        [
            raw_or_missing(self.book_scope),
            raw_or_missing(self.project_scope),
        ]
    }

    pub(super) fn from_lanes(lanes: [i16; 2], flags: FindingFlags) -> Result<Self, CodecError> {
        Ok(Self::new(
            deviation_from_raw(lanes[0])?,
            deviation_from_raw(lanes[1])?,
            flags.contains(FindingFlags::SATURATED),
        ))
    }
}

fn raw_or_missing(value: Option<QuantizedDeviation>) -> i16 {
    value.map_or(i16::MIN, QuantizedDeviation::raw)
}

fn deviation_from_raw(raw: i16) -> Result<Option<QuantizedDeviation>, CodecError> {
    if raw == i16::MIN {
        Ok(None)
    } else {
        QuantizedDeviation::from_raw(raw).map(Some)
    }
}
