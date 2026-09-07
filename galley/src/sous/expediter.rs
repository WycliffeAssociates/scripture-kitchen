//! The resident Sous coordinator: one pass, one Pantry, one snapshot out.
//!
//! ```text
//! let mut sous = Expediter::new(Brigade::default(), 1 << 20);
//! sous.update("books/mrk.usfm", Role::Target, &text)?;
//! sous.publish()?          -> SOUS corpus buffer, raw-book UTF-16
//! sous.last_mapped()       -> 6      // six chapters mapped
//! sous.publish()?          -> the same bytes
//! sous.last_mapped()       -> 0      // nothing re-read, nothing re-mapped
//! ```
//!
//! Same projected chapter, same schema, same observation — so a hit skips the
//! map. A book that retains its text is keyed lazily at
//! [`Expediter::publish`]; one that does not is keyed at
//! [`update`](Expediter::update), while its text is still in hand. Every
//! publication sweeps what no retained generation names. Why the two hashes
//! divide the work this way: `expediter.md`.

#[cfg(feature = "parallel")]
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::judge::{Channel, PatternKey};
use sous_core::substrate::ScalarKey;
use sous_core::{
    BookIndex, Chapter, ChapterInput, ChapterObs, ChapterPass, ConventionDigest, CoordinateSpace,
    CorpusTotals, CorpusWireError, FindingKind, Findings, PackedFinding, Pattern, PatternIndex,
    ProjectedBook, PublicationBook, Reasons, SnapshotId, SourceLengths, TargetLengths, TextRange,
    encode_to_corpus_buffer, for_each_chapter, judge_lengths,
};
#[cfg(feature = "parallel")]
use sous_core::{ChapterKey, Verse};
use xxhash_rust::xxh3::Xxh3Default;

use mise::books::BookKey;

use super::{OnionBook, PublishError, rebase_span};
use crate::pantry::{BookId, Pantry, RawChecksum, Retain, Role};

/// One chapter's cache identity: xxh3-128 over its projected text, its rebased
/// verse rows, and the pass schema.
///
/// A markup-only edit moves the [`RawChecksum`] and not this, so the
/// observation is reused and only its coordinates are rebased. The chapter's
/// own address is absent, which is what lets identical chapters share one
/// entry — see `expediter.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservationKey([u8; 16]);

impl ObservationKey {
    fn of<P: ChapterPass>(chapter: &ChapterInput<'_>) -> Self {
        let mut hasher = Xxh3Default::new();
        hasher.update(&(chapter.text.len() as u64).to_le_bytes());
        hasher.update(chapter.text.as_bytes());
        hasher.update(&(chapter.verses.len() as u64).to_le_bytes());
        for verse in chapter.verses {
            let key = verse.key();
            hasher.update(&key.chapter().to_le_bytes());
            hasher.update(&key.first().to_le_bytes());
            hasher.update(&key.last().to_le_bytes());
            hasher.update(&verse.text().from().to_le_bytes());
            hasher.update(&verse.text().to().to_le_bytes());
        }
        hasher.update(&P::SCHEMA.get().to_le_bytes());
        Self(hasher.digest128().to_be_bytes())
    }

    pub fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// What publication needs of one chapter without its text: whose observation,
/// and where to rebase it to.
///
/// No `ChapterKey`: a fold rebases by `start` and never reads an address.
#[derive(Debug, Clone, Copy)]
struct ChapterRow {
    observation: ObservationKey,
    /// Projected-book offset the fold rebases chapter coordinates by.
    start: u32,
}

/// One book's firing set by CONTENT: xxh3-128 over each pattern's glyph,
/// channel, and key, in table order.
///
/// Never over the indices — a publication renumbers the table whenever another
/// book's counts move a denominator, while what THIS book can be sited for is
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FiringHash([u8; 16]);

impl FiringHash {
    fn of(firing: &[PatternIndex], table: &[Pattern]) -> Self {
        let mut hasher = Xxh3Default::new();
        for index in firing {
            let pattern = &table[usize::from(index.get())];
            hasher.update(&pattern.glyph.raw().to_le_bytes());
            hasher.update(&[pattern.channel as u8]);
            hasher.update(&key_bytes(pattern.key));
        }
        Self(hasher.digest128().to_be_bytes())
    }
}

/// A pattern's identity across publications, which its table position is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PatternRef {
    glyph: ScalarKey,
    channel: Channel,
    key: PatternKey,
}

impl PatternRef {
    fn of(pattern: &Pattern) -> Self {
        Self {
            glyph: pattern.glyph,
            channel: pattern.channel,
            key: pattern.key,
        }
    }
}

/// One located row without its text: a span plus whatever names the pattern.
#[derive(Debug, Clone, Copy)]
struct SiteRow {
    from: u32,
    to: u32,
    kind: CachedKind,
}

/// A convention row keeps a content reference, so a renumbered table still
/// resolves it; anything else a `locate` pushed replays as it stands.
#[derive(Debug, Clone, Copy)]
enum CachedKind {
    Convention {
        pattern: PatternRef,
        reasons: Reasons,
    },
    Other(FindingKind),
}

impl SiteRow {
    fn of(row: &PackedFinding, table: &[Pattern]) -> Self {
        let kind = match row.kind() {
            FindingKind::Convention(digest) => CachedKind::Convention {
                pattern: PatternRef::of(&table[usize::from(digest.pattern().get())]),
                reasons: digest.reasons(),
            },
            other => CachedKind::Other(other),
        };
        Self {
            from: row.from(),
            to: row.to(),
            kind,
        }
    }
}

/// The `PatternKey` variant and its fields, flat, so the hash reads the claim
/// and not a pointer.
fn key_bytes(key: PatternKey) -> [u8; 10] {
    let mut out = [0u8; 10];
    match key {
        PatternKey::ExactNeighbor(neighbor) => {
            out[0] = 0;
            out[1..5].copy_from_slice(&neighbor.raw().to_le_bytes());
        }
        PatternKey::RunShape { pure, bucket } => {
            out[..3].copy_from_slice(&[1, u8::from(pure), bucket]);
        }
        PatternKey::Placement { side, class } => {
            out[..3].copy_from_slice(&[2, side as u8, class as u8]);
        }
        PatternKey::Rarity => out[0] = 3,
        PatternKey::PooledNeighbor(pool) => out[..2].copy_from_slice(&[4, pool as u8]),
        PatternKey::Casing { hash, form } => {
            out[0] = 5;
            out[1] = form as u8;
            out[2..10].copy_from_slice(&hash.to_le_bytes());
        }
        PatternKey::WordLength { hash, sigma } => {
            out[0] = 6;
            out[1] = sigma;
            out[2..10].copy_from_slice(&hash.to_le_bytes());
        }
        PatternKey::Doubled { hash, separated } => {
            out[0] = 7;
            out[1] = u8::from(separated);
            out[2..10].copy_from_slice(&hash.to_le_bytes());
        }
    }
    out
}

/// Previous checksums a book keeps beside its current one, so an undo of that
/// many edits still lands on a retained chapter table.
const DEFAULT_GENERATIONS: usize = 4;

/// Books whose released member's chapter rows are kept anyway, most recently
/// edited first: a keystroke lands in the book the last one landed in, and
/// that book then re-maps one chapter instead of all of them.
///
/// Two, because the price is that member's rows for a whole book — about
/// 100-150 KB of `Words` per Bible book (evidence.md, W1 grain).
const DEFAULT_HOT_BOOKS: usize = 2;

/// The Sous coordinator over one [`ChapterPass`].
///
/// Holds the [`Pantry`] the host mutates it through, the observation cache
/// keyed by content, and one chapter table per retained book text.
pub struct Expediter<P: ChapterPass> {
    pantry: Pantry,
    pass: P,
    /// What judging may vary without touching a chapter or a book.
    config: P::Config,
    /// Content-addressed across books: two identical chapters are one entry,
    /// and a fold still counts both positions.
    observations: FxHashMap<ObservationKey, P::Observation>,
    /// Keyed by the book's raw checksum, so an unchanged book publishes
    /// without touching its text.
    chapter_tables: FxHashMap<RawChecksum, Vec<ChapterRow>>,
    /// One book's fold product, in book coordinates and free of a book index,
    /// so a later publication judges it under whatever index it has then.
    aggregates: FxHashMap<RawChecksum, P::Aggregate>,
    /// One book's located rows for the last firing set seen, keyed by the same
    /// checksum: unchanged text plus an unchanged firing set is a replay.
    sites: FxHashMap<RawChecksum, (FiringHash, Box<[SiteRow]>)>,
    /// The corpus counts judging reads, kept across publications: a book whose
    /// checksum moved is subtracted at its old one and added at its new, and
    /// the rest are never touched.
    totals: CorpusTotals,
    /// Which checksum each book contributes to [`Self::totals`] right now.
    tallied: FxHashMap<BookId, RawChecksum>,
    /// Per book, its current checksum ahead of the previous ones still kept —
    /// exactly the tables the sweep spares.
    generations: FxHashMap<BookId, Vec<RawChecksum>>,
    /// Ring depth behind the current checksum.
    kept: usize,
    /// The books whose chapter rows survive [`ChapterPass::release`], most
    /// recently edited first; never longer than `hot_ceiling`.
    hot: Vec<BookId>,
    /// How many books keep them; [`DEFAULT_HOT_BOOKS`] unless a host says.
    hot_ceiling: usize,
    /// Books that fell out of [`Self::hot`] and still owe their rows back;
    /// released at the end of the next publication, never before its folds.
    cooling: Vec<BookId>,
    /// Set when a table or a ring changed, so something may now be
    /// unreachable; a publication that finds it clear skips the sweep.
    dirty: bool,
    /// Chapters mapped since the last publication.
    pending: u64,
    /// How many of those were a re-walk of what `release` shed, in place.
    pending_remaps: u64,
    misses: u64,
    remaps: u64,
    folds: u64,
    located: u64,
    /// Whether a book's missing chapters are mapped on rayon; the two
    /// settings publish the same bytes.
    #[cfg(feature = "parallel")]
    parallel: bool,
}

/// One missing chapter's map input as ranges: `for_each_chapter` lends its
/// `ChapterInput` for the callback only, and a parallel map outlives that.
///
/// Ranges, not copies — the projected text is the book's own, and the verse
/// rows are pooled into one allocation for the whole book.
#[cfg(feature = "parallel")]
struct Queued<O> {
    observation: ObservationKey,
    /// Into the projected book text.
    text: core::ops::Range<usize>,
    /// Into this book's verse pool.
    verses: core::ops::Range<usize>,
    key: ChapterKey,
    /// A shed observation lifted out of the cache, to be re-walked in place
    /// and put back; `None` maps from nothing.
    held: Option<O>,
}

/// What one chapter's cache entry owes this publication.
#[derive(Clone, Copy)]
enum Redo {
    /// No entry at all: every member maps.
    Whole,
    /// An entry a `release` emptied: only the shed members walk again.
    Shed,
}

/// `Sync` unconditionally, so the `parallel` feature adds no bound the serial
/// build does not already carry: a pass is a stateless rule, not a session.
impl<P: ChapterPass + Sync> Expediter<P> {
    /// `budget_bytes` is the Pantry's Warmer LRU ceiling.
    pub fn new(pass: P, budget_bytes: usize) -> Self {
        Self {
            pantry: Pantry::new(budget_bytes),
            pass,
            config: P::Config::default(),
            observations: FxHashMap::default(),
            chapter_tables: FxHashMap::default(),
            aggregates: FxHashMap::default(),
            sites: FxHashMap::default(),
            totals: CorpusTotals::default(),
            tallied: FxHashMap::default(),
            generations: FxHashMap::default(),
            kept: DEFAULT_GENERATIONS,
            hot: Vec::new(),
            hot_ceiling: DEFAULT_HOT_BOOKS,
            cooling: Vec::new(),
            dirty: false,
            pending: 0,
            pending_remaps: 0,
            misses: 0,
            remaps: 0,
            folds: 0,
            located: 0,
            #[cfg(feature = "parallel")]
            parallel: true,
        }
    }

    /// Test-only: the serial map inside a build that would take the parallel
    /// one, so one binary can compare the two publications.
    #[cfg(all(test, feature = "parallel"))]
    fn serial(mut self) -> Self {
        self.parallel = false;
        self
    }

    /// Previous chapter tables kept per book, the current one aside; the
    /// default is [`DEFAULT_GENERATIONS`].
    pub fn with_generations(mut self, kept: usize) -> Self {
        self.kept = kept;
        self
    }

    /// How many recently edited books keep a book-grain member's chapter rows
    /// instead of shedding them; the default is [`DEFAULT_HOT_BOOKS`].
    ///
    /// Zero is the behaviour before there was a hot set: every book re-maps
    /// whole. A pass that retains chapters never sheds any, so this changes
    /// nothing for it.
    pub fn with_hot_books(mut self, hot: usize) -> Self {
        self.hot_ceiling = hot;
        self.cool_beyond_ceiling();
        self
    }

    /// The books whose chapter rows the next publication will not shed, most
    /// recently edited first.
    pub fn hot_books(&self) -> &[BookId] {
        &self.hot
    }

    /// Moves whatever no longer fits the ceiling into [`Self::cooling`].
    fn cool_beyond_ceiling(&mut self) {
        while self.hot.len() > self.hot_ceiling {
            self.cooling
                .push(self.hot.pop().expect("longer than the ceiling"));
        }
    }

    /// Derive and retain this book's products under the role's own retention:
    /// a target keeps its text, a reference keeps none. A target's chapters
    /// are keyed at the next [`publish`](Self::publish).
    pub fn update(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        text: &str,
    ) -> Result<BookKey, PublishError> {
        Ok(self
            .pantry
            .update(id, role, text)
            .map_err(PublishError::Pantry)?
            .key())
    }

    /// Derive and retain this book's products, keeping or dropping its text.
    ///
    /// The Pantry refuses a `Target` that keeps no text: placing its findings
    /// rescans that text, so publication would have nothing to read.
    pub fn update_with(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        retain: Retain,
        text: &str,
    ) -> Result<BookKey, PublishError> {
        Ok(self
            .pantry
            .update_with(id, role, retain, text)
            .map_err(PublishError::Pantry)?
            .key())
    }

    /// Drop a book's products, its text, and its retained generations; the
    /// tables and observations go at the next [`publish`](Self::publish).
    ///
    /// `false` when the id was not registered.
    pub fn remove(&mut self, id: &BookId) -> bool {
        self.dirty |= self.generations.remove(id).is_some();
        // The rows go with the ring at the next sweep, so a removed book owes
        // nothing back and leaves no slot warm.
        self.hot.retain(|seen| seen != id);
        self.cooling.retain(|seen| seen != id);
        self.pantry.remove(id)
    }

    /// The registry, read-only: books are registered through
    /// [`update`](Self::update) so their chapters cannot go unkeyed.
    pub fn pantry(&self) -> &Pantry {
        &self.pantry
    }

    /// The pass this coordinator maps, folds, and judges with.
    pub fn pass(&self) -> &P {
        &self.pass
    }

    /// The judging config the next [`publish`](Self::publish) uses.
    pub fn config(&self) -> &P::Config {
        &self.config
    }

    /// Replaces the judging config; no chapter is remapped and no book
    /// refolded, because neither reads it.
    pub fn set_config(&mut self, config: P::Config) {
        self.config = config;
    }

    /// Chapters mapped for the last [`publish`](Self::publish).
    ///
    /// A chapter whose retained observation was only partly re-walked counts
    /// here too — a remap is a map of that chapter, honestly.
    /// [`last_remapped`](Self::last_remapped) says how many of these were.
    pub fn last_mapped(&self) -> u64 {
        self.misses
    }

    /// Chapters of [`last_mapped`](Self::last_mapped) that kept an observation
    /// and re-walked only the members [`ChapterPass::release`] had shed.
    pub fn last_remapped(&self) -> u64 {
        self.remaps
    }

    /// Books folded for the last [`publish`](Self::publish); the rest judged
    /// their cached aggregate.
    pub fn last_folded(&self) -> u64 {
        self.folds
    }

    /// Books rescanned for sites by the last [`publish`](Self::publish); the
    /// rest replayed the rows their cache already held.
    pub fn last_located(&self) -> u64 {
        self.located
    }

    /// Observations the cache holds after the last sweep.
    pub fn resident_observations(&self) -> usize {
        self.observations.len()
    }

    /// Chapter tables the cache holds — one per retained generation of every
    /// registered book.
    pub fn resident_tables(&self) -> usize {
        self.chapter_tables.len()
    }

    /// Aggregates the cache holds after the last sweep — one per retained
    /// generation for a chapter-grain pass, one per book for a pass that
    /// answers `false` to [`ChapterPass::RETAIN_CHAPTERS`].
    pub fn resident_aggregates(&self) -> usize {
        self.aggregates.len()
    }

    /// The Pantry's retained products plus this cache's own rows.
    ///
    /// Real on both sides: [`ChapterPass::aggregate_bytes`] and
    /// [`ChapterPass::observation_bytes`] sum the heap a pass hangs off each,
    /// so a hot book's unshed chapter rows are counted where they are held.
    pub fn resident_bytes(&self) -> usize {
        let rows: usize = self
            .chapter_tables
            .values()
            .map(|table| size_of::<RawChecksum>() + table.len() * size_of::<ChapterRow>())
            .sum();
        let cached: usize = self
            .aggregates
            .values()
            .map(|aggregate| size_of::<RawChecksum>() + self.pass.aggregate_bytes(aggregate))
            .sum();
        let sites: usize = self
            .sites
            .values()
            .map(|(_, rows)| {
                size_of::<RawChecksum>() + size_of::<FiringHash>() + size_of_val(&**rows)
            })
            .sum();
        let rings: usize = self
            .generations
            .values()
            .map(|ring| size_of::<BookId>() + ring.len() * size_of::<RawChecksum>())
            .sum();
        let held: usize = self
            .observations
            .values()
            .map(|obs| size_of::<ObservationKey>() + self.pass.observation_bytes(obs))
            .sum();
        self.pantry.resident_bytes()
            + held
            + rows
            + cached
            + sites
            + rings
            + self.totals.resident_bytes()
            + self.tallied.len() * (size_of::<BookId>() + size_of::<RawChecksum>())
    }

    /// Keys this book's chapters under the Pantry's current checksum and maps
    /// the ones the cache is missing; a checksum already keyed only ages.
    ///
    /// `fresh` names the observations this publication has already found whole
    /// and has not shed yet. A pass retained at book grain
    /// ([`ChapterPass::RETAIN_CHAPTERS`]) empties a member's chapter rows once
    /// the fold has read them, so a book whose aggregate is gone may only fold
    /// from those: it re-maps every chapter of its own that is not in the set.
    /// A chapter that kept its [`ObservationKey`] re-maps through
    /// [`ChapterPass::remap`], which walks only the members that were shed —
    /// so a keystroke rewalks the edited book's words and nothing else's
    /// glyphs.
    fn index_book(
        &mut self,
        id: &BookId,
        fresh: &mut FxHashSet<ObservationKey>,
    ) -> Result<(), PublishError> {
        #[cfg(feature = "parallel")]
        let parallel = self.parallel;
        let Self {
            pantry,
            pass,
            observations,
            chapter_tables,
            aggregates,
            generations,
            kept,
            hot,
            hot_ceiling,
            cooling,
            dirty,
            pending,
            pending_remaps,
            ..
        } = self;
        let products = pantry.products(id).expect("the caller named a live book");
        let checksum = products.checksum;
        let ring = generations.entry(id.clone()).or_default();
        if ring.first() != Some(&checksum) {
            ring.retain(|seen| *seen != checksum);
            ring.insert(0, checksum);
            ring.truncate(*kept + 1);
            *dirty = true;
            // The book's text moved, so it goes to the front of the hot set
            // and whatever that pushes off owes its rows back.
            if hot.first() != Some(id) {
                hot.retain(|seen| seen != id);
                hot.insert(0, id.clone());
                while hot.len() > *hot_ceiling {
                    cooling.push(hot.pop().expect("longer than the ceiling"));
                }
            }
        }
        let regrain = !P::RETAIN_CHAPTERS && !aggregates.contains_key(&checksum);
        // A row a hot book kept is as good as one mapped this publication:
        // `release` is what a fold may not read, and it never ran on these.
        if let Some(table) = chapter_tables.get(&checksum)
            && (!regrain
                || table.iter().all(|row| {
                    fresh.contains(&row.observation)
                        || observations
                            .get(&row.observation)
                            .is_some_and(|held| !pass.is_released(held))
                }))
        {
            return Ok(());
        }

        let text = products
            .text
            .ok_or_else(|| PublishError::NoText { id: id.clone() })?;
        // The one place projected text exists, and only for a book whose
        // chapters are not already keyed.
        let book = OnionBook::from_parts(text, products.mask.clone(), products.toc.clone())
            .map_err(|error| PublishError::InvalidBook {
                id: id.clone(),
                error,
            })?;
        let mut table = Vec::new();
        #[cfg(feature = "parallel")]
        let (mut queued, mut pool) = (Vec::<Queued<P::Observation>>::new(), Vec::<Verse>::new());
        for_each_chapter(&book, |start, chapter| {
            let observation = ObservationKey::of::<P>(&chapter);
            table.push(ChapterRow { observation, start });
            // Both settings visit a repeated chapter once and count it once.
            if fresh.contains(&observation) {
                return;
            }
            // A shed observation is not a hit: a re-grained book walks again
            // whatever `release` emptied, and only that.
            let redo = match observations.get(&observation) {
                None => Some(Redo::Whole),
                Some(held) if regrain && pass.is_released(held) => Some(Redo::Shed),
                Some(_) => None,
            };
            fresh.insert(observation);
            let Some(redo) = redo else { return };
            #[cfg(feature = "parallel")]
            if parallel {
                let text = start as usize..start as usize + chapter.text.len();
                debug_assert_eq!(&book.text()[text.clone()], chapter.text);
                let from = pool.len();
                pool.extend_from_slice(chapter.verses);
                queued.push(Queued {
                    observation,
                    text,
                    verses: from..pool.len(),
                    key: chapter.key,
                    held: match redo {
                        Redo::Whole => None,
                        Redo::Shed => observations.remove(&observation),
                    },
                });
                return;
            }
            match redo {
                Redo::Whole => {
                    let mapped = pass.map(chapter);
                    observations.insert(observation, mapped);
                }
                Redo::Shed => {
                    let held = observations
                        .get_mut(&observation)
                        .expect("the entry this arm just read");
                    pass.remap(chapter, held);
                    *pending_remaps += 1;
                }
            }
            *pending += 1;
        });
        // Collected in chapter order and inserted in it: the map is content
        // keyed, so the table it feeds is the same either way.
        #[cfg(feature = "parallel")]
        if parallel {
            let text = book.text();
            *pending_remaps += queued.iter().filter(|row| row.held.is_some()).count() as u64;
            let mapped: Vec<P::Observation> = queued
                .par_iter_mut()
                .map(|chapter| {
                    let input = ChapterInput {
                        text: &text[chapter.text.clone()],
                        verses: &pool[chapter.verses.clone()],
                        key: chapter.key,
                    };
                    match chapter.held.take() {
                        Some(mut held) => {
                            pass.remap(input, &mut held);
                            held
                        }
                        None => pass.map(input),
                    }
                })
                .collect();
            *pending += mapped.len() as u64;
            for (chapter, observation) in queued.iter().zip(mapped) {
                observations.insert(chapter.observation, observation);
            }
        }
        chapter_tables.insert(checksum, table);
        *dirty = true;
        Ok(())
    }

    /// Drops every chapter table outside a book's retained generations, and
    /// every observation no surviving table names.
    ///
    /// A pass that does not retain chapters ([`ChapterPass::RETAIN_CHAPTERS`])
    /// keeps its aggregate only for each book's CURRENT checksum: an older
    /// generation's chapters are gone already, so its aggregate is the one
    /// thing an undo cannot cheaply rebuild — and the one thing worth ageing
    /// out anyway, since re-mapping and re-folding a whole book is the price
    /// this pass already chose over keeping rows (`expediter.md`).
    ///
    /// A publication that added no table and aged no ring has nothing to free,
    /// and skips the walk.
    fn sweep(&mut self) {
        if !core::mem::take(&mut self.dirty) {
            return;
        }
        let live: FxHashSet<RawChecksum> = self.generations.values().flatten().copied().collect();
        self.chapter_tables
            .retain(|checksum, _| live.contains(checksum));
        if P::RETAIN_CHAPTERS {
            self.aggregates
                .retain(|checksum, _| live.contains(checksum));
        } else {
            let current: FxHashSet<RawChecksum> = self
                .generations
                .values()
                .filter_map(|ring| ring.first().copied())
                .collect();
            self.aggregates
                .retain(|checksum, _| current.contains(checksum));
        }
        self.sites.retain(|checksum, _| live.contains(checksum));
        let named: FxHashSet<ObservationKey> = self
            .chapter_tables
            .values()
            .flatten()
            .map(|row| row.observation)
            .collect();
        self.observations.retain(|key, _| named.contains(key));
    }

    /// One complete corpus publication over every `Target` book, in canonical
    /// order, in raw-book UTF-16.
    ///
    /// Maps only chapters whose [`ObservationKey`] is absent, folds only a
    /// book whose aggregate is missing, judges every Target book, and sweeps.
    pub fn publish(&mut self) -> Result<Vec<u8>, PublishError> {
        let books: Vec<(BookId, BookKey)> = self.pantry.books(Role::Target).to_vec();
        let references: Vec<(BookId, BookKey)> = self.pantry.books(Role::Reference).to_vec();
        if books.len() > usize::from(u16::MAX) + 1 {
            return Err(PublishError::Wire(CorpusWireError::BookCountOverflow {
                count: books.len(),
            }));
        }

        // Cleared per publication: what it names is what a book-grain pass may
        // still fold from.
        let mut fresh: FxHashSet<ObservationKey> = FxHashSet::default();
        for (id, _) in &books {
            self.index_book(id, &mut fresh)?;
        }
        self.misses = core::mem::take(&mut self.pending);
        self.remaps = core::mem::take(&mut self.pending_remaps);

        let mut projected_lens = Vec::with_capacity(books.len());
        let mut published_lens = Vec::with_capacity(books.len());
        for (id, _) in &books {
            let products = self.pantry.products(id).expect("the pantry listed this id");
            projected_lens.push(products.mask.len());
            published_lens.push(products.published_len);
        }

        // Scoped so the borrowed observations are released before the sweep.
        let (mut folds, mut located) = (0, 0);
        let (projected, patterns) = {
            let Self {
                pantry,
                pass,
                config,
                observations,
                chapter_tables,
                aggregates,
                sites,
                totals,
                tallied,
                generations,
                hot,
                cooling,
                ..
            } = &mut *self;
            let checksums: Vec<RawChecksum> = books
                .iter()
                .map(|(id, _)| {
                    pantry
                        .products(id)
                        .expect("the pantry listed this id")
                        .checksum
                })
                .collect();
            let mut folded: Vec<RawChecksum> = Vec::new();
            for checksum in &checksums {
                if aggregates.contains_key(checksum) {
                    continue;
                }
                let chapters: Vec<ChapterObs<&P::Observation>> = chapter_tables[checksum]
                    .iter()
                    .map(|row| ChapterObs {
                        start: row.start,
                        obs: &observations[&row.observation],
                    })
                    .collect();
                aggregates.insert(*checksum, pass.fold(&chapters));
                folds += 1;
                folded.push(*checksum);
            }
            // Book grain: every fold this publication needed has run, so the
            // rows it read are dropped — except the hot set's, which are the
            // point of keeping one. The aggregate is what survives either way.
            let cooled = core::mem::take(cooling);
            if !P::RETAIN_CHAPTERS {
                let warm: FxHashSet<RawChecksum> = hot
                    .iter()
                    .filter_map(|id| generations.get(id)?.first().copied())
                    .collect();
                let mut shed = |checksum: &RawChecksum| {
                    if warm.contains(checksum) {
                        return;
                    }
                    for row in &chapter_tables[checksum] {
                        if let Some(obs) = observations.get_mut(&row.observation) {
                            pass.release(obs);
                        }
                    }
                };
                for checksum in &folded {
                    shed(checksum);
                }
                // A book that fell out of the hot set gives back every
                // generation's rows, this publication's folds all done.
                for id in &cooled {
                    for checksum in generations.get(id).into_iter().flatten() {
                        if chapter_tables.contains_key(checksum) {
                            shed(checksum);
                        }
                    }
                }
            }

            // The resident corpus totals: only a book whose checksum moved is
            // subtracted at its old one and added at its new.
            let mut stale: Vec<&P::Aggregate> = Vec::new();
            let mut added: Vec<&P::Aggregate> = Vec::new();
            let live: FxHashSet<&BookId> = books.iter().map(|(id, _)| id).collect();
            for (index, (id, _)) in books.iter().enumerate() {
                match tallied.get(id) {
                    Some(seen) if *seen == checksums[index] => {}
                    seen => {
                        if let Some(seen) = seen {
                            stale.push(&aggregates[seen]);
                        }
                        added.push(&aggregates[&checksums[index]]);
                    }
                }
            }
            for (id, seen) in tallied.iter() {
                if !live.contains(id) {
                    stale.push(&aggregates[seen]);
                }
            }
            if !stale.is_empty() {
                pass.untally(totals, &stale);
            }
            if !added.is_empty() {
                pass.tally(totals, &added);
            }
            tallied.retain(|id, _| live.contains(id));
            for (index, (id, _)) in books.iter().enumerate() {
                match tallied.get_mut(id) {
                    Some(seen) => *seen = checksums[index],
                    None => {
                        tallied.insert(id.clone(), checksums[index]);
                    }
                }
            }

            let mut findings = Findings::new(projected_lens);
            {
                // Judging is corpus-level: every Target book, changed or not.
                let corpus: Vec<&P::Aggregate> = checksums
                    .iter()
                    .map(|checksum| &aggregates[checksum])
                    .collect();
                pass.judge_resident(&corpus, totals, config, &mut findings);

                // Then the source comparison, from lengths both sides already
                // retain. Every Target pairs with the Reference of the same
                // `BookKey`; a Target with none gets no ratios and no rows,
                // which is the contract and not an error. The pairing facts
                // are alignment structure, never findings, so they are
                // dropped here — a host that wants them runs the cold path.
                if let Some(lengths) = pass.length_config(config) {
                    let source: Vec<SourceLengths<'_>> = references
                        .iter()
                        .filter_map(|(id, key)| {
                            Some(SourceLengths {
                                book: *key,
                                verses: pantry.reference_lengths(id)?,
                            })
                        })
                        .collect();
                    if !source.is_empty() {
                        let target: Vec<TargetLengths<'_>> = books
                            .iter()
                            .zip(&corpus)
                            .map(|((_, key), aggregate)| TargetLengths {
                                book: *key,
                                verses: pass.verse_lengths(aggregate),
                            })
                            .collect();
                        judge_lengths(&target, &source, &lengths, &mut findings);
                    }
                }
            }

            // Then place what judging decided, per book, from the current text.
            let table: Vec<Pattern> = findings.patterns().to_vec();
            let resolver: FxHashMap<PatternRef, PatternIndex> = table
                .iter()
                .enumerate()
                .map(|(at, pattern)| (PatternRef::of(pattern), PatternIndex::new(at as u16)))
                .collect();
            let mut firing: Vec<PatternIndex> = Vec::new();
            for (index, (id, _)) in books.iter().enumerate() {
                let book = BookIndex::new(index).expect("a corpus indexes every book");
                let checksum = checksums[index];
                let aggregate = &aggregates[&checksum];
                pass.firing(aggregate, &table, &mut firing);
                let hash = FiringHash::of(&firing, &table);
                if let Some((seen, rows)) = sites.get(&checksum)
                    && *seen == hash
                {
                    replay(book, rows, &resolver, &mut findings);
                    continue;
                }
                let products = pantry.products(id).expect("the pantry listed this id");
                let text = products
                    .text
                    .ok_or_else(|| PublishError::NoText { id: id.clone() })?;
                // One reprojection per relocated book; no second text copy is
                // retained, exactly as `index_book` does it.
                let view = OnionBook::from_parts(text, products.mask.clone(), products.toc.clone())
                    .map_err(|error| PublishError::InvalidBook {
                        id: id.clone(),
                        error,
                    })?;
                let rows: Vec<Chapter> = view.chapters().collect();
                let before = findings.len();
                let verses: Vec<sous_core::Verse> = view.verses().collect();
                pass.locate(book, view.text(), &rows, &verses, aggregate, &mut findings);
                let cached: Box<[SiteRow]> = findings.rows()[before..]
                    .iter()
                    .map(|row| SiteRow::of(row, &table))
                    .collect();
                sites.insert(checksum, (hash, cached));
                located += 1;
            }
            findings.finish();
            findings.into_parts()
        };
        self.folds = folds;
        self.located = located;
        self.sweep();

        let mut per_book: Vec<Vec<PackedFinding>> = (0..books.len()).map(|_| Vec::new()).collect();
        for (row, finding) in projected.iter().enumerate() {
            let index = usize::from(finding.book_idx().get());
            let products = self
                .pantry
                .products(&books[index].0)
                .expect("judge named an open book");
            let (from, to) = rebase_span(
                products.mask,
                |byte| products.utf16.to_utf16(byte),
                finding.from()..finding.to(),
            );
            per_book[index].push(
                PackedFinding::new(
                    from,
                    to,
                    finding.book_idx(),
                    finding.kind(),
                    &published_lens,
                )
                .map_err(|error| PublishError::Rebase { row, error })?,
            );
        }

        let sections: Vec<PublicationBook<'_>> = books
            .iter()
            .zip(&per_book)
            .zip(&published_lens)
            .map(|(((id, key), findings), &published_len)| {
                PublicationBook::new(*key, id.as_str(), published_len, findings)
            })
            .collect();
        let snapshot = snapshot_id::<P>(&self.pantry, &books, &references);
        encode_to_corpus_buffer(snapshot, CoordinateSpace::Utf16, &sections, &patterns)
            .map_err(PublishError::Wire)
    }
}

/// Pushes one book's cached rows back, each convention row under the pattern
/// index THIS publication gave its content.
///
/// A row whose pattern the table no longer holds cannot happen: the firing
/// hash covers exactly the contents that produced these rows.
fn replay(
    book: BookIndex,
    rows: &[SiteRow],
    resolver: &FxHashMap<PatternRef, PatternIndex>,
    out: &mut Findings,
) {
    out.open_book(book);
    for row in rows {
        let kind = match row.kind {
            CachedKind::Convention { pattern, reasons } => {
                let index = resolver[&pattern];
                FindingKind::Convention(ConventionDigest::new(index, reasons))
            }
            CachedKind::Other(kind) => kind,
        };
        let span = TextRange::new(row.from, row.to).expect("a cached span keeps its order");
        out.push(span, kind)
            .expect("a replayed span lies inside the book it was found in");
    }
}

/// xxh3-128 over the canonical (`BookKey`, id, `RawChecksum`) table plus the
/// pass schema: what the publication is OF, not what it says.
fn snapshot_id<P: ChapterPass>(
    pantry: &Pantry,
    books: &[(BookId, BookKey)],
    references: &[(BookId, BookKey)],
) -> SnapshotId {
    let mut hasher = Xxh3Default::new();
    // References ride the identity too: swapping the declared source changes
    // what the publication is OF, not only what it says.
    for (role, rows) in [(0u8, books), (1, references)] {
        hasher.update(&[role]);
        for (id, key) in rows {
            hasher.update(&key.as_bytes());
            hasher.update(&(id.as_str().len() as u32).to_le_bytes());
            hasher.update(id.as_str().as_bytes());
            hasher.update(
                &pantry
                    .checksum(id)
                    .expect("the pantry listed this id")
                    .as_bytes(),
            );
        }
    }
    hasher.update(&P::SCHEMA.get().to_le_bytes());
    SnapshotId::new(hasher.digest128().to_be_bytes())
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use sous_core::hygiene::HygieneBytes;
    use sous_core::{
        BookIndex, Brigade, CorpusSnapshot, FindingKind, JudgingConfig, SchemaStamp, Substrate,
        Words,
    };

    use crate::pantry::{PantryError, Retain};

    /// A `\\` pair is the one backslash Onion's mask hands to Sous as content,
    /// so every chapter body below carries exactly one hygiene finding.
    fn book(code: &str, chapters: &[&str]) -> String {
        let mut text = format!("\\id {code}\n\\h {code}\n");
        for (number, body) in chapters.iter().enumerate() {
            text.push_str(&format!("\\c {}\n\\p\n\\v 1 {body}\n", number + 1));
        }
        text
    }

    fn mark() -> String {
        book(
            "MRK",
            &[
                "Jesus wept \\\\ here.",
                "He entered \\\\ Capernaum.",
                "A withered \\\\ hand.",
            ],
        )
    }

    fn genesis() -> String {
        book("GEN", &["In the \\\\ beginning."])
    }

    fn sous() -> Expediter<Brigade> {
        Expediter::new(Brigade::default(), 1 << 20)
    }

    /// A chapter-grain pass: `Brigade` carries `Words`, which is retained at
    /// book grain, so what one chapter's cache key does and does not cover is
    /// only visible through a pass that keeps its chapters.
    fn grain() -> Expediter<HygieneBytes> {
        Expediter::new(HygieneBytes, 1 << 20)
    }

    /// One member's own map and remap calls, which `last_mapped` cannot
    /// separate: it counts chapters, not the walks a chapter ran.
    #[derive(Debug, Default)]
    struct Counting<P> {
        inner: P,
        maps: AtomicU64,
        remaps: AtomicU64,
    }

    impl<P> Counting<P> {
        fn maps(&self) -> u64 {
            self.maps.load(Ordering::Relaxed)
        }

        fn remaps(&self) -> u64 {
            self.remaps.load(Ordering::Relaxed)
        }
    }

    impl<P: ChapterPass> ChapterPass for Counting<P> {
        type Observation = P::Observation;
        type Aggregate = P::Aggregate;
        type Config = P::Config;
        const SCHEMA: SchemaStamp = P::SCHEMA;
        const RETAIN_CHAPTERS: bool = P::RETAIN_CHAPTERS;

        fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
            self.maps.fetch_add(1, Ordering::Relaxed);
            self.inner.map(chapter)
        }

        fn fold(&self, book: &[ChapterObs<&Self::Observation>]) -> Self::Aggregate {
            self.inner.fold(book)
        }

        fn release(&self, obs: &mut Self::Observation) {
            self.inner.release(obs);
        }

        fn is_released(&self, observation: &Self::Observation) -> bool {
            self.inner.is_released(observation)
        }

        fn remap(&self, chapter: ChapterInput<'_>, observation: &mut Self::Observation) {
            self.remaps.fetch_add(1, Ordering::Relaxed);
            self.inner.remap(chapter, observation);
        }

        fn judge(&self, corpus: &[&Self::Aggregate], config: &Self::Config, out: &mut Findings) {
            self.inner.judge(corpus, config, out);
        }

        fn aggregate_bytes(&self, aggregate: &Self::Aggregate) -> usize {
            self.inner.aggregate_bytes(aggregate)
        }

        fn tally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
            self.inner.tally(totals, books);
        }

        fn untally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
            self.inner.untally(totals, books);
        }

        fn judge_resident(
            &self,
            corpus: &[&Self::Aggregate],
            totals: &CorpusTotals,
            config: &Self::Config,
            out: &mut Findings,
        ) {
            self.inner.judge_resident(corpus, totals, config, out);
        }

        fn locate(
            &self,
            book: BookIndex,
            text: &str,
            chapters: &[Chapter],
            verses: &[sous_core::Verse],
            aggregate: &Self::Aggregate,
            out: &mut Findings,
        ) {
            self.inner
                .locate(book, text, chapters, verses, aggregate, out);
        }

        fn firing(
            &self,
            aggregate: &Self::Aggregate,
            patterns: &[Pattern],
            out: &mut Vec<PatternIndex>,
        ) {
            self.inner.firing(aggregate, patterns, out);
        }
    }

    /// Every HYGIENE row of a published buffer as (book index, id, from, to).
    ///
    /// The substrate sites its conventions into the same sections; those rows
    /// are counted by [`site_rows`].
    fn rows(buffer: &[u8]) -> Vec<(u16, String, u32, u32)> {
        let snapshot = CorpusSnapshot::open(buffer).unwrap();
        (0..snapshot.len())
            .flat_map(|index| {
                let book = snapshot
                    .book(BookIndex::new(index).unwrap())
                    .expect("directory position");
                (0..book.len()).filter_map(move |row| {
                    let finding = book.at(row).unwrap();
                    matches!(finding.kind(), FindingKind::Hygiene(_)).then(|| {
                        (
                            finding.book_idx().get(),
                            book.id().to_string(),
                            finding.from(),
                            finding.to(),
                        )
                    })
                })
            })
            .collect()
    }

    /// Every convention row as (book index, pattern index, reason bits).
    fn site_rows(buffer: &[u8]) -> Vec<(u16, u16, u16)> {
        let snapshot = CorpusSnapshot::open(buffer).unwrap();
        (0..snapshot.len())
            .flat_map(|index| {
                let book = snapshot
                    .book(BookIndex::new(index).unwrap())
                    .expect("directory position");
                (0..book.len()).filter_map(move |row| {
                    let finding = book.at(row).unwrap();
                    let FindingKind::Convention(digest) = finding.kind() else {
                        return None;
                    };
                    Some((
                        finding.book_idx().get(),
                        digest.pattern().get(),
                        digest.reasons().bits(),
                    ))
                })
            })
            .collect()
    }

    fn ids(buffer: &[u8]) -> Vec<String> {
        let snapshot = CorpusSnapshot::open(buffer).unwrap();
        (0..snapshot.len())
            .map(|index| {
                snapshot
                    .book(BookIndex::new(index).unwrap())
                    .unwrap()
                    .id()
                    .to_string()
            })
            .collect()
    }

    /// Byte equality with a cold `analyze` is pinned where its oracle lives,
    /// in `galley/tests/equivalence.rs`; these tests pin what is reused.
    #[test]
    fn the_publication_is_in_canonical_order_and_maps_every_chapter_once() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let buffer = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 4, "one GEN chapter, three MRK");
        // Canonical order, not update order: GEN before MRK.
        assert_eq!(ids(&buffer), vec!["a/gen.usfm", "b/mrk.usfm"]);
    }

    #[test]
    fn a_second_publish_maps_nothing_and_republishes_the_same_bytes() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        let first = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 3);

        let second = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0);
        assert_eq!(first, second, "identity and every row");
    }

    /// A footnote whose content the verse-text mask removes: the projected
    /// chapters are byte-identical, the raw book is 16 UTF-16 units longer.
    #[test]
    fn a_markup_only_edit_maps_nothing_and_shifts_the_published_offsets() {
        let before = mark();
        let after = before.replace("Jesus", "Jesus\\f + \\ft note\\f*");
        let mut sous = grain();
        sous.update("b/mrk.usfm", Role::Target, &before).unwrap();
        let first = rows(&sous.publish().unwrap());

        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        let second = rows(&sous.publish().unwrap());
        assert_eq!(sous.last_mapped(), 0, "no projected chapter moved");

        let shifted: Vec<_> = first
            .iter()
            .map(|(book, id, from, to)| (*book, id.clone(), from + 16, to + 16))
            .collect();
        assert_eq!(second, shifted, "every span past the insertion moves by it");
    }

    /// `\v 1` to `\v 1-2` masks out identically but rekeys the verse row.
    #[test]
    fn a_verse_marker_edit_with_unchanged_content_remaps_only_its_chapter() {
        let before = mark();
        let after = before.replace("\\v 1 He entered", "\\v 1-2 He entered");
        let mut sous = grain();
        sous.update("b/mrk.usfm", Role::Target, &before).unwrap();
        sous.publish().unwrap();

        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 1, "the rekeyed chapter and no other");
    }

    #[test]
    fn a_one_chapter_content_edit_maps_exactly_one_chapter() {
        let before = mark();
        let after = before.replace("A withered", "A shrivelled");
        let mut sous = grain();
        sous.update("b/mrk.usfm", Role::Target, &before).unwrap();
        sous.publish().unwrap();

        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        let buffer = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 1);
        assert_eq!(rows(&buffer).len(), 3, "still one finding per chapter");
    }

    /// `Words` retains no chapter rows, so an edit anywhere in a COLD book
    /// re-maps that whole book — and no other. `with_hot_books(0)` is what
    /// makes every book cold; the hot set's own claim is pinned below.
    #[test]
    fn a_book_grain_pass_remaps_the_edited_book_and_nothing_else() {
        let mut sous = sous().with_hot_books(0);
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        sous.publish().unwrap();

        let after = mark().replace("A withered", "A shrivelled");
        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        let buffer = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 3, "MRK's three chapters, GEN's none");
        assert_eq!(sous.last_folded(), 1, "GEN judged its cached aggregate");
        assert_eq!(rows(&buffer).len(), 4, "still one finding per chapter");
    }

    /// What a book-grain member leaves resident is its aggregate, not its
    /// rows: the fold reads them and the publication drops them.
    #[test]
    fn a_book_grain_pass_sheds_its_chapter_rows_after_the_fold() {
        let mut sous = sous().with_hot_books(0);
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.resident_observations(), 3);
        assert!(
            sous.observations
                .values()
                .all(|(_, _, words)| words.words().is_empty()),
            "every word row is back to its empty default"
        );
    }

    /// A book-grain member sheds its rows, so its book is walked whole again —
    /// but only IT is: the chapter-grain member beside it keeps every slot the
    /// edit did not move, and maps the one chapter it did.
    #[test]
    fn a_keystroke_in_one_chapter_rewalks_words_for_the_book_but_glyphs_only_for_the_chapter() {
        let mut sous: Expediter<(Counting<Substrate>, Words)> =
            Expediter::new((Counting::default(), Words), 1 << 20).with_hot_books(0);
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        let before = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 3, "cold: every chapter");
        assert_eq!(sous.last_remapped(), 0, "nothing was held to re-walk");
        assert_eq!(sous.pass().0.maps(), 3, "the glyph walk ran three times");

        let after = mark().replace("A withered", "A shrivelled");
        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        let published = sous.publish().unwrap();
        assert_eq!(
            sous.last_mapped(),
            3,
            "the words are still a book-wide walk"
        );
        assert_eq!(
            sous.last_remapped(),
            2,
            "two chapters kept their glyph rows"
        );
        assert_eq!(
            sous.pass().0.maps(),
            4,
            "one more glyph walk, for the edited chapter alone"
        );
        assert_eq!(
            sous.pass().0.remaps(),
            0,
            "a member that shed nothing is not asked to walk again"
        );
        assert_ne!(published, before, "the edit is published");
    }

    /// Four books in canonical order, so a cold publication leaves the last
    /// two hot and the first two cold.
    fn four_books(sous: &mut Expediter<Brigade>) {
        for (id, code) in [
            ("a/gen.usfm", "GEN"),
            ("b/mrk.usfm", "MRK"),
            ("c/luk.usfm", "LUK"),
            ("d/rev.usfm", "REV"),
        ] {
            let text = match code {
                "GEN" => genesis(),
                "MRK" => mark(),
                other => book(other, &["A first \\\\ chapter.", "A second \\\\ one."]),
            };
            sous.update(id, Role::Target, &text).unwrap();
        }
        sous.publish().unwrap();
    }

    /// Every word row the observation cache is holding right now.
    fn word_row_bytes(sous: &Expediter<Brigade>) -> usize {
        sous.observations
            .values()
            .map(|(_, _, words)| words.resident_bytes())
            .sum()
    }

    /// The hot set is what turns the second keystroke in one book into one
    /// chapter's walk: the first one's rows were shed at the last fold, and
    /// the edit that shed them is what made the book hot.
    #[test]
    fn a_second_keystroke_in_a_hot_book_remaps_only_the_chapter_it_moved() {
        let mut sous = sous();
        four_books(&mut sous);
        assert_eq!(
            sous.hot_books(),
            [BookId::from("d/rev.usfm"), BookId::from("c/luk.usfm")],
            "the last two indexed"
        );

        let once = mark().replace("A withered", "A shrivelled");
        sous.update("b/mrk.usfm", Role::Target, &once).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 3, "cold: MRK's three chapters");
        assert_eq!(sous.last_remapped(), 2, "two of them kept a glyph row");
        assert_eq!(sous.hot_books()[0], BookId::from("b/mrk.usfm"));

        let twice = once.replace("He entered", "He walked into");
        sous.update("b/mrk.usfm", Role::Target, &twice).unwrap();
        let published = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 1, "hot: the moved chapter alone");
        assert_eq!(sous.last_remapped(), 0, "nothing was shed to walk again");
        assert_eq!(rows(&published).len(), 8, "one finding per chapter");
    }

    /// And it is bounded: the third book edited after it pushes the first out,
    /// and the rows it was keeping go back at that publication.
    #[test]
    fn a_book_that_falls_out_of_the_hot_set_walks_whole_again() {
        let mut sous = sous();
        four_books(&mut sous);
        let edited = mark().replace("A withered", "A shrivelled");
        sous.update("b/mrk.usfm", Role::Target, &edited).unwrap();
        sous.publish().unwrap();
        let warm = word_row_bytes(&sous);

        for (id, code) in [("c/luk.usfm", "LUK"), ("d/rev.usfm", "REV")] {
            let text = book(code, &["A first \\\\ chapter.", "A later \\\\ one."]);
            sous.update(id, Role::Target, &text).unwrap();
            sous.publish().unwrap();
        }
        assert!(
            !sous.hot_books().contains(&BookId::from("b/mrk.usfm")),
            "two other books were edited after it"
        );
        assert!(
            word_row_bytes(&sous) < warm,
            "MRK's word rows went back: {} B against {warm} B",
            word_row_bytes(&sous)
        );

        let again = edited.replace("He entered", "He walked into");
        sous.update("b/mrk.usfm", Role::Target, &again).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 3, "cold again: the whole book");
        assert_eq!(sous.last_remapped(), 2);
    }

    /// What the hot set costs is the rows it keeps, and `resident_bytes`
    /// says so: the same corpus with the set switched off is smaller by
    /// exactly one book's word rows.
    #[test]
    fn resident_bytes_counts_the_rows_a_hot_book_keeps() {
        let mut cold = sous().with_hot_books(0);
        let mut hot = sous();
        for sous in [&mut cold, &mut hot] {
            sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
            sous.publish().unwrap();
        }
        let kept: usize = hot
            .observations
            .values()
            .map(|obs| hot.pass().observation_bytes(obs))
            .sum::<usize>()
            - cold
                .observations
                .values()
                .map(|obs| cold.pass().observation_bytes(obs))
                .sum::<usize>();
        assert!(kept > 0, "MRK's three chapters hold cased words");
        assert_eq!(
            hot.resident_bytes() - cold.resident_bytes(),
            kept,
            "the difference is the rows and nothing else"
        );
    }

    /// The same chapter 1 in two books: one cache entry, two published rows.
    #[test]
    fn two_identical_chapters_share_one_observation_and_report_both() {
        let text = book("GEN", &["The same \\\\ words."]);
        let twin = text
            .replace("\\id GEN", "\\id MRK")
            .replace("\\h GEN", "\\h MRK");
        let mut sous = sous();
        sous.update("a/gen.usfm", Role::Target, &text).unwrap();
        sous.update("b/mrk.usfm", Role::Target, &twin).unwrap();
        let buffer = sous.publish().unwrap();

        assert_eq!(sous.last_mapped(), 1, "the second chapter hit");
        assert_eq!(sous.observations.len(), 1);
        let published = rows(&buffer);
        assert_eq!(published.len(), 2, "counted at both positions");
        assert_eq!(published[0].0, 0);
        assert_eq!(published[1].0, 1);
        assert_eq!(
            (published[0].2, published[0].3),
            (published[1].2, published[1].3),
            "identical books rebase identically"
        );
    }

    #[test]
    fn a_removed_book_and_its_id_leave_the_next_snapshot() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let before = sous.publish().unwrap();
        assert_eq!(ids(&before), vec!["a/gen.usfm", "b/mrk.usfm"]);

        assert!(sous.remove(&BookId::from("a/gen.usfm")));
        let after = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0, "the survivor was already mapped");
        assert_eq!(ids(&after), vec!["b/mrk.usfm"]);
        assert_ne!(
            CorpusSnapshot::open(&before).unwrap().snapshot_id(),
            CorpusSnapshot::open(&after).unwrap().snapshot_id(),
            "a different corpus is a different snapshot"
        );
    }

    /// A target's findings are placed by rescanning its own text, so the
    /// registry refuses one that would keep none.
    #[test]
    fn a_target_cannot_be_products_only() {
        let mut sous = sous();
        let id = BookId::from("b/mrk.usfm");
        assert_eq!(
            sous.update_with(id.clone(), Role::Target, Retain::ProductsOnly, &mark())
                .err(),
            Some(PublishError::Pantry(PantryError::TargetNeedsText {
                id: id.clone()
            }))
        );
        assert!(sous.pantry().books(Role::Target).is_empty());
    }

    /// A book whose text and firing set both stand replays its rows.
    #[test]
    fn an_unchanged_book_replays_its_sites() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let first = sous.publish().unwrap();
        assert_eq!(sous.last_located(), 2, "both books were rescanned once");
        assert!(!site_rows(&first).is_empty(), "the corpus sites something");

        let second = sous.publish().unwrap();
        assert_eq!(sous.last_located(), 0, "no text was read again");
        assert_eq!(second, first, "a replay publishes the same bytes");
    }

    /// A glyph added to one book moves the corpus denominators, so the other
    /// book's own firing set may stand while its neighbour's moves.
    #[test]
    fn a_denominator_flip_relocates_only_books_whose_firing_set_moved() {
        let mut sous = sous();
        // The dagger is GEN's alone, so MRK's firing set cannot move when
        // GEN's counts of it do.
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update(
            "a/gen.usfm",
            Role::Target,
            &book("GEN", &["In the \\\\ beginning\u{2020}."]),
        )
        .unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.last_located(), 2);

        sous.update(
            "a/gen.usfm",
            Role::Target,
            &book(
                "GEN",
                &["In the \\\\ beginning\u{2020}\u{2020}\u{2020}\u{2020}\u{2020}."],
            ),
        )
        .unwrap();
        sous.publish().unwrap();
        assert_eq!(
            sous.last_located(),
            1,
            "only the edited book; MRK's own firing set never moved"
        );
    }

    /// Judging config is in no chapter or book key, so a re-judge places its
    /// new patterns without mapping or folding anything.
    #[test]
    fn set_config_relocates_from_cached_aggregates_without_mapping() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        let before = sous.publish().unwrap();

        let config = JudgingConfig {
            rarity_floor: 1,
            ..JudgingConfig::default()
        };
        sous.set_config(((), config, config));
        let after = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0, "no chapter was mapped");
        assert_eq!(sous.last_folded(), 0, "no book was folded");
        assert_eq!(
            sous.last_located(),
            1,
            "the firing set moved, so it rescanned"
        );
        assert_ne!(site_rows(&after), site_rows(&before));
    }

    /// One book edited past its ring: the current table plus `kept` behind it,
    /// and no observation only a dropped table named.
    #[test]
    fn edits_past_the_ring_leave_one_table_per_generation_and_no_orphans() {
        let mut sous = sous().with_generations(1);
        for word in ["one", "two", "three", "four"] {
            let text = mark().replace("A withered", word);
            sous.update("b/mrk.usfm", Role::Target, &text).unwrap();
            sous.publish().unwrap();
        }

        assert_eq!(sous.resident_tables(), 2, "the current text and one before");
        assert_eq!(
            sous.resident_observations(),
            4,
            "chapters 1 and 2 are shared; chapter 3 survives twice"
        );
    }

    /// An undo restores byte-identical text, so it restores the checksum: two
    /// edits back is a table hit while the ring is that deep.
    ///
    /// A chapter-grain pass, deliberately: `Brigade` carries `Words`, whose
    /// aggregate is pruned to the current checksum only (see the aggregate
    /// tests below), so an in-ring undo of a book-grain pass re-maps anyway.
    #[test]
    fn an_undo_inside_the_ring_republishes_without_mapping() {
        let mut sous = grain().with_generations(2);
        let versions: Vec<String> = ["one", "two", "three"]
            .iter()
            .map(|word| mark().replace("A withered", word))
            .collect();
        let mut published = Vec::new();
        for text in &versions {
            sous.update("b/mrk.usfm", Role::Target, text).unwrap();
            published.push(sous.publish().unwrap());
        }

        sous.update("b/mrk.usfm", Role::Target, &versions[0])
            .unwrap();
        assert_eq!(sous.publish().unwrap(), published[0], "byte for byte");
        assert_eq!(sous.last_mapped(), 0, "the table was still resident");
    }

    #[test]
    fn an_undo_beyond_the_ring_remaps_only_its_chapter() {
        let mut sous = grain().with_generations(1);
        let versions: Vec<String> = ["one", "two", "three"]
            .iter()
            .map(|word| mark().replace("A withered", word))
            .collect();
        let mut published = Vec::new();
        for text in &versions {
            sous.update("b/mrk.usfm", Role::Target, text).unwrap();
            published.push(sous.publish().unwrap());
        }

        sous.update("b/mrk.usfm", Role::Target, &versions[0])
            .unwrap();
        assert_eq!(sous.publish().unwrap(), published[0]);
        assert_eq!(sous.last_mapped(), 1, "chapters 1 and 2 still hit");
    }

    /// A book-grain pass keeps its aggregate for the CURRENT checksum only:
    /// older generations are dropped at sweep even while the ring and the
    /// chapter tables behind them survive, so an in-ring undo still re-maps
    /// and re-folds the book it lands on — and publishes the identical bytes.
    #[test]
    fn a_book_grain_pass_keeps_one_aggregate_per_book_through_edits_and_an_undo() {
        let mut sous = sous().with_generations(4).with_hot_books(0);
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let versions: Vec<String> = ["one", "two", "three", "four"]
            .iter()
            .map(|word| mark().replace("A withered", word))
            .collect();
        let mut published = Vec::new();
        for text in &versions {
            sous.update("b/mrk.usfm", Role::Target, text).unwrap();
            published.push(sous.publish().unwrap());
        }
        assert_eq!(
            sous.resident_aggregates(),
            2,
            "one per book, not one per retained generation"
        );

        sous.update("b/mrk.usfm", Role::Target, &versions[0])
            .unwrap();
        let undone = sous.publish().unwrap();
        assert_eq!(undone, published[0], "byte for byte");
        assert!(
            sous.last_mapped() > 0,
            "the pruned aggregate forced a re-map"
        );
        assert_eq!(
            sous.resident_aggregates(),
            2,
            "still one per book after the undo"
        );
    }

    #[test]
    fn removing_a_book_drops_its_ring_and_every_table_in_it() {
        let mut sous = sous().with_generations(4);
        for word in ["one", "two"] {
            let text = mark().replace("A withered", word);
            sous.update("b/mrk.usfm", Role::Target, &text).unwrap();
            sous.publish().unwrap();
        }
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.resident_tables(), 3, "two MRK generations and GEN");

        assert!(sous.remove(&BookId::from("b/mrk.usfm")));
        sous.publish().unwrap();
        assert_eq!(sous.resident_tables(), 1);
        assert_eq!(sous.resident_observations(), 1, "GEN's one chapter");
    }

    #[test]
    fn one_id_line_under_two_ids_publishes_both_in_id_order() {
        let text = genesis();
        let mut sous = sous();
        sous.update("a/gen.usfm", Role::Target, &text).unwrap();
        sous.update("a/gen-copy.usfm", Role::Target, &text).unwrap();
        let buffer = sous.publish().unwrap();

        assert_eq!(sous.last_mapped(), 1, "one text, one chapter table");
        assert_eq!(ids(&buffer), vec!["a/gen-copy.usfm", "a/gen.usfm"]);
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot.book_by_id("a/gen.usfm").unwrap().index().get(),
            1,
            "the second row is reachable only by id"
        );
    }

    /// The other half of the parallel gate: `galley/tests/equivalence.rs`
    /// holds the whole-Bible run against the cold oracle.
    #[cfg(feature = "parallel")]
    #[test]
    fn the_parallel_map_publishes_the_serial_bytes() {
        // The twin is the same chapters under a second id, so the cross-book
        // dedup the parallel queue does for itself is exercised too.
        let books = [
            ("a/gen.usfm", genesis()),
            ("b/mrk.usfm", mark()),
            ("c/mrk-copy.usfm", mark()),
        ];
        let mut serial = sous().serial();
        let mut parallel = sous();
        for (id, text) in &books {
            serial.update(*id, Role::Target, text).unwrap();
            parallel.update(*id, Role::Target, text).unwrap();
        }
        assert_eq!(serial.publish().unwrap(), parallel.publish().unwrap());
        assert_eq!(serial.last_mapped(), 4, "GEN's chapter and MRK's three");
        assert_eq!(parallel.last_mapped(), serial.last_mapped());
        assert_eq!(
            parallel.resident_observations(),
            serial.resident_observations()
        );
    }

    #[test]
    fn publish_unchanged_folds_nothing_and_rejudges_to_the_same_bytes() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let first = sous.publish().unwrap();
        assert_eq!(sous.last_folded(), 2, "both books cold");

        let second = sous.publish().unwrap();
        assert_eq!(sous.last_folded(), 0, "no book's projection moved");
        assert_eq!(first, second, "the cached aggregates judge the same");
    }

    #[test]
    fn only_an_edited_book_is_folded_again() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        sous.publish().unwrap();

        let after = mark().replace("A withered", "A shrivelled");
        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.last_folded(), 1, "GEN judged its cached aggregate");
    }

    /// A config change is a re-judge and never a re-fold or a re-map.
    #[test]
    fn setting_the_config_folds_nothing_and_maps_nothing() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let first = sous.publish().unwrap();

        sous.set_config(<Brigade as ChapterPass>::Config::default());
        let second = sous.publish().unwrap();
        assert_eq!(sous.last_folded(), 0);
        assert_eq!(sous.last_mapped(), 0);
        assert_eq!(first, second, "the same config judges the same bytes");
    }

    /// The same aggregates, a different config, a different pattern table:
    /// judging is the only step a config reaches.
    #[test]
    fn changing_the_config_re_judges_from_cached_aggregates() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let before = CorpusSnapshot::open(&sous.publish().unwrap())
            .unwrap()
            .pattern_count();

        let config = JudgingConfig {
            rarity_floor: 10_000,
            ..JudgingConfig::default()
        };
        sous.set_config(((), config, config));
        let buffer = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0);
        assert_eq!(sous.last_folded(), 0);
        assert_ne!(
            CorpusSnapshot::open(&buffer).unwrap().pattern_count(),
            before
        );
    }

    // ---------------------------------------------------- the source pairing

    /// A book of `count` one-verse chapters is too slow to build; this is one
    /// chapter of `count` verses, verse `i` as long as `length(i)` says.
    fn sized(code: &str, count: usize, length: impl Fn(usize) -> usize) -> String {
        let mut text = format!("\\id {code}\n\\h {code}\n\\c 1\n\\p\n");
        for verse in 0..count {
            text.push_str(&format!(
                "\\v {} {}\n",
                verse + 1,
                "a".repeat(length(verse))
            ));
        }
        text
    }

    /// Sixty verses whose lengths cycle 40..=46, which is the varied sample a
    /// median and a MAD need; verse 61 is half length and verse 62 double.
    fn paired_target() -> String {
        sized("MRK", 62, |verse| match verse {
            60 => 20,
            61 => 80,
            other => 40 + other % 7,
        })
    }

    /// The same keys at a constant 40 characters.
    fn paired_source() -> String {
        sized("MRK", 62, |_| 40)
    }

    /// Every length row of a published buffer, as (book index, from, to).
    fn length_rows(buffer: &[u8]) -> Vec<(u16, u32, u32)> {
        let snapshot = CorpusSnapshot::open(buffer).unwrap();
        (0..snapshot.len())
            .flat_map(|index| {
                let book = snapshot
                    .book(BookIndex::new(index).unwrap())
                    .expect("directory position");
                (0..book.len()).filter_map(move |row| {
                    let finding = book.at(row).unwrap();
                    matches!(finding.kind(), FindingKind::LengthProportionality(_))
                        .then(|| (finding.book_idx().get(), finding.from(), finding.to()))
                })
            })
            .collect()
    }

    /// A reference is a whole optional corpus: with none registered the target
    /// publishes exactly what it published alone, and with one it gains the
    /// two verses whose length the source disagrees with.
    #[test]
    fn a_reference_of_the_same_key_adds_length_rows_and_nothing_else() {
        let mut sous = sous();
        sous.update("t/mrk.usfm", Role::Target, &paired_target())
            .unwrap();
        let alone = sous.publish().unwrap();
        assert!(length_rows(&alone).is_empty(), "no source, no ratios");

        sous.update("s/mrk.usfm", Role::Reference, &paired_source())
            .unwrap();
        let paired = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0, "a reference maps no target chapter");
        assert_eq!(sous.last_folded(), 0, "and folds no target book");
        assert_eq!(length_rows(&paired).len(), 2, "the short one and the long");
        assert_eq!(
            ids(&paired),
            vec!["t/mrk.usfm"],
            "a reference publishes no section of its own"
        );
        assert_eq!(
            rows(&paired),
            rows(&alone),
            "every target-only row survives the source"
        );
        assert_eq!(site_rows(&paired), site_rows(&alone));
    }

    /// The gate the charter names: the source choice legitimately changes the
    /// results without invalidating the target-only observations.
    #[test]
    fn replacing_the_reference_moves_only_the_length_rows() {
        let mut sous = sous();
        sous.update("t/mrk.usfm", Role::Target, &paired_target())
            .unwrap();
        sous.update("s/mrk.usfm", Role::Reference, &paired_source())
            .unwrap();
        let before = sous.publish().unwrap();
        assert_eq!(length_rows(&before).len(), 2);

        // A source that matches the target verse for verse: every ratio is
        // one, and the sample is degenerate, so nothing is an outlier.
        sous.update("s/mrk.usfm", Role::Reference, &paired_target())
            .unwrap();
        let after = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0, "no target chapter was re-mapped");
        assert_eq!(sous.last_folded(), 0, "no target book was re-folded");
        assert_eq!(sous.last_located(), 0, "no target text was read again");
        assert!(length_rows(&after).is_empty(), "against itself, nothing");
        assert_eq!(rows(&after), rows(&before), "target-only rows stand");
        assert_eq!(site_rows(&after), site_rows(&before));
        assert_ne!(
            CorpusSnapshot::open(&before).unwrap().snapshot_id(),
            CorpusSnapshot::open(&after).unwrap().snapshot_id(),
            "a different source is a different publication"
        );
    }

    /// Removing the reference removes the ratios and leaves the rest.
    #[test]
    fn removing_the_reference_removes_the_length_rows() {
        let mut sous = sous();
        sous.update("t/mrk.usfm", Role::Target, &paired_target())
            .unwrap();
        let alone = sous.publish().unwrap();
        sous.update("s/mrk.usfm", Role::Reference, &paired_source())
            .unwrap();
        assert_eq!(length_rows(&sous.publish().unwrap()).len(), 2);

        assert!(sous.remove(&BookId::from("s/mrk.usfm")));
        let after = sous.publish().unwrap();
        assert!(length_rows(&after).is_empty());
        assert_eq!(rows(&after), rows(&alone));
    }

    /// The length knobs live on the same judging config as every other knob,
    /// so moving them maps nothing and folds nothing.
    #[test]
    fn a_length_knob_re_judges_without_mapping_or_folding() {
        let mut sous = sous();
        sous.update("t/mrk.usfm", Role::Target, &paired_target())
            .unwrap();
        sous.update("s/mrk.usfm", Role::Reference, &paired_source())
            .unwrap();
        let before = sous.publish().unwrap();
        assert_eq!(length_rows(&before).len(), 2);

        let config = JudgingConfig {
            lengths: sous_core::LengthConfig {
                z_long: 1_000.0,
                ..sous_core::LengthConfig::default()
            },
            ..JudgingConfig::default()
        };
        sous.set_config(((), config, config));
        let after = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0);
        assert_eq!(sous.last_folded(), 0);
        assert_eq!(
            length_rows(&after).len(),
            1,
            "the long side is out of reach"
        );

        let off = JudgingConfig {
            lengths: sous_core::LengthConfig {
                enabled: false,
                ..sous_core::LengthConfig::default()
            },
            ..JudgingConfig::default()
        };
        sous.set_config(((), off, off));
        assert!(length_rows(&sous.publish().unwrap()).is_empty());
    }

    /// A reference of another key pairs with nothing, and says nothing.
    #[test]
    fn a_reference_of_another_book_is_silence_not_an_error() {
        let mut sous = sous();
        sous.update("t/mrk.usfm", Role::Target, &paired_target())
            .unwrap();
        sous.update("s/gen.usfm", Role::Reference, &sized("GEN", 62, |_| 40))
            .unwrap();
        let buffer = sous.publish().unwrap();
        assert!(length_rows(&buffer).is_empty());
        assert_eq!(ids(&buffer), vec!["t/mrk.usfm"]);
    }

    #[test]
    fn published_rows_carry_the_pass_findings_in_raw_utf16() {
        let mut sous = sous();
        // The onion is 2 UTF-16 units for 4 raw bytes, so the pair after it
        // publishes two units short of its raw offset.
        sous.update(
            "b/mrk.usfm",
            Role::Target,
            "\\id MRK\n\\c 1\n\\p\n\\v 1 An 🧅 \\\\ here.\n",
        )
        .unwrap();
        let buffer = sous.publish().unwrap();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        let book = snapshot.book_by_id("b/mrk.usfm").unwrap();
        let finding = (0..book.len())
            .map(|row| book.at(row).unwrap())
            .find(|row| matches!(row.kind(), FindingKind::Hygiene(_)))
            .expect("the pair is a hygiene row");

        let FindingKind::Hygiene(digest) = finding.kind() else {
            unreachable!("just filtered")
        };
        assert_eq!(digest.run(), 2);
        assert_eq!((finding.from(), finding.to()), (27, 29));
    }
}
