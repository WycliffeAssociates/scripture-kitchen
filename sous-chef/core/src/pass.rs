//! One chapter in, one detached observation out; one book's observations in,
//! one aggregate out; every book's aggregates in, findings out.
//!
//! ```text
//! analyze(corpus, &HygieneBytes)
//!   MRK chapter 1 at 0   "wept \\ here.\0\0\0"  → map → [StrandedBackslash 5..7, C0Control 13..16]
//!   MRK chapter 2 at 16  "An \0 more."          → map → [C0Control 3..4]
//!   fold([obs at 0, obs at 16])   → [StrandedBackslash 5..7, C0Control 13..16, C0Control 19..20]
//!   judge([&MRK aggregate], &(), out)
//!     → target[0] MRK  5..7   StrandedBackslash run 2
//!       target[0] MRK 13..16  C0Control run 3
//!       target[0] MRK 19..20  C0Control run 1
//! ```
//!
//! [`ChapterPass::map`] reads one chapter and nothing else, so a host may run
//! it in any order or retain its result. Neither fold nor judge can tell a
//! cached input from a fresh one; that is what makes cold and incremental
//! analysis equal. The fold and judge rules in full: pass.md.

use crate::{
    BookIndex, BookKey, Chapter, CodecError, Corpus, FindingKind, PackedFinding, ProjectedBook,
    TextRange, Verse,
    judge::{Pattern, PatternIndex},
    words::WordTotals,
};

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
}

/// The judge sink: rows in projected book coordinates, each naming its book.
///
/// A host builds one per invocation, names a book before judging it, and
/// publishes the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Findings {
    book_lengths: Vec<u32>,
    book: Option<BookIndex>,
    rows: Vec<PackedFinding>,
    patterns: Vec<Pattern>,
}

impl Findings {
    /// The lengths are the caller's book table, in book-index order.
    pub fn new(book_lengths: Vec<u32>) -> Self {
        Self {
            book_lengths,
            book: None,
            rows: Vec::new(),
            patterns: Vec::new(),
        }
    }

    /// Names the book every later [`push`](Self::push) belongs to.
    pub fn open_book(&mut self, book: BookIndex) {
        self.book = Some(book);
    }

    /// Records one finding; the span is checked against the open book.
    ///
    /// Panics if no book is open: a row must never land in a book by default.
    pub fn push(&mut self, span: TextRange, kind: FindingKind) -> Result<(), CodecError> {
        let book = self.book.expect("open_book before push");
        let row = PackedFinding::new(span.from(), span.to(), book, kind, &self.book_lengths)?;
        self.rows.push(row);
        Ok(())
    }

    /// Records one corpus-level pattern and returns its table position.
    ///
    /// Patterns belong to the corpus, not to a book, so they may be pushed
    /// either side of any [`open_book`](Self::open_book). A table past
    /// `u16::MAX` rows saturates here and the encoder refuses it.
    pub fn push_pattern(&mut self, pattern: Pattern) -> PatternIndex {
        let index = u16::try_from(self.patterns.len()).unwrap_or(u16::MAX);
        self.patterns.push(pattern);
        PatternIndex::new(index)
    }

    pub fn rows(&self) -> &[PackedFinding] {
        &self.rows
    }

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Puts every row in publication order: stable by `(book_idx, from, to)`,
    /// so a tie keeps the pushing judge's turn. A host calls it once, after
    /// the last judge.
    pub fn finish(&mut self) {
        self.rows
            .sort_by_key(|row| (row.book_idx().get(), row.from(), row.to()));
    }

    pub fn into_parts(self) -> (Vec<PackedFinding>, Vec<Pattern>) {
        (self.rows, self.patterns)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Calls `visit` with each chapter's projected start and its map input.
///
/// The one place a chapter input is assembled, so [`analyze`] and an
/// incremental host cannot build two different ones from the same book.
pub fn for_each_chapter<B: ProjectedBook>(book: &B, mut visit: impl FnMut(u32, ChapterInput<'_>)) {
    let text = book.text();
    let rows: Vec<Verse> = book.verses().collect();
    let mut verses = Vec::new();
    let mut at = 0;
    for chapter in book.chapters() {
        let span = chapter.text();
        at = collect_verses(&rows, at, chapter, &mut verses);
        visit(
            span.from(),
            ChapterInput {
                text: &text[span.from() as usize..span.to() as usize],
                verses: &verses,
                key: ChapterKey::new(book.key(), chapter.number()),
            },
        );
    }
}

/// [`analyze_with`] under the pass's default config.
///
/// This is the whole-corpus oracle an incremental host is measured against.
pub fn analyze<B: ProjectedBook, P: ChapterPass>(corpus: &Corpus<'_, B>, pass: &P) -> Findings {
    analyze_with(corpus, pass, &P::Config::default())
}

/// Maps every chapter, folds every book in caller order, judges the corpus
/// once, and orders the rows.
pub fn analyze_with<B: ProjectedBook, P: ChapterPass>(
    corpus: &Corpus<'_, B>,
    pass: &P,
    config: &P::Config,
) -> Findings {
    let book_lengths: Vec<u32> = corpus
        .books()
        .iter()
        .map(|book| u32::try_from(book.text().len()).expect("corpus validation bounds book length"))
        .collect();
    let mut out = Findings::new(book_lengths);

    let mut observations: Vec<(u32, P::Observation)> = Vec::new();
    let mut aggregates: Vec<P::Aggregate> = Vec::with_capacity(corpus.books().len());
    for (_, book) in corpus.iter() {
        observations.clear();
        for_each_chapter(book, |start, input| {
            observations.push((start, pass.map(input)));
        });
        let rows: Vec<ChapterObs<&P::Observation>> = observations
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();
        aggregates.push(pass.fold(&rows));
    }
    let views: Vec<&P::Aggregate> = aggregates.iter().collect();
    pass.judge(&views, config, &mut out);
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut verses: Vec<Verse> = Vec::new();
    for (index, book) in corpus.iter() {
        chapters.clear();
        chapters.extend(book.chapters());
        verses.clear();
        verses.extend(book.verses());
        pass.locate(
            index,
            book.text(),
            &chapters,
            &verses,
            &aggregates[index.get() as usize],
            &mut out,
        );
    }
    out.finish();
    out
}

/// Rebases this chapter's verse rows into `verses`, returning where the next
/// chapter resumes. Rows are non-decreasing by key, so each run is contiguous.
fn collect_verses(
    rows: &[Verse],
    mut at: usize,
    chapter: Chapter,
    verses: &mut Vec<Verse>,
) -> usize {
    verses.clear();
    let span = chapter.text();
    while rows
        .get(at)
        .is_some_and(|row| row.key().chapter() < chapter.number())
    {
        at += 1;
    }
    while let Some(row) = rows
        .get(at)
        .filter(|row| row.key().chapter() == chapter.number())
    {
        let text = row.text();
        debug_assert!(
            span.from() <= text.from() && text.to() <= span.to(),
            "corpus validation places every verse inside its chapter"
        );
        let rebased = TextRange::new(text.from() - span.from(), text.to() - span.from())
            .expect("a chapter-relative range keeps its order");
        verses.push(Verse::new(row.key(), rebased));
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Brigade, HygieneClass, HygieneDigest, Reasons, VerseKey, hygiene::HygieneBytes,
        substrate::Substrate, validate, words::Words,
    };

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

    fn range(from: u32, to: u32) -> TextRange {
        TextRange::new(from, to).unwrap()
    }

    fn chapter(number: u16, from: u32, to: u32) -> Chapter {
        Chapter::new(number, range(from, to)).unwrap()
    }

    fn verse(chapter: u16, number: u16, from: u32, to: u32) -> Verse {
        Verse::new(
            VerseKey::new(chapter, number, number).unwrap(),
            range(from, to),
        )
    }

    /// `x\0y` then `\0\0z`; one NUL in chapter 1, two in chapter 2.
    fn mrk() -> Book {
        Book {
            key: BookKey::new(*b"MRK"),
            text: "x\0y\0\0z",
            chapters: vec![chapter(1, 0, 3), chapter(2, 3, 6)],
            verses: vec![verse(1, 1, 0, 3), verse(2, 1, 3, 6)],
        }
    }

    fn genesis() -> Book {
        Book {
            key: BookKey::new(*b"GEN"),
            text: "p\0q",
            chapters: vec![chapter(1, 0, 3)],
            verses: vec![verse(1, 1, 0, 3)],
        }
    }

    /// The hygiene half of a row set; a substrate corpus judges conventions
    /// beside them and sites them.
    fn hygiene_rows(findings: &Findings) -> Vec<(u16, u32, u32, HygieneClass, u32)> {
        findings
            .rows()
            .iter()
            .filter_map(|row| {
                let FindingKind::Hygiene(digest) = row.kind() else {
                    return None;
                };
                Some((
                    row.book_idx().get(),
                    row.from(),
                    row.to(),
                    digest.class(),
                    digest.run(),
                ))
            })
            .collect()
    }

    #[test]
    fn analyze_equals_map_then_fold_then_judge_composed_by_hand() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();

        let mut hand = Findings::new(vec![6, 3]);
        let first = HygieneBytes.map(ChapterInput {
            text: "x\0y",
            verses: &[verse(1, 1, 0, 3)],
            key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
        });
        let second = HygieneBytes.map(ChapterInput {
            text: "\0\0z",
            verses: &[verse(2, 1, 0, 3)],
            key: ChapterKey::new(BookKey::new(*b"MRK"), 2),
        });
        let third = HygieneBytes.map(ChapterInput {
            text: "p\0q",
            verses: &[verse(1, 1, 0, 3)],
            key: ChapterKey::new(BookKey::new(*b"GEN"), 1),
        });
        let mark = HygieneBytes.fold(&[
            ChapterObs {
                start: 0,
                obs: &first,
            },
            ChapterObs {
                start: 3,
                obs: &second,
            },
        ]);
        let genesis = HygieneBytes.fold(&[ChapterObs {
            start: 0,
            obs: &third,
        }]);
        HygieneBytes.judge(&[&mark, &genesis], &(), &mut hand);
        hand.finish();

        let composed = analyze(&corpus, &HygieneBytes);
        assert_eq!(
            hygiene_rows(&composed),
            vec![
                (0, 1, 2, HygieneClass::C0Control, 1),
                (0, 3, 5, HygieneClass::C0Control, 2),
                (1, 1, 2, HygieneClass::C0Control, 1),
            ]
        );
        assert_eq!(composed.rows(), hand.rows());
    }

    #[test]
    fn analyze_judges_books_in_caller_order() {
        let forward = vec![mrk(), genesis()];
        let reversed = vec![genesis(), mrk()];
        let forward = analyze(&Corpus::try_new(&forward).unwrap(), &HygieneBytes);
        let reversed = analyze(&Corpus::try_new(&reversed).unwrap(), &HygieneBytes);

        let keys: Vec<_> = forward.rows().iter().map(|row| row.book_idx()).collect();
        assert_eq!(
            keys,
            vec![
                BookIndex::new(0).unwrap(),
                BookIndex::new(0).unwrap(),
                BookIndex::new(1).unwrap()
            ]
        );
        assert_eq!(
            hygiene_rows(&reversed),
            vec![
                (0, 1, 2, HygieneClass::C0Control, 1),
                (1, 1, 2, HygieneClass::C0Control, 1),
                (1, 3, 5, HygieneClass::C0Control, 2),
            ]
        );
    }

    #[test]
    fn map_sees_chapter_relative_text_and_verse_rows() {
        struct Echo;

        impl ChapterPass for Echo {
            type Observation = (String, Vec<(u16, u32, u32)>);
            type Aggregate = ();
            type Config = ();
            const SCHEMA: SchemaStamp = SchemaStamp::new(0);

            fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
                (
                    chapter.text.to_string(),
                    chapter
                        .verses
                        .iter()
                        .map(|verse| (verse.key().first(), verse.text().from(), verse.text().to()))
                        .collect(),
                )
            }

            fn fold(&self, _book: &[ChapterObs<&Self::Observation>]) {}

            fn judge(&self, _corpus: &[&()], _config: &(), _out: &mut Findings) {}
        }

        let book = Book {
            key: BookKey::new(*b"MRK"),
            text: "one two three",
            chapters: vec![chapter(1, 0, 7), chapter(2, 8, 13)],
            verses: vec![verse(1, 1, 0, 3), verse(1, 2, 4, 7), verse(2, 1, 8, 13)],
        };
        assert_eq!(validate(&book), Ok(()));
        let books = vec![book];
        let corpus = Corpus::try_new(&books).unwrap();

        let mut seen = Vec::new();
        for (_, book) in corpus.iter() {
            let text = book.text();
            let rows: Vec<Verse> = book.verses().collect();
            let mut verses = Vec::new();
            let mut at = 0;
            for chapter in book.chapters() {
                at = collect_verses(&rows, at, chapter, &mut verses);
                let span = chapter.text();
                seen.push(Echo.map(ChapterInput {
                    text: &text[span.from() as usize..span.to() as usize],
                    verses: &verses,
                    key: ChapterKey::new(book.key(), chapter.number()),
                }));
            }
        }

        assert_eq!(
            seen,
            vec![
                ("one two".to_string(), vec![(1, 0, 3), (2, 4, 7)]),
                ("three".to_string(), vec![(1, 0, 5)]),
            ]
        );
    }

    /// A NUL run abutting a masked `\c 2`: one run to the whole-book scan,
    /// one per chapter to the pass.
    fn seam() -> Book {
        Book {
            key: BookKey::new(*b"MRK"),
            text: "a\0\0\0\0\0\0b",
            chapters: vec![chapter(1, 0, 4), chapter(2, 4, 8)],
            verses: vec![verse(1, 1, 0, 4), verse(2, 1, 4, 8)],
        }
    }

    #[test]
    fn hygiene_through_analyze_equals_a_whole_book_scan_away_from_chapter_seams() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        let whole: Vec<_> = books
            .iter()
            .enumerate()
            .flat_map(|(index, book)| {
                crate::hygiene::scan(book.text())
                    .into_iter()
                    .map(move |finding| {
                        (
                            index as u16,
                            finding.span().from(),
                            finding.span().to(),
                            finding.class(),
                            finding.run(),
                        )
                    })
            })
            .collect();

        assert_eq!(hygiene_rows(&analyze(&corpus, &HygieneBytes)), whole);
    }

    #[test]
    fn a_run_across_a_masked_chapter_seam_is_two_findings() {
        let books = vec![seam()];
        let corpus = Corpus::try_new(&books).unwrap();

        let whole = crate::hygiene::scan(books[0].text());
        assert_eq!(whole.len(), 1);
        assert_eq!(whole[0].run(), 6);
        assert_eq!((whole[0].span().from(), whole[0].span().to()), (1, 7));

        assert_eq!(
            hygiene_rows(&analyze(&corpus, &HygieneBytes)),
            vec![
                (0, 1, 4, HygieneClass::C0Control, 3),
                (0, 4, 7, HygieneClass::C0Control, 3),
            ]
        );
    }

    /// Chapter 0 never reaches a pass, so front matter is out of scope.
    #[test]
    fn text_outside_every_chapter_is_not_mapped() {
        let books = vec![Book {
            key: BookKey::new(*b"MRK"),
            text: "\0abc",
            chapters: vec![chapter(1, 1, 4)],
            verses: vec![verse(1, 1, 1, 4)],
        }];
        let corpus = Corpus::try_new(&books).unwrap();

        assert_eq!(crate::hygiene::scan(books[0].text()).len(), 1);
        assert!(analyze(&corpus, &HygieneBytes).is_empty());
    }

    /// Chapter 1 carries a byte class and a scalar class alternately, so the
    /// two halves of a tuple emit rows that must interleave.
    fn mixed() -> Book {
        // "\u{301}a\0 \u{301}\0": free mark 0..2, NUL 3..4, free mark 4..7
        // (it takes the space it hangs on), NUL 7..8.
        Book {
            key: BookKey::new(*b"LUK"),
            text: "\u{301}a\0 \u{301}\0",
            chapters: vec![chapter(1, 0, 8)],
            verses: vec![verse(1, 1, 0, 8)],
        }
    }

    #[test]
    fn a_tuple_maps_both_and_judges_in_span_order() {
        let books = vec![mixed()];
        let corpus = Corpus::try_new(&books).unwrap();
        assert_eq!(
            hygiene_rows(&analyze(&corpus, &Brigade::default())),
            vec![
                (0, 0, 2, HygieneClass::FreeCombiningMark, 1),
                (0, 3, 4, HygieneClass::C0Control, 1),
                (0, 4, 7, HygieneClass::FreeCombiningMark, 1),
                (0, 7, 8, HygieneClass::C0Control, 1),
            ]
        );
    }

    /// Test-only stubs: two stamps, no rows.
    struct Stamped<const S: u32>;

    impl<const S: u32> ChapterPass for Stamped<S> {
        type Observation = ();
        type Aggregate = ();
        type Config = ();
        const SCHEMA: SchemaStamp = SchemaStamp::new(S);

        fn map(&self, _chapter: ChapterInput<'_>) -> Self::Observation {}

        fn fold(&self, _book: &[ChapterObs<&()>]) {}

        fn judge(&self, _corpus: &[&()], _config: &(), _out: &mut Findings) {}
    }

    #[test]
    fn a_tuple_schema_is_order_sensitive_and_not_either_half() {
        type Left = Stamped<1>;
        type Right = Stamped<2>;
        let forward = <(Left, Right) as ChapterPass>::SCHEMA;
        let backward = <(Right, Left) as ChapterPass>::SCHEMA;
        assert_ne!(forward, backward);
        assert_ne!(forward, Left::SCHEMA);
        assert_ne!(forward, Right::SCHEMA);
    }

    #[test]
    fn a_triple_schema_is_order_sensitive_and_not_a_member_or_a_pair() {
        type A = Stamped<1>;
        type B = Stamped<2>;
        type C = Stamped<3>;
        let forward = <(A, B, C) as ChapterPass>::SCHEMA;
        assert_ne!(forward, <(A, C, B) as ChapterPass>::SCHEMA);
        assert_ne!(forward, <(C, B, A) as ChapterPass>::SCHEMA);
        assert_ne!(forward, <(A, B) as ChapterPass>::SCHEMA);
        assert_ne!(forward, <(B, C) as ChapterPass>::SCHEMA);
        assert_ne!(forward, A::SCHEMA);
        assert_ne!(forward, C::SCHEMA);
        // The shipped triple, whose members do not share a stamp.
        let brigade = <Brigade as ChapterPass>::SCHEMA;
        assert_ne!(brigade, HygieneBytes::SCHEMA);
        assert_ne!(brigade, Substrate::SCHEMA);
        assert_ne!(brigade, Words::SCHEMA);
        assert_ne!(brigade, <(HygieneBytes, Substrate) as ChapterPass>::SCHEMA);
    }

    /// The word member is not decoration: a corpus with a casing minority gets
    /// its row and its site out of the shipped triple, beside the other two.
    #[test]
    fn the_brigade_judges_and_sites_its_word_member() {
        let text: &'static str = Box::leak(
            (("and David went. ".repeat(40)) + "and david went and david went.").into_boxed_str(),
        );
        let books = vec![Book {
            key: BookKey::new(*b"MRK"),
            text,
            chapters: vec![chapter(1, 0, text.len() as u32)],
            verses: vec![verse(1, 1, 0, text.len() as u32)],
        }];
        let corpus = Corpus::try_new(&books).unwrap();
        let findings = analyze(&corpus, &Brigade::default());

        let casing: Vec<_> = findings
            .patterns()
            .iter()
            .filter(|row| row.channel == crate::Channel::Casing)
            .collect();
        assert_eq!(casing.len(), 1);
        assert_eq!((casing[0].numerator, casing[0].denominator), (2, 42));
        assert!(casing[0].word_hash().is_some());

        let sites: Vec<_> = findings
            .rows()
            .iter()
            .filter(|row| match row.kind() {
                FindingKind::Convention(digest) => digest.reasons().contains(Reasons::CASING),
                _ => false,
            })
            .map(|row| &text[row.from() as usize..row.to() as usize])
            .collect();
        assert_eq!(sites, vec!["david", "david"]);
    }

    #[test]
    fn a_tuple_folds_each_book_from_a_fresh_state_in_both_halves() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        // Substrate contributes no row to these books; LastByte's runs spell
        // out the carry it was handed.
        let carried: Vec<_> = hygiene_rows(&analyze(&corpus, &(LastByte, Substrate)))
            .into_iter()
            .map(|(book, from, _, _, run)| (book, from, run - 1))
            .collect();

        assert_eq!(carried, vec![(0, 0, 0), (0, 3, u32::from(b'y')), (1, 0, 0)]);
    }

    /// Over the hygiene-kind rows, which are the two members that emit them;
    /// the word member's rows are convention-kind and are tested above.
    #[test]
    fn analyze_over_the_brigade_equals_its_members_analyzed_separately() {
        let books = vec![mixed(), mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        let mut apart = hygiene_rows(&analyze(&corpus, &HygieneBytes));
        apart.extend(hygiene_rows(&analyze(&corpus, &Substrate)));
        // Stable, so the first pass keeps its place on a tie — as the tuple's
        // own tail sort does.
        apart.sort_by_key(|row| (row.0, row.1, row.2));

        assert_eq!(hygiene_rows(&analyze(&corpus, &Brigade::default())), apart);
    }

    const fn detached<O: Send + 'static>() {}

    #[test]
    fn a_hygiene_observation_is_send_and_borrow_free() {
        detached::<<HygieneBytes as ChapterPass>::Observation>();
        detached::<<LastByte as ChapterPass>::Observation>();
    }

    /// Test-only. Records the seam state it carried into each chapter, so the
    /// emitted runs spell out fold's chapter order and every book's fresh
    /// start.
    struct LastByte;

    impl ChapterPass for LastByte {
        type Observation = u8;
        /// `(chapter start, the byte the previous chapter ended on)`.
        type Aggregate = Vec<(u32, u8)>;
        type Config = ();
        const SCHEMA: SchemaStamp = SchemaStamp::new(u32::MAX);

        fn map(&self, chapter: ChapterInput<'_>) -> u8 {
            chapter.text.as_bytes().last().copied().unwrap_or(0)
        }

        fn fold(&self, book: &[ChapterObs<&u8>]) -> Vec<(u32, u8)> {
            let mut carry = None;
            book.iter()
                .map(|chapter| {
                    let seen = carry.unwrap_or(0);
                    carry = Some(*chapter.obs);
                    (chapter.start, seen)
                })
                .collect()
        }

        fn judge(&self, corpus: &[&Vec<(u32, u8)>], _config: &(), out: &mut Findings) {
            for (index, book) in corpus.iter().enumerate() {
                out.open_book(BookIndex::new(index).unwrap());
                for &(start, seen) in book.iter() {
                    out.push(
                        TextRange::new(start, start).unwrap(),
                        FindingKind::Hygiene(
                            HygieneDigest::new(HygieneClass::C0Control, u32::from(seen) + 1)
                                .unwrap(),
                        ),
                    )
                    .expect("a chapter start lies inside its book");
                }
            }
        }
    }

    #[test]
    fn every_book_folds_from_a_fresh_state() {
        let books = vec![mrk(), genesis()];
        let corpus = Corpus::try_new(&books).unwrap();
        let carried: Vec<_> = hygiene_rows(&analyze(&corpus, &LastByte))
            .into_iter()
            .map(|(book, from, _, _, run)| (book, from, run - 1))
            .collect();

        // MRK chapter 1 ends in `y`, chapter 2 in `z`; GEN restarts at 0.
        assert_eq!(carried, vec![(0, 0, 0), (0, 3, u32::from(b'y')), (1, 0, 0)]);
    }

    #[test]
    fn fold_cannot_tell_a_reused_observation_from_a_fresh_one() {
        let books = vec![mrk()];
        let corpus = Corpus::try_new(&books).unwrap();
        let book = &books[0];
        let text = book.text();
        let rows: Vec<Verse> = book.verses().collect();

        let mut mapped = Vec::new();
        let mut verses = Vec::new();
        let mut at = 0;
        for chapter in book.chapters() {
            at = collect_verses(&rows, at, chapter, &mut verses);
            let span = chapter.text();
            let input = ChapterInput {
                text: &text[span.from() as usize..span.to() as usize],
                verses: &verses,
                key: ChapterKey::new(book.key(), chapter.number()),
            };
            mapped.push((span.from(), LastByte.map(input)));
        }
        let fresh: Vec<ChapterObs<&u8>> = mapped
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();

        let mut reused = fresh.clone();
        reused.reverse();
        reused.sort_by_key(|chapter| chapter.start);

        assert_eq!(LastByte.fold(&fresh), LastByte.fold(&reused));

        let mut direct = Findings::new(vec![6]);
        LastByte.judge(&[&LastByte.fold(&fresh)], &(), &mut direct);
        direct.finish();
        assert_eq!(direct.rows(), analyze(&corpus, &LastByte).rows());
    }
}
