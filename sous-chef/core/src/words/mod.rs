//! Level 2 words: one scan per chapter, counted by hash, case form, and
//! whether the position forced the capital.
//!
//! ```text
//! map("Then david went. David wept. DAVID sang.")
//!   hash(david)  free [Lower 1, Title 0, Upper 1, Mixed 0]  forced 1  len 5
//!   hash(then)   free [Lower 0, Title 1, Upper 0, Mixed 0]  forced 0  len 4
//!   hash(went)   free [Lower 1, …]                          forced 0  len 4
//!   …
//! judge over the corpus, word_support_floor 2, bands 25% at this size
//!   → Casing { hash(david), Upper }   1/2   5,000 bp   band 0
//! ```
//!
//! The row skips every word with no cased letter: it can hold no casing
//! convention, so an uncased script hashes nothing and stores nothing. The
//! walk rule, the forced rule, and the fold's seam argument: words.md.

use rustc_hash::FxHashMap;

use crate::judge::{Channel, JudgingConfig, Pattern, PatternIndex, PatternKey};
use crate::pass::{ChapterInput, ChapterObs, ChapterPass, CorpusTotals, Findings, SchemaStamp};
use crate::{BookIndex, Chapter, ConventionDigest, FindingKind, Reasons, TextRange, Verse};

pub(crate) mod fold;
#[cfg(test)]
mod tests;
mod totals;
pub mod walk;

pub use fold::fold_book;
pub use totals::{WordTally, WordTotals};
pub use walk::{Occurrence, for_each_word};

// ── The case form ───────────────────────────────────────────────────────

/// How one word occurrence is cased.
///
/// The four judged forms come first, so a form doubles as an index into
/// [`WordCount::free`]; `Uncased` is counted nowhere and refused on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Form {
    /// A cased letter, and no uppercase one.
    Lower = 0,
    /// The first letter is uppercase and no other letter is.
    Title = 1,
    /// Every cased letter is uppercase, and there are at least two.
    Upper = 2,
    /// Anything else holding an uppercase letter.
    Mixed = 3,
    /// No cased letter at all; never judged, never stored.
    Uncased = 4,
}

impl Form {
    /// The forms a [`WordCount::free`] lane holds, in lane order.
    pub const JUDGED: [Self; 4] = [Self::Lower, Self::Title, Self::Upper, Self::Mixed];
    pub const ALL: [Self; 5] = [
        Self::Lower,
        Self::Title,
        Self::Upper,
        Self::Mixed,
        Self::Uncased,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Lower => "Lower",
            Self::Title => "Title",
            Self::Upper => "Upper",
            Self::Mixed => "Mixed",
            Self::Uncased => "Uncased",
        }
    }

    /// `None` for a discriminant past the table.
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Lower),
            1 => Some(Self::Title),
            2 => Some(Self::Upper),
            3 => Some(Self::Mixed),
            4 => Some(Self::Uncased),
            _ => None,
        }
    }
}

// ── The observation ─────────────────────────────────────────────────────

/// One case-folded word's counts in one chapter.
///
/// The hash is the key: words fit a `u64` only 12-23% of the time outside
/// Latin, so the row keys by xxh3-64 and carries the scalar count beside it —
/// a hash cannot give a length back, and length is a fold over these rows
/// rather than a second lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordCount {
    /// xxh3-64 of the word's case-folded scalars.
    pub hash: u64,
    /// Free-position occurrences, indexed by [`Form`]: Lower, Title, Upper,
    /// Mixed. Saturating.
    pub free: [u16; 4],
    /// Occurrences the position forced, which no casing claim may use.
    pub forced: u16,
    /// Scalar count of the first occurrence, saturating.
    pub len: u8,
}

impl WordCount {
    pub const fn free_of(&self, form: Form) -> u16 {
        match form {
            Form::Uncased => 0,
            form => self.free[form as usize],
        }
    }

    pub const fn free_total(&self) -> u32 {
        self.free[0] as u32 + self.free[1] as u32 + self.free[2] as u32 + self.free[3] as u32
    }
}

/// One chapter's word counts: sorted by hash, detached, coordinate-free.
///
/// [`Default`] is the empty row a host leaves behind when it retains this pass
/// at book grain ([`ChapterPass::RETAIN_CHAPTERS`]) — the same 24 B an uncased
/// chapter's row costs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordRow {
    words: Box<[WordCount]>,
    cased: bool,
}

impl WordRow {
    /// Every case-folded word the chapter holds that has a cased letter,
    /// sorted by hash.
    pub fn words(&self) -> &[WordCount] {
        &self.words
    }

    /// Whether the chapter holds a cased letter at all.
    pub const fn cased(&self) -> bool {
        self.cased
    }

    /// Inline size plus every byte the lane owns; what a resident cache pays.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>() + size_of_val(&*self.words)
    }
}

// ── The book aggregate ──────────────────────────────────────────────────

/// One case-folded word's counts over one book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordTotal {
    pub hash: u64,
    pub free: [u32; 4],
    pub forced: u32,
    pub len: u8,
}

impl WordTotal {
    pub const fn free_of(&self, form: Form) -> u32 {
        match form {
            Form::Uncased => 0,
            form => self.free[form as usize],
        }
    }

    pub const fn free_total(&self) -> u64 {
        self.free[0] as u64 + self.free[1] as u64 + self.free[2] as u64 + self.free[3] as u64
    }
}

/// One book's word counts, merged by hash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordAggregate {
    words: Vec<WordTotal>,
    cased: bool,
}

impl WordAggregate {
    pub(crate) fn new(words: Vec<WordTotal>, cased: bool) -> Self {
        Self { words, cased }
    }

    /// Sorted by hash, so the judge merges books without hashing again.
    pub fn words(&self) -> &[WordTotal] {
        &self.words
    }

    /// Whether the book holds a cased letter. A corpus of `false` emits no
    /// casing row and does no work.
    pub const fn cased(&self) -> bool {
        self.cased
    }

    pub fn get(&self, hash: u64) -> Option<&WordTotal> {
        self.words
            .binary_search_by_key(&hash, |row| row.hash)
            .ok()
            .map(|at| &self.words[at])
    }

    /// Inline size plus the words vector's real heap: capacity, not length,
    /// since `fold_book` reserves `with_capacity` once and never regrows.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>() + self.words.capacity() * size_of::<WordTotal>()
    }
}

// ── The pass ────────────────────────────────────────────────────────────

/// The Level 2 word walk, riding beside [`crate::Substrate`] over the same
/// chapters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Words;

impl ChapterPass for Words {
    type Observation = WordRow;
    type Aggregate = WordAggregate;
    /// The same type `Substrate` judges under: the word knobs live in the one
    /// config struct, and a host sets the same value in both tuple slots.
    type Config = JudgingConfig;
    const SCHEMA: SchemaStamp = SchemaStamp::new(3);
    /// Book grain: a chapter's word rows are 5 KB of cased Latin against the
    /// substrate's 0.3, and the whole book is rewalked for the ~200 µs a phone
    /// never notices. `rules/word-conventions.md` carries the ruling.
    const RETAIN_CHAPTERS: bool = false;

    fn map(&self, chapter: ChapterInput<'_>) -> WordRow {
        walk::walk(chapter.text, chapter.verses)
    }

    fn fold(&self, book: &[ChapterObs<&WordRow>]) -> WordAggregate {
        fold_book(book)
    }

    fn release(&self, obs: &mut WordRow) {
        *obs = WordRow::default();
    }

    fn judge(&self, corpus: &[&WordAggregate], config: &JudgingConfig, out: &mut Findings) {
        if !config.channels.casing {
            return;
        }
        crate::judge::judge_casing(corpus, config, out);
    }

    fn aggregate_bytes(&self, aggregate: &WordAggregate) -> usize {
        aggregate.resident_bytes()
    }

    fn tally(&self, totals: &mut CorpusTotals, books: &[&WordAggregate]) {
        totals.words.add(books);
    }

    fn untally(&self, totals: &mut CorpusTotals, books: &[&WordAggregate]) {
        totals.words.remove(books);
    }

    fn judge_resident(
        &self,
        _corpus: &[&WordAggregate],
        totals: &CorpusTotals,
        config: &JudgingConfig,
        out: &mut Findings,
    ) {
        if !config.channels.casing {
            return;
        }
        crate::judge::judge_casing_totals(&totals.words, config, out);
    }

    /// Rewalks this book's words and sites every free occurrence whose
    /// `(hash, form)` a pattern named.
    fn locate(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &WordAggregate,
        out: &mut Findings,
    ) {
        let mut set = Vec::new();
        self.firing(aggregate, out.patterns(), &mut set);
        if set.is_empty() {
            return;
        }
        // Copied because `Findings` cannot lend its table and take a row at
        // once; a firing set is tens of rows, not thousands.
        let wanted: FxHashMap<(u64, Form), PatternIndex> = set
            .iter()
            .filter_map(|&index| {
                let pattern = out.patterns()[usize::from(index.get())];
                let PatternKey::Casing { hash, form } = pattern.key else {
                    return None;
                };
                Some(((hash, form), index))
            })
            .collect();

        let mut found: Vec<(TextRange, PatternIndex)> = Vec::new();
        let mut rebased: Vec<Verse> = Vec::new();
        let mut cursor = 0usize;
        for chapter in chapters {
            let span = chapter.text();
            let slice = &text[span.from() as usize..span.to() as usize];
            cursor = chapter_verses(verses, cursor, span, &mut rebased);
            for_each_word(slice, &rebased, |word| {
                if word.forced || word.form == Form::Uncased {
                    return;
                }
                if let Some(&index) = wanted.get(&(word.hash, word.form)) {
                    let at = TextRange::new(span.from() + word.from, span.from() + word.to)
                        .expect("a word grows forward");
                    found.push((at, index));
                }
            });
        }
        if found.is_empty() {
            return;
        }
        out.open_book(book);
        for (span, index) in found {
            out.push(
                span,
                FindingKind::Convention(ConventionDigest::new(index, Reasons::CASING)),
            )
            .expect("a located word lies inside the book it was found in");
        }
    }

    /// One merge walk over two hash-sorted lists, not one binary search per
    /// row: the judge emits casing rows in hash order and the aggregate is
    /// sorted the same way, and a probe per row into a 5k-row table for each
    /// of 66 books was a cache miss per probe — a quarter of a keystroke.
    fn firing(&self, aggregate: &WordAggregate, patterns: &[Pattern], out: &mut Vec<PatternIndex>) {
        out.clear();
        if !aggregate.cased() {
            return;
        }
        let words = aggregate.words();
        let mut at = 0usize;
        let mut last = 0u64;
        for (index, pattern) in patterns.iter().enumerate() {
            let PatternKey::Casing { hash, form } = pattern.key else {
                continue;
            };
            if hash < last {
                // Rows out of hash order: fall back to the probe for this one.
                if aggregate.get(hash).is_some_and(|row| row.free_of(form) > 0) {
                    out.push(PatternIndex::new(index as u16));
                }
                continue;
            }
            last = hash;
            while at < words.len() && words[at].hash < hash {
                at += 1;
            }
            if at < words.len() && words[at].hash == hash && words[at].free_of(form) > 0 {
                out.push(PatternIndex::new(index as u16));
            }
        }
    }
}

/// One book's contribution to a casing pattern's numerator — the oracle
/// `tests/casing_agree_with_counts.rs` measures the rescan against.
pub fn free_in(book: &WordAggregate, pattern: &Pattern) -> u64 {
    if pattern.channel != Channel::Casing {
        return 0;
    }
    let PatternKey::Casing { hash, form } = pattern.key else {
        return 0;
    };
    book.get(hash).map_or(0, |row| u64::from(row.free_of(form)))
}

/// This chapter's verse rows, rebased to it, returning where the next chapter
/// resumes. Rows are non-decreasing, so each chapter's run is contiguous.
fn chapter_verses(verses: &[Verse], mut at: usize, span: TextRange, out: &mut Vec<Verse>) -> usize {
    out.clear();
    while verses
        .get(at)
        .is_some_and(|row| row.text().to() <= span.from())
    {
        at += 1;
    }
    while let Some(row) = verses.get(at).filter(|row| row.text().to() <= span.to()) {
        let text = row.text();
        if text.from() >= span.from() {
            let rebased = TextRange::new(text.from() - span.from(), text.to() - span.from())
                .expect("a chapter-relative range keeps its order");
            out.push(Verse::new(row.key(), rebased));
        }
        at += 1;
    }
    at
}
