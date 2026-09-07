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
use sous_core::{
    BookIndex, Chapter, ChapterObs, ChapterPass, CoordinateSpace, CorpusTotals, CorpusWireError,
    Findings, MovedWords, PackedFinding, PairedBook, Pattern, PatternIndex, ProjectSpread,
    ProjectedBook, PublicationBook, SnapshotId, Verse, WordVerdicts, encode_to_corpus_buffer,
    for_each_chapter,
};
#[cfg(feature = "parallel")]
use sous_core::{ChapterInput, ChapterKey};
use xxhash_rust::xxh3::Xxh3Default;

use mise::books::BookKey;

use crate::onion::{self, lint};

use super::{OnionBook, PublishError, rebase_span};
use crate::pantry::derived::Store;
use crate::pantry::{BookId, Pantry, RawChecksum, Retain, Role, SourceLanes};

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
    observations: Store<ObservationKey, P::Observation>,
    /// Keyed by the book's raw checksum, so an unchanged book publishes
    /// without touching its text.
    chapter_tables: Store<RawChecksum, Vec<ChapterRow>>,
    /// One book's fold product, in book coordinates and free of a book index,
    /// so a later publication judges it under whatever index it has then.
    aggregates: Store<RawChecksum, P::Aggregate>,
    /// One book's located rows for the last firing set and terminal table
    /// seen, keyed by the same checksum: a replay needs all three, because the
    /// word walk reads the terminal table to PLACE its rows and a table that
    /// moved elsewhere in the corpus decides this book's occurrences
    /// differently while its own text and firing set stand still.
    sites: Store<RawChecksum, (FiringHash, TerminalHash, Box<[SiteRow]>)>,
    /// One book's firing hash for the pattern table it was walked against:
    /// a table whose rows say the same thing fires the same set, whatever
    /// this publication's counts and numbering are.
    firing: Store<RawChecksum, (TableHash, FiringHash)>,
    /// One HOT book chapter's own rows, in chapter-relative coordinates and
    /// keyed by content: a keystroke walks the chapter it landed in and
    /// replays its neighbours rebased. Held for the hot set alone, and every
    /// publication keeps exactly the keys that set names.
    chapter_sites: Store<ChapterSiteKey, Box<[SiteRow]>>,
    /// One target book's ratios against its declared source, keyed by BOTH
    /// checksums: the pairing is a pure function of the two books' rows, so a
    /// publication re-pairs only the books whose side moved.
    paired: Store<PairKey, PairedBook>,
    /// The pooled statistics beside the keys that produced them. The same keys
    /// are the same sample, so a publication that re-paired nothing skips the
    /// project-scope order statistics too.
    project: Option<(Vec<PairKey>, ProjectSpread)>,
    /// References the last publication would have walked and could not,
    /// because they were registered while the lane was off.
    wordless: u64,
    /// The corpus counts judging reads, kept across publications: a book whose
    /// checksum moved is subtracted at its old one and added at its new, and
    /// the rest are never touched.
    totals: CorpusTotals,
    /// Which checksum each book contributes to [`Self::totals`] right now.
    tallied: FxHashMap<BookId, RawChecksum>,
    /// The word channels' last verdicts, so a publication that moved one
    /// book's words re-judges those keys and keeps the rest.
    verdicts: WordVerdicts,
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
    /// Books whose firing set was walked by the last publication.
    walked: u64,
    sited: u64,
    pairings: u64,
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

mod keys;
mod pairing;
mod residency;
mod siting;

#[cfg(test)]
mod tests;

pub use keys::ObservationKey;

use keys::{ChapterRow, ChapterSiteKey, FiringHash, PairKey, PatternRef, TableHash, TerminalHash};
use pairing::pair_and_judge;
use residency::{DEFAULT_GENERATIONS, DEFAULT_HOT_BOOKS};
use siting::{SiteRow, projection, replay, site_by_chapter};

/// `Sync` unconditionally, so the `parallel` feature adds no bound the serial
/// build does not already carry: a pass is a stateless rule, not a session.
impl<P: ChapterPass + Sync> Expediter<P> {
    /// `budget_bytes` is the Pantry's ceiling on rebuildable products.
    pub fn new(pass: P, budget_bytes: usize) -> Self {
        Self {
            pantry: Pantry::new(budget_bytes),
            pass,
            config: P::Config::default(),
            observations: Store::default(),
            chapter_tables: Store::default(),
            aggregates: Store::default(),
            sites: Store::default(),
            firing: Store::default(),
            chapter_sites: Store::default(),
            paired: Store::default(),
            project: None,
            wordless: 0,
            totals: CorpusTotals::default(),
            tallied: FxHashMap::default(),
            verdicts: WordVerdicts::default(),
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
            walked: 0,
            sited: 0,
            pairings: 0,
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

    /// Derive and retain this book's products under the role's own retention:
    /// a target keeps its text, a reference keeps none. A target's chapters
    /// are keyed at the next [`publish`](Self::publish).
    pub fn update(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        text: &str,
    ) -> Result<BookKey, PublishError> {
        let retain = match role {
            Role::Target => Retain::Text,
            Role::Reference => Retain::ProductsOnly,
        };
        self.update_with(id, role, retain, text)
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
        let id = id.into();
        // A book that stops being a Target stops being cached as one: its ring
        // and its hot slot go here, and the publication that no longer lists it
        // untallies the aggregate the ring still named — the path `remove`
        // already takes.
        if role != Role::Target && self.pantry.role(&id) == Some(Role::Target) {
            self.forget_target(&id);
        }
        Ok(self
            .pantry
            .update_with(id, role, retain, self.source_lanes(), text)
            .map_err(PublishError::Pantry)?
            .key())
    }

    /// Which lanes a Reference registered RIGHT NOW keeps: the word lane only
    /// while the current config would judge with it.
    ///
    /// A host that turns `source_copy` on after its references are loaded has
    /// to re-send their text; `last_wordless_references` is how a publication
    /// says so out loud instead of publishing nothing.
    fn source_lanes(&self) -> SourceLanes {
        match self
            .pass
            .length_config(&self.config)
            .is_some_and(|lengths| lengths.source_copy)
        {
            true => SourceLanes::LengthsAndWords,
            false => SourceLanes::Lengths,
        }
    }

    /// Drop a book's products, its text, and its retained generations; the
    /// tables and observations go at the next [`publish`](Self::publish).
    ///
    /// `false` when the id was not registered.
    pub fn remove(&mut self, id: &BookId) -> bool {
        self.forget_target(id);
        self.pantry.remove(id)
    }

    /// Drops this id's ring and its claim on a hot slot.
    ///
    /// The tables, observations and rows keyed by its checksums go with the
    /// ring at the next sweep, and its aggregate is still resident when the
    /// same publication untallies it — so a book leaving the Target set owes
    /// nothing back and leaves no slot warm.
    fn forget_target(&mut self, id: &BookId) {
        self.dirty |= self.generations.remove(id).is_some();
        self.hot.retain(|seen| seen != id);
        self.cooling.retain(|seen| seen != id);
    }

    /// The registry, read-only: books are registered through
    /// [`update`](Self::update) so their chapters cannot go unkeyed.
    pub fn pantry(&self) -> &Pantry {
        &self.pantry
    }

    /// The Pantry's whole-book lint over loose text the host holds. One of
    /// the four free-text doors the coordinator forwards: a derivation over
    /// text that is not a registered book takes `&mut` and touches no book,
    /// so the registry stays read-only from outside.
    pub fn lint(&mut self, text: &str) -> lint::LintReport {
        self.pantry.lint(text)
    }

    /// The book, plated, off the same chunk cache. See [`lint`](Self::lint).
    pub fn parse(&mut self, text: &str, opts: onion::wire::ParseOptions) -> Vec<u8> {
        self.pantry.parse(text, opts)
    }

    /// The same ingredients as engine types. See [`lint`](Self::lint).
    pub fn parsed<'a>(
        &mut self,
        text: &'a str,
        opts: onion::wire::ParseOptions,
    ) -> onion::wire::Parsed<'a> {
        self.pantry.parsed(text, opts)
    }

    /// One recipe's mask over loose text. See [`lint`](Self::lint).
    pub fn masked(&mut self, text: &str, filter: &onion::mask::Filter) -> onion::mask::Mask {
        self.pantry.masked(text, filter)
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

    /// Chapters of [`last_located`](Self::last_located)'s books whose word
    /// rows were walked; the rest replayed the rows their chapter cache held.
    ///
    /// Zero for a book outside the hot set, which is sited whole or not at
    /// all. A keystroke into a hot book answers one.
    pub fn last_sited_chapters(&self) -> u64 {
        self.sited
    }

    /// Declared sources the last [`publish`](Self::publish) would have walked
    /// for source-copy runs and could not, because they were registered while
    /// `LengthConfig::source_copy` was off and so kept no word lane.
    ///
    /// Nonzero means those books published no code-3 row for a reason that is
    /// not "no run was found": the host re-sends their text to fix it.
    pub fn last_wordless_references(&self) -> u64 {
        self.wordless
    }

    /// Target books re-paired against their declared source by the last
    /// [`publish`](Self::publish); the rest judged the ratios their cache
    /// already held.
    pub fn last_paired(&self) -> u64 {
        self.pairings
    }

    /// Books whose firing set the last [`publish`](Self::publish) walked; the
    /// rest replayed the hash they were walked under.
    ///
    /// All of them whenever the pattern table's rows say something new, none
    /// when a keystroke moved only counts, one when a book's own text moved.
    pub fn last_firing_walks(&self) -> u64 {
        self.walked
    }

    /// Corpus word-tally keys the last [`publish`](Self::publish) judged: the
    /// whole tally when it judged everything, the delta's keys when it kept
    /// the previous publication's verdicts and merged.
    ///
    /// A keystroke answers one book's keys; a moved terminal table, a moved
    /// config, or a corpus-wide doubling recusal that crossed the bar answers
    /// the whole tally.
    pub fn last_words_judged(&self) -> usize {
        self.verdicts.last_judged()
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
                // A book that came back owes nothing back, and a cooling list
                // that keeps naming it grows across publications.
                cooling.retain(|seen| !hot.contains(seen));
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
        let (mut folds, mut located, mut sited) = (0, 0, 0);
        let (pairings, wordless);
        let mut walked = 0u64;
        let (projected, patterns) = {
            let Self {
                pantry,
                pass,
                config,
                observations,
                chapter_tables,
                aggregates,
                sites,
                firing: firing_sets,
                chapter_sites,
                paired,
                project,
                totals,
                tallied,
                verdicts,
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
            // The keys those two moves touch, named before either runs: a
            // judge re-decides exactly these and keeps every other verdict.
            let mut moved = MovedWords::default();
            if !stale.is_empty() {
                pass.moved_keys(&stale, &mut moved);
                pass.untally(totals, &stale);
            }
            if !added.is_empty() {
                pass.moved_keys(&added, &mut moved);
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

            // One projection per book per publication at most, shared by the
            // two steps that read text: the source-copy walk fills a slot on a
            // pair miss and the site rescan takes it. Empty unless the
            // source-copy lane is on, which is what keeps a cold publication
            // from holding a whole corpus of projected text at once.
            let mut views: Vec<Option<OnionBook>> = (0..books.len()).map(|_| None).collect();
            let mut findings = Findings::new(projected_lens);
            {
                // Judging is corpus-level: every Target book, changed or not.
                let corpus: Vec<&P::Aggregate> = checksums
                    .iter()
                    .map(|checksum| &aggregates[checksum])
                    .collect();
                pass.judge_kept(&corpus, totals, config, &moved, verdicts, &mut findings);

                (pairings, wordless) = pair_and_judge(
                    pass,
                    config,
                    pantry,
                    &books,
                    &references,
                    &checksums,
                    &corpus,
                    paired,
                    project,
                    &mut views,
                    &mut findings,
                )?;
            }

            // Then place what judging decided, per book, from the current text.
            let table: Vec<Pattern> = findings.patterns().to_vec();
            let resolver: FxHashMap<PatternRef, PatternIndex> = table
                .iter()
                .enumerate()
                .map(|(at, pattern)| (PatternRef::of(pattern), PatternIndex::at(at)))
                .collect();
            // A duplicate `(glyph, channel, key)` would collapse two rows to
            // one here and replay every cached row for the first under the
            // second's index, silently.
            debug_assert_eq!(
                resolver.len(),
                table.len(),
                "the judge names each pattern once"
            );
            let terminals = TerminalHash::of(findings.terminals());
            let identity = TableHash::of(&table);
            let hot_ids: FxHashSet<&BookId> = hot.iter().collect();
            // Every key this publication's hot books name, hit or miss: what
            // the chapter cache keeps once the loop is done, so a book that
            // left the hot set takes its chapters with it.
            let mut named: FxHashSet<ChapterSiteKey> = FxHashSet::default();
            let mut firing: Vec<PatternIndex> = Vec::new();
            for (index, (id, _)) in books.iter().enumerate() {
                let book = BookIndex::new(index).expect("a corpus indexes every book");
                let checksum = checksums[index];
                let aggregate = &aggregates[&checksum];
                let hash = match firing_sets.get(&checksum) {
                    Some((seen, hash)) if *seen == identity => *hash,
                    _ => {
                        pass.firing(aggregate, &table, &mut firing);
                        let hash = FiringHash::of(&firing, &table);
                        firing_sets.insert(checksum, (identity, hash));
                        walked += 1;
                        hash
                    }
                };
                let keys: Vec<ChapterSiteKey> = match P::CHAPTER_SITES && hot_ids.contains(id) {
                    true => chapter_tables[&checksum]
                        .iter()
                        .map(|row| ChapterSiteKey {
                            chapter: row.observation,
                            firing: hash,
                            terminals,
                        })
                        .collect(),
                    false => Vec::new(),
                };
                named.extend(keys.iter().copied());
                if let Some((seen, table, rows)) = sites.get(&checksum)
                    && *seen == hash
                    && *table == terminals
                {
                    replay(book, rows, 0, &resolver, &mut findings);
                    // The pair step's projection, if it made one: this book is
                    // done, so nothing is held to the end of the publication.
                    views[index] = None;
                    continue;
                }
                // The pair step's projection where it made one, and one of its
                // own otherwise; taken, so it is released as this book is
                // located rather than held to the end of the publication.
                let view = match views[index].take() {
                    Some(view) => view,
                    None => projection(pantry, id)?,
                };
                let rows: Vec<Chapter> = view.chapters().collect();
                let before = findings.len();
                let verses: Vec<Verse> = view.verses().collect();
                if keys.is_empty() {
                    pass.locate(book, view.text(), &rows, &verses, aggregate, &mut findings);
                } else {
                    sited += site_by_chapter(
                        pass,
                        book,
                        view.text(),
                        &rows,
                        &verses,
                        aggregate,
                        &keys,
                        chapter_sites,
                        &table,
                        &resolver,
                        &mut findings,
                    );
                }
                let cached: Box<[SiteRow]> = findings.rows()[before..]
                    .iter()
                    .map(|row| SiteRow::of(row, &table))
                    .collect();
                sites.insert(checksum, (hash, terminals, cached));
                located += 1;
            }
            chapter_sites.keep_live(|key| named.contains(key));
            findings.finish();
            findings.into_parts()
        };
        self.folds = folds;
        self.walked = walked;
        self.located = located;
        self.sited = sited;
        self.pairings = pairings;
        self.wordless = wordless;
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
        let snapshot = snapshot_id(&self.pass, &self.config, &self.pantry, &books, &references);
        encode_to_corpus_buffer(snapshot, CoordinateSpace::Utf16, &sections, &patterns)
            .map_err(PublishError::Wire)
    }
}

/// xxh3-128 over the canonical (`BookKey`, id, `RawChecksum`) table plus the
/// pass schema and its config stamp: what the publication is OF, not what it
/// says.
///
/// The config is in it because a `FindingHandle` is a snapshot plus a row:
/// two publications differing only by a knob name different rows, so they may
/// not share an identity.
fn snapshot_id<P: ChapterPass>(
    pass: &P,
    config: &P::Config,
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
    hasher.update(&pass.config_stamp(config).to_le_bytes());
    SnapshotId::new(hasher.digest128().to_be_bytes())
}
