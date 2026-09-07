//! The word channels' last verdicts, and the keys a publication moved.
//!
//! ```text
//! en_ulb, 66 books, 57 word patterns in a 121-row table
//!
//! publish     table T, config C                judge 14,093 keys, keep 57
//! keystroke   table T, config C, MRK's moved   judge  1,731 keys, merge
//! set_config  table T, config C'               judge 14,093 keys, keep anew
//! ```
//!
//! A word's verdicts are its own. Casing reads that word's rows, the terminal
//! table and the config; doubled reads its doubles row, the table, the config
//! and the corpus-wide recusal; a letter run reads its own row and the config.
//! So the rows one key produces do not depend on what else the tally holds,
//! and a publication that moved one book's words re-judges those keys and
//! keeps the rest — a whole judge's list either way, which a debug build
//! asserts. [`crate::Channel::WordLength`] is the exception, judged whole:
//! its ceiling is the corpus's own length distribution.

use crate::judge::{
    JudgingConfig, Pattern, PatternKey, TerminalTable, judge_words_for, judges_doubles, word_slot,
};
use crate::pass::Findings;
use crate::substrate::ScalarKey;

use super::{WordAggregate, WordTotals};

/// The corpus-tally keys one publication's books moved: a case-folded word's
/// hash, which keys both lanes it can stand in, and a letter, which keys its
/// run lane.
///
/// Both lists ascend without repeats, so a judge walks them beside the tally
/// instead of probing it per key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MovedWords {
    words: Vec<u64>,
    letters: Vec<ScalarKey>,
}

impl MovedWords {
    /// Word hashes, ascending.
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// Letters whose run lane moved, ascending.
    pub fn letters(&self) -> &[ScalarKey] {
        &self.letters
    }

    /// Keys named, both lanes together.
    pub fn len(&self) -> usize {
        self.words.len() + self.letters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.letters.is_empty()
    }

    pub fn clear(&mut self) {
        self.words.clear();
        self.letters.clear();
    }

    /// Names every key these books' rows hold — what a tally or an untally of
    /// them moves. Each lane is key-ascending already, so this is a merge.
    pub fn absorb(&mut self, books: &[&WordAggregate]) {
        for book in books {
            let words = ascending(book.words().len(), book.words().iter().map(|row| row.hash));
            let doubles = ascending(
                book.doubles().len(),
                book.doubles().iter().map(|row| row.hash),
            );
            let letters = ascending(
                book.letter_runs().len(),
                book.letter_runs().iter().map(|row| row.0),
            );
            self.words = union(&self.words, &union(&words, &doubles));
            self.letters = union(&self.letters, &letters);
        }
    }

    /// Whether this pattern's key is one the delta moved, so its verdict was
    /// judged again rather than kept.
    fn names(&self, pattern: &Pattern) -> bool {
        match pattern.key {
            PatternKey::Casing { hash, .. }
            | PatternKey::WordLength { hash, .. }
            | PatternKey::Doubled { hash, .. } => self.words.binary_search(&hash).is_ok(),
            PatternKey::LetterRun { .. } => self.letters.binary_search(&pattern.glyph).is_ok(),
            _ => false,
        }
    }

    /// Inline size plus both lists.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>() + size_of_val(&*self.words) + size_of_val(&*self.letters)
    }
}

/// An already-ascending sequence without its repeats.
fn ascending<T: Ord + Copy>(len: usize, rows: impl Iterator<Item = T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(len);
    for row in rows {
        debug_assert!(
            out.last().is_none_or(|last| *last <= row),
            "an aggregate lane is key-ascending"
        );
        if out.last() != Some(&row) {
            out.push(row);
        }
    }
    out
}

/// Two ascending sequences as one, without repeats.
fn union<T: Ord + Copy>(a: &[T], b: &[T]) -> Vec<T> {
    if a.is_empty() {
        return b.to_vec();
    }
    if b.is_empty() {
        return a.to_vec();
    }
    let mut out: Vec<T> = Vec::with_capacity(a.len() + b.len());
    let (mut left, mut right) = (0usize, 0usize);
    while left < a.len() && right < b.len() {
        let next = match a[left].cmp(&b[right]) {
            core::cmp::Ordering::Less => {
                left += 1;
                a[left - 1]
            }
            core::cmp::Ordering::Greater => {
                right += 1;
                b[right - 1]
            }
            core::cmp::Ordering::Equal => {
                left += 1;
                right += 1;
                a[left - 1]
            }
        };
        out.push(next);
    }
    out.extend_from_slice(&a[left..]);
    out.extend_from_slice(&b[right..]);
    out
}

/// The word channels' patterns from the last publication, beside the evidence
/// they stand on: a host keeps one and hands it back every time.
#[derive(Debug, Clone, Default)]
pub struct WordVerdicts {
    seen: Option<Seen>,
    /// Tally keys the last judge visited.
    judged: usize,
}

/// What a kept list depends on beyond one key's own rows. Any of it moving is
/// a whole re-judge, because it moves verdicts the delta never named.
#[derive(Debug, Clone)]
struct Seen {
    table: TerminalTable,
    config: JudgingConfig,
    /// The corpus-wide doubling recusal under that table and config.
    doubles: bool,
    patterns: Vec<Pattern>,
}

impl WordVerdicts {
    /// Tally keys the last [`judge`](Self::judge) visited: the whole tally
    /// when it re-judged everything, the delta's keys when it merged.
    pub fn last_judged(&self) -> usize {
        self.judged
    }

    /// Forgets everything, so the next judge is a whole one.
    pub fn clear(&mut self) {
        self.seen = None;
        self.judged = 0;
    }

    /// The word channels over `totals`, keeping every verdict `moved` does not
    /// name, pushed in the order the wire pins.
    pub(crate) fn judge(
        &mut self,
        corpus: &[&WordAggregate],
        totals: &WordTotals,
        table: &TerminalTable,
        config: &JudgingConfig,
        moved: &MovedWords,
        out: &mut Findings,
    ) {
        let doubles = config.channels.doubled && judges_doubles(totals, table, config);
        let kept = self.seen.take().filter(|seen| {
            !config.channels.word_length
                && seen.doubles == doubles
                && seen.config == *config
                && seen.table == *table
        });
        let patterns = match kept {
            Some(seen) => {
                let mut fresh = Vec::new();
                judge_words_for(corpus, totals, table, config, Some(moved), &mut fresh);
                self.judged = moved.len();
                merge(seen.patterns, fresh, moved)
            }
            None => {
                let mut whole = Vec::new();
                judge_words_for(corpus, totals, table, config, None, &mut whole);
                self.judged = totals.keys();
                whole
            }
        };
        #[cfg(debug_assertions)]
        {
            let mut whole = Vec::new();
            judge_words_for(corpus, totals, table, config, None, &mut whole);
            debug_assert_eq!(patterns, whole, "kept word verdicts equal a whole judge");
        }
        for pattern in &patterns {
            out.push_pattern(*pattern);
        }
        self.seen = Some(Seen {
            table: table.clone(),
            config: *config,
            doubles,
            patterns,
        });
    }

    /// Inline size plus the kept patterns and the table behind them.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>()
            + self.seen.as_ref().map_or(0, |seen| {
                size_of_val(&*seen.patterns) + size_of_val(seen.table.forcing())
            })
    }
}

/// The kept patterns with every moved key's rows dropped and the freshly
/// judged ones spliced into their places.
///
/// Both lists are in emission order and one key's rows are contiguous in each,
/// so this is one tandem walk and never a sort.
fn merge(kept: Vec<Pattern>, fresh: Vec<Pattern>, moved: &MovedWords) -> Vec<Pattern> {
    let mut out: Vec<Pattern> = Vec::with_capacity(kept.len() + fresh.len());
    let mut fresh = fresh.into_iter().peekable();
    for pattern in kept {
        if moved.names(&pattern) {
            continue;
        }
        let slot = word_slot(&pattern);
        while fresh.peek().is_some_and(|new| word_slot(new) < slot) {
            out.push(fresh.next().expect("peeked"));
        }
        out.push(pattern);
    }
    out.extend(fresh);
    out
}
