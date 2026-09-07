//! The corpus word tally: every book's rows merged by hash, and the two
//! updates a resident host makes instead of merging them again.
//!
//! ```text
//! merge([GEN, MRK])   hash(david) free [2, 40, 0, 0]  books [1, 2, 0, 0]  holders 2
//! remove([old MRK])   hash(david) free [2, 38, 0, 0]  books [1, 1, 0, 0]  holders 1
//! add([new MRK])      hash(david) free [2, 39, 0, 0]  books [1, 2, 0, 0]  holders 2
//! ```
//!
//! The tally after any sequence of updates is the merge of the books resident
//! then — counts, dispersion, and rows alike — which is what lets a host judge
//! the casing channel from it instead of from 66 aggregates. A row survives
//! while `holders` is nonzero, so a word held only in forced positions keeps
//! its all-zero row exactly as a fresh merge does.

use super::WordAggregate;

/// One case-folded word's corpus totals, by free-position [`super::Form`] lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WordTally {
    pub hash: u64,
    /// Free-position occurrences per lane, saturating.
    pub free: [u32; 4],
    /// Books whose own counts hold each lane — the row's dispersion.
    pub books: [u32; 4],
    /// Books holding the word at all, in any position; the row lives while
    /// this does.
    pub holders: u32,
}

impl WordTally {
    fn of(hash: u64) -> Self {
        Self {
            hash,
            ..Self::default()
        }
    }

    /// One book's chapter-folded lane counts.
    fn absorb_book(&mut self, free: [u32; 4]) {
        self.holders += 1;
        for (lane, count) in free.iter().enumerate() {
            if *count > 0 {
                self.free[lane] = self.free[lane].saturating_add(*count);
                self.books[lane] += 1;
            }
        }
    }

    fn absorb(&mut self, other: &Self) {
        self.holders += other.holders;
        for (lane, count) in other.free.iter().enumerate() {
            self.free[lane] = self.free[lane].saturating_add(*count);
            self.books[lane] += other.books[lane];
        }
    }

    fn release(&mut self, other: &Self) {
        self.holders = self.holders.saturating_sub(other.holders);
        for (lane, count) in other.free.iter().enumerate() {
            self.free[lane] = self.free[lane].saturating_sub(*count);
            self.books[lane] = self.books[lane].saturating_sub(other.books[lane]);
        }
    }
}

/// The corpus's word counts, one row per case-folded word, by hash ascending —
/// the emission order the wire pins.
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

    /// Rows by hash ascending.
    pub fn rows(&self) -> &[WordTally] {
        &self.rows
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
            while self.rows.get(at).is_some_and(|seen| seen.hash < row.hash) {
                at += 1;
            }
            match self.rows.get_mut(at) {
                Some(seen) if seen.hash == row.hash => {
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
            while from > 0 && self.rows[from - 1].hash > row.hash {
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
    let mut flat: Vec<(u64, u32, [u32; 4])> = corpus
        .iter()
        .enumerate()
        .flat_map(|(book, aggregate)| {
            aggregate
                .words()
                .iter()
                .map(move |word| (word.hash, book as u32, word.free))
        })
        .collect();
    flat.sort_unstable();
    let mut rows: Vec<WordTally> = Vec::with_capacity(flat.len());
    for (hash, _, free) in flat {
        match rows.last_mut() {
            Some(last) if last.hash == hash => last.absorb_book(free),
            _ => {
                let mut row = WordTally::of(hash);
                row.absorb_book(free);
                rows.push(row);
            }
        }
    }
    rows
}
