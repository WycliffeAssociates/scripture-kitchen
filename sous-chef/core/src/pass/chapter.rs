//! One chapter in, one detached observation out — the trait every pass is.
//!
//! ```text
//! pass.map(chapter)          -> an observation that reads nothing else
//! pass.fold(&chapters)       -> one book's aggregate, in book coordinates
//! pass.judge(&corpus, ..)    -> rows, from counts alone
//! ```
//!
//! Neither fold nor judge can tell a cached observation from a fresh one, and
//! that is what makes cold and incremental analysis equal. A tuple of passes
//! is a pass, so a brigade composes without a dispatch table.

use xxhash_rust::xxh3::xxh3_64;

use super::*;

/// Corpus-level counts a judge reads, kept resident by a host and updated one
/// book at a time instead of merged again every publication.
///
/// One concrete value rather than an associated type per pass: a host holds
/// exactly one whatever pass it drives, and a rule that wants resident totals
/// adds its own lane here. Today only [`crate::Words`] fills one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorpusTotals {
    pub words: WordTotals,
}

impl CorpusTotals {
    /// Inline size plus every lane's own allocation.
    pub fn resident_bytes(&self) -> usize {
        self.words.resident_bytes()
    }
}

/// The observation schema a host folds into every chapter-cache key.
///
/// Bump it whenever a `map` changes the shape or meaning of what it returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemaStamp(u32);

impl SchemaStamp {
    pub const fn new(stamp: u32) -> Self {
        Self(stamp)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The stamp of `self` composed with `other`, in that order.
    ///
    /// Order-sensitive and not either half, so a tuple pass cannot inherit a
    /// member's key.
    pub const fn then(self, other: Self) -> Self {
        Self((self.0.rotate_left(16) ^ other.0).wrapping_mul(0x9E37_79B1))
    }
}

/// Scripture identity for one independently mappable chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChapterKey {
    book: BookKey,
    chapter: u16,
}

impl ChapterKey {
    pub const fn new(book: BookKey, chapter: u16) -> Self {
        Self { book, chapter }
    }

    pub const fn book(self) -> BookKey {
        self.book
    }

    pub const fn chapter(self) -> u16 {
        self.chapter
    }
}

/// Everything one chapter's map may read, borrowed for that call only.
#[derive(Debug, Clone, Copy)]
pub struct ChapterInput<'a> {
    /// The chapter's masked projected text.
    pub text: &'a str,
    /// Verse rows rebased to that text.
    pub verses: &'a [Verse],
    pub key: ChapterKey,
}

/// One observation and the projected book offset a fold rebases it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterObs<O> {
    pub start: u32,
    pub obs: O,
}

/// A rule's chapter map, its book fold, and its corpus-level judgment.
pub trait ChapterPass {
    /// Detached, chapter-relative, borrow-free; cached by chapter content.
    type Observation: Send + 'static;
    /// One book's folded observations in book coordinates, every seam
    /// resolved; cached by book checksum.
    type Aggregate: Send + 'static;
    /// What judging may vary without touching a chapter or a book.
    type Config: Default;
    /// Part of every cache key a host builds for this pass.
    const SCHEMA: SchemaStamp;
    /// Whether a host may keep this pass's per-chapter observations between
    /// publications. A pass that says no is re-mapped over a whole book when
    /// that book's text moves, and the host keeps only the book's aggregate.
    ///
    /// A tuple retains chapters only if every member does, because one
    /// observation carries them all.
    const RETAIN_CHAPTERS: bool = true;
    /// Whether [`locate_book`](Self::locate_book) and
    /// [`locate_chapters`](Self::locate_chapters) really split this pass's
    /// rows, so a host may site one chapter and replay the others.
    ///
    /// A tuple says yes only when exactly one member does: two would each
    /// count every chapter, and the host could not tell whose rows are whose.
    const CHAPTER_SITES: bool = false;

    /// A pure function of this chapter; it never reads a neighbor.
    fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation;

    /// Folds one book's chapters in order, borrowing them so a cache stays
    /// their owner; seam state is the fold's own and nothing crosses a book.
    fn fold(&self, book: &[ChapterObs<&Self::Observation>]) -> Self::Aggregate;

    /// Empties what a member that does not retain chapters holds per chapter,
    /// once the fold has read it. Default: nothing — the pass keeps its own.
    fn release(&self, obs: &mut Self::Observation) {
        let _ = obs;
    }

    /// Whether [`release`](Self::release) emptied this observation, so a fold
    /// may not read it until it is walked again. Default: no.
    ///
    /// A pass that sheds rows says so from a flag it sets in `release`, not
    /// from the rows being empty: a chapter that honestly holds nothing is not
    /// a shed one.
    fn is_released(&self, observation: &Self::Observation) -> bool {
        let _ = observation;
        false
    }

    /// Walks this chapter again into an observation a `release` emptied.
    ///
    /// Default: the whole [`map`](Self::map). A tuple overrides it to run only
    /// the members that are released, so a host re-mapping one book-grain
    /// member's rows does not re-walk the chapters its neighbours still hold.
    fn remap(&self, chapter: ChapterInput<'_>, observation: &mut Self::Observation) {
        *observation = self.map(chapter);
    }

    /// Judges every book at once: `corpus[i]` is book `i`'s aggregate, and a
    /// judge calls `out.open_book(i)` before pushing that book's rows.
    fn judge(&self, corpus: &[&Self::Aggregate], config: &Self::Config, out: &mut Findings);

    /// Real bytes one resident aggregate holds, inline size included.
    ///
    /// Default: `size_of::<Self::Aggregate>()`, right for an aggregate with no
    /// heap of its own. A pass whose aggregate owns a `Vec` or a `Box<[_]>`
    /// overrides this with its real size, or a host's resident byte count
    /// undercounts by whatever heap the pass hangs off it.
    fn aggregate_bytes(&self, aggregate: &Self::Aggregate) -> usize {
        let _ = aggregate;
        size_of::<Self::Aggregate>()
    }

    /// Real bytes one resident chapter observation holds, inline size
    /// included — the same claim [`aggregate_bytes`](Self::aggregate_bytes)
    /// makes one level up.
    ///
    /// Default: `size_of::<Self::Observation>()`. A host that keeps a
    /// book-grain member's rows unreleased for its hot books pays this per
    /// chapter, so a pass whose observation owns a `Vec` or a `Box<[_]>`
    /// overrides it or the host's byte count is a fiction.
    fn observation_bytes(&self, observation: &Self::Observation) -> usize {
        let _ = observation;
        size_of::<Self::Observation>()
    }

    /// Adds these books to the corpus totals a host keeps resident.
    /// Default: none — the pass merges whatever it judges, every time.
    fn tally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        let _ = (totals, books);
    }

    /// Removes books previously tallied, exactly: the totals left are the
    /// totals of the books left.
    fn untally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        let _ = (totals, books);
    }

    /// Judges from totals a host already holds instead of merging them out of
    /// `corpus` again. Default: [`judge`](Self::judge), which merges its own.
    ///
    /// Equal to `judge` whenever `totals` is [`tally`](Self::tally) over
    /// exactly `corpus`. `galley/tests/equivalence.rs` pins that as bytes.
    fn judge_resident(
        &self,
        corpus: &[&Self::Aggregate],
        totals: &CorpusTotals,
        config: &Self::Config,
        out: &mut Findings,
    ) {
        let _ = totals;
        self.judge(corpus, config, out);
    }

    /// Names every corpus-tally key these books' rows hold — the keys a
    /// [`tally`](Self::tally) or [`untally`](Self::untally) of them moves.
    ///
    /// Default: none, which is what a pass that keeps no totals moves.
    fn moved_keys(&self, books: &[&Self::Aggregate], moved: &mut MovedWords) {
        let _ = (books, moved);
    }

    /// [`judge_resident`](Self::judge_resident) with every verdict `moved`
    /// does not name answered from `kept`, which it then refills.
    ///
    /// Equal to `judge_resident` pattern for pattern whenever `moved` names
    /// every key whose totals moved since `kept` was filled; a debug build
    /// asserts that against a whole judge. Default: `judge_resident`, which
    /// judges everything and keeps nothing.
    fn judge_kept(
        &self,
        corpus: &[&Self::Aggregate],
        totals: &CorpusTotals,
        config: &Self::Config,
        moved: &MovedWords,
        kept: &mut WordVerdicts,
        out: &mut Findings,
    ) {
        let _ = (moved, kept);
        self.judge_resident(corpus, totals, config, out);
    }

    /// Rescans one book's current text for the sites of what [`judge`](Self::judge)
    /// emitted, calling `out.open_book(book)` first. Default: none.
    ///
    /// The one step besides `map` that reads text, and it reads it only to
    /// PLACE what the counts already decided. Its structural inputs are `map`'s
    /// own, book-wide: a rule whose map read verse rows has to read the same
    /// rows here or it would place something it did not count.
    fn locate(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        let _ = (book, text, chapters, verses, aggregate, out);
    }

    /// Sites the rows [`locate`](Self::locate) places that no single chapter
    /// owns, calling `out.open_book(book)` first if it places any.
    ///
    /// Default: the whole `locate` — right for a pass whose walk reads across
    /// chapters, since it then owns every row it places.
    fn locate_book(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        self.locate(book, text, chapters, verses, aggregate, out);
    }

    /// Sites `chapters[range]`, pushing for each chapter in turn exactly the
    /// rows [`locate`](Self::locate) would push for it, and appending that
    /// chapter's row count to `counts`.
    ///
    /// Default: nothing, matching a [`locate_book`](Self::locate_book) that
    /// placed every row. A pass overrides both together, and the pair must
    /// equal `locate` over `0..chapters.len()`, row for row and in order.
    ///
    /// Only a pass whose walk restarts at every chapter may override it. A
    /// tuple forwards to each member in turn, so `counts` is one run per
    /// overriding member: a host that splits the rows by chapter needs the
    /// tuple to hold exactly one.
    #[allow(clippy::too_many_arguments)]
    fn locate_chapters(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        range: Range<usize>,
        aggregate: &Self::Aggregate,
        counts: &mut Vec<u32>,
        out: &mut Findings,
    ) {
        let _ = (book, text, chapters, verses, range, aggregate, counts, out);
    }

    /// The per-verse projected grapheme lengths this pass's aggregate carries,
    /// for the corpus-level source comparison a host runs beside it.
    ///
    /// Default: none — a pass that does not walk verse rows pairs nothing. A
    /// tuple answers with its first member that carries any, since one member
    /// owns the lane and the rest are silent about it.
    fn verse_lengths<'a>(&self, aggregate: &'a Self::Aggregate) -> &'a [VerseLength] {
        let _ = aggregate;
        &[]
    }

    /// The length-proportionality knobs this pass's config carries, so a host
    /// moves one judging config rather than two.
    ///
    /// Default: none, which reads as the rule being off for this pass. The
    /// member that fills [`verse_lengths`](Self::verse_lengths) is the one
    /// that answers here, and a tuple takes its first answer.
    fn length_config(&self, config: &Self::Config) -> Option<LengthConfig> {
        let _ = config;
        None
    }

    /// This config as one number, for the identity a host gives a publication.
    ///
    /// Two publications that differ only by a knob are two snapshots, so a
    /// host folds this into its snapshot id beside the corpus and the schema.
    /// Default: zero, which is the whole of a pass with no knobs.
    fn config_stamp(&self, config: &Self::Config) -> u64 {
        let _ = config;
        0
    }

    /// The table positions [`locate`](Self::locate) would scan this book for.
    ///
    /// A host caches sites under a hash of these rows' content, so a book whose
    /// own firing set did not move replays instead of rescanning. Default: none,
    /// matching the default `locate`.
    fn firing(
        &self,
        aggregate: &Self::Aggregate,
        patterns: &[Pattern],
        out: &mut Vec<PatternIndex>,
    ) {
        let _ = (aggregate, patterns);
        out.clear();
    }
}

/// Two passes over the same chapters as one: both maps run, both folds run,
/// both judges run, and [`Findings::finish`] puts the rows in span order.
impl<A: ChapterPass, B: ChapterPass> ChapterPass for (A, B) {
    type Observation = (A::Observation, B::Observation);
    type Aggregate = (A::Aggregate, B::Aggregate);
    type Config = (A::Config, B::Config);
    const SCHEMA: SchemaStamp = A::SCHEMA.then(B::SCHEMA);
    const RETAIN_CHAPTERS: bool = A::RETAIN_CHAPTERS && B::RETAIN_CHAPTERS;
    const CHAPTER_SITES: bool = A::CHAPTER_SITES ^ B::CHAPTER_SITES;

    fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
        (self.0.map(chapter), self.1.map(chapter))
    }

    fn fold(&self, book: &[ChapterObs<&Self::Observation>]) -> Self::Aggregate {
        // Two views over the borrowed pairs: a fold takes one observation
        // type. Two small vectors per book per publication.
        let left: Vec<ChapterObs<&A::Observation>> = book
            .iter()
            .map(|chapter| ChapterObs {
                start: chapter.start,
                obs: &chapter.obs.0,
            })
            .collect();
        let right: Vec<ChapterObs<&B::Observation>> = book
            .iter()
            .map(|chapter| ChapterObs {
                start: chapter.start,
                obs: &chapter.obs.1,
            })
            .collect();
        (self.0.fold(&left), self.1.fold(&right))
    }

    fn release(&self, obs: &mut Self::Observation) {
        self.0.release(&mut obs.0);
        self.1.release(&mut obs.1);
    }

    fn is_released(&self, observation: &Self::Observation) -> bool {
        self.0.is_released(&observation.0) || self.1.is_released(&observation.1)
    }

    /// Only the released members walk again; the rest keep the slot they hold.
    fn remap(&self, chapter: ChapterInput<'_>, observation: &mut Self::Observation) {
        if self.0.is_released(&observation.0) {
            self.0.remap(chapter, &mut observation.0);
        }
        if self.1.is_released(&observation.1) {
            self.1.remap(chapter, &mut observation.1);
        }
    }

    fn judge(&self, corpus: &[&Self::Aggregate], config: &Self::Config, out: &mut Findings) {
        let left: Vec<&A::Aggregate> = corpus.iter().map(|book| &book.0).collect();
        let right: Vec<&B::Aggregate> = corpus.iter().map(|book| &book.1).collect();
        self.0.judge(&left, &config.0, out);
        self.1.judge(&right, &config.1, out);
    }

    fn aggregate_bytes(&self, aggregate: &Self::Aggregate) -> usize {
        self.0.aggregate_bytes(&aggregate.0) + self.1.aggregate_bytes(&aggregate.1)
    }

    fn observation_bytes(&self, observation: &Self::Observation) -> usize {
        self.0.observation_bytes(&observation.0) + self.1.observation_bytes(&observation.1)
    }

    fn tally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        let left: Vec<&A::Aggregate> = books.iter().map(|book| &book.0).collect();
        let right: Vec<&B::Aggregate> = books.iter().map(|book| &book.1).collect();
        self.0.tally(totals, &left);
        self.1.tally(totals, &right);
    }

    fn untally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        let left: Vec<&A::Aggregate> = books.iter().map(|book| &book.0).collect();
        let right: Vec<&B::Aggregate> = books.iter().map(|book| &book.1).collect();
        self.0.untally(totals, &left);
        self.1.untally(totals, &right);
    }

    fn judge_resident(
        &self,
        corpus: &[&Self::Aggregate],
        totals: &CorpusTotals,
        config: &Self::Config,
        out: &mut Findings,
    ) {
        let left: Vec<&A::Aggregate> = corpus.iter().map(|book| &book.0).collect();
        let right: Vec<&B::Aggregate> = corpus.iter().map(|book| &book.1).collect();
        self.0.judge_resident(&left, totals, &config.0, out);
        self.1.judge_resident(&right, totals, &config.1, out);
    }

    fn moved_keys(&self, books: &[&Self::Aggregate], moved: &mut MovedWords) {
        let left: Vec<&A::Aggregate> = books.iter().map(|book| &book.0).collect();
        let right: Vec<&B::Aggregate> = books.iter().map(|book| &book.1).collect();
        self.0.moved_keys(&left, moved);
        self.1.moved_keys(&right, moved);
    }

    fn judge_kept(
        &self,
        corpus: &[&Self::Aggregate],
        totals: &CorpusTotals,
        config: &Self::Config,
        moved: &MovedWords,
        kept: &mut WordVerdicts,
        out: &mut Findings,
    ) {
        let left: Vec<&A::Aggregate> = corpus.iter().map(|book| &book.0).collect();
        let right: Vec<&B::Aggregate> = corpus.iter().map(|book| &book.1).collect();
        self.0
            .judge_kept(&left, totals, &config.0, moved, kept, out);
        self.1
            .judge_kept(&right, totals, &config.1, moved, kept, out);
    }

    fn locate(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        self.0
            .locate(book, text, chapters, verses, &aggregate.0, out);
        self.1
            .locate(book, text, chapters, verses, &aggregate.1, out);
    }

    fn locate_book(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        self.0
            .locate_book(book, text, chapters, verses, &aggregate.0, out);
        self.1
            .locate_book(book, text, chapters, verses, &aggregate.1, out);
    }

    fn locate_chapters(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        range: Range<usize>,
        aggregate: &Self::Aggregate,
        counts: &mut Vec<u32>,
        out: &mut Findings,
    ) {
        self.0.locate_chapters(
            book,
            text,
            chapters,
            verses,
            range.clone(),
            &aggregate.0,
            counts,
            out,
        );
        self.1.locate_chapters(
            book,
            text,
            chapters,
            verses,
            range,
            &aggregate.1,
            counts,
            out,
        );
    }

    fn firing(
        &self,
        aggregate: &Self::Aggregate,
        patterns: &[Pattern],
        out: &mut Vec<PatternIndex>,
    ) {
        let mut second = Vec::new();
        self.0.firing(&aggregate.0, patterns, out);
        self.1.firing(&aggregate.1, patterns, &mut second);
        out.append(&mut second);
    }

    fn verse_lengths<'a>(&self, aggregate: &'a Self::Aggregate) -> &'a [VerseLength] {
        let first = self.0.verse_lengths(&aggregate.0);
        if first.is_empty() {
            self.1.verse_lengths(&aggregate.1)
        } else {
            first
        }
    }

    fn length_config(&self, config: &Self::Config) -> Option<LengthConfig> {
        self.0
            .length_config(&config.0)
            .or_else(|| self.1.length_config(&config.1))
    }

    fn config_stamp(&self, config: &Self::Config) -> u64 {
        compose_stamps(&[
            self.0.config_stamp(&config.0),
            self.1.config_stamp(&config.1),
        ])
    }
}

/// Three passes over the same chapters as one, mirroring the pair: the
/// composed `SCHEMA` is `A.then(B).then(C)`, so no arrangement of the same
/// members shares a key.
impl<A: ChapterPass, B: ChapterPass, C: ChapterPass> ChapterPass for (A, B, C) {
    type Observation = (A::Observation, B::Observation, C::Observation);
    type Aggregate = (A::Aggregate, B::Aggregate, C::Aggregate);
    type Config = (A::Config, B::Config, C::Config);
    const SCHEMA: SchemaStamp = A::SCHEMA.then(B::SCHEMA).then(C::SCHEMA);
    const RETAIN_CHAPTERS: bool = A::RETAIN_CHAPTERS && B::RETAIN_CHAPTERS && C::RETAIN_CHAPTERS;
    const CHAPTER_SITES: bool =
        A::CHAPTER_SITES as u8 + B::CHAPTER_SITES as u8 + C::CHAPTER_SITES as u8 == 1;

    fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
        (
            self.0.map(chapter),
            self.1.map(chapter),
            self.2.map(chapter),
        )
    }

    fn fold(&self, book: &[ChapterObs<&Self::Observation>]) -> Self::Aggregate {
        // Three views over the borrowed triples: a fold takes one observation
        // type. Three small vectors per book per publication.
        let left: Vec<ChapterObs<&A::Observation>> = book
            .iter()
            .map(|chapter| ChapterObs {
                start: chapter.start,
                obs: &chapter.obs.0,
            })
            .collect();
        let middle: Vec<ChapterObs<&B::Observation>> = book
            .iter()
            .map(|chapter| ChapterObs {
                start: chapter.start,
                obs: &chapter.obs.1,
            })
            .collect();
        let right: Vec<ChapterObs<&C::Observation>> = book
            .iter()
            .map(|chapter| ChapterObs {
                start: chapter.start,
                obs: &chapter.obs.2,
            })
            .collect();
        (
            self.0.fold(&left),
            self.1.fold(&middle),
            self.2.fold(&right),
        )
    }

    fn release(&self, obs: &mut Self::Observation) {
        self.0.release(&mut obs.0);
        self.1.release(&mut obs.1);
        self.2.release(&mut obs.2);
    }

    fn is_released(&self, observation: &Self::Observation) -> bool {
        self.0.is_released(&observation.0)
            || self.1.is_released(&observation.1)
            || self.2.is_released(&observation.2)
    }

    /// Only the released members walk again; the rest keep the slot they hold.
    fn remap(&self, chapter: ChapterInput<'_>, observation: &mut Self::Observation) {
        if self.0.is_released(&observation.0) {
            self.0.remap(chapter, &mut observation.0);
        }
        if self.1.is_released(&observation.1) {
            self.1.remap(chapter, &mut observation.1);
        }
        if self.2.is_released(&observation.2) {
            self.2.remap(chapter, &mut observation.2);
        }
    }

    fn judge(&self, corpus: &[&Self::Aggregate], config: &Self::Config, out: &mut Findings) {
        let left: Vec<&A::Aggregate> = corpus.iter().map(|book| &book.0).collect();
        let middle: Vec<&B::Aggregate> = corpus.iter().map(|book| &book.1).collect();
        let right: Vec<&C::Aggregate> = corpus.iter().map(|book| &book.2).collect();
        self.0.judge(&left, &config.0, out);
        self.1.judge(&middle, &config.1, out);
        self.2.judge(&right, &config.2, out);
    }

    fn aggregate_bytes(&self, aggregate: &Self::Aggregate) -> usize {
        self.0.aggregate_bytes(&aggregate.0)
            + self.1.aggregate_bytes(&aggregate.1)
            + self.2.aggregate_bytes(&aggregate.2)
    }

    fn observation_bytes(&self, observation: &Self::Observation) -> usize {
        self.0.observation_bytes(&observation.0)
            + self.1.observation_bytes(&observation.1)
            + self.2.observation_bytes(&observation.2)
    }

    fn tally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        let left: Vec<&A::Aggregate> = books.iter().map(|book| &book.0).collect();
        let middle: Vec<&B::Aggregate> = books.iter().map(|book| &book.1).collect();
        let right: Vec<&C::Aggregate> = books.iter().map(|book| &book.2).collect();
        self.0.tally(totals, &left);
        self.1.tally(totals, &middle);
        self.2.tally(totals, &right);
    }

    fn untally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        let left: Vec<&A::Aggregate> = books.iter().map(|book| &book.0).collect();
        let middle: Vec<&B::Aggregate> = books.iter().map(|book| &book.1).collect();
        let right: Vec<&C::Aggregate> = books.iter().map(|book| &book.2).collect();
        self.0.untally(totals, &left);
        self.1.untally(totals, &middle);
        self.2.untally(totals, &right);
    }

    fn judge_resident(
        &self,
        corpus: &[&Self::Aggregate],
        totals: &CorpusTotals,
        config: &Self::Config,
        out: &mut Findings,
    ) {
        let left: Vec<&A::Aggregate> = corpus.iter().map(|book| &book.0).collect();
        let middle: Vec<&B::Aggregate> = corpus.iter().map(|book| &book.1).collect();
        let right: Vec<&C::Aggregate> = corpus.iter().map(|book| &book.2).collect();
        self.0.judge_resident(&left, totals, &config.0, out);
        self.1.judge_resident(&middle, totals, &config.1, out);
        self.2.judge_resident(&right, totals, &config.2, out);
    }

    fn moved_keys(&self, books: &[&Self::Aggregate], moved: &mut MovedWords) {
        let left: Vec<&A::Aggregate> = books.iter().map(|book| &book.0).collect();
        let middle: Vec<&B::Aggregate> = books.iter().map(|book| &book.1).collect();
        let right: Vec<&C::Aggregate> = books.iter().map(|book| &book.2).collect();
        self.0.moved_keys(&left, moved);
        self.1.moved_keys(&middle, moved);
        self.2.moved_keys(&right, moved);
    }

    fn judge_kept(
        &self,
        corpus: &[&Self::Aggregate],
        totals: &CorpusTotals,
        config: &Self::Config,
        moved: &MovedWords,
        kept: &mut WordVerdicts,
        out: &mut Findings,
    ) {
        let left: Vec<&A::Aggregate> = corpus.iter().map(|book| &book.0).collect();
        let middle: Vec<&B::Aggregate> = corpus.iter().map(|book| &book.1).collect();
        let right: Vec<&C::Aggregate> = corpus.iter().map(|book| &book.2).collect();
        self.0
            .judge_kept(&left, totals, &config.0, moved, kept, out);
        self.1
            .judge_kept(&middle, totals, &config.1, moved, kept, out);
        self.2
            .judge_kept(&right, totals, &config.2, moved, kept, out);
    }

    fn locate(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        self.0
            .locate(book, text, chapters, verses, &aggregate.0, out);
        self.1
            .locate(book, text, chapters, verses, &aggregate.1, out);
        self.2
            .locate(book, text, chapters, verses, &aggregate.2, out);
    }

    fn locate_book(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        self.0
            .locate_book(book, text, chapters, verses, &aggregate.0, out);
        self.1
            .locate_book(book, text, chapters, verses, &aggregate.1, out);
        self.2
            .locate_book(book, text, chapters, verses, &aggregate.2, out);
    }

    fn locate_chapters(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[Verse],
        range: Range<usize>,
        aggregate: &Self::Aggregate,
        counts: &mut Vec<u32>,
        out: &mut Findings,
    ) {
        self.0.locate_chapters(
            book,
            text,
            chapters,
            verses,
            range.clone(),
            &aggregate.0,
            counts,
            out,
        );
        self.1.locate_chapters(
            book,
            text,
            chapters,
            verses,
            range.clone(),
            &aggregate.1,
            counts,
            out,
        );
        self.2.locate_chapters(
            book,
            text,
            chapters,
            verses,
            range,
            &aggregate.2,
            counts,
            out,
        );
    }

    fn firing(
        &self,
        aggregate: &Self::Aggregate,
        patterns: &[Pattern],
        out: &mut Vec<PatternIndex>,
    ) {
        let mut rest = Vec::new();
        self.0.firing(&aggregate.0, patterns, out);
        self.1.firing(&aggregate.1, patterns, &mut rest);
        out.append(&mut rest);
        self.2.firing(&aggregate.2, patterns, &mut rest);
        out.append(&mut rest);
    }

    fn verse_lengths<'a>(&self, aggregate: &'a Self::Aggregate) -> &'a [VerseLength] {
        for lane in [
            self.0.verse_lengths(&aggregate.0),
            self.1.verse_lengths(&aggregate.1),
            self.2.verse_lengths(&aggregate.2),
        ] {
            if !lane.is_empty() {
                return lane;
            }
        }
        &[]
    }

    fn length_config(&self, config: &Self::Config) -> Option<LengthConfig> {
        self.0
            .length_config(&config.0)
            .or_else(|| self.1.length_config(&config.1))
            .or_else(|| self.2.length_config(&config.2))
    }

    fn config_stamp(&self, config: &Self::Config) -> u64 {
        compose_stamps(&[
            self.0.config_stamp(&config.0),
            self.1.config_stamp(&config.1),
            self.2.config_stamp(&config.2),
        ])
    }
}

/// A tuple's members' stamps as one, in member order: two arrangements of the
/// same knobs are two stamps, exactly as they are two schemas.
fn compose_stamps(stamps: &[u64]) -> u64 {
    let mut bytes = [0u8; 24];
    for (at, stamp) in stamps.iter().enumerate() {
        bytes[at * 8..at * 8 + 8].copy_from_slice(&stamp.to_le_bytes());
    }
    xxh3_64(&bytes[..stamps.len() * 8])
}
