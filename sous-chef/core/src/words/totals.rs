//! The corpus word tally: every book's rows merged by hash, and the two
//! updates a resident host makes instead of merging them again.
//!
//! ```text
//! merge([GEN, MRK])   hash(david) None [2, 40, 0, 0]  holders 2
//! remove([old MRK])   hash(david) None [2, 38, 0, 0]  holders 1
//! add([new MRK])      hash(david) None [2, 39, 0, 0]  holders 2
//! ```
//!
//! The tally after any sequence of updates is the merge of the books resident
//! then — counts and rows alike — which is what lets a host judge the word
//! channels from it instead of from 66 aggregates. A row survives while
//! `holders` is nonzero. The casing lane's key is `(hash, before)` and not the
//! hash alone, because forced and free are a judging decision the config may
//! move, so the tally cannot pre-sum them; [`WordTotals::by_word`] hands the
//! judge one word's rows at a time. The doubles lane keys by hash alone — a
//! double is a double whatever stood before it — and rides the same two
//! updates.

use super::{Before, DoubleTotal, WordAggregate};

/// One case-folded word's corpus totals under one [`Before`], by [`super::Form`]
/// lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WordTally {
    pub hash: u64,
    /// Occurrences per lane, saturating.
    pub counts: [u32; 4],
    /// [`Before`], packed.
    before: u32,
    /// Books holding this `(hash, before)`; the row lives while this does.
    pub holders: u32,
}

impl WordTally {
    fn of(hash: u64, before: u32) -> Self {
        Self {
            hash,
            before,
            ..Self::default()
        }
    }

    pub const fn before(&self) -> Before {
        match Before::from_raw(self.before) {
            Some(before) => before,
            None => Before::None,
        }
    }

    /// One book's chapter-folded lane counts.
    fn absorb_book(&mut self, counts: [u32; 4]) {
        self.holders += 1;
        for (lane, count) in counts.iter().enumerate() {
            self.counts[lane] = self.counts[lane].saturating_add(*count);
        }
    }
}

impl Lane for WordTally {
    type Key = (u64, u32);

    fn key(&self) -> (u64, u32) {
        (self.hash, self.before)
    }

    fn holders(&self) -> u32 {
        self.holders
    }

    fn absorb(&mut self, other: &Self) {
        self.holders += other.holders;
        for (lane, count) in other.counts.iter().enumerate() {
            self.counts[lane] = self.counts[lane].saturating_add(*count);
        }
    }

    fn release(&mut self, other: &Self) {
        self.holders = self.holders.saturating_sub(other.holders);
        for (lane, count) in other.counts.iter().enumerate() {
            self.counts[lane] = self.counts[lane].saturating_sub(*count);
        }
    }
}

/// One case-folded word's corpus doubling, keyed by hash alone.
///
/// `uncased` is the word's occurrences the casing lane refuses, so the two
/// lanes partition the word's occurrences and the doubled channel's
/// denominator is their sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DoubleTally {
    pub hash: u64,
    pub uncased: u32,
    pub bare: u32,
    pub separated: u32,
    /// Books holding this hash; the row lives while this does.
    pub holders: u32,
}

impl DoubleTally {
    fn of(row: &DoubleTotal) -> Self {
        Self {
            hash: row.hash,
            uncased: 0,
            bare: 0,
            separated: 0,
            holders: 0,
        }
    }

    fn absorb_book(&mut self, row: &DoubleTotal) {
        self.holders += 1;
        self.uncased = self.uncased.saturating_add(row.uncased);
        self.bare = self.bare.saturating_add(row.bare);
        self.separated = self.separated.saturating_add(row.separated);
    }

    /// Whether the corpus doubled this word at all, which is what the recusal
    /// share counts.
    pub const fn doubles(&self) -> bool {
        self.bare + self.separated >= 2
    }
}

impl Lane for DoubleTally {
    type Key = u64;

    fn key(&self) -> u64 {
        self.hash
    }

    fn holders(&self) -> u32 {
        self.holders
    }

    fn absorb(&mut self, other: &Self) {
        self.holders += other.holders;
        self.uncased = self.uncased.saturating_add(other.uncased);
        self.bare = self.bare.saturating_add(other.bare);
        self.separated = self.separated.saturating_add(other.separated);
    }

    fn release(&mut self, other: &Self) {
        self.holders = self.holders.saturating_sub(other.holders);
        self.uncased = self.uncased.saturating_sub(other.uncased);
        self.bare = self.bare.saturating_sub(other.bare);
        self.separated = self.separated.saturating_sub(other.separated);
    }
}

/// What both lanes have in common, so `add` and `remove` are written once.
trait Lane: Copy + Default {
    type Key: Ord + Copy;

    fn key(&self) -> Self::Key;
    fn holders(&self) -> u32;
    fn absorb(&mut self, other: &Self);
    fn release(&mut self, other: &Self);
}

/// The corpus's word counts: the casing lane one row per
/// `(case-folded word, before)`, the doubles lane one row per word, both by
/// hash ascending — the emission order the wire pins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordTotals {
    rows: Vec<WordTally>,
    doubles: Vec<DoubleTally>,
}

impl WordTotals {
    /// The tally of a whole corpus at once: one sort, then one grouping pass.
    pub fn merge(corpus: &[&WordAggregate]) -> Self {
        Self {
            rows: delta_of(corpus),
            doubles: doubles_of(corpus),
        }
    }

    /// Rows by `(hash, before)` ascending.
    pub fn rows(&self) -> &[WordTally] {
        &self.rows
    }

    /// The doubles lane, by hash ascending.
    pub fn doubles(&self) -> &[DoubleTally] {
        &self.doubles
    }

    /// One word's rows at a time, in hash order: the judge sums the lanes of
    /// whichever `Before`s the terminal table left free.
    pub fn by_word(&self) -> impl Iterator<Item = &[WordTally]> {
        self.rows.chunk_by(|left, right| left.hash == right.hash)
    }

    /// The share of this corpus's distinct words that appear doubled, in basis
    /// points — the recusal statistic.
    ///
    /// A share, never a count: Jonah and a whole Bible must answer the same
    /// way. The vocabulary is the union of the two lanes, because a word with
    /// no cased letter is in the doubles lane alone.
    pub fn doubling_share_bp(&self) -> u16 {
        let mut distinct = 0u64;
        let mut doubling = 0u64;
        let mut cased = self.by_word().peekable();
        for row in &self.doubles {
            while cased.peek().is_some_and(|word| word[0].hash < row.hash) {
                cased.next();
                distinct += 1;
            }
            if cased.peek().is_some_and(|word| word[0].hash == row.hash) {
                cased.next();
            }
            distinct += 1;
            doubling += u64::from(row.doubles());
        }
        distinct += cased.count() as u64;
        crate::judge::share_bp(doubling, distinct)
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.doubles.is_empty()
    }

    /// Adds these books' counts. The result is [`merge`](Self::merge) over the
    /// books already tallied and these.
    pub fn add(&mut self, corpus: &[&WordAggregate]) {
        apply(&mut self.rows, &delta_of(corpus), true);
        apply(&mut self.doubles, &doubles_of(corpus), true);
    }

    /// Removes books previously added, exactly: every word they hold is in the
    /// tally, so the result is the merge of what is left.
    pub fn remove(&mut self, corpus: &[&WordAggregate]) {
        apply(&mut self.rows, &delta_of(corpus), false);
        apply(&mut self.doubles, &doubles_of(corpus), false);
    }

    /// Inline size plus both lanes' own allocation; what a resident host pays.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>() + size_of_val(&*self.rows) + size_of_val(&*self.doubles)
    }
}

/// One tandem walk over two key-sorted sequences: rows the tally holds move in
/// place, rows it does not are spliced in afterwards, and a row no book holds
/// any more is compacted out.
fn apply<L: Lane>(held: &mut Vec<L>, delta: &[L], add: bool) {
    let (mut at, mut dead) = (0usize, false);
    let mut missing: Vec<usize> = Vec::new();
    for (index, row) in delta.iter().enumerate() {
        while held.get(at).is_some_and(|seen| seen.key() < row.key()) {
            at += 1;
        }
        match held.get_mut(at) {
            Some(seen) if seen.key() == row.key() => {
                if add {
                    seen.absorb(row);
                } else {
                    seen.release(row);
                    dead |= seen.holders() == 0;
                }
                at += 1;
            }
            // Removing a word the tally does not hold cannot happen: a book is
            // only ever removed after it was added.
            _ => {
                debug_assert!(add, "a removed book's every word is in the tally");
                if add {
                    missing.push(index);
                }
            }
        }
    }
    if dead {
        held.retain(|row| row.holders() > 0);
    }
    if !missing.is_empty() {
        splice(held, delta, &missing);
    }
}

/// Merges the absent rows in from the back, so only the tail past the lowest
/// new key moves at all.
fn splice<L: Lane>(rows: &mut Vec<L>, delta: &[L], missing: &[usize]) {
    let end = rows.len();
    rows.resize(end + missing.len(), L::default());
    let (mut write, mut read) = (rows.len(), end);
    for &index in missing.iter().rev() {
        let row = delta[index];
        let mut from = read;
        while from > 0 && rows[from - 1].key() > row.key() {
            from -= 1;
        }
        if from < read {
            write -= read - from;
            rows.copy_within(from..read, write);
            read = from;
        }
        write -= 1;
        rows[write] = row;
    }
    debug_assert_eq!(write, read, "every hole is filled exactly once");
}

/// These books merged into rows of their own: the same shape the tally holds,
/// so adding and removing are one walk over two sorted sequences.
fn delta_of(corpus: &[&WordAggregate]) -> Vec<WordTally> {
    let mut flat: Vec<(u64, u32, u32, [u32; 4])> = corpus
        .iter()
        .enumerate()
        .flat_map(|(book, aggregate)| {
            aggregate
                .words()
                .iter()
                .map(move |word| (word.hash, word.before_raw(), book as u32, word.counts))
        })
        .collect();
    flat.sort_unstable();
    let mut rows: Vec<WordTally> = Vec::with_capacity(flat.len());
    for (hash, before, _, counts) in flat {
        match rows.last_mut() {
            Some(last) if last.key() == (hash, before) => last.absorb_book(counts),
            _ => {
                let mut row = WordTally::of(hash, before);
                row.absorb_book(counts);
                rows.push(row);
            }
        }
    }
    rows
}

/// The same for the doubles lane, keyed by hash alone.
fn doubles_of(corpus: &[&WordAggregate]) -> Vec<DoubleTally> {
    let mut flat: Vec<(u64, u32, DoubleTotal)> = corpus
        .iter()
        .enumerate()
        .flat_map(|(book, aggregate)| {
            aggregate
                .doubles()
                .iter()
                .map(move |row| (row.hash, book as u32, *row))
        })
        .collect();
    flat.sort_unstable_by_key(|(hash, book, _)| (*hash, *book));
    let mut rows: Vec<DoubleTally> = Vec::with_capacity(flat.len());
    for (hash, _, row) in flat {
        match rows.last_mut() {
            Some(last) if last.hash == hash => last.absorb_book(&row),
            _ => {
                let mut held = DoubleTally::of(&row);
                held.absorb_book(&row);
                rows.push(held);
            }
        }
    }
    rows
}
