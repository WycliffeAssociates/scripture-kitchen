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
//! ```text
//! scan("a \u{301} \u{feff}b\u{a0}\u{a0}c\u{fdd0}")
//!   → FreeCombiningMark   1..4    run 1   // no base; the span takes the space it hangs on
//!     MisplacedFormat     5..8    run 1   // a stray BOM mid-text
//!     NoBreakSpace        9..13   run 2   // NBSP beside NBSP
//!     Noncharacter        14..17  run 1
//! ```
//!
//! Rule contract (rules.md):
//! 1. Observation: maximal same-class runs of C0 controls (bar tab, LF, and
//!    the CR of CRLF), DEL, C1 controls, U+FFFD, stray CR, backslashes,
//!    line-initial merge-conflict markers, combining marks with no base,
//!    misplaced format characters, NBSP beside whitespace, and
//!    noncharacters, in projected content.
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
//! The four classifier-dependent checks make only deterministic claims. A
//! decomposed grapheme's mark has a base and stays silent; ZWJ/ZWNJ between
//! letters stays silent; NBSP inside a phrase stays silent, because none of
//! those is mechanically wrong. NBSP's "leading or trailing a verse" case
//! reads the edges of the analyzed text, which is the whole book until the
//! verse-aware walk lands.
//!
//! Every emitted span passes through `widen_to_atoms`, so a finding never
//! splits a rendered grapheme (charter invariant 6). The five control-shaped
//! classes provably cannot move; U+FFFD and a stranded backslash are
//! ordinary bases, so a following combining mark legitimately joins them.
//!
//! Scan shape, measured against the roofline bench: an autovectorized
//! range filter for C0/DEL, `memchr3` for the lead bytes `\`, `C2`, `EF`,
//! a second `memchr3` for marker lead bytes, and an eight-byte SWAR ASCII
//! skip ahead of the byte trie for the scalar pass. Clean text never leaves
//! the fast paths.

use crate::unicode::{Class, atoms::widen_to_atoms, bits, class_of, lookup::trie_at};
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

    /// Offending code points in the run (marker lines count as one). The
    /// span may be one atom wider after grapheme snapping.
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
    scan_controls(text, out);
    scan_needles(text, out);
    scan_conflict_markers(text, out);
    scan_scalars(text, out);
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

fn scan_controls(text: &str, out: &mut Vec<HygieneFinding>) {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let end = (at + BLOCK).min(bytes.len());
        // A pure OR-reduction so the block test vectorizes; the branch sits
        // once per block, not once per byte.
        let hit = bytes[at..end]
            .iter()
            .fold(false, |acc, &b| acc | is_control(b));
        if hit {
            at = classify_controls(text, at, end, out);
        } else {
            at = end;
        }
    }
}

/// Slow path over one block. Returns where the fast path resumes, which may
/// pass `end` when a run continues into the next block.
fn classify_controls(
    text: &str,
    mut at: usize,
    end: usize,
    out: &mut Vec<HygieneFinding>,
) -> usize {
    let bytes = text.as_bytes();
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
        push(text, out, class, from, at, (at - from) as u32);
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

fn scan_needles(text: &str, out: &mut Vec<HygieneFinding>) {
    let bytes = text.as_bytes();
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
        push(text, out, class, at, end, ((end - at) / width) as u32);
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
fn scan_conflict_markers(text: &str, out: &mut Vec<HygieneFinding>) {
    let bytes = text.as_bytes();
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
        push(text, out, HygieneClass::ConflictMarker, at, end, 1);
        resume = end;
    }
}

// ── Scalar-level checks ─────────────────────────────────────────────────

/// The bits that put a scalar on the slow path. Every one of them lives
/// above U+007F, which is what lets the ASCII lane skip whole words.
const SUSPECT: u16 = bits::MARK | bits::FORMAT | bits::NONCHARACTER;
const NBSP: [u8; 2] = [0xc2, 0xa0];
const HIGH_BITS: u64 = 0x8080_8080_8080_8080;
/// Consecutive ASCII scalars before the eight-byte lane re-arms. Without
/// hysteresis the chunk test costs Indic and Greek text more than it saves.
const REARM_AFTER: u32 = 32;

fn scan_scalars(text: &str, out: &mut Vec<HygieneFinding>) {
    let bytes = text.as_bytes();
    let (mut at, mut armed, mut ascii_run) = (0usize, true, 0u32);
    while at < bytes.len() {
        if armed && at + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"));
            if word & HIGH_BITS == 0 {
                at += 8;
                continue;
            }
            armed = false;
            ascii_run = 0;
        }
        let (class, width) = trie_at(&bytes[at..]);
        at = if class.bits() & SUSPECT != 0 {
            ascii_run = 0;
            suspect_run(text, at, class, out)
        } else if bytes[at..].starts_with(&NBSP) && nbsp_is_suspect(text, at) {
            ascii_run = 0;
            scalar_run(text, at, HygieneClass::NoBreakSpace, out, |text, at| {
                text.as_bytes()[at..].starts_with(&NBSP)
            })
        } else {
            if width == 1 {
                ascii_run += 1;
                armed |= ascii_run >= REARM_AFTER;
            } else {
                ascii_run = 0;
            }
            at + width
        };
    }
}

/// Dispatches one MARK / FORMAT / NONCHARACTER scalar, returning where the
/// walk resumes.
fn suspect_run(text: &str, at: usize, class: Class, out: &mut Vec<HygieneFinding>) -> usize {
    if class.is_noncharacter() {
        return scalar_run(text, at, HygieneClass::Noncharacter, out, |text, at| {
            class_at(text, at).is_noncharacter()
        });
    }
    if class.is_mark() {
        if !mark_is_free(text, at) {
            return at + class_width(text, at);
        }
        // Every mark after the first is equally baseless, so the run is one
        // finding.
        return scalar_run(text, at, HygieneClass::FreeCombiningMark, out, |text, at| {
            class_at(text, at).is_mark()
        });
    }
    if format_is_placed(text, at, class) {
        return at + class_width(text, at);
    }
    scalar_run(text, at, HygieneClass::MisplacedFormat, out, |text, at| {
        let class = class_at(text, at);
        class.is_format() && !format_is_placed(text, at, class)
    })
}

/// "A decomposed grapheme's combining mark is not a free mark" (rules.md):
/// only a mark with nothing behind it, or with something that cannot carry
/// a mark, is reportable.
fn mark_is_free(text: &str, at: usize) -> bool {
    match prev_class(text, at) {
        None => true,
        Some(prev) => {
            prev.is_whitespace() || prev.is_control() || (prev.is_format() && !prev.is_glue())
        }
    }
}

/// A `Cf` scalar is placed when it is doing the one job its class defines:
/// joining two letters (ZWJ/ZWNJ, which rules.md requires stay silent in
/// Indic text) or introducing the scalar after it (Grapheme_Cluster_Break
/// Prepend). Everything else — a stray BOM, a bidi control adrift in a
/// verse — is reportable.
fn format_is_placed(text: &str, at: usize, class: Class) -> bool {
    let joinable = |class: Class| class.is_alphabetic() || class.is_mark();
    if class.is_extender() {
        return prev_class(text, at).is_some_and(joinable)
            && next_class(text, at).is_some_and(joinable);
    }
    if class.is_prepend() {
        return next_class(text, at)
            .is_some_and(|next| joinable(next) || next.is_decimal_digit());
    }
    false
}

/// NBSP claims nothing about typography: it is reportable only where it
/// cannot be doing its job — beside other whitespace, or at an edge of the
/// analyzed text.
fn nbsp_is_suspect(text: &str, at: usize) -> bool {
    match (prev_class(text, at), next_class(text, at)) {
        (None, _) | (_, None) => true,
        (Some(prev), Some(next)) => prev.is_whitespace() || next.is_whitespace(),
    }
}

fn class_at(text: &str, at: usize) -> Class {
    trie_at(&text.as_bytes()[at..]).0
}

fn class_width(text: &str, at: usize) -> usize {
    trie_at(&text.as_bytes()[at..]).1
}

fn prev_class(text: &str, at: usize) -> Option<Class> {
    text[..at].chars().next_back().map(class_of)
}

/// The scalar after the one starting at `at`.
fn next_class(text: &str, at: usize) -> Option<Class> {
    text[at..].chars().nth(1).map(class_of)
}

/// Consumes the maximal run of scalars `member` accepts and pushes one
/// finding over it.
fn scalar_run(
    text: &str,
    at: usize,
    class: HygieneClass,
    out: &mut Vec<HygieneFinding>,
    member: impl Fn(&str, usize) -> bool,
) -> usize {
    let mut end = at;
    let mut run = 0u32;
    while end < text.len() && member(text, end) {
        end += class_width(text, end);
        run += 1;
    }
    push(text, out, class, at, end, run);
    end
}

/// Classes whose scalars are Grapheme_Cluster_Break Control, CR, or LF, so
/// UAX #29 breaks on both of their edges and widening provably cannot move
/// them. U+FFFD and a backslash are ordinary bases; a combining mark behind
/// one legitimately joins it, and that is invariant 6 working, not drift.
const UNMOVABLE: [HygieneClass; 5] = [
    HygieneClass::C0Control,
    HygieneClass::Delete,
    HygieneClass::C1Control,
    HygieneClass::StrayCarriageReturn,
    HygieneClass::ConflictMarker,
];

fn push(
    text: &str,
    out: &mut Vec<HygieneFinding>,
    class: HygieneClass,
    from: usize,
    to: usize,
    run: u32,
) {
    let exact = TextRange::new(from as u32, to as u32).expect("runs advance forward");
    let span = widen_to_atoms(text, exact);
    debug_assert!(
        !UNMOVABLE.contains(&class) || span == exact,
        "{class:?} at {from}..{to} moved to {}..{} under widening",
        span.from(),
        span.to()
    );
    out.push(HygieneFinding { class, span, run });
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

    #[test]
    fn scalar_doc_example_is_exact() {
        let text = "a \u{301} \u{feff}b\u{a0}\u{a0}c\u{fdd0}";
        assert_eq!(
            rows(text),
            vec![
                (HygieneClass::FreeCombiningMark, 1, 4, 1),
                (HygieneClass::MisplacedFormat, 5, 8, 1),
                (HygieneClass::NoBreakSpace, 9, 13, 2),
                (HygieneClass::Noncharacter, 14, 17, 1),
            ]
        );
    }

    #[test]
    fn a_decomposed_graphemes_combining_mark_is_not_a_free_mark() {
        // rules.md, Level 1a required examples.
        assert!(rows("e\u{301}tait").is_empty());
        assert!(rows("\u{3b1}\u{314}\u{301}").is_empty());
        // Marks stacked on a real base stay silent however deep.
        assert!(rows("a\u{301}\u{308}\u{327}").is_empty());
    }

    #[test]
    fn a_bare_combining_mark_after_a_space_or_at_the_start_is_reported() {
        assert_eq!(
            rows("word \u{301}\u{308} next"),
            vec![(HygieneClass::FreeCombiningMark, 4, 9, 2)]
        );
        assert_eq!(
            rows("\u{301}word"),
            vec![(HygieneClass::FreeCombiningMark, 0, 2, 1)]
        );
        // A mark behind a control has no base either; the control run keeps
        // its own exact span.
        assert_eq!(
            rows("a\0\u{301}b"),
            vec![
                (HygieneClass::C0Control, 1, 2, 1),
                (HygieneClass::FreeCombiningMark, 2, 4, 1),
            ]
        );
    }

    #[test]
    fn zwj_and_zwnj_between_letters_are_silent() {
        // rules.md: ZWJ/ZWNJ in Indic text never enters the inventory.
        assert!(rows("\u{915}\u{94d}\u{200d}\u{937}").is_empty());
        assert!(rows("\u{915}\u{94d}\u{200c}\u{937}").is_empty());
        assert!(rows("\u{62a}\u{200c}\u{62a}").is_empty());
        // The same joiner with nothing to join is reportable.
        assert_eq!(
            rows("\u{200d} a"),
            vec![(HygieneClass::MisplacedFormat, 0, 3, 1)]
        );
    }

    #[test]
    fn a_stray_byte_order_mark_mid_text_is_reported() {
        assert_eq!(
            rows("in the\u{feff} beginning"),
            vec![(HygieneClass::MisplacedFormat, 6, 9, 1)]
        );
        // An Arabic number sign is Prepend: introducing a digit is its job.
        assert!(rows("\u{600}7").is_empty());
        // The span takes the space with it: a Prepend owns what follows.
        assert_eq!(
            rows("\u{600} "),
            vec![(HygieneClass::MisplacedFormat, 0, 3, 1)]
        );
    }

    #[test]
    fn noncharacters_are_reported_as_runs() {
        assert_eq!(
            rows("a\u{fdd0}\u{fdd1}b"),
            vec![(HygieneClass::Noncharacter, 1, 7, 2)]
        );
        assert_eq!(
            rows("a\u{ffff}b"),
            vec![(HygieneClass::Noncharacter, 1, 4, 1)]
        );
        assert_eq!(
            rows("a\u{10fffe}b"),
            vec![(HygieneClass::Noncharacter, 1, 5, 1)]
        );
        // U+FFFD keeps its own class and does not become a noncharacter.
        assert_eq!(
            rows("a\u{fffd}b"),
            vec![(HygieneClass::ReplacementChar, 1, 4, 1)]
        );
    }

    #[test]
    fn nbsp_speaks_only_where_the_claim_is_deterministic() {
        // French spacing around punctuation is convention, not damage.
        assert!(rows("Jésus\u{a0}: parle").is_empty());
        assert!(rows("\u{ab}\u{a0}mot\u{a0}\u{bb}").is_empty());
        assert_eq!(
            rows("word \u{a0}next"),
            vec![(HygieneClass::NoBreakSpace, 5, 7, 1)]
        );
        assert_eq!(
            rows("word\u{a0} next"),
            vec![(HygieneClass::NoBreakSpace, 4, 6, 1)]
        );
        assert_eq!(
            rows("\u{a0}word"),
            vec![(HygieneClass::NoBreakSpace, 0, 2, 1)]
        );
        assert_eq!(
            rows("word\u{a0}"),
            vec![(HygieneClass::NoBreakSpace, 4, 6, 1)]
        );
    }

    #[test]
    fn every_emitted_span_lies_on_atom_boundaries() {
        let text = "a\u{301}\0\u{301} \u{a0}\u{a0}\u{fdd0}\\\u{feff}\r\n\u{915}\u{94d}\u{937}";
        for finding in scan(text) {
            let span = finding.span();
            assert_eq!(
                crate::unicode::atoms::widen_to_atoms(text, span),
                span,
                "{:?} at {}..{} is not atom-aligned",
                finding.class(),
                span.from(),
                span.to()
            );
        }
    }
}
