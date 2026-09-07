//! Nothing is reachable that no live generation names, and every resident
//! byte is counted where it is held.
//!
//! ```text
//! sous.resident_bytes()   -> the Pantry's products plus this cache's rows
//! sous.publish()          -> sweep: tables outside a ring, observations no
//!                            surviving table names
//! ```
//!
//! The ring is the only root: a book keeps its current checksum ahead of
//! [`DEFAULT_GENERATIONS`] previous ones, and everything keyed by a checksum
//! no ring holds is dropped. The hot set is the one exception a fold reads,
//! and it is bounded by [`DEFAULT_HOT_BOOKS`].

use rustc_hash::FxHashSet;
use sous_core::{ChapterPass, PairedBook};

use super::Expediter;
use super::keys::{ChapterRow, FiringHash, ObservationKey, TableHash, TerminalHash};
use crate::pantry::{BookId, Budget, RawChecksum, Tally, Tier};

/// Previous checksums a book keeps beside its current one, so an undo of that
/// many edits still lands on a retained chapter table.
pub(super) const DEFAULT_GENERATIONS: usize = 4;

/// Books whose released member's chapter rows are kept anyway, most recently
/// edited first: a keystroke lands in the book the last one landed in, and
/// that book then re-maps one chapter instead of all of them.
///
/// Two, because the price is that member's rows for a whole book — about
/// 100-150 KB of `Words` per Bible book (evidence.md, W1 grain).
pub(super) const DEFAULT_HOT_BOOKS: usize = 2;

impl<P: ChapterPass + Sync> Expediter<P> {
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
    pub(super) fn cool_beyond_ceiling(&mut self) {
        while self.hot.len() > self.hot_ceiling {
            self.cooling
                .push(self.hot.pop().expect("longer than the ceiling"));
        }
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

    /// The declared ceiling the rebuildable tier's chunk products are held
    /// under; the other tiers are counted against it, not evicted.
    pub fn budget(&self) -> Budget {
        self.pantry.budget()
    }

    /// The Pantry's retained products plus this cache's own rows.
    ///
    /// Real on both sides: [`ChapterPass::aggregate_bytes`] and
    /// [`ChapterPass::observation_bytes`] sum the heap a pass hangs off each,
    /// so a hot book's unshed chapter rows are counted where they are held.
    pub fn resident_bytes(&self) -> usize {
        self.tally().total()
    }

    /// The same bytes, attributed to their tier — with no residual, so
    /// `tally().total() == resident_bytes()` always.
    ///
    /// A book's text and products and the rings that name them are pinned;
    /// the hot set's chapter rows are the hot tier; every content-addressed
    /// derived value is rebuildable, because losing one is a miss and never a
    /// wrong answer. Only the rebuildable tier's chunk products are enforced
    /// against the [`Budget`] in this slice.
    pub fn tally(&self) -> Tally {
        let rings: usize = self
            .generations
            .values()
            .map(|ring| size_of::<BookId>() + ring.len() * size_of::<RawChecksum>())
            .sum();
        let derived = self
            .observations
            .resident_bytes(|obs| self.pass.observation_bytes(obs))
            + self
                .chapter_tables
                .resident_bytes(|table| table.len() * size_of::<ChapterRow>())
            + self
                .aggregates
                .resident_bytes(|aggregate| self.pass.aggregate_bytes(aggregate))
            + self.sites.resident_bytes(|(_, _, rows)| {
                size_of::<FiringHash>() + size_of::<TerminalHash>() + size_of_val(&**rows)
            })
            + self
                .firing
                .resident_bytes(|_| size_of::<TableHash>() + size_of::<FiringHash>())
            + self.paired.resident_bytes(PairedBook::resident_bytes)
            + self.totals.resident_bytes()
            + self.verdicts.resident_bytes();

        let mut tally = self.pantry.tally();
        tally.add(
            Tier::Pinned,
            rings + self.tallied.len() * (size_of::<BookId>() + size_of::<RawChecksum>()),
        );
        tally.add(
            Tier::Hot,
            self.chapter_sites
                .resident_bytes(|rows| size_of_val(&**rows)),
        );
        tally.add(Tier::Rebuildable, derived);
        tally
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
    pub(super) fn sweep(&mut self) {
        if !core::mem::take(&mut self.dirty) {
            return;
        }
        let live: FxHashSet<RawChecksum> = self.generations.values().flatten().copied().collect();
        self.chapter_tables
            .keep_live(|checksum| live.contains(checksum));
        if P::RETAIN_CHAPTERS {
            self.aggregates
                .keep_live(|checksum| live.contains(checksum));
        } else {
            let current: FxHashSet<RawChecksum> = self
                .generations
                .values()
                .filter_map(|ring| ring.first().copied())
                .collect();
            self.aggregates
                .keep_live(|checksum| current.contains(checksum));
        }
        self.sites.keep_live(|checksum| live.contains(checksum));
        self.firing.keep_live(|checksum| live.contains(checksum));
        let named: FxHashSet<ObservationKey> = self
            .chapter_tables
            .values()
            .flatten()
            .map(|row| row.observation)
            .collect();
        self.observations.keep_live(|key| named.contains(key));
    }
}
