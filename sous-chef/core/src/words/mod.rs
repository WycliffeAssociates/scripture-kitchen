//! Level 2 words: one scan per chapter, three lanes out of it — counts by
//! hash, case form and preceding glyph; doubling by hash alone; and the runs
//! of one letter inside a word.
//!
//! ```text
//! map("Then david went. David wept. DAVID sang. Go go. Theee.")
//!   casing lane
//!     hash(david)  Glyph('.')  [Lower 0, Title 1, Upper 0, Mixed 0]  len 5
//!     hash(david)  None        [Lower 1, Title 0, Upper 1, Mixed 0]  len 5
//!     hash(then)   Start       [Lower 0, Title 1, Upper 0, Mixed 0]  len 4
//!     hash(went)   None        [Lower 1, …]                          len 4
//!     …
//!   doubles lane
//!     hash(go)     uncased 0  bare 1  separated 0
//!   letter-run lane
//!     'e'          [0, 1, 0, 0, 0, 0, 0]   // one run of three, in `Theee`
//! judge, terminal table forces '.', word_support_floor 2, 25% at this size
//!   → Casing { hash(david), Upper }   1/2   5,000 bp   band 0
//! ```
//!
//! The walk decides nothing about capitals; the judge splits free from forced
//! by asking the corpus's own terminal table what each stored glyph does. The
//! casing lane skips every word with no cased letter — it can hold no casing
//! convention — and the other two lanes count those words instead, because
//! neither doubling nor a repeated letter has anything to do with case. The
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
mod verdicts;
pub mod walk;

pub use fold::fold_book;
pub use totals::{DoubleTally, RunTally, WordTally, WordTotals};
pub use verdicts::{MovedWords, WordVerdicts};
pub use walk::{Gap, Occurrence, for_each_letter_run, for_each_word, gap_between, word_around};

// ── The letter-run lane ─────────────────────────────────────────────────

/// The shortest run the lane counts; a single letter is not a repeat.
pub const LETTER_RUN_MIN: u8 = 2;
/// Lanes per letter, one per length in [`LETTER_RUN_MIN`]`..=`[`LETTER_RUN_MAX`].
pub const LETTER_RUN_LANES: usize = 7;
/// The longest length the lane distinguishes; longer runs saturate into it.
pub const LETTER_RUN_MAX: u8 = LETTER_RUN_MIN + LETTER_RUN_LANES as u8 - 1;

/// The lane a run of `length` is counted in.
pub(crate) const fn letter_run_lane(length: u8) -> usize {
    (length - LETTER_RUN_MIN) as usize
}

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

// ── The doubles lane ────────────────────────────────────────────────────

/// One case-folded word's doubling in one chapter, keyed by hash alone: a
/// double is a double whatever stood before it.
///
/// `uncased` is the lane's other job. A word with no cased letter is refused
/// by [`WordCount`], so the two lanes **partition** a word's occurrences and
/// the doubled channel's denominator is their sum — which is what lets an
/// uncased script be judged for doubling at all.
///
/// `separated` is a lane of its own, keyed by the separator's last glyph and
/// sorted by it: a pair split by a glyph that forces a capital in this
/// corpus's own [`TerminalTable`] is two sentences, not a double, and the walk
/// cannot know the table. Rows exist only for a word that actually doubled, so
/// the extra heap word this lane costs is spent on rows that are already rare
/// (`words.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubleCount {
    /// xxh3-64 of the word's case-folded scalars.
    pub hash: u64,
    /// Occurrences of this word with no cased letter. Saturating.
    pub uncased: u16,
    /// Times this word was immediately followed by itself, whitespace only
    /// between. Saturating.
    pub bare: u16,
    /// The same with a nonletter run between (`na, na`), one entry per
    /// distinct last glyph of the run, ascending, each count saturating.
    pub separated: Box<[(ScalarKey, u16)]>,
}

impl DoubleCount {
    pub(crate) fn new(hash: u64) -> Self {
        Self {
            hash,
            uncased: 0,
            bare: 0,
            separated: Box::default(),
        }
    }

    pub(crate) fn add(&mut self, gap: Gap) {
        match gap {
            Gap::Bare => self.bare = self.bare.saturating_add(1),
            Gap::Separated(glyph) => self.add_separated(glyph),
        }
    }

    fn add_separated(&mut self, glyph: ScalarKey) {
        match self.separated.binary_search_by_key(&glyph, |&(g, _)| g) {
            Ok(at) => {
                let count = &mut self.separated[at].1;
                *count = count.saturating_add(1);
            }
            Err(at) => {
                let mut grown = Vec::with_capacity(self.separated.len() + 1);
                grown.extend_from_slice(&self.separated[..at]);
                grown.push((glyph, 1));
                grown.extend_from_slice(&self.separated[at..]);
                self.separated = grown.into_boxed_slice();
            }
        }
    }

    /// Every separated occurrence, whatever the separator — the superset a
    /// book claims a row for, before the judge's own forcing filter narrows
    /// it to a numerator.
    pub fn separated_total(&self) -> u32 {
        self.separated
            .iter()
            .map(|&(_, count)| u32::from(count))
            .sum()
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
    doubles: Box<[DoubleCount]>,
    letter_runs: Box<[(ScalarKey, [u16; LETTER_RUN_LANES])]>,
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

    /// Every word the chapter doubled, plus every word with an uncased
    /// occurrence, sorted by hash.
    pub fn doubles(&self) -> &[DoubleCount] {
        &self.doubles
    }

    /// Every letter the chapter repeated inside a word, ascending, with one
    /// counter per run length `2..=8+`.
    pub fn letter_runs(&self) -> &[(ScalarKey, [u16; LETTER_RUN_LANES])] {
        &self.letter_runs
    }

    /// Whether the chapter holds a cased letter at all.
    pub const fn cased(&self) -> bool {
        self.cased
    }

    /// Inline size plus every byte the three lanes own, the doubles lane's own
    /// per-glyph heap included; what a resident cache pays.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>()
            + size_of_val(&*self.words)
            + size_of_val(&*self.doubles)
            + size_of_val(&*self.letter_runs)
            + self
                .doubles
                .iter()
                .map(|row| size_of_val(&*row.separated))
                .sum::<usize>()
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

/// One case-folded word's doubling over one book, `separated` widened to
/// `u32` per glyph and merged by [`merge_glyph_lane`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DoubleTotal {
    pub hash: u64,
    pub uncased: u32,
    pub bare: u32,
    /// One entry per distinct separator glyph, ascending.
    pub separated: Box<[(ScalarKey, u32)]>,
}

impl DoubleTotal {
    /// Every occurrence of this lane, whatever the separator — the superset a
    /// book claims a row for; [`Self::free_separated`] is the judged
    /// numerator.
    pub fn count_of(&self, separated: bool) -> u64 {
        if separated {
            self.separated
                .iter()
                .map(|&(_, count)| u64::from(count))
                .sum()
        } else {
            u64::from(self.bare)
        }
    }

    /// The separated lane's occurrences whose last glyph does NOT force a
    /// capital in `table` — a pair whose separator forces one is a sentence
    /// boundary, not a doubling.
    pub fn free_separated(&self, table: &TerminalTable) -> u64 {
        self.separated
            .iter()
            .filter(|&&(glyph, _)| !table.forces(glyph))
            .map(|&(_, count)| u64::from(count))
            .sum()
    }
}

/// One book's word counts, merged by hash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordAggregate {
    words: Vec<WordTotal>,
    doubles: Vec<DoubleTotal>,
    letter_runs: Vec<(ScalarKey, [u32; LETTER_RUN_LANES])>,
    cased: bool,
}

impl WordAggregate {
    pub(crate) fn new(
        words: Vec<WordTotal>,
        doubles: Vec<DoubleTotal>,
        letter_runs: Vec<(ScalarKey, [u32; LETTER_RUN_LANES])>,
        cased: bool,
    ) -> Self {
        Self {
            words,
            doubles,
            letter_runs,
            cased,
        }
    }

    /// The letter-run lane, by letter ascending.
    pub fn letter_runs(&self) -> &[(ScalarKey, [u32; LETTER_RUN_LANES])] {
        &self.letter_runs
    }

    /// This book's run counts for one letter, if it repeated it at all.
    pub fn letter_runs_for(&self, letter: ScalarKey) -> Option<&[u32; LETTER_RUN_LANES]> {
        self.letter_runs
            .binary_search_by_key(&letter, |row| row.0)
            .ok()
            .map(|at| &self.letter_runs[at].1)
    }

    /// The doubles lane, by hash ascending.
    pub fn doubles(&self) -> &[DoubleTotal] {
        &self.doubles
    }

    /// This book's doubles row for one word, if it holds one.
    pub fn doubles_for(&self, hash: u64) -> Option<&DoubleTotal> {
        self.doubles
            .binary_search_by_key(&hash, |row| row.hash)
            .ok()
            .map(|at| &self.doubles[at])
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

    /// Inline size plus both lanes' real heap: capacity, not length, since
    /// `fold_book` reserves `with_capacity` once and shrinks after the merge.
    /// Each doubles row's own per-glyph lane is separate heap, counted by its
    /// length since that box is never over-reserved.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>()
            + self.words.capacity() * size_of::<WordTotal>()
            + self.doubles.capacity() * size_of::<DoubleTotal>()
            + self.letter_runs.capacity() * size_of::<(ScalarKey, [u32; LETTER_RUN_LANES])>()
            + self
                .doubles
                .iter()
                .map(|row| size_of_val(&*row.separated))
                .sum::<usize>()
    }
}

/// Merges two glyph-keyed lanes, sorted ascending, into a third: `add` sums a
/// shared key, `remove` (`add: false`) subtracts and drops any key whose count
/// reaches zero — which only removal can produce, and only when the last book
/// holding that separator glyph leaves. Shared by the book fold and the corpus
/// tally, since both merge the same shape.
pub(crate) fn merge_glyph_lane(
    a: &[(ScalarKey, u32)],
    b: &[(ScalarKey, u32)],
    add: bool,
) -> Box<[(ScalarKey, u32)]> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        match (a.get(i), b.get(j)) {
            (Some(&(ka, va)), Some(&(kb, vb))) => match ka.cmp(&kb) {
                core::cmp::Ordering::Less => {
                    out.push((ka, va));
                    i += 1;
                }
                core::cmp::Ordering::Greater => {
                    if add {
                        out.push((kb, vb));
                    }
                    j += 1;
                }
                core::cmp::Ordering::Equal => {
                    let merged = if add {
                        va.saturating_add(vb)
                    } else {
                        va.saturating_sub(vb)
                    };
                    if merged > 0 {
                        out.push((ka, merged));
                    }
                    i += 1;
                    j += 1;
                }
            },
            (Some(&(ka, va)), None) => {
                out.push((ka, va));
                i += 1;
            }
            (None, Some(&(kb, vb))) => {
                if add {
                    out.push((kb, vb));
                }
                j += 1;
            }
            (None, None) => break,
        }
    }
    out.into_boxed_slice()
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
    /// The walk restarts at every chapter — a word chain and a doubled pair
    /// both end at a seam — so one chapter's rows are the rows a whole-book
    /// walk finds there.
    const CHAPTER_SITES: bool = true;

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
        if !judges_anything(corpus, config) {
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

    /// The rows, not the 24-byte header: a host keeping a hot book's chapters
    /// unreleased pays this per chapter.
    fn observation_bytes(&self, observation: &WordRow) -> usize {
        observation.resident_bytes()
    }

    fn tally(&self, totals: &mut CorpusTotals, books: &[&WordAggregate]) {
        totals.words.add(books);
    }

    fn untally(&self, totals: &mut CorpusTotals, books: &[&WordAggregate]) {
        totals.words.remove(books);
    }

    fn moved_keys(&self, books: &[&WordAggregate], moved: &mut MovedWords) {
        moved.absorb(books);
    }

    /// The same channels again, over the tally keys `moved` names alone: the
    /// rest of the list is the one `kept` already holds.
    ///
    /// A corpus with nothing to judge, or one whose terminal table has not
    /// been published yet, forgets the kept list rather than keeping it beside
    /// evidence that never arrived.
    fn judge_kept(
        &self,
        corpus: &[&WordAggregate],
        totals: &CorpusTotals,
        config: &JudgingConfig,
        moved: &MovedWords,
        kept: &mut WordVerdicts,
        out: &mut Findings,
    ) {
        if !judges_anything(corpus, config) {
            kept.clear();
            return;
        }
        let Some(table) = out.terminals().cloned() else {
            kept.clear();
            return;
        };
        kept.judge(corpus, &totals.words, &table, config, moved, out);
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
        if !judges_anything(corpus, config) {
            return;
        }
        let Some(table) = out.terminals().cloned() else {
            return;
        };
        crate::judge::judge_words(corpus, &totals.words, &table, config, out);
    }

    /// Rewalks this book's words and sites what the counts named: a casing
    /// row's free occurrences of one `(hash, form)`, a length row's every
    /// occurrence of one hash, a doubled row's every pair, whose span covers
    /// both words and the separator, and a letter-run row's every run, whose
    /// span is the word the run sits inside. The rescan reads the same
    /// terminal table the judge did, or it would place positions the counts
    /// never held.
    fn locate(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &WordAggregate,
        out: &mut Findings,
    ) {
        let mut counts = Vec::new();
        self.locate_chapters(
            book,
            text,
            chapters,
            verses,
            0..chapters.len(),
            aggregate,
            &mut counts,
            out,
        );
    }

    /// None: every row this pass places is the chapter's own, so
    /// [`locate_chapters`](ChapterPass::locate_chapters) places them all.
    fn locate_book(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &WordAggregate,
        out: &mut Findings,
    ) {
        let _ = (book, text, chapters, verses, aggregate, out);
    }

    /// The firing set and its lane maps are read once for the whole range; the
    /// walk itself restarts at every chapter, so one chapter's rows are the
    /// rows a whole-book walk would have found there.
    fn locate_chapters(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        range: core::ops::Range<usize>,
        aggregate: &WordAggregate,
        counts: &mut Vec<u32>,
        out: &mut Findings,
    ) {
        let Some(sites) = WordSites::of(self, aggregate, out) else {
            counts.extend(range.map(|_| 0));
            return;
        };
        let mut found: Vec<(TextRange, PatternIndex, Reasons)> = Vec::new();
        let mut rebased: Vec<Verse> = Vec::new();
        let mut cursor = 0usize;
        for chapter in &chapters[range] {
            let span = chapter.text();
            let slice = &text[span.from() as usize..span.to() as usize];
            cursor = chapter_verses(verses, cursor, span, &mut rebased);
            let before = found.len();
            sites.walk(slice, span.from(), &rebased, &mut found);
            counts.push((found.len() - before) as u32);
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

    /// One merge walk per lane, not one binary search per row: the judge emits
    /// each lane's rows in hash order and the aggregate is sorted the same way,
    /// and a probe per row into a 5k-row table for each of 66 books was a cache
    /// miss per probe — a quarter of a keystroke.
    ///
    /// On the casing lane, position-blind on purpose: a book claims a row whose
    /// word it holds at all, and `locate` applies the free/forced split. A
    /// superset costs a rescan that finds nothing; reading the terminal table
    /// here would put a judging decision in a cache key. The doubles lane's
    /// `separated` key is blind the same way now: a book claims the row when
    /// ANY separator glyph doubled the word, forcing ones included, and
    /// `locate` is where the table narrows it to the non-forcing glyphs the
    /// judge actually counted.
    fn firing(&self, aggregate: &WordAggregate, patterns: &[Pattern], out: &mut Vec<PatternIndex>) {
        out.clear();
        let words = aggregate.words();
        let doubles = aggregate.doubles();
        // One cursor per lane: each lane's rows arrive in hash order, but the
        // doubles rows come after the casing rows and restart at the lowest
        // hash again.
        let (mut word_at, mut word_last) = (0usize, 0u64);
        let (mut double_at, mut double_last) = (0usize, 0u64);
        for (index, pattern) in patterns.iter().enumerate() {
            if let PatternKey::LetterRun { length } = pattern.key {
                // Its own lane and its own key: a letter, not a hash.
                if aggregate
                    .letter_runs_for(pattern.glyph)
                    .is_some_and(|lanes| lanes[letter_run_lane(length)] > 0)
                {
                    out.push(PatternIndex::new(index as u16));
                }
                continue;
            }
            let Some(hash) = pattern.word_hash() else {
                continue;
            };
            let held = match pattern.key {
                PatternKey::Doubled { separated, .. } => {
                    if hash < double_last {
                        // Rows out of hash order: probe for this one.
                        holds_double(aggregate.doubles_for(hash), separated)
                    } else {
                        double_last = hash;
                        while double_at < doubles.len() && doubles[double_at].hash < hash {
                            double_at += 1;
                        }
                        holds_double(
                            doubles.get(double_at).filter(|row| row.hash == hash),
                            separated,
                        )
                    }
                }
                _ if !aggregate.cased() => false,
                _ if hash < word_last => holds(aggregate.rows_for(hash), pattern),
                _ => {
                    word_last = hash;
                    while word_at < words.len() && words[word_at].hash < hash {
                        word_at += 1;
                    }
                    let mut end = word_at;
                    while end < words.len() && words[end].hash == hash {
                        end += 1;
                    }
                    holds(&words[word_at..end], pattern)
                }
            };
            if held {
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

/// The same for the doubles lane: `count_of` is the unfiltered superset over
/// every separator glyph, so this can hold true for a book whose only
/// `separated` occurrences all force a capital — `locate` is what applies the
/// table and finds nothing there, exactly as the casing lane's superset does.
fn holds_double(row: Option<&DoubleTotal>, separated: bool) -> bool {
    row.is_some_and(|row| row.count_of(separated) > 0)
}

/// Whether any word channel has something to judge here.
///
/// The two casing-lane channels need a cased corpus; `Doubled` and
/// `LetterRun` do not, so an uncased script no longer short-circuits the whole
/// pass.
fn judges_anything(corpus: &[&WordAggregate], config: &JudgingConfig) -> bool {
    let cased = (config.channels.casing || config.channels.word_length)
        && corpus.iter().any(|book| book.cased());
    cased || config.channels.doubled || config.channels.letter_runs
}

/// One book's contribution to a word pattern's numerator — the oracle
/// `tests/casing_agree_with_counts.rs` measures the rescan against.
///
/// `table` is the corpus's, because which stored `Before`s are free is a
/// corpus fact and not a property of this book.
pub fn free_in(book: &WordAggregate, pattern: &Pattern, table: &TerminalTable) -> u64 {
    crate::judge::free_of(book, pattern.glyph, &pattern.key, table)
}
/// One book's firing set, resolved into the lane maps the walk probes, plus
/// the corpus terminal table it reads free from forced with.
///
/// Read once per publication per book: the walk itself is per chapter, and
/// nothing here varies between them.
struct WordSites {
    table: TerminalTable,
    cased: FxHashMap<(u64, Form), PatternIndex>,
    long: FxHashMap<u64, PatternIndex>,
    twice: FxHashMap<(u64, bool), PatternIndex>,
    sticky: FxHashMap<(ScalarKey, u8), PatternIndex>,
}

impl WordSites {
    /// `None` when this book fires nothing, so no chapter of it is walked.
    fn of(words: &Words, aggregate: &WordAggregate, out: &Findings) -> Option<Self> {
        let mut set = Vec::new();
        words.firing(aggregate, out.patterns(), &mut set);
        if set.is_empty() {
            return None;
        }
        // Copied because `Findings` cannot lend its table and take a row at
        // once; a firing set is tens of rows, not thousands.
        let mut sites = Self {
            table: out.terminals().cloned().unwrap_or_default(),
            cased: FxHashMap::default(),
            long: FxHashMap::default(),
            twice: FxHashMap::default(),
            sticky: FxHashMap::default(),
        };
        for &index in &set {
            let pattern = out.patterns()[usize::from(index.get())];
            match pattern.key {
                PatternKey::Casing { hash, form } => {
                    sites.cased.insert((hash, form), index);
                }
                PatternKey::WordLength { hash, .. } => {
                    sites.long.insert(hash, index);
                }
                PatternKey::Doubled { hash, separated } => {
                    sites.twice.insert((hash, separated), index);
                }
                PatternKey::LetterRun { length } => {
                    sites.sticky.insert((pattern.glyph, length), index);
                }
                _ => {}
            }
        }
        Some(sites)
    }

    /// One chapter's rows, in walk order, spans rebased by the chapter's
    /// projected `start`.
    fn walk(
        &self,
        slice: &str,
        start: u32,
        verses: &[Verse],
        found: &mut Vec<(TextRange, PatternIndex, Reasons)>,
    ) {
        let mut runs: Vec<PatternIndex> = Vec::new();
        // The pair, not the word: a chapter seam ends it, which is what a
        // fresh scan per chapter already says.
        let mut previous: Option<(u64, u32, u32)> = None;
        for_each_word(slice, verses, |word| {
            if let Some((hash, from, to)) = previous.replace((word.hash, word.from, word.to))
                && hash == word.hash
                && let Some(gap) = gap_between(slice, to, word.from)
                // A separator whose last glyph forces a capital in this
                // corpus's own table is a sentence terminal, not a
                // doubling: `go. Go` is two sentences.
                && let Some(separated) = match gap {
                    Gap::Bare => Some(false),
                    Gap::Separated(glyph) if !self.table.forces(glyph) => Some(true),
                    Gap::Separated(_) => None,
                }
                && let Some(&index) = self.twice.get(&(hash, separated))
            {
                // The span covers both words and what stood between them.
                let at =
                    TextRange::new(start + from, start + word.to).expect("a pair grows forward");
                let reason = if separated {
                    Reasons::DOUBLED_SEPARATED
                } else {
                    Reasons::DOUBLED_BARE
                };
                found.push((at, index, reason));
            }
            // The letter-run rows this word holds, one per RUN: the
            // channel counts runs, and a word may hold two of one length.
            runs.clear();
            if !self.sticky.is_empty() {
                let text = &slice[word.from as usize..word.to as usize];
                for_each_letter_run(text, |letter, length| {
                    if let Some(&index) = self.sticky.get(&(letter, length)) {
                        runs.push(index);
                    }
                });
            }
            let (casing, length) = if word.form == Form::Uncased {
                // An uncased word is in no casing row; its runs still are.
                (None, None)
            } else {
                (
                    word.before
                        .is_free(&self.table)
                        .then(|| self.cased.get(&(word.hash, word.form)))
                        .flatten(),
                    self.long.get(&word.hash),
                )
            };
            // One span, one row: a word two channels named carries both
            // reasons and the finer channel's index, as a run does.
            let named = match (casing, length) {
                (Some(&index), Some(_)) => {
                    Some((index, Reasons::CASING.union(Reasons::WORD_LENGTH)))
                }
                (Some(&index), None) => Some((index, Reasons::CASING)),
                (None, Some(&index)) => Some((index, Reasons::WORD_LENGTH)),
                (None, None) => None,
            };
            if named.is_none() && runs.is_empty() {
                return;
            }
            let at =
                TextRange::new(start + word.from, start + word.to).expect("a word grows forward");
            // A merged row keeps the finer channel's index, so the first
            // run's own row is the one the bit rides; any further run of
            // the same word is a row of its own.
            let rest = match named {
                Some((index, reasons)) => {
                    let reasons = if runs.is_empty() {
                        reasons
                    } else {
                        reasons.union(Reasons::LETTER_RUN)
                    };
                    found.push((at, index, reasons));
                    1
                }
                None => 0,
            };
            for &index in runs.iter().skip(rest) {
                found.push((at, index, Reasons::LETTER_RUN));
            }
        });
    }
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
