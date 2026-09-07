//! What a book fires, and where in its text those rows actually sit.
//!
//! ```text
//! firing(MRK's rows, table, &mut set)  -> [4, 9]   // the word rows MRK holds
//! locate_chapters(MRK, text, 0..1, ..) -> Casing   at 12..17
//!                                        LetterRun at 40..46
//! ```
//!
//! The firing set and its lane maps are read once per book; the walk itself
//! restarts at every chapter, so one chapter's rows are the rows a whole-book
//! walk would have found there. The free/forced split is applied HERE and not
//! in the walk — the corpus's terminal table is what says which occurrences
//! the punctuation decided.

use rustc_hash::FxHashMap;

use crate::judge::{Pattern, PatternIndex, PatternKey, TerminalTable};
use crate::pass::{Findings, collect_verses};
use crate::substrate::ScalarKey;
use crate::{BookIndex, Chapter, ConventionDigest, FindingKind, Reasons, TextRange, Verse};

use super::{
    DoubleTotal, Form, Gap, WordAggregate, WordTotal, for_each_letter_run, for_each_word,
    gap_between, letter_run_lane,
};

/// One merge walk per lane, not one binary search per row: the judge emits
/// each lane's rows in hash order and the aggregate is sorted the same way,
/// and a probe per row into a 5k-row table for each of 66 books was a cache
/// miss per probe — a quarter of a keystroke.
///
/// On the casing lane, position-blind on purpose: a book claims a row whose
/// word it holds at all, and `locate` applies the free/forced split. A
/// superset costs a rescan that finds nothing; reading the terminal table
/// here would put a judging decision in a cache key. The doubles lane's
/// `separated` key is blind the same way now: a book claims the row when
/// ANY separator glyph doubled the word, forcing ones included, and
/// `locate` is where the table narrows it to the non-forcing glyphs the
/// judge actually counted.
pub(super) fn firing(aggregate: &WordAggregate, patterns: &[Pattern], out: &mut Vec<PatternIndex>) {
    out.clear();
    let words = aggregate.words();
    let doubles = aggregate.doubles();
    // One cursor per lane: each lane's rows arrive in hash order, but the
    // doubles rows come after the casing rows and restart at the lowest
    // hash again.
    let (mut word_at, mut word_last) = (0usize, 0u64);
    let (mut double_at, mut double_last) = (0usize, 0u64);
    for (index, pattern) in PatternIndex::over(patterns) {
        if let PatternKey::LetterRun { length } = pattern.key {
            // Its own lane and its own key: a letter, not a hash.
            if aggregate
                .letter_runs_for(pattern.glyph)
                .is_some_and(|lanes| lanes[letter_run_lane(length)] > 0)
            {
                out.push(index);
            }
            continue;
        }
        let Some(hash) = pattern.word_hash() else {
            continue;
        };
        let held = match pattern.key {
            PatternKey::Doubled { separated, .. } => {
                if hash < double_last {
                    // Rows out of hash order: probe for this one.
                    holds_double(aggregate.doubles_for(hash), separated)
                } else {
                    double_last = hash;
                    while double_at < doubles.len() && doubles[double_at].hash < hash {
                        double_at += 1;
                    }
                    holds_double(
                        doubles.get(double_at).filter(|row| row.hash == hash),
                        separated,
                    )
                }
            }
            _ if !aggregate.cased() => false,
            _ if hash < word_last => holds(aggregate.rows_for(hash), pattern),
            _ => {
                word_last = hash;
                while word_at < words.len() && words[word_at].hash < hash {
                    word_at += 1;
                }
                let mut end = word_at;
                while end < words.len() && words[end].hash == hash {
                    end += 1;
                }
                holds(&words[word_at..end], pattern)
            }
        };
        if held {
            out.push(index);
        }
    }
}

/// The firing set and its lane maps are read once for the whole range; the
/// walk itself restarts at every chapter, so one chapter's rows are the
/// rows a whole-book walk would have found there.
#[allow(clippy::too_many_arguments)]
pub(super) fn locate_chapters(
    book: BookIndex,
    text: &str,
    chapters: &[Chapter],
    verses: &[Verse],
    range: core::ops::Range<usize>,
    aggregate: &WordAggregate,
    counts: &mut Vec<u32>,
    out: &mut Findings,
) {
    let Some(sites) = WordSites::of(aggregate, out) else {
        counts.extend(range.map(|_| 0));
        return;
    };
    let mut found: Vec<(TextRange, PatternIndex, Reasons)> = Vec::new();
    let mut rebased: Vec<Verse> = Vec::new();
    let mut cursor = 0usize;
    for chapter in &chapters[range] {
        let span = chapter.text();
        let slice = &text[span.from() as usize..span.to() as usize];
        cursor = collect_verses(verses, cursor, *chapter, &mut rebased);
        let before = found.len();
        sites.walk(slice, span.from(), &rebased, &mut found);
        counts.push((found.len() - before) as u32);
    }
    if found.is_empty() {
        return;
    }
    out.open_book(book);
    for (span, index, reasons) in found {
        out.push(
            span,
            FindingKind::Convention(ConventionDigest::new(index, reasons)),
        )
        .expect("a located word lies inside the book it was found in");
    }
}

/// Whether this book's rows for one word hold what the pattern counts.
fn holds(rows: &[WordTotal], pattern: &Pattern) -> bool {
    match pattern.key {
        PatternKey::Casing { form, .. } => rows.iter().any(|row| row.count_of(form) > 0),
        PatternKey::WordLength { .. } => !rows.is_empty(),
        _ => false,
    }
}

/// The same for the doubles lane: `count_of` is the unfiltered superset over
/// every separator glyph, so this can hold true for a book whose only
/// `separated` occurrences all force a capital — `locate` is what applies the
/// table and finds nothing there, exactly as the casing lane's superset does.
fn holds_double(row: Option<&DoubleTotal>, separated: bool) -> bool {
    row.is_some_and(|row| row.count_of(separated) > 0)
}

/// One book's firing set, resolved into the lane maps the walk probes, plus
/// the corpus terminal table it reads free from forced with.
///
/// Read once per publication per book: the walk itself is per chapter, and
/// nothing here varies between them.
struct WordSites {
    table: TerminalTable,
    cased: FxHashMap<(u64, Form), PatternIndex>,
    long: FxHashMap<u64, PatternIndex>,
    twice: FxHashMap<(u64, bool), PatternIndex>,
    sticky: FxHashMap<(ScalarKey, u8), PatternIndex>,
}

impl WordSites {
    /// `None` when this book fires nothing, so no chapter of it is walked.
    fn of(aggregate: &WordAggregate, out: &Findings) -> Option<Self> {
        let mut set = Vec::new();
        firing(aggregate, out.patterns(), &mut set);
        if set.is_empty() {
            return None;
        }
        // Copied because `Findings` cannot lend its table and take a row at
        // once; a firing set is tens of rows, not thousands.
        let mut sites = Self {
            table: out.terminals().cloned().unwrap_or_default(),
            cased: FxHashMap::default(),
            long: FxHashMap::default(),
            twice: FxHashMap::default(),
            sticky: FxHashMap::default(),
        };
        for &index in &set {
            let pattern = out.patterns()[usize::from(index.get())];
            match pattern.key {
                PatternKey::Casing { hash, form } => {
                    sites.cased.insert((hash, form), index);
                }
                PatternKey::WordLength { hash, .. } => {
                    sites.long.insert(hash, index);
                }
                PatternKey::Doubled { hash, separated } => {
                    sites.twice.insert((hash, separated), index);
                }
                PatternKey::LetterRun { length } => {
                    sites.sticky.insert((pattern.glyph, length), index);
                }
                _ => {}
            }
        }
        Some(sites)
    }

    /// One chapter's rows, in walk order, spans rebased by the chapter's
    /// projected `start`.
    fn walk(
        &self,
        slice: &str,
        start: u32,
        verses: &[Verse],
        found: &mut Vec<(TextRange, PatternIndex, Reasons)>,
    ) {
        let mut runs: Vec<PatternIndex> = Vec::new();
        // The pair, not the word: a chapter seam ends it, which is what a
        // fresh scan per chapter already says.
        let mut previous: Option<(u64, u32, u32)> = None;
        for_each_word(slice, verses, |word| {
            if let Some((hash, from, to)) = previous.replace((word.hash, word.from, word.to))
                && hash == word.hash
                && let Some(gap) = gap_between(slice, to, word.from)
                // A separator whose last glyph forces a capital in this
                // corpus's own table is a sentence terminal, not a
                // doubling: `go. Go` is two sentences.
                && let Some(separated) = match gap {
                    Gap::Bare => Some(false),
                    Gap::Separated(glyph) if !self.table.forces(glyph) => Some(true),
                    Gap::Separated(_) => None,
                }
                && let Some(&index) = self.twice.get(&(hash, separated))
            {
                // The span covers both words and what stood between them.
                let at =
                    TextRange::new(start + from, start + word.to).expect("a pair grows forward");
                let reason = if separated {
                    Reasons::DOUBLED_SEPARATED
                } else {
                    Reasons::DOUBLED_BARE
                };
                found.push((at, index, reason));
            }
            // The letter-run rows this word holds, one per RUN: the
            // channel counts runs, and a word may hold two of one length.
            runs.clear();
            if !self.sticky.is_empty() {
                let text = &slice[word.from as usize..word.to as usize];
                for_each_letter_run(text, |letter, length| {
                    if let Some(&index) = self.sticky.get(&(letter, length)) {
                        runs.push(index);
                    }
                });
            }
            let (casing, length) = if word.form == Form::Uncased {
                // An uncased word is in no casing row; its runs still are.
                (None, None)
            } else {
                (
                    word.before
                        .is_free(&self.table)
                        .then(|| self.cased.get(&(word.hash, word.form)))
                        .flatten(),
                    self.long.get(&word.hash),
                )
            };
            // One span, one row: a word two channels named carries both
            // reasons and the finer channel's index, as a run does.
            let named = match (casing, length) {
                (Some(&index), Some(_)) => {
                    Some((index, Reasons::CASING.union(Reasons::WORD_LENGTH)))
                }
                (Some(&index), None) => Some((index, Reasons::CASING)),
                (None, Some(&index)) => Some((index, Reasons::WORD_LENGTH)),
                (None, None) => None,
            };
            if named.is_none() && runs.is_empty() {
                return;
            }
            let at =
                TextRange::new(start + word.from, start + word.to).expect("a word grows forward");
            // A merged row keeps the finer channel's index, so the first
            // run's own row is the one the bit rides; any further run of
            // the same word is a row of its own.
            let rest = match named {
                Some((index, reasons)) => {
                    let reasons = if runs.is_empty() {
                        reasons
                    } else {
                        reasons.union(Reasons::LETTER_RUN)
                    };
                    found.push((at, index, reasons));
                    1
                }
                None => 0,
            };
            for &index in runs.iter().skip(rest) {
                found.push((at, index, Reasons::LETTER_RUN));
            }
        });
    }
}
