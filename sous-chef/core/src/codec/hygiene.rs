//! Lanes for `RuleCode::Hygiene`: the class discriminant, then the run
//! length. The scan is `crate::hygiene`; this is only its wire shape.
//!
//! ```text
//! lanes  [0, 223]              →  C0Control, a 223-code-point run
//! lanes  [1, i16::MAX] + SATURATED  →  Delete, a run longer than 32767
//! ```

use mise::unicode::Class;

use super::{CodecError, FindingFlags};

/// One deterministic hygiene class. The discriminant rides the first payload
/// lane, so values are fixed and append-only like rule codes.
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
    /// A combining mark with nothing to combine with.
    FreeCombiningMark = 7,
    /// A General_Category `Cf` scalar outside any glue position.
    MisplacedFormat = 8,
    /// U+00A0 beside other whitespace or at an edge of the analyzed text.
    NoBreakSpace = 9,
    /// U+FDD0..=U+FDEF or U+xxFFFE/U+xxFFFF.
    Noncharacter = 10,
}

impl HygieneClass {
    pub const ALL: [Self; 11] = [
        Self::C0Control,
        Self::Delete,
        Self::C1Control,
        Self::ReplacementChar,
        Self::StrayCarriageReturn,
        Self::StrandedBackslash,
        Self::ConflictMarker,
        Self::FreeCombiningMark,
        Self::MisplacedFormat,
        Self::NoBreakSpace,
        Self::Noncharacter,
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
            Self::FreeCombiningMark => "FreeCombiningMark",
            Self::MisplacedFormat => "MisplacedFormat",
            Self::NoBreakSpace => "NoBreakSpace",
            Self::Noncharacter => "Noncharacter",
        }
    }

    /// What the class finds, in one plain sentence: the generated `SousCode`
    /// docs, for agents. Hosts word their own labels.
    pub const fn rule(self) -> &'static str {
        match self {
            Self::C0Control => {
                "An invisible control character (U+0000..U+001F other than tab and line ends)."
            }
            Self::Delete => "The DELETE control character, U+007F.",
            Self::C1Control => "An invisible control character in U+0080..U+009F.",
            Self::ReplacementChar => {
                "The replacement character U+FFFD, left where text failed to decode."
            }
            Self::StrayCarriageReturn => "A carriage return not followed by a line feed.",
            Self::StrandedBackslash => "A backslash in the text that starts no marker.",
            Self::ConflictMarker => "A merge-conflict line (`<<<<<<<`, `=======`, `>>>>>>>`).",
            Self::FreeCombiningMark => "A combining accent with no letter to sit on.",
            Self::MisplacedFormat => {
                "An invisible formatting character outside any place it joins letters."
            }
            Self::NoBreakSpace => {
                "A no-break space beside other white space or at the edge of the text."
            }
            Self::Noncharacter => "A Unicode noncharacter, which no text should contain.",
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

/// Where a run sits, judged on the scalars just outside it in the order
/// `judge` asks: Alone, AtEdge, BesideSpace, BesidePunctuation,
/// BetweenLetters, Other. Discriminants are append-only.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Placement {
    /// White space on at least one side.
    BesideSpace = 0,
    /// No scalar on exactly one side: the run opens or closes its chapter.
    AtEdge = 1,
    /// A letter or a mark on both sides.
    BetweenLetters = 2,
    /// Punctuation on at least one side, white space on neither.
    BesidePunctuation = 3,
    /// No scalar on either side: the run is its chapter's whole text.
    Alone = 4,
    /// None of the above: a digit, a symbol, or a control beside it.
    Other = 5,
}

impl Placement {
    pub const ALL: [Self; 6] = [
        Self::BesideSpace,
        Self::AtEdge,
        Self::BetweenLetters,
        Self::BesidePunctuation,
        Self::Alone,
        Self::Other,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::BesideSpace => "BesideSpace",
            Self::AtEdge => "AtEdge",
            Self::BetweenLetters => "BetweenLetters",
            Self::BesidePunctuation => "BesidePunctuation",
            Self::Alone => "Alone",
            Self::Other => "Other",
        }
    }

    /// `prev` and `next` are the classes just outside the run, `None` at a
    /// chapter edge.
    pub fn judge(prev: Option<Class>, next: Option<Class>) -> Self {
        let either = |test: fn(Class) -> bool| prev.is_some_and(test) || next.is_some_and(test);
        let letter = |class: Option<Class>| class.is_some_and(|c| c.is_alphabetic() || c.is_mark());
        match (prev, next) {
            (None, None) => Self::Alone,
            (None, _) | (_, None) => Self::AtEdge,
            _ if either(Class::is_whitespace) => Self::BesideSpace,
            _ if either(Class::is_punctuation) => Self::BesidePunctuation,
            _ if letter(prev) && letter(next) => Self::BetweenLetters,
            _ => Self::Other,
        }
    }
}

/// Where the run sits in its published span, which atom widening may have
/// grown on either side.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// The span is the run.
    Exact = 0,
    /// The run starts the span; the span runs on past it.
    AtStart = 1,
    /// The run ends the span; the span starts before it.
    AtEnd = 2,
    /// The span is wider on both sides, or its run is not one contiguous
    /// piece of the published text.
    Neither = 3,
}

impl Anchor {
    pub const ALL: [Self; 4] = [Self::Exact, Self::AtStart, Self::AtEnd, Self::Neither];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Exact => "Exact",
            Self::AtStart => "AtStart",
            Self::AtEnd => "AtEnd",
            Self::Neither => "Neither",
        }
    }

    pub fn of(run: (u32, u32), span: (u32, u32)) -> Self {
        match (run.0 == span.0, run.1 == span.1) {
            (true, true) => Self::Exact,
            (true, false) => Self::AtStart,
            (false, true) => Self::AtEnd,
            (false, false) => Self::Neither,
        }
    }
}

/// What a fix needs to know about one run beyond its class and length, packed
/// as the byte lane A carries above the class:
///
/// ```text
/// bit  7    6               5         4..3    2..0
///      0    lone_is_silent  one_unit  anchor  placement
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HygieneSite(u8);

impl HygieneSite {
    const ANCHOR_SHIFT: u8 = 3;
    const ONE_UNIT: u8 = 1 << 5;
    const LONE_IS_SILENT: u8 = 1 << 6;

    /// `one_unit`: every scalar of the run is one UTF-16 unit, so `run` is
    /// its UTF-16 length. `lone_is_silent`: the run repeats one scalar, and
    /// that scalar alone between the same neighbours is no finding, so keeping
    /// the first is a whole fix.
    pub const fn new(
        placement: Placement,
        anchor: Anchor,
        one_unit: bool,
        lone_is_silent: bool,
    ) -> Self {
        Self(
            placement as u8
                | (anchor as u8) << Self::ANCHOR_SHIFT
                | if one_unit { Self::ONE_UNIT } else { 0 }
                | if lone_is_silent {
                    Self::LONE_IS_SILENT
                } else {
                    0
                },
        )
    }

    pub const fn placement(self) -> Placement {
        Placement::ALL[(self.0 & 0b111) as usize]
    }

    pub const fn anchor(self) -> Anchor {
        Anchor::ALL[((self.0 >> Self::ANCHOR_SHIFT) & 0b11) as usize]
    }

    pub const fn one_unit(self) -> bool {
        self.0 & Self::ONE_UNIT != 0
    }

    pub const fn lone_is_silent(self) -> bool {
        self.0 & Self::LONE_IS_SILENT != 0
    }

    /// The same site restated for a published span its run does not sit in
    /// as one piece.
    pub const fn unanchored(self) -> Self {
        Self::new(
            self.placement(),
            Anchor::Neither,
            self.one_unit(),
            self.lone_is_silent(),
        )
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub fn from_bits(bits: u8) -> Result<Self, CodecError> {
        if bits & 0x80 != 0 || usize::from(bits & 0b111) >= Placement::ALL.len() {
            return Err(CodecError::UnknownHygieneSite(bits));
        }
        Ok(Self(bits))
    }
}

/// The class and the offending code points in the maximal run. `run`
/// saturates at `i16::MAX` and sets `SATURATED`.
///
/// The span is that run snapped out to grapheme-atom edges, so a run hanging
/// off a base publishes a span one atom wider than its count.
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
        // A saturated row reads back exactly i16::MAX.
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
        bad_class[12] = 11;
        assert_eq!(
            PackedFinding::decode(&bad_class, &[1]),
            Err(CodecError::UnknownHygieneClass(11))
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
