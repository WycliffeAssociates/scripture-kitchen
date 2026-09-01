//! Level 1a deterministic hygiene: byte states no writing system admits.
//!
//! ```text
//! scan("one\0\0\0two\r\nthree\rfour \\ five\n<<<<<<< HEAD\n")
//!   → C0Control            3..6    run 3     // one finding for the NUL run
//!     StrayCarriageReturn  16..17  run 1     // the CRLF at 9..11 is silent
//!     StrandedBackslash    22..23  run 1
//!     ConflictMarker       29..41  run 1     // the whole marker line
//! ```
//!
//! Rule contract (rules.md):
//! 1. Observation: maximal same-class runs of C0 controls (bar tab, LF, and
//!    the CR of CRLF), DEL, C1 controls, U+FFFD, stray CR, backslashes, and
//!    line-initial merge-conflict markers in projected content.
//! 2. Claim: "this content contains bytes that are mechanically suspect"
//!    — a hit inside analyzable text, not a language judgment.
//! 3. Not established: intent. A tab-separated table or an escaped
//!    backslash convention is content the reviewer may accept.
//! 4. Deterministic lane: enable/disable only; no floor, no bands.
//! 5. Per chapter: the scan over one chapter's masked text is the whole
//!    observation; there is no reduce, so Galley may cache the rows by
//!    chapter content. `Carry = ()`: a run is maximal within its chapter,
//!    and a run abutting a masked `\c` marker is two findings by design.
//! 6. Config: none changes observations; enablement only filters.
//! 7. Wire: class and run length ride the two payload lanes; the span is the
//!    exact run.
//! 8. Pinned: the 223-NUL run, CRLF/stray CR, masked-vs-stranded backslash,
//!    and the conflict marker, in this module and the Onion adapter tests.
//!
//! Checks needing the Unicode classifier — combining mark without a base,
//! misplaced NBSP/format characters, noncharacters beyond U+FFFD — wait for
//! Stage 1's classifier and are not approximated here.
//!
//! Scan shape, measured against the roofline bench: an autovectorized
//! range filter for C0/DEL, `memchr3` for the lead bytes `\`, `C2`, `EF`,
//! and a second `memchr3` for marker lead bytes. Clean text never leaves
//! the fast paths.

use crate::{
    BookIndex, CodecError, FindingKind, HygieneClass, HygieneDigest, PackedFinding, TextRange,
};

/// One maximal run in projected-book UTF-8 coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HygieneFinding {
    class: HygieneClass,
    span: TextRange,
    run: u32,
}

impl HygieneFinding {
    pub const fn class(self) -> HygieneClass {
        self.class
    }

    pub const fn span(self) -> TextRange {
        self.span
    }

    /// Code points (or marker lines) in the run.
    pub const fn run(self) -> u32 {
        self.run
    }

    pub fn to_packed(
        self,
        book: BookIndex,
        book_lengths: &[u32],
    ) -> Result<PackedFinding, CodecError> {
        PackedFinding::new(
            self.span.from(),
            self.span.to(),
            book,
            FindingKind::Hygiene(HygieneDigest::new(self.class, self.run)?),
            book_lengths,
        )
    }
}

/// Scan one projected book; findings come back ordered by start offset.
pub fn scan(text: &str) -> Vec<HygieneFinding> {
    let mut out = Vec::new();
    scan_into(text, &mut out);
    out
}

pub fn scan_into(text: &str, out: &mut Vec<HygieneFinding>) {
    let start = out.len();
    let bytes = text.as_bytes();
    scan_controls(bytes, out);
    scan_needles(bytes, out);
    scan_conflict_markers(bytes, out);
    out[start..].sort_by_key(|finding| (finding.span.from(), finding.class as u8));
    debug_assert!(out[start..].iter().all(|finding| {
        text.is_char_boundary(finding.span.from() as usize)
            && text.is_char_boundary(finding.span.to() as usize)
    }));
}

const BLOCK: usize = 64;

/// A byte the range filter must stop on: C0 other than tab/LF, or DEL.
#[inline(always)]
fn is_control(b: u8) -> bool {
    (b < 0x20 && b != b'\n' && b != b'\t') || b == 0x7f
}

fn scan_controls(bytes: &[u8], out: &mut Vec<HygieneFinding>) {
    let mut at = 0;
    while at < bytes.len() {
        let end = (at + BLOCK).min(bytes.len());
        // A pure OR-reduction so the block test vectorizes; the branch sits
        // once per block, not once per byte.
        let hit = bytes[at..end]
            .iter()
            .fold(false, |acc, &b| acc | is_control(b));
        if hit {
            at = classify_controls(bytes, at, end, out);
        } else {
            at = end;
        }
    }
}

/// Slow path over one block. Returns where the fast path resumes, which may
/// pass `end` when a run continues into the next block.
fn classify_controls(
    bytes: &[u8],
    mut at: usize,
    end: usize,
    out: &mut Vec<HygieneFinding>,
) -> usize {
    while at < end {
        let b = bytes[at];
        if !is_control(b) {
            at += 1;
            continue;
        }
        let class = match b {
            0x7f => HygieneClass::Delete,
            b'\r' => {
                if bytes.get(at + 1) == Some(&b'\n') {
                    at += 2;
                    continue;
                }
                HygieneClass::StrayCarriageReturn
            }
            _ => HygieneClass::C0Control,
        };
        let from = at;
        at += 1;
        while at < bytes.len() && same_control_class(bytes, at, class) {
            at += 1;
        }
        push(out, class, from, at, (at - from) as u32);
    }
    at
}

fn same_control_class(bytes: &[u8], at: usize, class: HygieneClass) -> bool {
    let b = bytes[at];
    match class {
        HygieneClass::Delete => b == 0x7f,
        HygieneClass::StrayCarriageReturn => b == b'\r' && bytes.get(at + 1) != Some(&b'\n'),
        _ => b < 0x20 && b != b'\n' && b != b'\t' && b != b'\r',
    }
}

const C1_LEAD: u8 = 0xc2;
const FFFD_LEAD: u8 = 0xef;
const FFFD: [u8; 3] = [0xef, 0xbf, 0xbd];

fn scan_needles(bytes: &[u8], out: &mut Vec<HygieneFinding>) {
    let mut resume = 0;
    for at in memchr::memchr3_iter(b'\\', C1_LEAD, FFFD_LEAD, bytes) {
        // A run already consumed its later needle hits.
        if at < resume {
            continue;
        }
        let Some((class, width)) = needle_class(bytes, at) else {
            continue;
        };
        let mut end = at + width;
        while needle_class(bytes, end) == Some((class, width)) {
            end += width;
        }
        push(out, class, at, end, ((end - at) / width) as u32);
        resume = end;
    }
}

/// The class a needle hit begins, with its byte width, once the following
/// bytes confirm it. `C2` and `EF` also lead ordinary characters.
fn needle_class(bytes: &[u8], at: usize) -> Option<(HygieneClass, usize)> {
    match bytes.get(at)? {
        b'\\' => Some((HygieneClass::StrandedBackslash, 1)),
        &C1_LEAD => (0x80..=0x9f)
            .contains(bytes.get(at + 1)?)
            .then_some((HygieneClass::C1Control, 2)),
        &FFFD_LEAD => {
            (bytes.get(at..at + 3)? == FFFD).then_some((HygieneClass::ReplacementChar, 3))
        }
        _ => None,
    }
}

/// Marker lines start with seven of one byte; `=======` is the whole line,
/// the other two carry a label. One `memchr3` over the three lead bytes
/// replaces three `memmem` passes: `<`, `=`, `>` are rare in scripture text,
/// so this pass runs at needle speed.
fn scan_conflict_markers(bytes: &[u8], out: &mut Vec<HygieneFinding>) {
    let mut resume = 0;
    for at in memchr::memchr3_iter(b'<', b'=', b'>', bytes) {
        if at < resume || (at != 0 && bytes[at - 1] != b'\n') {
            continue;
        }
        let lead = bytes[at];
        let Some(head) = bytes.get(at..at + 7) else {
            break;
        };
        if head.iter().any(|&b| b != lead) {
            continue;
        }
        let after = bytes.get(at + 7).copied();
        let well_formed = match lead {
            b'=' => matches!(after, None | Some(b'\n' | b'\r')),
            _ => after == Some(b' '),
        };
        if !well_formed {
            continue;
        }
        let end =
            memchr::memchr2(b'\n', b'\r', &bytes[at + 7..]).map_or(bytes.len(), |n| at + 7 + n);
        push(out, HygieneClass::ConflictMarker, at, end, 1);
        resume = end;
    }
}

fn push(out: &mut Vec<HygieneFinding>, class: HygieneClass, from: usize, to: usize, run: u32) {
    out.push(HygieneFinding {
        class,
        span: TextRange::new(from as u32, to as u32).expect("runs advance forward"),
        run,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str) -> Vec<(HygieneClass, u32, u32, u32)> {
        scan(text)
            .into_iter()
            .map(|f| (f.class, f.span.from(), f.span.to(), f.run))
            .collect()
    }

    #[test]
    fn module_doc_example_is_exact() {
        let text = "one\0\0\0two\r\nthree\rfour \\ five\n<<<<<<< HEAD\n";
        assert_eq!(
            rows(text),
            vec![
                (HygieneClass::C0Control, 3, 6, 3),
                (HygieneClass::StrayCarriageReturn, 16, 17, 1),
                (HygieneClass::StrandedBackslash, 22, 23, 1),
                (HygieneClass::ConflictMarker, 29, 41, 1),
            ]
        );
    }

    #[test]
    fn two_hundred_twenty_three_nuls_produce_one_finding_spanning_the_run() {
        // The run starts mid-block and crosses two block edges.
        let text = format!("{}{}tail", "x".repeat(50), "\0".repeat(223));
        assert_eq!(rows(&text), vec![(HygieneClass::C0Control, 50, 273, 223)]);
    }

    #[test]
    fn crlf_is_silent_and_a_stray_cr_is_reported() {
        assert!(rows("a\r\nb\r\n").is_empty());
        assert_eq!(
            rows("a\rb"),
            vec![(HygieneClass::StrayCarriageReturn, 1, 2, 1)]
        );
        // `\r\r\n`: the first CR is stray, the second closes a CRLF.
        assert_eq!(
            rows("a\r\r\nb"),
            vec![(HygieneClass::StrayCarriageReturn, 1, 2, 1)]
        );
        assert_eq!(
            rows("\r\r"),
            vec![(HygieneClass::StrayCarriageReturn, 0, 2, 2)]
        );
    }

    #[test]
    fn tab_and_lf_pass_while_other_c0_and_del_are_runs_by_class() {
        assert!(rows("a\tb\nc").is_empty());
        assert_eq!(
            rows("a\x01\x02\x7f\x7fb"),
            vec![
                (HygieneClass::C0Control, 1, 3, 2),
                (HygieneClass::Delete, 3, 5, 2)
            ]
        );
    }

    #[test]
    fn c1_and_replacement_runs_confirm_their_continuation_bytes() {
        // © (C2 A9) and ¿ (C2 BF) share the C1 lead byte; only C2 80..9F is C1.
        assert!(rows("©Â¿").is_empty());
        assert_eq!(
            rows("a\u{85}\u{9f}b"),
            vec![(HygieneClass::C1Control, 1, 5, 2)]
        );
        assert_eq!(
            rows("a\u{fffd}\u{fffd}b"),
            vec![(HygieneClass::ReplacementChar, 1, 7, 2)]
        );
        // EF leads most of the BMP's upper range; a non-FFFD EF is silent.
        assert!(rows("\u{ff01}\u{fefe}").is_empty());
    }

    #[test]
    fn backslashes_in_content_are_reported_as_a_run() {
        assert_eq!(
            rows("a\\\\b"),
            vec![(HygieneClass::StrandedBackslash, 1, 3, 2)]
        );
    }

    #[test]
    fn conflict_markers_must_be_line_initial_and_equals_must_fill_the_line() {
        let text = "a\n<<<<<<< ours\nx\n=======\ny\n>>>>>>> theirs\n";
        assert_eq!(
            rows(text),
            vec![
                (HygieneClass::ConflictMarker, 2, 14, 1),
                (HygieneClass::ConflictMarker, 17, 24, 1),
                (HygieneClass::ConflictMarker, 27, 41, 1),
            ]
        );
        assert!(rows("a <<<<<<< b\n======= c\n").is_empty());
        assert_eq!(
            rows("=======\r\n"),
            vec![(HygieneClass::ConflictMarker, 0, 7, 1)]
        );
        assert_eq!(
            rows(">>>>>>> end"),
            vec![(HygieneClass::ConflictMarker, 0, 11, 1)]
        );
    }

    #[test]
    fn packed_rows_carry_class_run_and_span() {
        let finding = scan("\0\0")[0];
        let packed = finding.to_packed(BookIndex::new(0).unwrap(), &[2]).unwrap();
        assert_eq!((packed.from(), packed.to()), (0, 2));
        let FindingKind::Hygiene(digest) = packed.kind() else {
            panic!("hygiene kind")
        };
        assert_eq!((digest.class(), digest.run()), (HygieneClass::C0Control, 2));
    }

    #[test]
    fn clean_multilingual_text_is_silent() {
        assert!(rows("In the beginning\tκαὶ ὁ λόγος\nበመጀመሪያ 🧅 अथ\n").is_empty());
    }
}
