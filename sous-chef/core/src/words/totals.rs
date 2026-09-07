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
//! updates, as does the letter-run lane, keyed by the folded letter.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::{Before, DoubleTotal, LETTER_RUN_LANES, WordAggregate, WordTotal, merge_glyph_lane};
use crate::judge::TerminalTable;
use crate::substrate::ScalarKey;

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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DoubleTally {
    pub hash: u64,
    pub uncased: u32,
    pub bare: u32,
    /// One entry per distinct separator glyph, ascending. A pair whose glyph
    /// forces a capital in the corpus's own [`TerminalTable`] is a sentence
    /// boundary, not a doubling — [`Self::separated_free`] is the judged sum.
    pub separated: Box<[(ScalarKey, u32)]>,
    /// Books holding this hash; the row lives while this does.
    pub holders: u32,
}

impl DoubleTally {
    fn of(row: &DoubleTotal) -> Self {
        Self {
            hash: row.hash,
            uncased: 0,
            bare: 0,
            separated: Box::default(),
            holders: 0,
        }
    }

    fn absorb_book(&mut self, row: &DoubleTotal) {
        self.holders += 1;
        self.uncased = self.uncased.saturating_add(row.uncased);
        self.bare = self.bare.saturating_add(row.bare);
        self.separated = merge_glyph_lane(&self.separated, &row.separated, true);
    }

    /// The separated lane's occurrences whose last glyph does NOT force a
    /// capital in `table`.
    pub fn separated_free(&self, table: &TerminalTable) -> u64 {
        self.separated
            .iter()
            .filter(|&&(glyph, _)| !table.forces(glyph))
            .map(|&(_, count)| u64::from(count))
            .sum()
    }

    /// Whether the corpus doubled this word at all, which is what the recusal
    /// share counts — bare plus the separated pairs whose separator does not
    /// force a capital, so a sentence boundary never counts toward doubling
    /// being productive in this language.
    pub fn doubles(&self, table: &TerminalTable) -> bool {
        u64::from(self.bare) + self.separated_free(table) >= 2
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
        self.separated = merge_glyph_lane(&self.separated, &other.separated, true);
    }

    fn release(&mut self, other: &Self) {
        self.holders = self.holders.saturating_sub(other.holders);
        self.uncased = self.uncased.saturating_sub(other.uncased);
        self.bare = self.bare.saturating_sub(other.bare);
        self.separated = merge_glyph_lane(&self.separated, &other.separated, false);
    }
}

/// One letter's repeat history over the corpus: how many runs of it there
/// were at each length `2..=8+`, and how many books hold any of them.
///
/// The denominator a run length is judged against is this row's own sum — the
/// letter's whole habit of repeating — so a letter no corpus doubles says
/// nothing about a letter that is doubled everywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTally {
    pub letter: ScalarKey,
    /// Runs by length, index `length - 2`; the last lane counts 8 or more.
    pub lengths: [u32; LETTER_RUN_LANES],
    /// Books holding this letter's runs; the row lives while this does.
    pub holders: u32,
}

impl RunTally {
    fn of(letter: ScalarKey) -> Self {
        Self {
            letter,
            lengths: [0; LETTER_RUN_LANES],
            holders: 0,
        }
    }

    fn absorb_book(&mut self, lanes: &[u32; LETTER_RUN_LANES]) {
        self.holders += 1;
        for (slot, count) in self.lengths.iter_mut().zip(lanes) {
            *slot = slot.saturating_add(*count);
        }
    }

    /// Every run of this letter, whatever its length: the channel's
    /// denominator.
    pub fn runs(&self) -> u64 {
        self.lengths.iter().map(|&count| u64::from(count)).sum()
    }
}

impl Default for RunTally {
    fn default() -> Self {
        Self::of(ScalarKey::NONE)
    }
}

impl Lane for RunTally {
    type Key = ScalarKey;

    fn key(&self) -> ScalarKey {
        self.letter
    }

    fn holders(&self) -> u32 {
        self.holders
    }

    fn absorb(&mut self, other: &Self) {
        self.holders += other.holders;
        for (slot, count) in self.lengths.iter_mut().zip(other.lengths) {
            *slot = slot.saturating_add(count);
        }
    }

    fn release(&mut self, other: &Self) {
        self.holders = self.holders.saturating_sub(other.holders);
        for (slot, count) in self.lengths.iter_mut().zip(other.lengths) {
            *slot = slot.saturating_sub(count);
        }
    }
}

/// What the lanes have in common, so `add` and `remove` are written once.
trait Lane: Clone + Default {
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
    letter_runs: Vec<RunTally>,
}

impl WordTotals {
    /// The tally of a whole corpus at once: one merge walk per lane.
    pub fn merge(corpus: &[&WordAggregate]) -> Self {
        Self {
            rows: delta_of(corpus),
            doubles: doubles_of(corpus),
            letter_runs: runs_of(corpus),
        }
    }

    /// The letter-run lane, by letter ascending.
    pub fn letter_runs(&self) -> &[RunTally] {
        &self.letter_runs
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
    ///
    /// A pair whose separator forces a capital in `table` is a sentence
    /// boundary, so it counts toward neither the word's doubling nor the
    /// share — the recusal answers to the same evidence the numerator does.
    pub fn doubling_share_bp(&self, table: &TerminalTable) -> u16 {
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
            doubling += u64::from(row.doubles(table));
        }
        distinct += cased.count() as u64;
        crate::judge::share_bp(doubling, distinct)
    }

    /// Distinct keys the word channels judge: one per case-folded word in
    /// either lane — the two partition a word's occurrences, so a word may
    /// stand in both — plus one per letter with a repeat history.
    pub fn keys(&self) -> usize {
        let mut distinct = 0usize;
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
        }
        distinct + cased.count() + self.letter_runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.doubles.is_empty() && self.letter_runs.is_empty()
    }

    /// Adds these books' counts. The result is [`merge`](Self::merge) over the
    /// books already tallied and these.
    pub fn add(&mut self, corpus: &[&WordAggregate]) {
        apply(&mut self.rows, &delta_of(corpus), true);
        apply(&mut self.doubles, &doubles_of(corpus), true);
        apply(&mut self.letter_runs, &runs_of(corpus), true);
    }

    /// Removes books previously added, exactly: every word they hold is in the
    /// tally, so the result is the merge of what is left.
    pub fn remove(&mut self, corpus: &[&WordAggregate]) {
        apply(&mut self.rows, &delta_of(corpus), false);
        apply(&mut self.doubles, &doubles_of(corpus), false);
        apply(&mut self.letter_runs, &runs_of(corpus), false);
    }

    /// Inline size plus every lane's own allocation, each doubled row's own
    /// per-glyph lane included; what a resident host pays.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>()
            + size_of_val(&*self.rows)
            + size_of_val(&*self.doubles)
            + size_of_val(&*self.letter_runs)
            + self
                .doubles
                .iter()
                .map(|row| size_of_val(&*row.separated))
                .sum::<usize>()
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

/// Merges the absent rows in: `missing` names them in ascending key order
/// (the order `apply` walked `delta`), so this is one tandem merge of two
/// sorted sequences rather than a probe per new row.
fn splice<L: Lane>(rows: &mut Vec<L>, delta: &[L], missing: &[usize]) {
    let mut merged: Vec<L> = Vec::with_capacity(rows.len() + missing.len());
    let mut at = 0usize;
    for &index in missing {
        let key = delta[index].key();
        while at < rows.len() && rows[at].key() < key {
            merged.push(rows[at].clone());
            at += 1;
        }
        merged.push(delta[index].clone());
    }
    merged.extend_from_slice(&rows[at..]);
    *rows = merged;
}

/// Walks several key-ascending lanes as one ascending sequence, ties broken by
/// lane order, without moving a row: one cursor per lane and a heap over their
/// heads. One lane skips the heap, which is the shape every incremental update
/// has.
fn merged<'a, T, K: Ord>(lanes: &[&'a [T]], key: impl Fn(&T) -> K, mut visit: impl FnMut(&'a T)) {
    match lanes {
        [] => {}
        [only] => only.iter().for_each(&mut visit),
        _ => {
            let mut at = vec![0usize; lanes.len()];
            let mut heap: BinaryHeap<Reverse<(K, usize)>> = lanes
                .iter()
                .enumerate()
                .filter_map(|(lane, rows)| Some(Reverse((key(rows.first()?), lane))))
                .collect();
            while let Some(Reverse((_, lane))) = heap.pop() {
                visit(&lanes[lane][at[lane]]);
                at[lane] += 1;
                if let Some(next) = lanes[lane].get(at[lane]) {
                    heap.push(Reverse((key(next), lane)));
                }
            }
        }
    }
}

/// The rows every lane holds, so one merge reserves once.
fn rows_in<T>(lanes: &[&[T]]) -> usize {
    lanes.iter().map(|lane| lane.len()).sum()
}

/// These books merged into rows of their own: the same shape the tally holds,
/// so adding and removing are one walk over two sorted sequences.
///
/// Each book's lane is already `(hash, before)` ascending, so this is a copy
/// for one book and a merge for several — never a sort.
fn delta_of(corpus: &[&WordAggregate]) -> Vec<WordTally> {
    let lanes: Vec<&[WordTotal]> = corpus.iter().map(|book| book.words()).collect();
    let mut rows: Vec<WordTally> = Vec::with_capacity(rows_in(&lanes));
    merged(
        &lanes,
        |word| (word.hash, word.before_raw()),
        |word| match rows.last_mut() {
            Some(last) if last.key() == (word.hash, word.before_raw()) => {
                last.absorb_book(word.counts);
            }
            _ => {
                let mut row = WordTally::of(word.hash, word.before_raw());
                row.absorb_book(word.counts);
                rows.push(row);
            }
        },
    );
    rows
}

/// The same for the letter-run lane, keyed by the folded letter.
fn runs_of(corpus: &[&WordAggregate]) -> Vec<RunTally> {
    let lanes: Vec<&[(ScalarKey, [u32; LETTER_RUN_LANES])]> =
        corpus.iter().map(|book| book.letter_runs()).collect();
    let mut rows: Vec<RunTally> = Vec::with_capacity(rows_in(&lanes));
    merged(
        &lanes,
        |row| row.0,
        |&(letter, lanes)| match rows.last_mut() {
            Some(last) if last.letter == letter => last.absorb_book(&lanes),
            _ => {
                let mut held = RunTally::of(letter);
                held.absorb_book(&lanes);
                rows.push(held);
            }
        },
    );
    rows
}

/// The same for the doubles lane, keyed by hash alone.
fn doubles_of(corpus: &[&WordAggregate]) -> Vec<DoubleTally> {
    let lanes: Vec<&[DoubleTotal]> = corpus.iter().map(|book| book.doubles()).collect();
    let mut rows: Vec<DoubleTally> = Vec::with_capacity(rows_in(&lanes));
    merged(
        &lanes,
        |row| row.hash,
        |row| match rows.last_mut() {
            Some(last) if last.hash == row.hash => last.absorb_book(row),
            _ => {
                let mut held = DoubleTally::of(row);
                held.absorb_book(row);
                rows.push(held);
            }
        },
    );
    rows
}
