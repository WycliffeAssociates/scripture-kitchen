//! Level 2 words: one scan per chapter, counted by hash, by case form, and by
//! the glyph that stood before the word.
//!
//! ```text
//! map("Then david went. David wept. DAVID sang.")
//!   hash(david)  Glyph('.')  [Lower 0, Title 1, Upper 0, Mixed 0]  len 5
//!   hash(david)  None        [Lower 1, Title 0, Upper 1, Mixed 0]  len 5
//!   hash(then)   Start       [Lower 0, Title 1, Upper 0, Mixed 0]  len 4
//!   hash(went)   None        [Lower 1, …]                          len 4
//!   …
//! judge, terminal table forces '.', word_support_floor 2, 25% at this size
//!   → Casing { hash(david), Upper }   1/2   5,000 bp   band 0
//! ```
//!
//! The walk decides nothing about capitals; the judge splits free from forced
//! by asking the corpus's own terminal table what each stored glyph does. The
//! row skips every word with no cased letter: it can hold no casing
//! convention, so an uncased script hashes nothing and stores nothing. The
//! walk rule, the terminal rule, and the fold's seam argument: words.md.

use rustc_hash::FxHashMap;

use crate::judge::{JudgingConfig, Pattern, PatternIndex, PatternKey, TerminalTable};
use crate::pass::{ChapterInput, ChapterObs, ChapterPass, CorpusTotals, Findings, SchemaStamp};
use crate::substrate::ScalarKey;
use crate::{BookIndex, Chapter, ConventionDigest, FindingKind, Reasons, TextRange, Verse};

pub(crate) mod fold;
#[cfg(test)]
mod tests;
mod totals;
pub mod walk;

pub use fold::fold_book;
pub use totals::{WordTally, WordTotals};
pub use walk::{Occurrence, for_each_word};

// ── What stood before ───────────────────────────────────────────────────

/// What the walk saw in front of one word occurrence.
///
/// The row stores this and nothing more; whether a `Glyph` *forces* a capital
/// is a corpus fact the judge reads off [`TerminalTable`], so the same walk
/// serves a corpus that capitalizes after a comma and one that does not.
/// Quotes and brackets are transparent, so an opening quote records the
/// terminal behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Before {
    /// A word, or space and then a word.
    None,
    /// The first word of a chapter or of a verse.
    Start,
    /// The last atom of the nonletter run in front of it.
    Glyph(ScalarKey),
}

impl Before {
    /// One past the last scalar, so a packed `Before` sorts glyphs first and
    /// never collides with one.
    const NONE_RAW: u32 = 0x0011_0000;
    const START_RAW: u32 = 0x0011_0001;

    /// The four bytes the row stores this in, beside the length.
    pub const fn raw(self) -> u32 {
        match self {
            Self::None => Self::NONE_RAW,
            Self::Start => Self::START_RAW,
            Self::Glyph(key) => key.raw(),
        }
    }

    /// `None` for a value that is neither sentinel nor scalar.
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            Self::NONE_RAW => Some(Self::None),
            Self::START_RAW => Some(Self::Start),
            _ => match ScalarKey::from_raw(raw) {
                Some(key) => Some(Self::Glyph(key)),
                None => None,
            },
        }
    }

    /// Whether the WORD chose the capital here, which is the only evidence a
    /// casing claim may use. A chapter or verse start never does.
    pub fn is_free(self, table: &TerminalTable) -> bool {
        match self {
            Self::None => true,
            Self::Start => false,
            Self::Glyph(key) => !table.forces(key),
        }
    }
}

/// Ordered by the packed value, so a row sorted by `(hash, before.raw())` is
/// sorted by `(hash, before)` too.
impl Ord for Before {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.raw().cmp(&other.raw())
    }
}

impl PartialOrd for Before {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

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

/// One case-folded word's counts in one chapter, under one [`Before`].
///
/// The key is `(hash, before)`: words fit a `u64` only 12-23% of the time
/// outside Latin, so the row keys by xxh3-64 and carries the scalar count
/// beside it — a hash cannot give a length back. `before` is packed into the
/// four bytes next to that count, which is what keeps the row at 24 B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordCount {
    /// xxh3-64 of the word's case-folded scalars.
    pub hash: u64,
    /// Occurrences indexed by [`Form`]: Lower, Title, Upper, Mixed.
    /// Saturating.
    pub counts: [u16; 4],
    /// [`Before`], packed.
    before: u32,
    /// Scalar count of the first occurrence, saturating.
    pub len: u8,
}

impl WordCount {
    pub(crate) const fn new(hash: u64, before: Before, len: u8) -> Self {
        Self {
            hash,
            counts: [0; 4],
            before: before.raw(),
            len,
        }
    }

    /// What stood in front of these occurrences.
    pub const fn before(&self) -> Before {
        match Before::from_raw(self.before) {
            Some(before) => before,
            None => Before::None,
        }
    }

    pub const fn count_of(&self, form: Form) -> u16 {
        match form {
            Form::Uncased => 0,
            form => self.counts[form as usize],
        }
    }

    pub const fn total(&self) -> u32 {
        self.counts[0] as u32
            + self.counts[1] as u32
            + self.counts[2] as u32
            + self.counts[3] as u32
    }
}

/// One chapter's word counts: sorted by hash, detached, coordinate-free.
///
/// [`ChapterPass::release`] leaves the empty row a host keeps when it retains
/// this pass at book grain ([`ChapterPass::RETAIN_CHAPTERS`]) — the same 24 B
/// an uncased chapter's row costs, and flagged, so an uncased chapter's
/// honestly empty row is never mistaken for a shed one. [`Default`] is NOT
/// released.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordRow {
    words: Box<[WordCount]>,
    cased: bool,
    /// Set only by [`ChapterPass::release`]; read by
    /// [`ChapterPass::is_released`] to decide the row must be walked again.
    released: bool,
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

/// One case-folded word's counts over one book, under one [`Before`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordTotal {
    pub hash: u64,
    pub counts: [u32; 4],
    /// [`Before`], packed.
    before: u32,
    pub len: u8,
}

impl WordTotal {
    pub(crate) const fn new(hash: u64, before: u32, counts: [u32; 4], len: u8) -> Self {
        Self {
            hash,
            counts,
            before,
            len,
        }
    }

    pub const fn before(&self) -> Before {
        match Before::from_raw(self.before) {
            Some(before) => before,
            None => Before::None,
        }
    }

    /// The packed key the rows are sorted by, after the hash.
    pub(crate) const fn before_raw(&self) -> u32 {
        self.before
    }

    pub const fn count_of(&self, form: Form) -> u32 {
        match form {
            Form::Uncased => 0,
            form => self.counts[form as usize],
        }
    }

    pub const fn total(&self) -> u64 {
        self.counts[0] as u64
            + self.counts[1] as u64
            + self.counts[2] as u64
            + self.counts[3] as u64
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

    /// Sorted by `(hash, before)`, so the judge merges books without hashing
    /// again and one word's rows are contiguous.
    pub fn words(&self) -> &[WordTotal] {
        &self.words
    }

    /// Whether the book holds a cased letter. A corpus of `false` emits no
    /// casing row and does no work.
    pub const fn cased(&self) -> bool {
        self.cased
    }

    /// Every row this book holds for one word, one per [`Before`] it was seen
    /// under; empty when the book does not hold it.
    pub fn rows_for(&self, hash: u64) -> &[WordTotal] {
        let Ok(at) = self.words.binary_search_by_key(&hash, |row| row.hash) else {
            return &[];
        };
        let mut from = at;
        while from > 0 && self.words[from - 1].hash == hash {
            from -= 1;
        }
        let mut to = at + 1;
        while to < self.words.len() && self.words[to].hash == hash {
            to += 1;
        }
        &self.words[from..to]
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
        *obs = WordRow {
            released: true,
            ..WordRow::default()
        };
    }

    /// The flag, not the emptiness: an uncased chapter's row is empty and
    /// still whole.
    fn is_released(&self, observation: &WordRow) -> bool {
        observation.released
    }

    /// Nothing to judge until the substrate has published the corpus's
    /// terminal table into the sink: forced and free are read off it, and a
    /// word channel that guessed instead would make a claim from a rule
    /// nobody measured. `Brigade` judges `Substrate` first, which is what puts
    /// it there; `Words` alone abstains and says so.
    fn judge(&self, corpus: &[&WordAggregate], config: &JudgingConfig, out: &mut Findings) {
        if !judges_anything(config) || corpus.iter().all(|book| !book.cased()) {
            return;
        }
        let Some(table) = out.terminals().cloned() else {
            return;
        };
        crate::judge::judge_words(corpus, &WordTotals::merge(corpus), &table, config, out);
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

    /// The same channels over a tally a host already holds; the merge is the
    /// only thing it skips.
    fn judge_resident(
        &self,
        corpus: &[&WordAggregate],
        totals: &CorpusTotals,
        config: &JudgingConfig,
        out: &mut Findings,
    ) {
        if !judges_anything(config) || corpus.iter().all(|book| !book.cased()) {
            return;
        }
        let Some(table) = out.terminals().cloned() else {
            return;
        };
        crate::judge::judge_words(corpus, &totals.words, &table, config, out);
    }

    /// Rewalks this book's words and sites what the counts named: a casing
    /// row's free occurrences of one `(hash, form)`, and a length row's every
    /// occurrence of one hash. The rescan reads the same terminal table the
    /// judge did, or it would place positions the counts never held.
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
        let table = out.terminals().cloned().unwrap_or_default();
        // Copied because `Findings` cannot lend its table and take a row at
        // once; a firing set is tens of rows, not thousands.
        let mut cased: FxHashMap<(u64, Form), PatternIndex> = FxHashMap::default();
        let mut long: FxHashMap<u64, PatternIndex> = FxHashMap::default();
        for &index in &set {
            match out.patterns()[usize::from(index.get())].key {
                PatternKey::Casing { hash, form } => {
                    cased.insert((hash, form), index);
                }
                PatternKey::WordLength { hash, .. } => {
                    long.insert(hash, index);
                }
                _ => {}
            }
        }

        let mut found: Vec<(TextRange, PatternIndex, Reasons)> = Vec::new();
        let mut rebased: Vec<Verse> = Vec::new();
        let mut cursor = 0usize;
        for chapter in chapters {
            let span = chapter.text();
            let slice = &text[span.from() as usize..span.to() as usize];
            cursor = chapter_verses(verses, cursor, span, &mut rebased);
            for_each_word(slice, &rebased, |word| {
                if word.form == Form::Uncased {
                    return;
                }
                let casing = word
                    .before
                    .is_free(&table)
                    .then(|| cased.get(&(word.hash, word.form)))
                    .flatten();
                let length = long.get(&word.hash);
                // One span, one row: a word both channels named carries both
                // reasons and the finer channel's index, as a run does.
                let (index, reasons) = match (casing, length) {
                    (Some(&index), Some(_)) => (index, Reasons::CASING.union(Reasons::WORD_LENGTH)),
                    (Some(&index), None) => (index, Reasons::CASING),
                    (None, Some(&index)) => (index, Reasons::WORD_LENGTH),
                    (None, None) => return,
                };
                let at = TextRange::new(span.from() + word.from, span.from() + word.to)
                    .expect("a word grows forward");
                found.push((at, index, reasons));
            });
        }
        if found.is_empty() {
            return;
        }
        out.open_book(book);
        for (span, index, reasons) in found {
            out.push(
                span,
                FindingKind::Convention(ConventionDigest::new(index, reasons)),
            )
            .expect("a located word lies inside the book it was found in");
        }
    }

    /// One merge walk over two hash-sorted lists, not one binary search per
    /// row: the judge emits word rows in hash order and the aggregate is
    /// sorted the same way, and a probe per row into a 5k-row table for each
    /// of 66 books was a cache miss per probe — a quarter of a keystroke.
    ///
    /// Position-blind on purpose: a book claims a row whose word it holds at
    /// all, and `locate` applies the free/forced split. A superset costs a
    /// rescan that finds nothing; reading the terminal table here would put a
    /// judging decision in a cache key.
    fn firing(&self, aggregate: &WordAggregate, patterns: &[Pattern], out: &mut Vec<PatternIndex>) {
        out.clear();
        if !aggregate.cased() {
            return;
        }
        let words = aggregate.words();
        let mut at = 0usize;
        let mut last = 0u64;
        for (index, pattern) in patterns.iter().enumerate() {
            let Some(hash) = pattern.word_hash() else {
                continue;
            };
            if hash < last {
                // Rows out of hash order: fall back to the probe for this one.
                if holds(aggregate.rows_for(hash), pattern) {
                    out.push(PatternIndex::new(index as u16));
                }
                continue;
            }
            last = hash;
            while at < words.len() && words[at].hash < hash {
                at += 1;
            }
            let mut end = at;
            while end < words.len() && words[end].hash == hash {
                end += 1;
            }
            if holds(&words[at..end], pattern) {
                out.push(PatternIndex::new(index as u16));
            }
        }
    }
}

/// Whether this book's rows for one word hold what the pattern counts.
fn holds(rows: &[WordTotal], pattern: &Pattern) -> bool {
    match pattern.key {
        PatternKey::Casing { form, .. } => rows.iter().any(|row| row.count_of(form) > 0),
        PatternKey::WordLength { .. } => !rows.is_empty(),
        _ => false,
    }
}

/// Whether either word channel is on.
const fn judges_anything(config: &JudgingConfig) -> bool {
    config.channels.casing || config.channels.word_length
}

/// One book's contribution to a word pattern's numerator — the oracle
/// `tests/casing_agree_with_counts.rs` measures the rescan against.
///
/// `table` is the corpus's, because which stored `Before`s are free is a
/// corpus fact and not a property of this book.
pub fn free_in(book: &WordAggregate, pattern: &Pattern, table: &TerminalTable) -> u64 {
    crate::judge::free_of(book, &pattern.key, table)
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
