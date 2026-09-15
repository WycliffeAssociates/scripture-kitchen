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
//! One finding per maximal same-class run, every span snapped out to
//! grapheme-atom edges. A chapter is an edge of text, so a run is maximal
//! within its chapter. The four scalar classes ride the substrate walk
//! instead: `ScalarSites` is the machine it drives, and `substrate.rs`
//! publishes the lane.
//!
//! What each class claims and when it stays silent: `rules/hygiene.md`.
//! Scan shape, throughput, and the lone-backslash caveat: hygiene.md.

use crate::pass::{ChapterInput, ChapterObs, ChapterPass, Findings, SchemaStamp};
use mise::unicode::{Class, bits};

use crate::unicode::atoms::widen_to_atoms;
use crate::{
    BookIndex, CodecError, FindingKind, HygieneClass, HygieneDigest, PackedFinding, TextRange,
};

/// The Level 1a byte sweeps: one scan per chapter, no seam state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HygieneBytes;

impl ChapterPass for HygieneBytes {
    type Observation = Vec<HygieneFinding>;
    type Aggregate = Box<[HygieneFinding]>;
    type Config = ();
    const SCHEMA: SchemaStamp = SchemaStamp::new(2);

    fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
        scan(chapter.text)
    }

    /// A run abutting a masked `\c` is two findings, one per chapter.
    fn fold(&self, book: &[ChapterObs<&Self::Observation>]) -> Self::Aggregate {
        book.iter()
            .flat_map(|chapter| {
                chapter
                    .obs
                    .iter()
                    .map(|finding| finding.rebased(chapter.start))
            })
            .collect()
    }

    fn judge(&self, corpus: &[&Self::Aggregate], _config: &(), out: &mut Findings) {
        for (index, book) in corpus.iter().enumerate() {
            out.open_book(BookIndex::new(index).expect("a corpus indexes every book"));
            for finding in book.iter() {
                finding.push_into(out);
            }
        }
    }

    /// The fat pointer plus the boxed slice's own bytes.
    fn aggregate_bytes(&self, aggregate: &Self::Aggregate) -> usize {
        size_of::<Self::Aggregate>() + size_of_val(&**aggregate)
    }

    /// Capacity, not length: the `Vec` `scan` handed back keeps what it grew.
    fn observation_bytes(&self, observation: &Self::Observation) -> usize {
        size_of::<Self::Observation>() + observation.capacity() * size_of::<HygieneFinding>()
    }
}

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

    /// Offending code points in the run; a marker line counts as one. The
    /// span may be one atom wider after grapheme snapping.
    pub const fn run(self) -> u32 {
        self.run
    }

    /// The same finding in book coordinates, given its chapter's start.
    pub(crate) fn rebased(self, start: u32) -> Self {
        Self {
            span: TextRange::new(self.span.from() + start, self.span.to() + start)
                .expect("a rebased chapter span keeps its order"),
            ..self
        }
    }

    /// Pushes this book-coordinate finding into the open book.
    pub(crate) fn push_into(self, out: &mut Findings) {
        out.push(
            self.span,
            FindingKind::Hygiene(
                HygieneDigest::new(self.class, self.run).expect("a scanned run is never empty"),
            ),
        )
        .expect("a folded span lies inside the book it came from");
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

/// Scan one projected book for the byte classes; findings come back ordered
/// by start offset. The four scalar classes ride `substrate::Substrate`.
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
        // A pure OR-reduction: the block test vectorizes and the branch
        // sits once per block.
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

/// A line-initial run of three or more `<`, `=`, or `>` is a marker line.
/// Git writes seven, but a truncated or half-resolved conflict leaves fewer,
/// so the check accepts the short form; the whole line is the span.
fn scan_conflict_markers(text: &str, out: &mut Vec<HygieneFinding>) {
    let bytes = text.as_bytes();
    let mut resume = 0;
    for at in memchr::memchr3_iter(b'<', b'=', b'>', bytes) {
        if at < resume || (at != 0 && bytes[at - 1] != b'\n') {
            continue;
        }
        let lead = bytes[at];
        let run = bytes[at..].iter().take_while(|&&b| b == lead).count();
        if run < 3 {
            continue;
        }
        let end =
            memchr::memchr2(b'\n', b'\r', &bytes[at + run..]).map_or(bytes.len(), |n| at + run + n);
        push(text, out, HygieneClass::ConflictMarker, at, end, 1);
        resume = end;
    }
}

// ── Scalar-level sites ──────────────────────────────────────────────────

/// The bits that put a scalar on the site machine. All live above U+007F, so
/// the substrate walk's ASCII lane can skip whole words.
pub(crate) const SUSPECT: u16 = bits::MARK | bits::FORMAT | bits::NONCHARACTER;
/// U+00A0, the one suspect scalar no bit names.
pub(crate) const NBSP: u32 = 0xA0;

/// A suspect whose verdict needs the scalar after it.
struct Pending {
    at: usize,
    width: usize,
    cp: u32,
    class: Class,
    prev: Option<Class>,
}

/// The open maximal run, in exact (unwidened) chapter coordinates.
struct Run {
    class: HygieneClass,
    from: usize,
    to: usize,
    len: u32,
}

/// The four scalar classes as a streaming machine the substrate walk drives.
///
/// A chapter end is an edge of text, so a run never crosses a masked `\c`.
pub(crate) struct ScalarSites {
    sites: Vec<(HygieneClass, usize, usize, u32)>,
    run: Option<Run>,
    pending: Option<Pending>,
}

impl ScalarSites {
    pub(crate) const fn new() -> Self {
        Self {
            sites: Vec::new(),
            run: None,
            pending: None,
        }
    }

    /// A deferred verdict is waiting for the next scalar, whatever it is.
    #[inline(always)]
    pub(crate) const fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// One scalar in; a deferred verdict may settle and a run may close.
    ///
    /// `at` is the chapter byte offset; `prev` is `None` at chapter start.
    #[inline]
    pub(crate) fn step(
        &mut self,
        at: usize,
        width: usize,
        cp: u32,
        class: Class,
        prev: Option<Class>,
    ) {
        if let Some(held) = self.pending.take() {
            self.settle(held, Some(class));
        }
        self.open(at, width, cp, class, prev);
    }

    /// Chapter end: a pending verdict resolves against no next scalar, the
    /// open run closes, and every span widens to atom edges.
    pub(crate) fn finish(mut self, text: &str) -> Box<[HygieneFinding]> {
        if let Some(held) = self.pending.take() {
            self.settle(held, None);
        }
        self.close();
        self.sites
            .iter()
            .map(|&(class, from, to, run)| {
                let exact = TextRange::new(from as u32, to as u32).expect("runs advance forward");
                HygieneFinding {
                    class,
                    span: widen_to_atoms(text, exact),
                    run,
                }
            })
            .collect()
    }

    /// Extends the open run, or decides this scalar on its own.
    fn open(&mut self, at: usize, width: usize, cp: u32, class: Class, prev: Option<Class>) {
        if let Some(run) = &self.run {
            // Members are contiguous, so a gap is an interruption.
            if run.to == at {
                match run.class {
                    HygieneClass::FreeCombiningMark if class.is_mark() => {
                        return self.extend(width);
                    }
                    HygieneClass::Noncharacter if class.is_noncharacter() => {
                        return self.extend(width);
                    }
                    HygieneClass::NoBreakSpace if cp == NBSP => return self.extend(width),
                    // Membership needs this format's own `next`.
                    HygieneClass::MisplacedFormat if class.is_format() => {
                        self.defer(at, width, cp, class, prev);
                        return;
                    }
                    _ => {}
                }
            }
            self.close();
        }

        if class.is_noncharacter() {
            self.start(HygieneClass::Noncharacter, at, width);
        } else if class.is_mark() {
            if mark_is_free(prev) {
                // Every mark after the first is equally baseless: one finding.
                self.start(HygieneClass::FreeCombiningMark, at, width);
            }
        } else if class.is_format() || cp == NBSP {
            self.defer(at, width, cp, class, prev);
        }
    }

    /// Resolves a deferred format or NBSP now that its `next` is known.
    fn settle(&mut self, held: Pending, next: Option<Class>) {
        if held.cp == NBSP {
            if nbsp_is_suspect(held.prev, next) {
                self.start(HygieneClass::NoBreakSpace, held.at, held.width);
            }
            return;
        }
        if format_is_placed(held.class, held.prev, next) {
            self.close();
            return;
        }
        match &self.run {
            Some(run) if run.class == HygieneClass::MisplacedFormat && run.to == held.at => {
                self.extend(held.width);
            }
            _ => {
                self.close();
                self.start(HygieneClass::MisplacedFormat, held.at, held.width);
            }
        }
    }

    fn defer(&mut self, at: usize, width: usize, cp: u32, class: Class, prev: Option<Class>) {
        self.pending = Some(Pending {
            at,
            width,
            cp,
            class,
            prev,
        });
    }

    fn start(&mut self, class: HygieneClass, at: usize, width: usize) {
        debug_assert!(self.run.is_none(), "a run opens only where none is open");
        self.run = Some(Run {
            class,
            from: at,
            to: at + width,
            len: 1,
        });
    }

    fn extend(&mut self, width: usize) {
        let run = self.run.as_mut().expect("an extended run is open");
        run.to += width;
        run.len += 1;
    }

    fn close(&mut self) {
        if let Some(run) = self.run.take() {
            self.sites.push((run.class, run.from, run.to, run.len));
        }
    }
}

/// Only a mark with nothing behind it, or with something that cannot carry a
/// mark, is reportable. A decomposed grapheme has a base.
fn mark_is_free(prev: Option<Class>) -> bool {
    match prev {
        None => true,
        Some(prev) => {
            prev.is_whitespace() || prev.is_control() || (prev.is_format() && !prev.is_glue())
        }
    }
}

/// A `Cf` scalar is placed when it does the one job its class defines: joins
/// two letters (ZWJ/ZWNJ), or introduces the scalar after it (GCB Prepend).
fn format_is_placed(class: Class, prev: Option<Class>, next: Option<Class>) -> bool {
    let joinable = |class: Class| class.is_alphabetic() || class.is_mark();
    if class.is_extender() {
        return prev.is_some_and(joinable) && next.is_some_and(joinable);
    }
    if class.is_prepend() {
        return next.is_some_and(|next| joinable(next) || next.is_decimal_digit());
    }
    false
}

/// NBSP claims nothing about typography. It is reportable only where it
/// cannot be doing its job: beside whitespace, or at an edge of the text.
fn nbsp_is_suspect(prev: Option<Class>, next: Option<Class>) -> bool {
    match (prev, next) {
        (None, _) | (_, None) => true,
        (Some(prev), Some(next)) => prev.is_whitespace() || next.is_whitespace(),
    }
}

/// Classes whose scalars are GCB Control, CR, or LF: UAX #29 breaks on both
/// edges, so widening provably cannot move them. U+FFFD and a backslash are
/// ordinary bases and may legitimately widen.
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
    fn conflict_markers_are_line_initial_runs_of_three_or_more() {
        let text = "a\n<<<<<<< ours\nx\n=======\ny\n>>>>>>> theirs\n";
        assert_eq!(
            rows(text),
            vec![
                (HygieneClass::ConflictMarker, 2, 14, 1),
                (HygieneClass::ConflictMarker, 17, 24, 1),
                (HygieneClass::ConflictMarker, 27, 41, 1),
            ]
        );
        assert!(rows("a <<<<<<< b\n== c\n>> d\n").is_empty());
        assert_eq!(
            rows("=======\r\n"),
            vec![(HygieneClass::ConflictMarker, 0, 7, 1)]
        );
        // Truncated or half-resolved markers still count from three bytes.
        assert_eq!(
            rows("<<< ours\n=== theirs\n"),
            vec![
                (HygieneClass::ConflictMarker, 0, 8, 1),
                (HygieneClass::ConflictMarker, 9, 19, 1),
            ]
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
