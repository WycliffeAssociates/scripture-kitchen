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
//! `holders` is nonzero. The key is `(hash, before)` and not the hash alone,
//! because forced and free are a judging decision the config may move, so the
//! tally cannot pre-sum them; [`WordTotals::by_word`] hands the judge one
//! word's rows at a time.

use super::{Before, WordAggregate};

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

    /// The sort key: the hash, then the packed `Before`.
    const fn key(&self) -> (u64, u32) {
        (self.hash, self.before)
    }

    /// One book's chapter-folded lane counts.
    fn absorb_book(&mut self, counts: [u32; 4]) {
        self.holders += 1;
        for (lane, count) in counts.iter().enumerate() {
            self.counts[lane] = self.counts[lane].saturating_add(*count);
        }
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

/// The corpus's word counts, one row per `(case-folded word, before)`, by hash
/// ascending — the emission order the wire pins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordTotals {
    rows: Vec<WordTally>,
}

impl WordTotals {
    /// The tally of a whole corpus at once: one sort, then one grouping pass.
    pub fn merge(corpus: &[&WordAggregate]) -> Self {
        Self {
            rows: delta_of(corpus),
        }
    }

    /// Rows by `(hash, before)` ascending.
    pub fn rows(&self) -> &[WordTally] {
        &self.rows
    }

    /// One word's rows at a time, in hash order: the judge sums the lanes of
    /// whichever `Before`s the terminal table left free.
    pub fn by_word(&self) -> impl Iterator<Item = &[WordTally]> {
        self.rows.chunk_by(|left, right| left.hash == right.hash)
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Adds these books' counts. The result is [`merge`](Self::merge) over the
    /// books already tallied and these.
    pub fn add(&mut self, corpus: &[&WordAggregate]) {
        self.apply(&delta_of(corpus), true);
    }

    /// Removes books previously added, exactly: every word they hold is in the
    /// tally, so the result is the merge of what is left.
    pub fn remove(&mut self, corpus: &[&WordAggregate]) {
        self.apply(&delta_of(corpus), false);
    }

    /// Inline size plus the rows' own allocation; what a resident host pays.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>() + size_of_val(&*self.rows)
    }

    /// One tandem walk over two hash-sorted sequences: rows the tally holds
    /// move in place, rows it does not are spliced in afterwards, and a row no
    /// book holds any more is compacted out.
    fn apply(&mut self, delta: &[WordTally], add: bool) {
        let (mut at, mut dead) = (0usize, false);
        let mut missing: Vec<usize> = Vec::new();
        for (index, row) in delta.iter().enumerate() {
            while self.rows.get(at).is_some_and(|seen| seen.key() < row.key()) {
                at += 1;
            }
            match self.rows.get_mut(at) {
                Some(seen) if seen.key() == row.key() => {
                    if add {
                        seen.absorb(row);
                    } else {
                        seen.release(row);
                        dead |= seen.holders == 0;
                    }
                    at += 1;
                }
                // Removing a word the tally does not hold cannot happen: a
                // book is only ever removed after it was added.
                _ => {
                    debug_assert!(add, "a removed book's every word is in the tally");
                    if add {
                        missing.push(index);
                    }
                }
            }
        }
        if dead {
            self.rows.retain(|row| row.holders > 0);
        }
        if !missing.is_empty() {
            self.splice(delta, &missing);
        }
    }

    /// Merges the absent rows in from the back, so only the tail past the
    /// lowest new hash moves at all.
    fn splice(&mut self, delta: &[WordTally], missing: &[usize]) {
        let held = self.rows.len();
        self.rows.resize(held + missing.len(), WordTally::default());
        let (mut write, mut read) = (self.rows.len(), held);
        for &index in missing.iter().rev() {
            let row = delta[index];
            let mut from = read;
            while from > 0 && self.rows[from - 1].key() > row.key() {
                from -= 1;
            }
            if from < read {
                write -= read - from;
                self.rows.copy_within(from..read, write);
                read = from;
            }
            write -= 1;
            self.rows[write] = row;
        }
        debug_assert_eq!(write, read, "every hole is filled exactly once");
    }
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
