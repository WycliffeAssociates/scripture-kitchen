//! The headerless fixed-width finding record used by Sous transport.
//!
//! This module freezes one semantic record without choosing a surrounding
//! snapshot envelope. Records validate their projection context at creation
//! and decode time, while encoding derives the wire tag and payload from a
//! typed finding-kind union.

use core::fmt;

use crate::BookIndex;

pub const RECORD_LEN: usize = 16;

/// The only active v1 rule discriminant.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCode {
    LengthProportionality = 0,
}

impl TryFrom<u8> for RuleCode {
    type Error = CodecError;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::LengthProportionality),
            other => Err(CodecError::UnknownRuleCode(other)),
        }
    }
}

impl From<RuleCode> for u8 {
    fn from(code: RuleCode) -> Self {
        code as u8
    }
}

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

/// Code-specific semantic payloads. Adding a future payload requires a new
/// rule code and an explicit wire interpretation; it cannot share a generic
/// numerator/denominator shape accidentally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    LengthProportionality(ProportionalityDigest),
}

/// A checked semantic representation of one 16-byte wire record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedFinding {
    from: u32,
    to: u32,
    book_idx: BookIndex,
    kind: FindingKind,
}

impl PackedFinding {
    /// Constructs a record after checking the span against its selected book.
    pub fn new(
        from: u32,
        to: u32,
        book_idx: BookIndex,
        kind: FindingKind,
        book_lengths: &[u32],
    ) -> Result<Self, CodecError> {
        validate_context(from, to, book_idx, book_lengths)?;
        Ok(Self {
            from,
            to,
            book_idx,
            kind,
        })
    }

    pub const fn from(self) -> u32 {
        self.from
    }

    pub const fn to(self) -> u32 {
        self.to
    }

    pub const fn book_idx(self) -> BookIndex {
        self.book_idx
    }

    pub const fn kind(self) -> FindingKind {
        self.kind
    }

    /// Returns the active code derived from this finding's typed kind.
    pub const fn code(self) -> RuleCode {
        match self.kind {
            FindingKind::LengthProportionality(_) => RuleCode::LengthProportionality,
        }
    }

    /// Returns representation flags derived from this finding's typed payload.
    pub const fn flags(self) -> FindingFlags {
        match self.kind {
            FindingKind::LengthProportionality(digest) => {
                if digest.saturated() {
                    FindingFlags::SATURATED
                } else {
                    FindingFlags::NONE
                }
            }
        }
    }

    /// Writes exactly the v1 record fields in little-endian order.
    pub fn encode(self) -> [u8; RECORD_LEN] {
        let mut bytes = [0; RECORD_LEN];
        bytes[0..4].copy_from_slice(&self.from.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.to.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.book_idx.get().to_le_bytes());
        bytes[10] = self.code().into();
        bytes[11] = self.flags().bits();
        match self.kind {
            FindingKind::LengthProportionality(digest) => {
                bytes[12..14].copy_from_slice(&raw_or_missing(digest.book_scope()).to_le_bytes());
                bytes[14..16]
                    .copy_from_slice(&raw_or_missing(digest.project_scope()).to_le_bytes());
            }
        }
        bytes
    }

    /// Decodes one record and validates its span against the supplied book table.
    pub fn decode(bytes: &[u8], book_lengths: &[u32]) -> Result<Self, CodecError> {
        if bytes.len() != RECORD_LEN {
            return Err(CodecError::InvalidLength {
                actual: bytes.len(),
            });
        }
        let from = u32::from_le_bytes(bytes[0..4].try_into().expect("record length checked"));
        let to = u32::from_le_bytes(bytes[4..8].try_into().expect("record length checked"));
        let book_idx_raw =
            u16::from_le_bytes(bytes[8..10].try_into().expect("record length checked"));
        let book_idx =
            BookIndex::new(usize::from(book_idx_raw)).expect("every u16 wire index fits BookIndex");
        let code = RuleCode::try_from(bytes[10])?;
        let flags = FindingFlags::from_bits(bytes[11])?;
        let book_raw = i16::from_le_bytes(bytes[12..14].try_into().expect("record length checked"));
        let project_raw =
            i16::from_le_bytes(bytes[14..16].try_into().expect("record length checked"));
        let kind = match code {
            RuleCode::LengthProportionality => {
                FindingKind::LengthProportionality(ProportionalityDigest::new(
                    deviation_from_raw(book_raw)?,
                    deviation_from_raw(project_raw)?,
                    flags.contains(FindingFlags::SATURATED),
                ))
            }
        };
        Self::new(from, to, book_idx, kind, book_lengths)
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

/// Representation flags currently needed by the compact payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FindingFlags(u8);

impl FindingFlags {
    pub const NONE: Self = Self(0);
    pub const SATURATED: Self = Self(1 << 0);
    const KNOWN_BITS: u8 = Self::SATURATED.0;

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    fn from_bits(bits: u8) -> Result<Self, CodecError> {
        if bits & !Self::KNOWN_BITS != 0 {
            return Err(CodecError::UnknownFlags(bits));
        }
        Ok(Self(bits))
    }
}

fn validate_context(
    from: u32,
    to: u32,
    book_idx: BookIndex,
    book_lengths: &[u32],
) -> Result<(), CodecError> {
    if from > to {
        return Err(CodecError::ReversedSpan { from, to });
    }
    let Some(&book_len) = book_lengths.get(usize::from(book_idx.get())) else {
        return Err(CodecError::InvalidBookIndex {
            index: book_idx.get(),
        });
    };
    if to > book_len {
        return Err(CodecError::SpanOutOfBounds { from, to, book_len });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    InvalidLength { actual: usize },
    UnknownRuleCode(u8),
    UnknownFlags(u8),
    MissingDeviationSentinel,
    ReversedSpan { from: u32, to: u32 },
    InvalidBookIndex { index: u16 },
    SpanOutOfBounds { from: u32, to: u32, book_len: u32 },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(
                    f,
                    "finding record has {actual} bytes; expected {RECORD_LEN}"
                )
            }
            Self::UnknownRuleCode(code) => write!(f, "unknown finding rule code {code}"),
            Self::UnknownFlags(flags) => write!(f, "unknown finding flags 0x{flags:02x}"),
            Self::MissingDeviationSentinel => {
                f.write_str("i16::MIN is reserved for an unavailable deviation")
            }
            Self::ReversedSpan { from, to } => write!(f, "reversed finding span {from}..{to}"),
            Self::InvalidBookIndex { index } => {
                write!(f, "book index {index} is absent from the book table")
            }
            Self::SpanOutOfBounds { from, to, book_len } => {
                write!(
                    f,
                    "finding span {from}..{to} exceeds selected book length {book_len}"
                )
            }
        }
    }
}

impl std::error::Error for CodecError {}

#[cfg(test)]
mod tests {
    use super::*;

    const LENGTH_KIND: FindingKind = FindingKind::LengthProportionality(ProportionalityDigest {
        book_scope: None,
        project_scope: None,
        saturated: false,
    });
    static BOOK_LENGTHS: [u32; 65_536] = [u32::MAX; 65_536];

    fn deviation(raw: i16) -> QuantizedDeviation {
        QuantizedDeviation::from_raw(raw).unwrap()
    }

    fn finding(
        from: u32,
        to: u32,
        book_idx: usize,
        book_scope: Option<QuantizedDeviation>,
        project_scope: Option<QuantizedDeviation>,
        saturated: bool,
    ) -> PackedFinding {
        PackedFinding::new(
            from,
            to,
            BookIndex::new(book_idx).unwrap(),
            FindingKind::LengthProportionality(ProportionalityDigest::new(
                book_scope,
                project_scope,
                saturated,
            )),
            &BOOK_LENGTHS,
        )
        .unwrap()
    }

    #[test]
    fn golden_vector_is_exact_little_endian_record_layout() {
        let record = finding(
            0x1122_3344,
            0x5566_7788,
            0x1234,
            Some(deviation(0x1234)),
            Some(deviation(-0x1234)),
            true,
        );
        assert_eq!(RECORD_LEN, 16);
        assert_eq!(
            record.encode(),
            [
                0x44, 0x33, 0x22, 0x11, 0x88, 0x77, 0x66, 0x55, 0x34, 0x12, 0x00, 0x01, 0x34, 0x12,
                0xcc, 0xed,
            ]
        );
    }

    #[test]
    fn semantic_round_trip_preserves_typed_fields() {
        let record = finding(3, 17, 1, Some(deviation(0x0180)), None, false);
        assert_eq!(
            PackedFinding::decode(&record.encode(), &[4, 17]),
            Ok(record)
        );
        let FindingKind::LengthProportionality(digest) = record.kind();
        assert_eq!(digest.book_scope().unwrap().raw(), 0x0180);
        assert_eq!(digest.project_scope(), None);
    }

    #[test]
    fn positive_negative_and_missing_q8_8_values_round_trip() {
        let record = finding(
            0,
            0,
            0,
            Some(deviation(0x0180)),
            Some(deviation(-0x0180)),
            false,
        );
        let decoded = PackedFinding::decode(&record.encode(), &[0]).unwrap();
        let FindingKind::LengthProportionality(digest) = decoded.kind();
        assert_eq!(digest.book_scope().unwrap().raw(), 0x0180);
        assert_eq!(digest.project_scope().unwrap().raw(), -0x0180);
        assert!((digest.book_scope().unwrap().as_f64() - 1.5).abs() < f64::EPSILON);

        let missing = finding(0, 0, 0, None, None, false);
        let FindingKind::LengthProportionality(digest) =
            PackedFinding::decode(&missing.encode(), &[0])
                .unwrap()
                .kind();
        assert_eq!(digest.book_scope(), None);
        assert_eq!(digest.project_scope(), None);
    }

    #[test]
    fn saturation_metadata_round_trips_for_both_scopes() {
        let record = finding(0, 0, 0, None, None, true);
        assert!(record.flags().contains(FindingFlags::SATURATED));
        let decoded = PackedFinding::decode(&record.encode(), &[0]).unwrap();
        let FindingKind::LengthProportionality(digest) = decoded.kind();
        assert!(digest.saturated());
    }

    #[test]
    fn maximum_quantized_value_does_not_imply_saturation() {
        let record = finding(
            0,
            0,
            0,
            Some(deviation(i16::MAX)),
            Some(deviation(i16::MAX)),
            false,
        );
        assert!(!record.flags().contains(FindingFlags::SATURATED));
        let FindingKind::LengthProportionality(digest) =
            PackedFinding::decode(&record.encode(), &[0])
                .unwrap()
                .kind();
        assert_eq!(digest.book_scope().unwrap().raw(), i16::MAX);
        assert_eq!(digest.project_scope().unwrap().raw(), i16::MAX);
    }

    #[test]
    fn malformed_wire_values_fail_closed() {
        let record = finding(0, 0, 0, None, None, false);
        let mut unknown_code = record.encode();
        unknown_code[10] = 1;
        assert_eq!(
            PackedFinding::decode(&unknown_code, &[0]),
            Err(CodecError::UnknownRuleCode(1))
        );

        let mut unknown_flags = record.encode();
        unknown_flags[11] = 0x80;
        assert_eq!(
            PackedFinding::decode(&unknown_flags, &[0]),
            Err(CodecError::UnknownFlags(0x80))
        );

        assert_eq!(
            PackedFinding::decode(&[0; 15], &[0]),
            Err(CodecError::InvalidLength { actual: 15 })
        );
        assert_eq!(
            PackedFinding::decode(&[0; 17], &[0]),
            Err(CodecError::InvalidLength { actual: 17 })
        );
    }

    #[test]
    fn context_rejects_reversed_out_of_book_and_invalid_index() {
        assert_eq!(
            PackedFinding::new(3, 2, BookIndex::new(0).unwrap(), LENGTH_KIND, &[3]),
            Err(CodecError::ReversedSpan { from: 3, to: 2 })
        );
        assert_eq!(
            PackedFinding::new(0, 4, BookIndex::new(0).unwrap(), LENGTH_KIND, &[3]),
            Err(CodecError::SpanOutOfBounds {
                from: 0,
                to: 4,
                book_len: 3,
            })
        );
        assert_eq!(
            PackedFinding::new(0, 0, BookIndex::new(1).unwrap(), LENGTH_KIND, &[0]),
            Err(CodecError::InvalidBookIndex { index: 1 })
        );
    }

    #[test]
    fn maximum_context_index_and_ending_span_are_valid() {
        let lengths = vec![0; 65_536];
        let record =
            PackedFinding::new(0, 0, BookIndex::new(65_535).unwrap(), LENGTH_KIND, &lengths)
                .unwrap();
        assert_eq!(record.book_idx().get(), u16::MAX);
    }

    #[test]
    fn sentinel_is_not_constructible_as_a_quantized_deviation() {
        assert_eq!(
            QuantizedDeviation::from_raw(i16::MIN),
            Err(CodecError::MissingDeviationSentinel)
        );
    }
}
