//! The chapter/verse designator INTERPRETER: the one place that reads inside a
//! [`TokenKind::Designator`](crate::TokenKind::Designator) span.
//!
//! The scanner carves the region after `\c`/`\v` as ONE token and never looks
//! at its bytes (scanner.rs `payload_end`); this module is the judgement half:
//! span in, [`Designator`] out. Ordering lint is only the first consumer — the
//! vref/reference exports inherit the same reading and the rules below.
//!
//! # By example (span in → what it IS)
//!
//! ```text
//! 12       →  first 12, last 12
//! 12a      →  first 12, last 12    a SEGMENT is a label, not a coordinate
//! 12-14    →  first 12, last 14
//! 1,3      →  first 1,  last 3     the endpoints; the hole is not modelled
//! 3-1      →  first 3,  last 3     `last` is the LARGEST number, so first <= last
//! 1<RLM>-3 →  first 1,  last 3     U+200F belongs to the SEPARATOR, not the segment
//! 01       →  Malformed            no leading zero (but a continuation `1-03` is fine)
//! 1-       →  Malformed            separator with nothing after it
//! ```
//!
//! VERSE follows the spec's own pattern, `[1-9][0-9]*[\p{L}\p{Mn}]*` then any
//! number of `RLM?[-,][0-9]+[\p{L}\p{Mn}]*` continuations. CHAPTER has no spec
//! pattern, but the chapter milestone's `sid` is `[A-Z1-4]{3} ?[0-9]+` — digits
//! only — so a bare integer it is, and `\c 12b` is MALFORMED. That assumption
//! is the module's only one, and [`chapter`] the one place to revisit it.
//!
//! # Comparison rules (these become vref's)
//!
//! - `first` is what a reference sorts by, `last` what the next verse must
//!   exceed; two designators OVERLAP when neither's `last` is below the other's
//!   `first`. Lint reads an equal first (or one landing exactly on the previous
//!   `last`) as a DUPLICATE, a lower one as OUT OF ORDER.
//! - The SEGMENT suffix takes part in no comparison: two segments of one verse
//!   are the same verse. Callers needing segment identity read the span.
//! - Numbers SATURATE at [`u32::MAX`]: junk either way, but it must be ORDERED
//!   junk rather than wrap into a false "out of order".
//!
//! # The Unicode reading (deliberately lenient)
//!
//! `\p{L}`/`\p{Mn}` would need a general-category table and this crate ships no
//! dependencies, so a segment character is an ASCII letter or ANY non-ASCII
//! scalar: accepts a little the spec rejects, rejects nothing it allows.
//! `designator-malformed` crying wolf on a legitimate Arabic or Devanagari
//! segment would be worse than missing a nicety.

/// What one designator span IS.
///
/// Two states and no borrowed bytes: consumers need the covered range and
/// nothing else, and a `Malformed` designator is flagged and excluded from the
/// sequence, never reinterpreted or repaired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Designator {
    /// Matches the pattern. `first <= last`, both saturating.
    Wellformed { first: u32, last: u32 },
    /// Does not match: empty, a leading zero, text-first, a stray `?`, a
    /// separator with no number after it, trailing junk.
    Malformed,
}

impl Designator {
    /// The covered range, or `None` when malformed.
    pub fn range(self) -> Option<(u32, u32)> {
        match self {
            Self::Wellformed { first, last } => Some((first, last)),
            Self::Malformed => None,
        }
    }
}

/// U+200F RIGHT-TO-LEFT MARK, UTF-8. Legal immediately before a separator.
const RLM: [u8; 3] = [0xE2, 0x80, 0x8F];

/// Reads a `\v` designator span against the spec's VERSE pattern.
pub fn verse(span: &[u8]) -> Designator {
    let mut at = 0;

    // A leading zero is malformed rather than silently equal to the unpadded
    // number.
    let Some(first) = take_number(span, &mut at, false) else {
        return Designator::Malformed;
    };
    take_segment(span, &mut at);
    let mut last = first;

    while at < span.len() {
        // The RTL mark belongs to the SEPARATOR, not the segment before it, so
        // `1‏-3` and `1-3` read alike.
        if span[at..].starts_with(&RLM) {
            at += RLM.len();
        }
        match span.get(at) {
            Some(b'-' | b',') => at += 1,
            _ => return Designator::Malformed,
        }
        let Some(number) = take_number(span, &mut at, true) else {
            return Designator::Malformed;
        };
        take_segment(span, &mut at);
        last = last.max(number);
    }

    Designator::Wellformed { first, last }
}

/// Reads a `\c` designator span: a bare positive integer, nothing else.
///
/// See the module doc for why letters are refused (`sid`'s `[0-9]+`).
pub fn chapter(span: &[u8]) -> Designator {
    let mut at = 0;
    match take_number(span, &mut at, false) {
        Some(number) if at == span.len() => Designator::Wellformed {
            first: number,
            last: number,
        },
        _ => Designator::Malformed,
    }
}

/// `[1-9][0-9]*` (or `[0-9]+` when `leading_zero_ok`), saturating. Advances
/// `at` only on success.
fn take_number(span: &[u8], at: &mut usize, leading_zero_ok: bool) -> Option<u32> {
    let from = *at;
    let first = *span.get(from)?;
    if !first.is_ascii_digit() || (first == b'0' && !leading_zero_ok) {
        return None;
    }
    let mut value: u32 = 0;
    let mut index = from;
    while let Some(byte) = span.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        // Saturating, never wrapping: junk must stay ORDERED junk.
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(byte - b'0'));
        index += 1;
    }
    *at = index;
    Some(value)
}

/// `[\p{L}\p{Mn}]*` under this module's lenient reading: ASCII letters plus
/// any non-ASCII scalar. Never fails — an empty segment is the common case.
fn take_segment(span: &[u8], at: &mut usize) {
    while let Some(&byte) = span.get(*at) {
        if byte.is_ascii() {
            if !byte.is_ascii_alphabetic() {
                return;
            }
            *at += 1;
        } else {
            // Token spans are always valid UTF-8, so this walks whole scalars.
            // The RTL mark is the one non-ASCII scalar a segment must not eat:
            // it introduces a separator.
            if span[*at..].starts_with(&RLM) {
                return;
            }
            *at += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Designator {
        verse(text.as_bytes())
    }

    fn c(text: &str) -> Designator {
        chapter(text.as_bytes())
    }

    fn well(first: u32, last: u32) -> Designator {
        Designator::Wellformed { first, last }
    }

    #[test]
    fn plain_numbers() {
        assert_eq!(v("1"), well(1, 1));
        assert_eq!(v("12"), well(12, 12));
        assert_eq!(v("176"), well(176, 176));
    }

    #[test]
    fn segments_do_not_change_the_range() {
        // `12a` and `12b` are the same verse.
        assert_eq!(v("12a"), well(12, 12));
        assert_eq!(v("12b"), well(12, 12));
        assert_eq!(v("7ab"), well(7, 7));
        assert_eq!(v("7ب"), well(7, 7));
        assert_eq!(v("7\u{0951}"), well(7, 7)); // a combining mark (Mn)
    }

    #[test]
    fn ranges_and_lists_cover_their_span() {
        assert_eq!(v("12-14"), well(12, 14));
        assert_eq!(v("1,3"), well(1, 3));
        assert_eq!(v("1-2,4-6"), well(1, 6));
        assert_eq!(v("12a-14b"), well(12, 14));
        // Backwards is well-FORMED; `last` stays the largest number, so
        // ordering never sees a range that runs uphill.
        assert_eq!(v("3-1"), well(3, 3));
    }

    #[test]
    fn the_rtl_mark_is_part_of_the_separator() {
        assert_eq!(v("1\u{200F}-3"), well(1, 3));
        assert_eq!(v("1\u{200F},3"), well(1, 3));
        assert_eq!(v("1\u{200F}"), Designator::Malformed);
    }

    #[test]
    fn continuation_numbers_may_carry_a_leading_zero_but_the_first_may_not() {
        assert_eq!(v("1-03"), well(1, 3));
        assert_eq!(v("01"), Designator::Malformed);
        assert_eq!(v("0"), Designator::Malformed);
    }

    #[test]
    fn malformed_shapes() {
        assert_eq!(v(""), Designator::Malformed);
        assert_eq!(v("?"), Designator::Malformed);
        assert_eq!(v("a1"), Designator::Malformed);
        assert_eq!(v("1-"), Designator::Malformed);
        assert_eq!(v("1,"), Designator::Malformed);
        assert_eq!(v("1--2"), Designator::Malformed);
        assert_eq!(v("1.2"), Designator::Malformed);
        assert_eq!(v("1a2"), Designator::Malformed); // digits cannot follow a segment
        assert_eq!(v("1 2"), Designator::Malformed);
    }

    #[test]
    fn huge_numbers_saturate_instead_of_panicking() {
        let many = "9".repeat(64);
        assert_eq!(v(&many), well(u32::MAX, u32::MAX));
        assert_eq!(v(&format!("1-{many}")), well(1, u32::MAX));
        assert_eq!(c(&many), well(u32::MAX, u32::MAX));
    }

    #[test]
    fn chapters_are_bare_integers() {
        assert_eq!(c("1"), well(1, 1));
        assert_eq!(c("150"), well(150, 150));
        // No segments, no ranges, no leading zero.
        assert_eq!(c("12b"), Designator::Malformed);
        assert_eq!(c("1-2"), Designator::Malformed);
        assert_eq!(c("01"), Designator::Malformed);
        assert_eq!(c(""), Designator::Malformed);
    }

    #[test]
    fn range_reports_the_pair_or_nothing() {
        assert_eq!(v("12-14").range(), Some((12, 14)));
        assert_eq!(v("?").range(), None);
    }
}
