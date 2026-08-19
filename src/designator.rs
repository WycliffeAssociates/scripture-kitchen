//! The chapter/verse designator INTERPRETER: the one place that reads inside a
//! [`TokenKind::Designator`](crate::TokenKind::Designator) span.
//!
//! The scanner carves the region after `\c`/`\v` as ONE token and never looks
//! at its bytes (scanner.rs `payload_end`). This module is the judgement half:
//! span in, [`Designator`] out. It has its own module because ORDERING LINT is
//! only its first consumer — the vref/reference exports want exactly the same
//! reading, and the comparison rules below are the rules they will inherit.
//!
//! # The patterns
//!
//! VERSE (the spec's own pattern, quoted in planning/NEXT-STEPS.md):
//!
//! ```text
//! [1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*
//! ```
//!
//! i.e. a leading integer with no leading zero, an optional letter/mark
//! SEGMENT suffix (`12a`, `7ب`), then any number of `-` range or `,` list
//! continuations, each of which may be preceded by U+200F RIGHT-TO-LEFT MARK
//! (RTL scripts write the separator with the mark so the digits order
//! visually). Continuation numbers are `[0-9]+` — the spec allows a leading
//! zero there and we follow it rather than inventing a stricter rule.
//!
//! CHAPTER is the degenerate case: a bare positive integer. The 3.1 spec gives
//! no explicit pattern for `\c`'s number, but the chapter milestone's `sid`
//! attribute is patterned `[A-Z1-4]{3} ?[0-9]+` — digits only — and every
//! example is numeric. ASSUMPTION RECORDED: `\c 12b` is MALFORMED here. If a
//! real corpus ever shows lettered chapters, this is the one function to
//! revisit.
//!
//! # Comparison rules (these become vref's)
//!
//! - A designator COVERS a closed integer range. `12` covers 12..=12,
//!   `12-14` covers 12..=14, `1,3` covers 1..=3 (the list's endpoints; we do
//!   not model the hole, because no consumer has asked for set semantics and
//!   guessing one would be synthesis).
//! - [`Designator::Wellformed::first`] is the FIRST component's number — what
//!   a reference sorts by. [`Designator::Wellformed::last`] is the LARGEST
//!   number anywhere in the designator — what the next verse must exceed.
//!   For every sane designator these are the two endpoints; for a backwards
//!   one (`3-1`) `last` stays 3, so `first <= last` always holds.
//! - Two designators OVERLAP when neither's `last` is below the other's
//!   `first`. Ordering lint's reading of an overlap is: equal firsts (or a
//!   first landing exactly on the previous `last`) is a DUPLICATE, a first
//!   below the previous `last` is OUT OF ORDER.
//! - The SEGMENT suffix (`12a`) participates in no comparison at all. It is a
//!   partial-verse label, not a coordinate, and two segments of one verse are
//!   the same verse — which is precisely why `\v 12a` then `\v 12b` must not
//!   read as a duplicate. Callers that need segment identity read the span.
//! - Numbers SATURATE at [`u32::MAX`]. A designator of a thousand digits is
//!   nonsense either way; it must never panic, and saturating keeps the
//!   ordering monotonic instead of wrapping into a false "out of order".
//!
//! # The Unicode reading (deliberately lenient)
//!
//! `\p{L}` and `\p{Mn}` would need a general-category table, and this crate
//! ships no dependencies. The rule used instead: a segment character is an
//! ASCII letter, or ANY non-ASCII scalar. That accepts a few things the spec
//! would reject (a non-ASCII punctuation mark used as a segment), and rejects
//! nothing the spec allows. That direction is chosen on purpose — lint's
//! standing law is that crying wolf on valid text is worse than missing a
//! nicety, and `designator-malformed` firing on a legitimate Arabic or
//! Devanagari segment would be exactly that.

/// What one designator span IS.
///
/// Only two states, and no borrowed bytes: ordering lint needs the covered
/// range and nothing else, and a `Malformed` designator is never
/// reinterpreted — it is flagged and excluded from the sequence (never
/// repaired, per the standing law).
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

    // The leading integer: `[1-9][0-9]*`, so a leading zero is malformed
    // rather than silently equal to the unpadded number.
    let Some(first) = take_number(span, &mut at, false) else {
        return Designator::Malformed;
    };
    take_segment(span, &mut at);
    let mut last = first;

    while at < span.len() {
        // The RTL mark is optional and belongs to the SEPARATOR, not to the
        // segment before it — consumed here so `1‏-3` and `1-3` read alike.
        if span[at..].starts_with(&RLM) {
            at += RLM.len();
        }
        match span.get(at) {
            Some(b'-' | b',') => at += 1,
            // Anything else after a well-formed prefix is trailing junk.
            _ => return Designator::Malformed,
        }
        // Continuation numbers are `[0-9]+`: a leading zero IS allowed here.
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
        // Saturating, never wrapping: a 40-digit designator is junk, but it
        // must stay ORDERED junk (see the module doc).
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
            // A continuation byte here would mean the span is not valid UTF-8;
            // token spans always are, so this walks whole scalars. The RTL
            // mark is the one non-ASCII scalar that must NOT be eaten as a
            // segment — it introduces a separator.
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
        // `12a` and `12b` are the same verse — that is the whole reason the
        // segment is not part of the comparison.
        assert_eq!(v("12a"), well(12, 12));
        assert_eq!(v("12b"), well(12, 12));
        // Multi-letter and non-ASCII segments, under the lenient reading.
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
        // Backwards is well-FORMED (the pattern allows it); `last` stays the
        // largest number so ordering never sees a range that runs uphill.
        assert_eq!(v("3-1"), well(3, 3));
    }

    #[test]
    fn the_rtl_mark_is_part_of_the_separator() {
        assert_eq!(v("1\u{200F}-3"), well(1, 3));
        assert_eq!(v("1\u{200F},3"), well(1, 3));
        // Trailing, with nothing after it: junk.
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
        assert_eq!(v("1 2"), Designator::Malformed); // the scanner never carves this, but be sure
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
        // The recorded assumption: no segments, no ranges, no leading zero.
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
