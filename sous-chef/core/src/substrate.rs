//! Level 1b substrate: one scalar walk per chapter, one detached count row.
//!
//! ```text
//! map("He said, \u{201C}Go.\u{201D} 12,345")           // D is the pooled digit key
//!   scalars  ' '×3  ','×2  '.'  '\u{201C}'  '\u{201D}'  'H' 'e' 'a' …  D×5
//!   pairs    (',', Letter, Space)  ('.', Letter, Nonletter)  (D, Digit, Edge)
//!            ('\u{201C}', Space, Letter)  ('\u{201D}', Nonletter, Space)  (',', Digit, Digit)
//!   runs     [',']  ['.', '\u{201D}']  ['\u{201C}']  [D, D, ',', D, D, D]
//!   follows  '\u{201C}' → upper 1                    // its run ends before `G`
//!   lead     Letter, upper                        trail  Digit, D open both ways
//!   counts   21 scalars, 5 words
//! ```
//!
//! ```text
//! map("a \u{301} \u{feff}b\u{a0}\u{a0}c\u{fdd0}").hygiene()
//!   → FreeCombiningMark   1..4    run 1   // no base; the span takes the space it hangs on
//!     MisplacedFormat     5..8    run 1   // a stray BOM mid-text
//!     NoBreakSpace        9..13   run 2   // NBSP beside NBSP
//!     Noncharacter        14..17  run 1
//! ```
//!
//! Every lane is a sorted vector of plain scalars, so a fold merges two of
//! them in one pass and a host may cache one under its chapter key. The
//! layout table, the interning argument, and the seam argument: substrate.md.

use rustc_hash::FxHashMap;

use crate::BookIndex;
use crate::hygiene::{HygieneFinding, NBSP, SUSPECT, ScalarSites};
use crate::pass::{ChapterInput, ChapterObs, ChapterPass, Findings, SchemaStamp};
use crate::unicode::{
    Class,
    lookup::{ascii_class, trie_at},
};

// ── Keys ────────────────────────────────────────────────────────────────

/// Raw value of [`ScalarKey::DIGITS`]; not a scalar, so it cannot collide.
const DIGITS_RAW: u32 = u32::MAX;

/// One scalar as a count key, or the pooled decimal-digit lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScalarKey(u32);

impl ScalarKey {
    /// Charter invariant 7: every Unicode decimal digit counts here.
    pub const DIGITS: Self = Self(DIGITS_RAW);

    pub const fn of(c: char) -> Self {
        Self(c as u32)
    }

    /// `None` for the pooled digit lane.
    pub const fn scalar(self) -> Option<char> {
        char::from_u32(self.0)
    }

    pub const fn is_digits(self) -> bool {
        self.0 == DIGITS_RAW
    }
}

/// A neighbor's outer class — the G0 rung of the evidence ladder.
///
/// Glue rides its base (charter invariant 8), so a mark reads as `Letter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum OuterClass {
    Letter = 0,
    Space = 1,
    Digit = 2,
    Nonletter = 3,
    /// Off the end of the chapter; the fold resolves it at a seam.
    #[default]
    Edge = 4,
}

impl OuterClass {
    const COUNT: usize = 5;

    /// The order matters: whitespace first, then the pooled digit lane, then
    /// anything a word is built from.
    const fn of(class: Class) -> Self {
        if class.is_whitespace() {
            Self::Space
        } else if class.is_decimal_digit() {
            Self::Digit
        } else if class.is_alphabetic() || class.is_glue() {
            Self::Letter
        } else {
            Self::Nonletter
        }
    }
}

/// One G0 pair triple: a nonletter and the outer class either side of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PairKey {
    scalar: ScalarKey,
    prev: OuterClass,
    next: OuterClass,
}

impl PairKey {
    pub const fn new(scalar: ScalarKey, prev: OuterClass, next: OuterClass) -> Self {
        Self { scalar, prev, next }
    }

    pub const fn scalar(self) -> ScalarKey {
        self.scalar
    }

    pub const fn prev(self) -> OuterClass {
        self.prev
    }

    pub const fn next(self) -> OuterClass {
        self.next
    }
}

/// The casing of the letter a nonletter run hands off to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Case {
    Upper = 0,
    Lower = 1,
    Uncased = 2,
}

impl Case {
    const COUNT: usize = 3;

    const fn of(class: Class) -> Self {
        if class.is_uppercase() {
            Self::Upper
        } else if class.is_lowercase() {
            Self::Lower
        } else {
            Self::Uncased
        }
    }
}

/// How often a run terminal was followed by an upper, lower, or uncased letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FollowCounts([u32; Case::COUNT]);

impl FollowCounts {
    pub const fn get(self, case: Case) -> u32 {
        self.0[case as usize]
    }

    pub const fn total(self) -> u32 {
        self.0[0] + self.0[1] + self.0[2]
    }

    fn add(&mut self, other: Self) {
        for (slot, count) in self.0.iter_mut().zip(other.0) {
            *slot += count;
        }
    }
}

/// Continuation-length buckets for one glyph: 1, 2, 3, 4, 5, and 6-or-more.
pub const RUN_BUCKETS: usize = 6;

/// A same-glyph continuation histogram, saturating in its last bucket.
pub type RunLengths = [u32; RUN_BUCKETS];

// ── The open edges ──────────────────────────────────────────────────────

/// What one end of a chapter owes its neighbor, and nothing more.
///
/// A leading edge fills `outer`, `open_pair`, and `edge_case`; a trailing
/// edge fills `outer`, `open_pair`, `open_follow`, and `blank`. The slot the
/// other side does not use stays `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edge {
    /// The edge scalar's own class, which is what the neighbor chapter sees.
    outer: OuterClass,
    /// The edge scalar when it is a nonletter, with the one neighbor class it
    /// already knows: `next` on a leading edge, `prev` on a trailing one.
    open_pair: Option<(ScalarKey, OuterClass)>,
    /// A nonletter run terminal with only whitespace between it and this end.
    open_follow: Option<ScalarKey>,
    /// This end's nearest letter, when only whitespace separates them.
    edge_case: Option<Case>,
    /// The chapter held whitespace and nothing else, so a neighbor's open
    /// follow survives it.
    blank: bool,
}

impl Edge {
    pub const fn outer(self) -> OuterClass {
        self.outer
    }

    pub const fn open_pair(self) -> Option<(ScalarKey, OuterClass)> {
        self.open_pair
    }

    pub const fn open_follow(self) -> Option<ScalarKey> {
        self.open_follow
    }

    pub const fn edge_case(self) -> Option<Case> {
        self.edge_case
    }

    pub const fn blank(self) -> bool {
        self.blank
    }
}

// ── The observation ─────────────────────────────────────────────────────

/// One chapter's counts: detached, sorted, and free of coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterRow {
    scalars: Box<[(ScalarKey, u32)]>,
    pairs: Box<[(PairKey, u32)]>,
    /// `(offset into run_atoms, length, count)`, sorted by the sequence.
    runs: Box<[(u32, u32, u32)]>,
    run_atoms: Box<[ScalarKey]>,
    follows: Box<[(ScalarKey, FollowCounts)]>,
    hygiene: Box<[HygieneFinding]>,
    lead: Edge,
    trail: Edge,
    scalar_count: u32,
    word_count: u32,
}

impl ChapterRow {
    /// Every scalar the chapter holds except glue, digits pooled under
    /// [`ScalarKey::DIGITS`], sorted by key.
    pub fn scalars(&self) -> &[(ScalarKey, u32)] {
        &self.scalars
    }

    /// G0 pair triples for every nonletter, sorted by key.
    pub fn pairs(&self) -> &[(PairKey, u32)] {
        &self.pairs
    }

    /// Maximal nonletter runs by scalar sequence, sorted by that sequence.
    pub fn runs(&self) -> impl ExactSizeIterator<Item = (&[ScalarKey], u32)> {
        self.runs
            .iter()
            .map(|&(at, len, count)| (&self.run_atoms[at as usize..][..len as usize], count))
    }

    /// Same-glyph continuation lengths, derived from [`Self::runs`] — the run
    /// sequences already carry them, so the row does not store them twice.
    pub fn run_lengths(&self) -> Vec<(ScalarKey, RunLengths)> {
        let mut out: Vec<(ScalarKey, RunLengths)> = Vec::new();
        for (atoms, count) in self.runs() {
            tally_run_lengths(atoms, count, &mut out);
        }
        out.sort_unstable_by_key(|entry| entry.0);
        out
    }

    /// Terminal follow table, sorted by key.
    pub fn follows(&self) -> &[(ScalarKey, FollowCounts)] {
        &self.follows
    }

    /// Hygiene's four scalar classes, chapter-relative and ordered by start.
    ///
    /// The one site lane a row carries; substrate.md says why it earns it.
    pub fn hygiene(&self) -> &[HygieneFinding] {
        &self.hygiene
    }

    pub const fn lead(&self) -> Edge {
        self.lead
    }

    pub const fn trail(&self) -> Edge {
        self.trail
    }

    pub const fn scalar_count(&self) -> u32 {
        self.scalar_count
    }

    pub const fn word_count(&self) -> u32 {
        self.word_count
    }

    /// Inline size plus every byte the lanes own; what a resident cache pays.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>()
            + size_of_val(&*self.scalars)
            + size_of_val(&*self.pairs)
            + size_of_val(&*self.runs)
            + size_of_val(&*self.run_atoms)
            + size_of_val(&*self.follows)
            + size_of_val(&*self.hygiene)
    }
}

/// Adds one run's same-glyph continuations into an unsorted tally.
fn tally_run_lengths(atoms: &[ScalarKey], count: u32, out: &mut Vec<(ScalarKey, RunLengths)>) {
    let mut at = 0;
    while at < atoms.len() {
        let key = atoms[at];
        let mut end = at + 1;
        while end < atoms.len() && atoms[end] == key {
            end += 1;
        }
        let bucket = (end - at).min(RUN_BUCKETS) - 1;
        match out.iter_mut().find(|entry| entry.0 == key) {
            Some(entry) => entry.1[bucket] += count,
            None => {
                let mut buckets = [0u32; RUN_BUCKETS];
                buckets[bucket] = count;
                out.push((key, buckets));
            }
        }
        at = end;
    }
}

// ── The pass ────────────────────────────────────────────────────────────

/// The Level 1b walk: one scalar pass per chapter, seams resolved in the fold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Substrate;

impl ChapterPass for Substrate {
    type Observation = ChapterRow;
    type Aggregate = BookAggregate;
    /// D2a-2 replaces this with the judging bands.
    type Config = ();
    const SCHEMA: SchemaStamp = SchemaStamp::new(2);

    fn map(&self, chapter: ChapterInput<'_>) -> ChapterRow {
        walk(chapter.text)
    }

    fn fold(&self, book: &[ChapterObs<&ChapterRow>]) -> BookAggregate {
        fold_book(book, &mut Edge::default())
    }

    /// Publishes the hygiene lane; the counts D1a folded wait for D2a-2.
    ///
    /// A site run abutting a masked `\c` is two findings, one per chapter.
    fn judge(&self, corpus: &[&BookAggregate], _config: &(), out: &mut Findings) {
        for (index, book) in corpus.iter().enumerate() {
            out.open_book(BookIndex::new(index).expect("a corpus indexes every book"));
            for finding in &book.hygiene {
                finding.push_into(out);
            }
        }
    }
}

// ── The book aggregate ──────────────────────────────────────────────────

/// One book's merged counts with every chapter seam resolved, plus the
/// hygiene lane in book coordinates.
///
/// The fold product a host caches per book checksum; judging reads it and
/// keeps nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookAggregate {
    scalars: Vec<(ScalarKey, u32)>,
    pairs: Vec<(PairKey, u32)>,
    runs: Vec<(Box<[ScalarKey]>, u32)>,
    follows: Vec<(ScalarKey, FollowCounts)>,
    hygiene: Vec<HygieneFinding>,
    scalar_count: u64,
    word_count: u64,
    chapters: u32,
}

impl BookAggregate {
    pub fn scalars(&self) -> &[(ScalarKey, u32)] {
        &self.scalars
    }

    pub fn pairs(&self) -> &[(PairKey, u32)] {
        &self.pairs
    }

    pub fn runs(&self) -> impl ExactSizeIterator<Item = (&[ScalarKey], u32)> {
        self.runs.iter().map(|(atoms, count)| (&**atoms, *count))
    }

    /// Same-glyph continuation lengths over the book, derived from the runs.
    pub fn run_lengths(&self) -> Vec<(ScalarKey, RunLengths)> {
        let mut out: Vec<(ScalarKey, RunLengths)> = Vec::new();
        for (atoms, count) in self.runs() {
            tally_run_lengths(atoms, count, &mut out);
        }
        out.sort_unstable_by_key(|entry| entry.0);
        out
    }

    pub fn follows(&self) -> &[(ScalarKey, FollowCounts)] {
        &self.follows
    }

    /// The scalar hygiene sites of every chapter, in book coordinates.
    pub fn hygiene(&self) -> &[HygieneFinding] {
        &self.hygiene
    }

    pub const fn scalar_count(&self) -> u64 {
        self.scalar_count
    }

    pub const fn word_count(&self) -> u64 {
        self.word_count
    }

    pub const fn chapters(&self) -> u32 {
        self.chapters
    }
}

/// Merges one book's rows in order, resolving each chapter seam against the
/// previous chapter's trailing edge, and rebases the hygiene lane.
///
/// A nonletter run that straddles a masked `\c` stays two runs, as it does
/// for hygiene; every other lane reads as if the book were one string.
pub fn fold_book(book: &[ChapterObs<&ChapterRow>], carry: &mut Edge) -> BookAggregate {
    let mut out = BookAggregate::default();
    let mut scalar_scratch = Vec::new();
    let mut pair_scratch = Vec::new();
    let mut run_scratch = Vec::new();
    let mut follow_scratch = Vec::new();

    for chapter in book {
        let row = chapter.obs;
        out.chapters += 1;
        merge_counts(&mut out.scalars, &row.scalars, &mut scalar_scratch);
        merge_counts(&mut out.pairs, &row.pairs, &mut pair_scratch);
        merge_runs(&mut out.runs, row, &mut run_scratch);
        merge_follows(&mut out.follows, &row.follows, &mut follow_scratch);
        out.hygiene
            .extend(row.hygiene.iter().map(|site| site.rebased(chapter.start)));
        out.scalar_count += u64::from(row.scalar_count);
        out.word_count += u64::from(row.word_count);

        if row.scalar_count == 0 {
            // An empty chapter is not a neighbor; the carry passes through it.
            continue;
        }
        if carry.outer != OuterClass::Edge {
            resolve_seam(&mut out, *carry, row.lead);
        }

        let mut next = row.trail;
        if row.scalar_count == 1
            && carry.outer != OuterClass::Edge
            && let Some((key, _)) = next.open_pair
        {
            // Lead and trail are the same scalar: its `prev` was just fixed.
            next.open_pair = Some((key, carry.outer));
        }
        if next.blank {
            next.open_follow = carry.open_follow;
        }
        *carry = next;
    }
    out
}

/// One seam: the pair either side of it, the follow across it, and the word
/// the masked `\c` split in two.
fn resolve_seam(out: &mut BookAggregate, trail: Edge, lead: Edge) {
    if let Some((key, prev)) = trail.open_pair {
        bump(
            &mut out.pairs,
            PairKey::new(key, prev, OuterClass::Edge),
            -1,
        );
        bump(&mut out.pairs, PairKey::new(key, prev, lead.outer), 1);
    }
    if let Some((key, next)) = lead.open_pair {
        bump(
            &mut out.pairs,
            PairKey::new(key, OuterClass::Edge, next),
            -1,
        );
        bump(&mut out.pairs, PairKey::new(key, trail.outer, next), 1);
    }
    if let (Some(key), Some(case)) = (trail.open_follow, lead.edge_case) {
        let mut counts = FollowCounts::default();
        counts.0[case as usize] = 1;
        match out.follows.binary_search_by_key(&key, |entry| entry.0) {
            Ok(at) => out.follows[at].1.add(counts),
            Err(at) => out.follows.insert(at, (key, counts)),
        }
    }
    let joins = |class: OuterClass| matches!(class, OuterClass::Letter | OuterClass::Digit);
    if joins(trail.outer) && joins(lead.outer) {
        out.word_count -= 1;
    }
}

/// Adds `delta` to one sorted count, inserting or deleting the row as needed.
fn bump(counts: &mut Vec<(PairKey, u32)>, key: PairKey, delta: i32) {
    match counts.binary_search_by_key(&key, |entry| entry.0) {
        Ok(at) => {
            let next = i64::from(counts[at].1) + i64::from(delta);
            debug_assert!(
                next >= 0,
                "a seam fix never removes a count that is not there"
            );
            if next == 0 {
                counts.remove(at);
            } else {
                counts[at].1 = next as u32;
            }
        }
        Err(at) => {
            debug_assert!(
                delta > 0,
                "a seam fix never removes a count that is not there"
            );
            counts.insert(at, (key, delta.unsigned_abs()));
        }
    }
}

/// Sorted merge of two count lanes, `dst` taking the sum.
fn merge_counts<K: Ord + Copy>(
    dst: &mut Vec<(K, u32)>,
    src: &[(K, u32)],
    scratch: &mut Vec<(K, u32)>,
) {
    if src.is_empty() {
        return;
    }
    if dst.is_empty() {
        dst.extend_from_slice(src);
        return;
    }
    scratch.clear();
    scratch.reserve(dst.len() + src.len());
    let (mut left, mut right) = (0, 0);
    while left < dst.len() && right < src.len() {
        match dst[left].0.cmp(&src[right].0) {
            std::cmp::Ordering::Less => {
                scratch.push(dst[left]);
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                scratch.push(src[right]);
                right += 1;
            }
            std::cmp::Ordering::Equal => {
                scratch.push((dst[left].0, dst[left].1 + src[right].1));
                left += 1;
                right += 1;
            }
        }
    }
    scratch.extend_from_slice(&dst[left..]);
    scratch.extend_from_slice(&src[right..]);
    std::mem::swap(dst, scratch);
}

fn merge_follows(
    dst: &mut Vec<(ScalarKey, FollowCounts)>,
    src: &[(ScalarKey, FollowCounts)],
    scratch: &mut Vec<(ScalarKey, FollowCounts)>,
) {
    if src.is_empty() {
        return;
    }
    if dst.is_empty() {
        dst.extend_from_slice(src);
        return;
    }
    scratch.clear();
    let (mut left, mut right) = (0, 0);
    while left < dst.len() && right < src.len() {
        match dst[left].0.cmp(&src[right].0) {
            std::cmp::Ordering::Less => {
                scratch.push(dst[left]);
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                scratch.push(src[right]);
                right += 1;
            }
            std::cmp::Ordering::Equal => {
                let mut counts = dst[left].1;
                counts.add(src[right].1);
                scratch.push((dst[left].0, counts));
                left += 1;
                right += 1;
            }
        }
    }
    scratch.extend_from_slice(&dst[left..]);
    scratch.extend_from_slice(&src[right..]);
    std::mem::swap(dst, scratch);
}

fn merge_runs(
    dst: &mut Vec<(Box<[ScalarKey]>, u32)>,
    row: &ChapterRow,
    scratch: &mut Vec<(Box<[ScalarKey]>, u32)>,
) {
    if row.runs.is_empty() {
        return;
    }
    scratch.clear();
    let mut left = 0;
    let mut src = row.runs();
    let mut next = src.next();
    while left < dst.len() {
        let Some((atoms, count)) = next else { break };
        match dst[left].0.as_ref().cmp(atoms) {
            std::cmp::Ordering::Less => {
                scratch.push(std::mem::take(&mut dst[left]));
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                scratch.push((atoms.into(), count));
                next = src.next();
            }
            std::cmp::Ordering::Equal => {
                let mut held = std::mem::take(&mut dst[left]);
                held.1 += count;
                scratch.push(held);
                left += 1;
                next = src.next();
            }
        }
    }
    for entry in &mut dst[left..] {
        scratch.push(std::mem::take(entry));
    }
    while let Some((atoms, count)) = next {
        scratch.push((atoms.into(), count));
        next = src.next();
    }
    std::mem::swap(dst, scratch);
}

// ── The walk ────────────────────────────────────────────────────────────

/// Consecutive ASCII scalars before the eight-byte lane re-arms; the
/// hysteresis the Stage 1 classifier bench measured.
const REARM_AFTER: u32 = 32;
const HIGH_BITS: u64 = 0x8080_8080_8080_8080;
/// No dense id yet, and no run or pending pair open.
const NO_ID: u32 = u32::MAX;
/// Slots in the non-ASCII inventory's open-addressed table. A chapter holds
/// 150-200 distinct scalars at the tier's worst (Amharic, Greek), so half of
/// this stays empty and a probe almost always lands first try.
const PROBE: usize = 512;

/// One distinct nonletter's array-indexed counters, resolved to a scalar key
/// only when the row is built.
struct Slot {
    key: ScalarKey,
    pairs: [u32; OuterClass::COUNT * OuterClass::COUNT],
    follows: FollowCounts,
}

/// The per-scalar state, kept in a local the counters cannot alias.
///
/// Every counter write below goes through a heap pointer, so a compiler that
/// found this state behind the same `&mut` would reload all twelve fields
/// after each one. It is small and never escapes, so it stays in registers.
#[derive(Clone, Copy)]
struct Hot {
    prev: OuterClass,
    /// The previous scalar's own bits, which is what a hygiene site reads.
    prev_class: Class,
    /// Mirrors `ScalarSites::pending`, so the gate reads a register rather
    /// than the counter struct — worth 3pt on Latin.
    site_pending: bool,
    /// A nonletter whose `next` class the following scalar supplies.
    pending: u32,
    pending_prev: OuterClass,
    pending_first: bool,
    /// A run terminal still looking for the letter it hands off to.
    awaiting: u32,
    /// Where the open run started in the atom arena.
    run_open: u32,
    in_word: bool,
    seen_nonspace: bool,
    all_space: bool,
    scalar_count: u32,
    word_count: u32,
}

impl Hot {
    const fn new() -> Self {
        Self {
            prev: OuterClass::Edge,
            prev_class: Class::from_bits(0),
            site_pending: false,
            pending: NO_ID,
            pending_prev: OuterClass::Edge,
            pending_first: false,
            awaiting: NO_ID,
            run_open: NO_ID,
            in_word: false,
            seen_nonspace: false,
            all_space: true,
            scalar_count: 0,
            word_count: 0,
        }
    }
}

/// Per-call scratch: the counters and the intern tables. Nothing here is
/// shared, retained, or locked, so a parallel map needs no lock either.
struct Counters {
    ascii: [u32; 128],
    probe_keys: [u32; PROBE],
    probe_counts: [u32; PROBE],
    probe_live: u32,
    /// Only what the probe table declined to take once it was half full.
    other: FxHashMap<u32, u32>,
    digits: u32,
    nonletter_ascii: [u32; 128],
    nonletter_other: FxHashMap<u32, u32>,
    nonletter_digits: u32,
    slots: Vec<Slot>,
    run_atoms: Vec<u32>,
    run_spans: Vec<(u32, u32)>,
    lead: Edge,
    trail_pair: Option<(ScalarKey, OuterClass)>,
    sites: ScalarSites,
}

fn walk(text: &str) -> ChapterRow {
    let bytes = text.as_bytes();
    let mut counters = Counters::new(bytes.len());
    let mut hot = Hot::new();
    let (mut at, mut armed, mut ascii_run) = (0usize, true, 0u32);
    while at < bytes.len() {
        if armed && at + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"));
            if word & HIGH_BITS == 0 {
                for (offset, &byte) in bytes[at..at + 8].iter().enumerate() {
                    counters.step::<true>(
                        &mut hot,
                        u32::from(byte),
                        ascii_class(byte),
                        at + offset,
                        1,
                    );
                }
                at += 8;
                continue;
            }
            armed = false;
            ascii_run = 0;
        }
        let (class, width) = trie_at(&bytes[at..]);
        counters.step::<false>(&mut hot, scalar_at(&bytes[at..], width), class, at, width);
        if width == 1 {
            ascii_run += 1;
            armed |= ascii_run >= REARM_AFTER;
        } else {
            ascii_run = 0;
        }
        at += width;
    }
    counters.finish(hot, text)
}

/// The code point at `bytes[0]`, whose UTF-8 width `trie_at` already read.
#[inline]
fn scalar_at(bytes: &[u8], width: usize) -> u32 {
    match width {
        1 => u32::from(bytes[0]),
        2 => (u32::from(bytes[0] & 0x1F) << 6) | u32::from(bytes[1] & 0x3F),
        3 => {
            (u32::from(bytes[0] & 0x0F) << 12)
                | (u32::from(bytes[1] & 0x3F) << 6)
                | u32::from(bytes[2] & 0x3F)
        }
        _ => {
            (u32::from(bytes[0] & 0x07) << 18)
                | (u32::from(bytes[1] & 0x3F) << 12)
                | (u32::from(bytes[2] & 0x3F) << 6)
                | u32::from(bytes[3] & 0x3F)
        }
    }
}

/// Not a letter, not glue, not whitespace: what the nonletter inventory
/// counts, digits included — they pool into one key, not out of the lane.
#[inline]
const fn is_nonletter(class: Class) -> bool {
    !class.is_alphabetic() && !class.is_glue() && !class.is_whitespace()
}

impl Counters {
    /// `hint` is the chapter's byte length. Nonletters run 5-15% of scalars
    /// and their runs a tenth of that, so the two arenas are sized from it
    /// once instead of doubling a dozen times per chapter.
    fn new(hint: usize) -> Self {
        Self {
            ascii: [0; 128],
            probe_keys: [0; PROBE],
            probe_counts: [0; PROBE],
            probe_live: 0,
            other: FxHashMap::default(),
            digits: 0,
            nonletter_ascii: [NO_ID; 128],
            nonletter_other: FxHashMap::default(),
            nonletter_digits: NO_ID,
            slots: Vec::new(),
            run_atoms: Vec::with_capacity(hint / 8),
            run_spans: Vec::with_capacity(hint / 32),
            lead: Edge::default(),
            trail_pair: None,
            sites: ScalarSites::new(),
        }
    }

    /// `ASCII` is the eight-byte lane, where no scalar is a mark, a format
    /// character, a noncharacter, or U+00A0: only a pending verdict reaches
    /// the site machine there, so the gate is one branch.
    #[inline(always)]
    fn step<const ASCII: bool>(
        &mut self,
        hot: &mut Hot,
        cp: u32,
        class: Class,
        at: usize,
        width: usize,
    ) {
        let outer = OuterClass::of(class);
        if hot.scalar_count == 0 {
            self.lead.outer = outer;
        }
        if !class.is_whitespace() && !hot.seen_nonspace {
            hot.seen_nonspace = true;
            hot.all_space = false;
            if class.is_alphabetic() {
                self.lead.edge_case = Some(Case::of(class));
            }
        }

        if !class.is_glue() {
            if class.is_decimal_digit() {
                self.digits += 1;
            } else if cp < 128 {
                self.ascii[cp as usize] += 1;
            } else {
                self.count_wide(cp);
            }
        }

        if hot.pending != NO_ID {
            let slot = &mut self.slots[hot.pending as usize];
            slot.pairs[pair_index(hot.pending_prev, outer)] += 1;
            if hot.pending_first {
                self.lead.open_pair = Some((slot.key, outer));
            }
            hot.pending = NO_ID;
        }

        if is_nonletter(class) {
            let id = self.intern(cp, class);
            if hot.run_open == NO_ID {
                hot.run_open = self.run_atoms.len() as u32;
            }
            self.run_atoms.push(id);
            hot.awaiting = NO_ID;
            hot.pending = id;
            hot.pending_prev = hot.prev;
            hot.pending_first = hot.scalar_count == 0;
        } else {
            self.close_run(hot);
            if class.is_alphabetic() {
                if hot.awaiting != NO_ID {
                    self.slots[hot.awaiting as usize].follows.0[Case::of(class) as usize] += 1;
                    hot.awaiting = NO_ID;
                }
            } else if !class.is_whitespace() {
                hot.awaiting = NO_ID;
            }
        }

        if class.is_alphabetic() || class.is_glue() || class.is_decimal_digit() {
            if !hot.in_word {
                hot.word_count += 1;
                hot.in_word = true;
            }
        } else {
            hot.in_word = false;
        }

        let gated = if ASCII {
            hot.site_pending
        } else {
            (class.bits() & SUSPECT != 0) | (cp == NBSP) | hot.site_pending
        };
        if gated {
            let prev = (hot.scalar_count > 0).then_some(hot.prev_class);
            self.sites.step(at, width, cp, class, prev);
            hot.site_pending = self.sites.pending();
        }

        hot.prev = outer;
        hot.prev_class = class;
        hot.scalar_count += 1;
    }

    /// Ends the open run, if any, and leaves its terminal awaiting a letter.
    #[inline(always)]
    fn close_run(&mut self, hot: &mut Hot) {
        if hot.run_open != NO_ID {
            let len = self.run_atoms.len() as u32 - hot.run_open;
            self.run_spans.push((hot.run_open, len));
            hot.run_open = NO_ID;
            hot.awaiting = *self.run_atoms.last().expect("a closed run has atoms");
        }
    }

    /// One non-ASCII scalar into the inventory. The table never fills past
    /// half, so an empty slot always ends the probe and a key is in exactly
    /// one of the two structures.
    #[inline]
    fn count_wide(&mut self, cp: u32) {
        let mut at = (cp.wrapping_mul(0x9E37_79B1) >> 20) as usize & (PROBE - 1);
        loop {
            let key = self.probe_keys[at];
            if key == cp {
                self.probe_counts[at] += 1;
                return;
            }
            if key == 0 {
                if (self.probe_live as usize) * 2 < PROBE {
                    self.probe_keys[at] = cp;
                    self.probe_counts[at] = 1;
                    self.probe_live += 1;
                    return;
                }
                break;
            }
            at = (at + 1) & (PROBE - 1);
        }
        *self.other.entry(cp).or_insert(0) += 1;
    }

    /// The chapter-local dense id of one nonletter; glue never reaches here.
    #[inline]
    fn intern(&mut self, cp: u32, class: Class) -> u32 {
        if class.is_decimal_digit() {
            if self.nonletter_digits == NO_ID {
                self.nonletter_digits = self.push_slot(ScalarKey::DIGITS);
            }
            return self.nonletter_digits;
        }
        if cp < 128 {
            let seen = self.nonletter_ascii[cp as usize];
            if seen != NO_ID {
                return seen;
            }
            let id = self.push_slot(ScalarKey(cp));
            self.nonletter_ascii[cp as usize] = id;
            return id;
        }
        if let Some(&seen) = self.nonletter_other.get(&cp) {
            return seen;
        }
        let id = self.push_slot(ScalarKey(cp));
        self.nonletter_other.insert(cp, id);
        id
    }

    fn push_slot(&mut self, key: ScalarKey) -> u32 {
        let id = self.slots.len() as u32;
        self.slots.push(Slot {
            key,
            pairs: [0; OuterClass::COUNT * OuterClass::COUNT],
            follows: FollowCounts::default(),
        });
        id
    }

    fn finish(mut self, mut hot: Hot, text: &str) -> ChapterRow {
        if hot.pending != NO_ID {
            let slot = &mut self.slots[hot.pending as usize];
            slot.pairs[pair_index(hot.pending_prev, OuterClass::Edge)] += 1;
            let key = slot.key;
            if hot.pending_first {
                self.lead.open_pair = Some((key, OuterClass::Edge));
            }
            self.trail_pair = Some((key, hot.pending_prev));
        }
        self.close_run(&mut hot);
        let hygiene = self.sites.finish(text);

        let trail = Edge {
            outer: hot.prev,
            open_pair: self.trail_pair,
            open_follow: (hot.awaiting != NO_ID).then(|| self.slots[hot.awaiting as usize].key),
            edge_case: None,
            blank: hot.all_space && hot.scalar_count > 0,
        };

        let mut scalars: Vec<(ScalarKey, u32)> =
            Vec::with_capacity(self.other.len() + self.probe_live as usize + 64);
        for (cp, count) in self.ascii.iter().enumerate() {
            if *count > 0 {
                scalars.push((ScalarKey(cp as u32), *count));
            }
        }
        for (key, count) in self.probe_keys.iter().zip(&self.probe_counts) {
            if *count > 0 {
                scalars.push((ScalarKey(*key), *count));
            }
        }
        scalars.extend(
            self.other
                .iter()
                .map(|(cp, count)| (ScalarKey(*cp), *count)),
        );
        if self.digits > 0 {
            scalars.push((ScalarKey::DIGITS, self.digits));
        }
        scalars.sort_unstable_by_key(|entry| entry.0);

        let mut pairs: Vec<(PairKey, u32)> = Vec::with_capacity(self.slots.len() * 3);
        let mut follows: Vec<(ScalarKey, FollowCounts)> = Vec::with_capacity(self.slots.len());
        for slot in &self.slots {
            for (index, count) in slot.pairs.iter().enumerate() {
                if *count > 0 {
                    pairs.push((
                        PairKey::new(
                            slot.key,
                            outer_at(index / OuterClass::COUNT),
                            outer_at(index % OuterClass::COUNT),
                        ),
                        *count,
                    ));
                }
            }
            if slot.follows.total() > 0 {
                follows.push((slot.key, slot.follows));
            }
        }
        pairs.sort_unstable_by_key(|entry| entry.0);
        follows.sort_unstable_by_key(|entry| entry.0);

        let keys: Vec<ScalarKey> = self.slots.iter().map(|slot| slot.key).collect();
        let atoms = &self.run_atoms;
        let mut spans = std::mem::take(&mut self.run_spans);
        spans.sort_unstable_by(|left, right| {
            let a = &atoms[left.0 as usize..][..left.1 as usize];
            let b = &atoms[right.0 as usize..][..right.1 as usize];
            a.iter()
                .map(|id| keys[*id as usize])
                .cmp(b.iter().map(|id| keys[*id as usize]))
        });

        let mut runs: Vec<(u32, u32, u32)> = Vec::with_capacity(spans.len() / 4 + 4);
        let mut run_atoms: Vec<ScalarKey> = Vec::with_capacity(spans.len() / 2 + 4);
        for (start, len) in spans {
            let sequence = &atoms[start as usize..][..len as usize];
            let same = runs.last().is_some_and(|&(at, held, _)| {
                held == len
                    && run_atoms[at as usize..][..held as usize]
                        .iter()
                        .zip(sequence)
                        .all(|(key, id)| *key == keys[*id as usize])
            });
            if same {
                runs.last_mut().expect("just checked").2 += 1;
                continue;
            }
            let at = run_atoms.len() as u32;
            run_atoms.extend(sequence.iter().map(|id| keys[*id as usize]));
            runs.push((at, len, 1));
        }

        ChapterRow {
            scalars: scalars.into_boxed_slice(),
            pairs: pairs.into_boxed_slice(),
            runs: runs.into_boxed_slice(),
            run_atoms: run_atoms.into_boxed_slice(),
            follows: follows.into_boxed_slice(),
            hygiene,
            lead: self.lead,
            trail,
            scalar_count: hot.scalar_count,
            word_count: hot.word_count,
        }
    }
}

#[inline]
const fn pair_index(prev: OuterClass, next: OuterClass) -> usize {
    prev as usize * OuterClass::COUNT + next as usize
}

const fn outer_at(index: usize) -> OuterClass {
    match index {
        0 => OuterClass::Letter,
        1 => OuterClass::Space,
        2 => OuterClass::Digit,
        3 => OuterClass::Nonletter,
        _ => OuterClass::Edge,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pass::{ChapterKey, analyze};
    use crate::{BookKey, Chapter, Corpus, ProjectedBook, TextRange, Verse, VerseKey};

    fn row(text: &str) -> ChapterRow {
        Substrate.map(ChapterInput {
            text,
            verses: &[],
            key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
        })
    }

    fn aggregate(chapters: &[&str]) -> BookAggregate {
        let rows: Vec<ChapterRow> = chapters.iter().map(|text| row(text)).collect();
        let view: Vec<ChapterObs<&ChapterRow>> = rows
            .iter()
            .map(|obs| ChapterObs { start: 0, obs })
            .collect();
        fold_book(&view, &mut Edge::default())
    }

    fn count(row: &ChapterRow, c: char) -> u32 {
        row.scalars()
            .iter()
            .find(|entry| entry.0 == ScalarKey::of(c))
            .map_or(0, |entry| entry.1)
    }

    fn pair(row: &ChapterRow, c: char, prev: OuterClass, next: OuterClass) -> u32 {
        row.pairs()
            .iter()
            .find(|entry| entry.0 == PairKey::new(ScalarKey::of(c), prev, next))
            .map_or(0, |entry| entry.1)
    }

    #[test]
    fn every_lane_is_sorted_by_key() {
        let row = row("He said, \u{201C}Go.\u{201D} 12,345 \u{967}\u{968}");
        assert!(row.scalars().windows(2).all(|w| w[0].0 < w[1].0));
        assert!(row.pairs().windows(2).all(|w| w[0].0 < w[1].0));
        assert!(row.follows().windows(2).all(|w| w[0].0 < w[1].0));
        let runs: Vec<Vec<ScalarKey>> = row.runs().map(|(atoms, _)| atoms.to_vec()).collect();
        assert!(runs.windows(2).all(|w| w[0] < w[1]));
    }

    /// Charter invariant 7: one lane, whatever the number system.
    #[test]
    fn every_decimal_digit_pools_under_one_key() {
        let row = row("7 \u{967}");
        assert_eq!(count(&row, '7'), 0);
        assert_eq!(count(&row, '\u{967}'), 0);
        assert_eq!(
            row.scalars()
                .iter()
                .find(|entry| entry.0 == ScalarKey::DIGITS)
                .map(|entry| entry.1),
            Some(2)
        );
    }

    /// Charter invariant 8: glue rides its base and never enters the inventory.
    #[test]
    fn glue_never_appears_as_a_scalar_a_pair_member_or_a_run_member() {
        let row = row("e\u{301} \u{915}\u{94d}\u{937} \u{915}\u{200d}\u{915}");
        for glue in ['\u{301}', '\u{94d}', '\u{200d}'] {
            assert_eq!(count(&row, glue), 0, "{glue:?} is in the inventory");
            assert!(
                row.pairs()
                    .iter()
                    .all(|entry| entry.0.scalar() != ScalarKey::of(glue)),
                "{glue:?} is a pair member"
            );
            assert!(
                row.runs()
                    .all(|(atoms, _)| !atoms.contains(&ScalarKey::of(glue))),
                "{glue:?} is a run member"
            );
        }
    }

    /// The Swahili convention: an apostrophe wholly inside a word.
    #[test]
    fn a_word_medial_apostrophe_reads_letter_to_letter() {
        let row = row("ng'ombe ng'ombe");
        assert_eq!(pair(&row, '\'', OuterClass::Letter, OuterClass::Letter), 2);
        // A word is letters, glue, and digits, so the apostrophe splits one.
        assert_eq!(row.word_count(), 4);
    }

    #[test]
    fn a_run_of_the_same_glyph_lands_in_its_own_length_bucket() {
        let lengths = row("a,b,,c,,,d,,,,,,,e").run_lengths();
        let commas = lengths
            .iter()
            .find(|entry| entry.0 == ScalarKey::of(','))
            .expect("commas");
        assert_eq!(commas.1, [1, 1, 1, 0, 0, 1]);
    }

    #[test]
    fn a_terminal_records_the_casing_of_the_letter_it_hands_off_to() {
        let row = row("one. Two. three. \u{5d0}");
        let follows = row.follows();
        let dot = follows
            .iter()
            .find(|entry| entry.0 == ScalarKey::of('.'))
            .expect("the terminal");
        assert_eq!(dot.1.get(Case::Upper), 1);
        assert_eq!(dot.1.get(Case::Lower), 1);
        assert_eq!(dot.1.get(Case::Uncased), 1);
    }

    #[test]
    fn an_empty_chapter_is_all_default_edges() {
        let row = row("");
        assert_eq!(row.lead(), Edge::default());
        assert_eq!(row.trail(), Edge::default());
        assert_eq!(row.scalar_count(), 0);
        assert_eq!(row.word_count(), 0);
    }

    #[test]
    fn mapping_the_same_chapter_twice_gives_identical_rows() {
        let text = "He said, \u{201C}Go.\u{201D} 12,345 \u{915}\u{94d}\u{937}a";
        assert_eq!(row(text), row(text));
    }

    #[test]
    fn a_crlf_variant_differs_only_where_the_cr_is() {
        let lf = row("one.\ntwo,\nthree");
        let crlf = row("one.\r\ntwo,\r\nthree");
        assert_eq!(count(&crlf, '\r'), 2);
        assert_eq!(count(&lf, '\r'), 0);
        let without_cr = |row: &ChapterRow| -> Vec<(ScalarKey, u32)> {
            row.scalars()
                .iter()
                .filter(|entry| entry.0 != ScalarKey::of('\r'))
                .copied()
                .collect()
        };
        assert_eq!(without_cr(&lf), without_cr(&crlf));
        assert_eq!(lf.pairs(), crlf.pairs());
        assert_eq!(lf.follows(), crlf.follows());
        assert_eq!(lf.word_count(), crlf.word_count());
        assert_eq!(lf.scalar_count() + 2, crlf.scalar_count());
    }

    fn assert_seam_agrees(whole: &str, split: &[&str]) {
        let one = aggregate(&[whole]);
        let many = aggregate(split);
        assert_eq!(one.pairs(), many.pairs(), "pairs across {split:?}");
        assert_eq!(one.follows(), many.follows(), "follows across {split:?}");
        assert_eq!(one.scalars(), many.scalars(), "scalars across {split:?}");
        assert_eq!(
            one.word_count(),
            many.word_count(),
            "words across {split:?}"
        );
    }

    #[test]
    fn a_seam_resolves_to_the_counts_of_the_unsplit_text() {
        assert_seam_agrees("one. Two, three", &["one. ", "Two, three"]);
        assert_seam_agrees("one. Two, three", &["one.", " Two, three"]);
        assert_seam_agrees("one.Two", &["one.", "Two"]);
        assert_seam_agrees("a,b", &["a", ",", "b"]);
        assert_seam_agrees("one. \u{201C}Two", &["one. ", "\u{201C}Two"]);
        assert_seam_agrees("word", &["wo", "rd"]);
        assert_seam_agrees("one.  Two", &["one. ", " ", "Two"]);
        assert_seam_agrees("one. Two", &["one.", "", " Two"]);
    }

    /// The hygiene ruling, kept: a run abutting a masked `\c` is two runs.
    #[test]
    fn a_run_straddling_a_seam_stays_two_runs() {
        let one = aggregate(&["a,,b"]);
        let split = aggregate(&["a,", ",b"]);
        let shapes = |book: &BookAggregate| -> Vec<(Vec<ScalarKey>, u32)> {
            book.runs()
                .map(|(atoms, count)| (atoms.to_vec(), count))
                .collect()
        };
        assert_eq!(shapes(&one), vec![(vec![ScalarKey::of(','); 2], 1)]);
        assert_eq!(shapes(&split), vec![(vec![ScalarKey::of(',')], 2)]);
        // Everything else still reads as one string.
        assert_eq!(one.pairs(), split.pairs());
    }

    struct Book {
        key: BookKey,
        text: &'static str,
        chapters: Vec<Chapter>,
        verses: Vec<Verse>,
    }

    impl ProjectedBook for Book {
        fn key(&self) -> BookKey {
            self.key
        }

        fn text(&self) -> &str {
            self.text
        }

        fn chapters(&self) -> impl Iterator<Item = Chapter> {
            self.chapters.iter().copied()
        }

        fn verses(&self) -> impl Iterator<Item = Verse> {
            self.verses.iter().copied()
        }
    }

    fn book(key: &[u8; 3], text: &'static str, len: u32) -> Book {
        Book {
            key: BookKey::new(*key),
            text,
            chapters: vec![Chapter::new(1, TextRange::new(0, len).unwrap()).unwrap()],
            verses: vec![Verse::new(
                VerseKey::new(1, 1, 1).unwrap(),
                TextRange::new(0, len).unwrap(),
            )],
        }
    }

    /// Charter invariant 2: no state crosses a book, so the second book's
    /// first chapter sees `Edge` on its left, not the first book's last scalar.
    #[test]
    fn every_book_folds_from_a_fresh_edge() {
        let books = vec![book(b"GEN", "a,", 2), book(b"MRK", ",b", 2)];
        let corpus = Corpus::try_new(&books).unwrap();
        // These books hold no hygiene site, so judging emits nothing; the
        // seam claim is the three folds below.
        assert!(analyze(&corpus, &Substrate).is_empty());

        // The same two chapters inside one book resolve their seam; in two
        // books each keeps the `Edge` its own end saw.
        assert_eq!(
            aggregate(&["a,", ",b"]).pairs().to_vec(),
            vec![
                (
                    PairKey::new(
                        ScalarKey::of(','),
                        OuterClass::Letter,
                        OuterClass::Nonletter
                    ),
                    1
                ),
                (
                    PairKey::new(
                        ScalarKey::of(','),
                        OuterClass::Nonletter,
                        OuterClass::Letter
                    ),
                    1
                )
            ]
        );
        assert_eq!(
            aggregate(&["a,"]).pairs().to_vec(),
            vec![(
                PairKey::new(ScalarKey::of(','), OuterClass::Letter, OuterClass::Edge),
                1
            )]
        );
        assert_eq!(
            aggregate(&[",b"]).pairs().to_vec(),
            vec![(
                PairKey::new(ScalarKey::of(','), OuterClass::Edge, OuterClass::Letter),
                1
            )]
        );
    }

    /// The lanes are what a resident cache pays per chapter, so the inline
    /// size is a fact worth pinning: it is 20% of the tier's median row. The
    /// hygiene lane is 16 of these bytes and is almost always empty.
    #[test]
    fn the_row_and_its_edges_are_the_size_the_budget_assumes() {
        assert_eq!(size_of::<ChapterRow>(), 144);
        assert_eq!(size_of::<Edge>(), 20);
    }

    /// Hygiene's four scalar classes, read off the lane the walk fills.
    mod hygiene_sites {
        use crate::HygieneClass;

        fn rows(text: &str) -> Vec<(HygieneClass, u32, u32, u32)> {
            super::row(text)
                .hygiene()
                .iter()
                .map(|f| (f.class(), f.span().from(), f.span().to(), f.run()))
                .collect()
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
            // rules/hygiene.md, required examples.
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
            // A mark behind a control has no base either; the control run is
            // `HygieneBytes`' own row, not the walk's.
            assert_eq!(
                rows("a\0\u{301}b"),
                vec![(HygieneClass::FreeCombiningMark, 2, 4, 1)]
            );
        }

        #[test]
        fn zwj_and_zwnj_between_letters_are_silent() {
            // ZWJ/ZWNJ in Indic text never enters the inventory.
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
            // U+FFFD keeps its own class and is a byte sweep's row, not one here.
            assert!(rows("a\u{fffd}b").is_empty());
        }

        #[test]
        fn nbsp_speaks_only_where_the_claim_is_deterministic() {
            // French spacing around punctuation is convention, not damage.
            assert!(rows("J\u{e9}sus\u{a0}: parle").is_empty());
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
            for finding in super::row(text).hygiene() {
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

        /// A chapter end is an edge of text, so a run abutting a masked `\c`
        /// is one finding per chapter — the hygiene ruling, kept.
        #[test]
        fn a_site_run_stops_at_the_chapter_edge() {
            assert_eq!(
                rows("a \u{301}\u{301}"),
                vec![(HygieneClass::FreeCombiningMark, 1, 6, 2)]
            );
            assert_eq!(
                rows("a \u{301}"),
                vec![(HygieneClass::FreeCombiningMark, 1, 4, 1)]
            );
            assert_eq!(
                rows("\u{301}"),
                vec![(HygieneClass::FreeCombiningMark, 0, 2, 1)]
            );
        }
    }

    const fn detached<O: Send + 'static>() {}

    #[test]
    fn a_substrate_observation_is_send_and_borrow_free() {
        detached::<<Substrate as ChapterPass>::Observation>();
    }
}
