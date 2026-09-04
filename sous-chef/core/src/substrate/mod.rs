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

use crate::hygiene::HygieneFinding;
use crate::judge::{JudgingConfig, PatternIndex};
use crate::pass::{ChapterInput, ChapterObs, ChapterPass, Findings, SchemaStamp};
use crate::sites;
use crate::unicode::Class;
use crate::{BookIndex, ConventionDigest, FindingKind};

pub(crate) mod fold;
#[cfg(test)]
mod tests;
pub(crate) mod walk;

pub use fold::fold_book;

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

    /// The wire value: a code point, or `u32::MAX` for the pooled lane.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// `None` for a value that is neither a scalar nor the pooled key.
    pub const fn from_raw(raw: u32) -> Option<Self> {
        if raw == DIGITS_RAW || char::from_u32(raw).is_some() {
            Some(Self(raw))
        } else {
            None
        }
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

    pub const ALL: [Self; Self::COUNT] = [
        Self::Letter,
        Self::Space,
        Self::Digit,
        Self::Nonletter,
        Self::Edge,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Letter => "Letter",
            Self::Space => "Space",
            Self::Digit => "Digit",
            Self::Nonletter => "Nonletter",
            Self::Edge => "Edge",
        }
    }

    /// `None` for a discriminant past the table.
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Letter),
            1 => Some(Self::Space),
            2 => Some(Self::Digit),
            3 => Some(Self::Nonletter),
            4 => Some(Self::Edge),
            _ => None,
        }
    }

    /// The order matters: whitespace first, then the pooled digit lane, then
    /// anything a word is built from.
    ///
    /// Glue answers `Letter` rather than its base's class, so a neighbour asks
    /// its immediate neighbour and never walks back over a mark.
    pub const fn of(class: Class) -> Self {
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

/// Not a letter, not glue, not whitespace: what the nonletter inventory
/// counts and what a run is built from, digits included — they pool into one
/// key, not out of the lane.
#[inline]
pub const fn is_nonletter(class: Class) -> bool {
    !class.is_alphabetic() && !class.is_glue() && !class.is_whitespace()
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
    type Config = JudgingConfig;
    const SCHEMA: SchemaStamp = SchemaStamp::new(2);

    fn map(&self, chapter: ChapterInput<'_>) -> ChapterRow {
        walk::walk(chapter.text)
    }

    fn fold(&self, book: &[ChapterObs<&ChapterRow>]) -> BookAggregate {
        fold_book(book, &mut Edge::default())
    }

    /// Publishes the hygiene lane, then judges the corpus's counts into the
    /// pattern table.
    ///
    /// A site run abutting a masked `\c` is two findings, one per chapter.
    fn judge(&self, corpus: &[&BookAggregate], config: &JudgingConfig, out: &mut Findings) {
        for (index, book) in corpus.iter().enumerate() {
            out.open_book(BookIndex::new(index).expect("a corpus indexes every book"));
            for finding in &book.hygiene {
                finding.push_into(out);
            }
        }
        crate::judge::judge_corpus(corpus, config, out);
    }

    /// Rescans this book's text for every pattern its own counts hold, and
    /// pushes one `Convention` row per matching run.
    fn locate(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[crate::Chapter],
        aggregate: &BookAggregate,
        out: &mut Findings,
    ) {
        let mut set = Vec::new();
        sites::firing(aggregate, out.patterns(), &mut set);
        if set.is_empty() {
            return;
        }
        // Copied because `Findings` cannot lend its table and take a row at
        // once; a firing set is tens of rows, not thousands.
        let table: Vec<(PatternIndex, crate::judge::Pattern)> = set
            .iter()
            .map(|&index| (index, out.patterns()[usize::from(index.get())]))
            .collect();
        let mut found = Vec::new();
        sites::locate(text, chapters, &table, &mut found);
        out.open_book(book);
        for site in found {
            out.push(
                site.span,
                FindingKind::Convention(ConventionDigest::new(site.headline, site.reasons)),
            )
            .expect("a located span lies inside the book it was found in");
        }
    }

    fn firing(
        &self,
        aggregate: &BookAggregate,
        patterns: &[crate::judge::Pattern],
        out: &mut Vec<PatternIndex>,
    ) {
        sites::firing(aggregate, patterns, out);
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
