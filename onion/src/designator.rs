//! The chapter/verse designator INTERPRETER: the one place that reads inside a
//! [`TokenKind::Designator`](crate::TokenKind::Designator) span.
//!
//! The scanner carves the region after `\c`/`\v` as ONE token and never looks
//! at its bytes (scanner.rs `payload_end`); this module is the judgement half:
//! span in, [`Designator`] and [`members`] out. Ordering lint is only the first
//! consumer — the Toc, vref and the reference exports inherit the same reading
//! and the rules below.
//!
//! # By example (span in → what it IS)
//!
//! ```text
//!                hull (Designator)     members (written order)
//! 12       →     first 12, last 12     12
//! 12a      →     first 12, last 12     12a                 a SEGMENT is a place inside 12
//! 12-14    →     first 12, last 14     12–14
//! 12a-14b  →     first 12, last 14     12a–14b
//! 1,3      →     first 1,  last 3      1  3                the hole (2) is not covered
//! 1-2,4-6  →     first 1,  last 6      1–2  4–6
//! 3-1      →     first 3,  last 3      3–1                 kept as written; the hull is ordered
//! 1<RLM>-3 →     first 1,  last 3      1–3                 U+200F belongs to the SEPARATOR
//! 12␠      →     first 12, last 12     12                  the folded delimiter is not part of it
//! 01       →     Malformed             (none)              no leading zero (a continuation `1-03` is fine)
//! 1-       →     Malformed             (none)              separator with nothing after it
//! ```
//!
//! VERSE follows the spec's own pattern, `[1-9][0-9]*[\p{L}\p{Mn}]*` then any
//! number of `RLM?[-,][0-9]+[\p{L}\p{Mn}]*` continuations. CHAPTER has no spec
//! pattern, but the chapter milestone's `sid` is `[A-Z1-4]{3} ?[0-9]+` — digits
//! only — so a bare integer it is, and `\c 12b` is MALFORMED. That assumption
//! is the module's only one, and [`chapter`] the one place to revisit it.
//!
//! # Two readings of one span
//!
//! - The HULL ([`Designator`]) is the lowest and highest number named. It is
//!   what a sequence sorts by and what a bridge renders as (`MRK 6:1-3`).
//! - The MEMBERS ([`members`]) are what the designator actually covers: `-`
//!   joins two points into one member, `,` starts the next. `\v 1,3,5` covers
//!   1, 3 and 5 and not 2 or 4; `\v 12a` covers the place `12a` and not `12b`.
//!   A backwards member (`3-1`) is kept as written — judging it is lint's job.
//!
//! # Comparison rules (these become vref's)
//!
//! - `first` is what a reference sorts by, `last` what the next verse must
//!   exceed; two designators OVERLAP when neither's `last` is below the other's
//!   `first`. Lint reads an equal first (or one landing exactly on the previous
//!   `last`) as a DUPLICATE, a lower one as OUT OF ORDER — except inside a
//!   LIST's holes, which a later verse may fill (see `lint/ordering.rs`).
//! - The HULL ignores segments: `12a` and `12b` have one hull, and are still
//!   two places. A caller needing segment identity reads [`members`].
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

/// What one designator span's HULL is.
///
/// Two states and no borrowed bytes: a sequence needs the covered range and
/// nothing else, and a `Malformed` designator is flagged and excluded from the
/// sequence, never reinterpreted or repaired. What it covers inside that range
/// is [`members`].
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

/// The designator PROPER: the token span minus the DELIMITER run the scanner
/// folded onto its tail (scanner.rs `payload_end`).
///
/// ```text
/// "12 "  →  "12"      the delimiter before the verse text
/// "12"   →  "12"      a newline delimiter is its own token, so nothing to trim
/// ```
///
/// Every consumer of designator bytes starts here — the readers below, the
/// exports' numbers, lint's renumber splice — because a designator never
/// contains whitespace, so the trim can only take the delimiter.
pub fn label(span: &[u8]) -> &[u8] {
    crate::scanner::payload_label(span)
}

/// Reads a `\v` designator span against the spec's VERSE pattern.
pub fn verse(span: &[u8]) -> Designator {
    let span = label(span);
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
    let span = label(span);
    let mut at = 0;
    match take_number(span, &mut at, false) {
        Some(number) if at == span.len() => Designator::Wellformed {
            first: number,
            last: number,
        },
        _ => Designator::Malformed,
    }
}

/// One endpoint of a [`Member`]: a number and the segment written after it.
///
/// `segment_start..segment_end` are byte offsets into the LABEL (the span
/// [`label`] returns, which starts where the span does); equal when no segment
/// is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub number: u32,
    pub segment_start: u32,
    pub segment_end: u32,
}

impl Point {
    /// Whether a segment is written after the number (`12a`).
    pub fn has_segment(&self) -> bool {
        self.segment_end > self.segment_start
    }
}

/// One thing a designator covers: a single place (`from == to`) or a range
/// joined by `-`. Members are separated by `,`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub from: Point,
    pub to: Point,
}

impl Member {
    /// The numbers this member covers, low to high. A segment covers part of
    /// its number, so it counts as that number here.
    pub fn numbers(&self) -> (u32, u32) {
        let (a, b) = (self.from.number, self.to.number);
        (a.min(b), a.max(b))
    }
}

/// What a `\v` designator span covers, member by member, in written order.
///
/// Empty for a malformed span: a designator [`verse`] refuses covers nothing.
/// Allocation-free — the iterator walks the label it borrows.
pub fn members(span: &[u8]) -> Members<'_> {
    let label = label(span);
    let wellformed = matches!(verse(span), Designator::Wellformed { .. });
    Members {
        label,
        at: if wellformed { 0 } else { label.len() },
    }
}

/// The iterator [`members`] returns.
#[derive(Debug, Clone)]
pub struct Members<'a> {
    label: &'a [u8],
    at: usize,
}

impl Iterator for Members<'_> {
    type Item = Member;

    fn next(&mut self) -> Option<Member> {
        if self.at >= self.label.len() {
            return None;
        }
        // The span was checked well-formed, so every read here succeeds; only
        // the first number of the whole designator refuses a leading zero, and
        // that was checked too.
        let from = self.point()?;
        let mut to = from;
        loop {
            if self.label[self.at..].starts_with(&RLM) {
                self.at += RLM.len();
            }
            match self.label.get(self.at) {
                Some(b'-') => {
                    self.at += 1;
                    to = self.point()?;
                }
                Some(b',') => {
                    self.at += 1;
                    break;
                }
                _ => break,
            }
        }
        Some(Member { from, to })
    }
}

impl Members<'_> {
    fn point(&mut self) -> Option<Point> {
        let number = take_number(self.label, &mut self.at, true)?;
        let segment_start = self.at as u32;
        take_segment(self.label, &mut self.at);
        Some(Point {
            number,
            segment_start,
            segment_end: self.at as u32,
        })
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
    fn segments_do_not_change_the_hull() {
        // `12a` and `12b` share a hull; `members` tells them apart.
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

    /// The span carries the delimiter the scanner folded into it, and reading it
    /// begins by stepping back over that run.
    #[test]
    fn the_folded_delimiter_is_not_part_of_the_designator() {
        assert_eq!(label(b"12 "), b"12");
        assert_eq!(label(b"12  \t"), b"12");
        assert_eq!(label(b"12"), b"12");
        assert_eq!(v("1 "), well(1, 1));
        assert_eq!(v("12-14 "), well(12, 14));
        assert_eq!(c("150 "), well(150, 150));
        // A space INSIDE is still junk — only a trailing run can be delimiter.
        assert_eq!(v("1 2"), Designator::Malformed);
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

    /// Each member as `(from, to)`, a point rendered with its segment.
    fn covers(text: &str) -> Vec<(String, String)> {
        let label = label(text.as_bytes());
        let show = |p: Point| {
            let segment = &label[p.segment_start as usize..p.segment_end as usize];
            format!("{}{}", p.number, String::from_utf8_lossy(segment))
        };
        members(text.as_bytes())
            .map(|m| (show(m.from), show(m.to)))
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn a_plain_number_is_one_point() {
        assert_eq!(covers("12"), pairs(&[("12", "12")]));
        assert_eq!(covers("12 "), pairs(&[("12", "12")]));
    }

    #[test]
    fn a_segment_is_part_of_the_place_it_names() {
        assert_eq!(covers("12a"), pairs(&[("12a", "12a")]));
        assert_eq!(covers("12b"), pairs(&[("12b", "12b")]));
        assert_ne!(covers("12a"), covers("12b"));
        assert_eq!(covers("12a-14b"), pairs(&[("12a", "14b")]));
        assert_eq!(covers("7ب"), pairs(&[("7ب", "7ب")]));
        let bare = members(b"12").next().unwrap();
        let lettered = members(b"12a").next().unwrap();
        assert!(!bare.from.has_segment());
        assert!(lettered.from.has_segment());
        assert_eq!(lettered.numbers(), (12, 12));
    }

    #[test]
    fn a_list_covers_its_members_and_not_its_holes() {
        assert_eq!(
            covers("1,3,5"),
            pairs(&[("1", "1"), ("3", "3"), ("5", "5")])
        );
        assert_eq!(covers("1-2,4-6"), pairs(&[("1", "2"), ("4", "6")]));
        let numbers: Vec<_> = members(b"1,3,5").map(|m| m.numbers()).collect();
        assert!(!numbers.iter().any(|&(lo, hi)| (lo..=hi).contains(&2)));
        // The hull still spans the holes: it is what the sequence sorts by.
        assert_eq!(v("1,3,5"), well(1, 5));
    }

    #[test]
    fn the_rtl_mark_separates_members_too() {
        assert_eq!(covers("1\u{200F}-3"), pairs(&[("1", "3")]));
        assert_eq!(covers("1\u{200F},3"), pairs(&[("1", "1"), ("3", "3")]));
    }

    #[test]
    fn a_backwards_member_is_kept_as_written() {
        assert_eq!(covers("3-1"), pairs(&[("3", "1")]));
        assert_eq!(members(b"3-1").next().unwrap().numbers(), (1, 3));
    }

    #[test]
    fn a_malformed_designator_covers_nothing() {
        for junk in ["", "?", "01", "1-", "1,", "1a2", "1 2"] {
            assert_eq!(members(junk.as_bytes()).count(), 0, "{junk:?}");
        }
    }

    #[test]
    fn range_reports_the_pair_or_nothing() {
        assert_eq!(v("12-14").range(), Some((12, 14)));
        assert_eq!(v("?").range(), None);
    }
}
