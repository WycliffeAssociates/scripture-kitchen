//! The word channels: a corpus's own spellings judged against each other.
//!
//! ```text
//! "David" 297 / "DAVID" 3      -> a Casing row on the minority form
//! free_of(word, terminals)     -> the occurrences a sentence start did not force
//! ```
//!
//! Every row is a minority form against the same corpus's majority — one
//! case-folded word's own hash, so two spellings are two keys and never each
//! other's evidence. Casing and doubling drop the occurrences a terminal table
//! forced first: a capital after a full stop is grammar, not a choice. Word
//! length and letter runs read every occurrence, because what stood before a
//! word decides nothing about how long it is.

use super::*;

/// One row per case-folded word whose minority form in FREE positions falls
/// under the word staircase, then one per word far longer than the corpus's.
///
/// Free is decided here and not in the walk: the row carries the glyph that
/// stood before each occurrence, and `table` says which glyphs this corpus
/// capitalizes after. A position the punctuation decided is out of both
/// numerator and denominator.
pub(crate) fn judge_words(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    table: &TerminalTable,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let mut patterns = Vec::new();
    judge_words_for(corpus, totals, table, config, None, &mut patterns);
    for pattern in patterns {
        out.push_pattern(pattern);
    }
}

/// The word channels over the tally keys `moved` names, or over every key when
/// it is `None`, in the order the wire pins.
///
/// The channels are independent per key: a word's casing verdict reads its own
/// rows, the table and the config; its doubled verdict reads its doubles row,
/// the table, the config and the corpus-wide recusal; a letter's run verdict
/// reads its own row and the config. So the rows a moved key produces are the
/// rows a whole judge would produce for it, whatever else the corpus holds —
/// which is what lets a host keep the rest.
///
/// [`Channel::WordLength`] is the exception and is judged whole: its ceiling is
/// the corpus's own length distribution, so one word moving moves every verdict.
pub(crate) fn judge_words_for(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    table: &TerminalTable,
    config: &JudgingConfig,
    moved: Option<&MovedWords>,
    out: &mut Vec<Pattern>,
) {
    if config.channels.casing {
        casing(corpus, totals, table, config, moved, out);
    }
    if config.channels.word_length {
        debug_assert!(moved.is_none(), "the length shape is the whole corpus's");
        word_length(corpus, totals, config, out);
    }
    if config.channels.doubled && judges_doubles(totals, table, config) {
        doubled(corpus, totals, table, config, moved, out);
    }
    if config.channels.letter_runs {
        letter_runs(corpus, totals, config, moved, out);
    }
}

/// Where a word pattern stands in the emission order above: its channel group,
/// then its own key. One key's rows are contiguous, so a merge of two lists in
/// this order is one tandem walk.
pub(crate) fn word_slot(pattern: &Pattern) -> (u8, u64) {
    match pattern.key {
        PatternKey::Casing { hash, .. } => (0, hash),
        PatternKey::WordLength { hash, .. } => (1, hash),
        PatternKey::Doubled { hash, .. } => (2, hash),
        PatternKey::LetterRun { .. } => (3, u64::from(pattern.glyph.raw())),
        _ => (4, 0),
    }
}

/// One row per run length a letter reaches under its own repeat history's
/// band, longest-established-first ruled out by the support gate below.
///
/// The denominator is every run of THAT letter of two or more — the letter's
/// own habit — so `theee` is judged against thousands of `ee` and a script
/// that never doubles a letter judges nothing. Two guards, and they are
/// different claims:
///
/// * the band, which is the ordinary staircase over that denominator, and
/// * a support gate: every shorter length must itself stand on at least
///   `word_support_floor` runs, so `eee` speaks only where `ee` is
///   established and a language with no `ee` and one `eee` says nothing.
///
/// Length 2 never fires. It is the whole denominator's floor, and a letter
/// doubled at all is not evidence of anything.
pub(super) fn letter_runs(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    config: &JudgingConfig,
    moved: Option<&MovedWords>,
    out: &mut Vec<Pattern>,
) {
    match moved {
        None => {
            for row in totals.letter_runs() {
                letter_run(corpus, row, config, out);
            }
        }
        Some(moved) => {
            let rows = totals.letter_runs();
            let mut at = 0usize;
            for letter in moved.letters() {
                if seek(rows, &mut at, letter, |row| row.letter) {
                    letter_run(corpus, &rows[at], config, out);
                    at += 1;
                }
            }
        }
    }
}

/// One letter's run lengths against its own repeat history.
pub(super) fn letter_run(
    corpus: &[&WordAggregate],
    row: &RunTally,
    config: &JudgingConfig,
    out: &mut Vec<Pattern>,
) {
    let runs = row.runs();
    let Some((band, ceiling)) = entitled_words(runs, config) else {
        return;
    };
    for lane in letter_run_lane(LETTER_RUN_MIN) + 1..row.lengths.len() {
        let count = u64::from(row.lengths[lane]);
        let share = share_bp(count, runs);
        if count == 0 || share >= ceiling {
            continue;
        }
        if row.lengths[..lane]
            .iter()
            .any(|&shorter| u64::from(shorter) < u64::from(config.word_support_floor))
        {
            continue;
        }
        let key = PatternKey::LetterRun {
            length: LETTER_RUN_MIN + lane as u8,
        };
        out.push(Pattern {
            glyph: row.letter,
            channel: Channel::LetterRun,
            key,
            band: Some(band),
            numerator: saturate(count),
            denominator: saturate(runs),
            share_bp: reported_share(count, runs),
            books: word_books(corpus, row.letter, &key, &TerminalTable::default()),
        });
    }
}

/// Whether doubling is a slip in this corpus or a feature of the language.
///
/// The share is of the vocabulary, never a count: Jonah and a whole Bible must
/// answer the same way. `Always` and `Never` are the host's override, the same
/// shape [`LetterRoster`] has. A separated pair whose separator forces a
/// capital is a sentence boundary rather than a doubling, so it does not
/// count toward the recusal either — a language does not become "productive"
/// from `go. Go` and `Up! Up`.
pub(crate) fn judges_doubles(
    totals: &WordTotals,
    table: &TerminalTable,
    config: &JudgingConfig,
) -> bool {
    match config.doubles {
        DoublesPolicy::Always => true,
        DoublesPolicy::Never => false,
        DoublesPolicy::Auto => totals.doubling_share_bp(table) <= config.doubles_productive_bp,
    }
}

/// One row per case-folded word doubled under the word staircase, adjacent and
/// punctuation-separated kept apart.
///
/// The denominator is the word's own occurrences — every one the corpus
/// counted, forced or free, cased or not — so `vous vous` x300 against `vous`
/// x9,000 is 3.3% and silent, while `the the` once against `the` x60,000 is
/// 0.17 bp and fires. A word doubled every time it appears owns its whole
/// denominator and never fires.
///
/// The separated numerator sums only the glyphs `table` does NOT force: a
/// separator that forces a capital ends one sentence and starts the next, so
/// `go. Go` and `Up! Up` are never a doubling here — the pair is real to the
/// walk, which cannot read the table, and unreal to the judge, which can.
///
/// The two lanes are hash-sorted, so this is one tandem walk and not a probe
/// per doubled word.
pub(super) fn doubled(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    table: &TerminalTable,
    config: &JudgingConfig,
    moved: Option<&MovedWords>,
    out: &mut Vec<Pattern>,
) {
    let (doubles, rows) = (totals.doubles(), totals.rows());
    // One cursor per lane, both ascending: a word's doubling and its casing
    // rows are read together and neither cursor ever goes back.
    let mut cased = 0usize;
    match moved {
        None => {
            for row in doubles {
                let held = held_of(rows, &mut cased, row.hash);
                doubled_word(corpus, row, held, table, config, out);
            }
        }
        Some(moved) => {
            let mut at = 0usize;
            for hash in moved.words() {
                if !seek(doubles, &mut at, hash, |row| row.hash) {
                    continue;
                }
                let row = &doubles[at];
                at += 1;
                let held = held_of(rows, &mut cased, row.hash);
                doubled_word(corpus, row, held, table, config, out);
            }
        }
    }
}

/// The occurrences of one word the casing lane holds, from a cursor that only
/// moves forward. The two lanes partition a word's occurrences: a cased one is
/// in the casing lane, an uncased one is in the doubles row itself.
pub(super) fn held_of(rows: &[WordTally], at: &mut usize, hash: u64) -> u64 {
    while rows.get(*at).is_some_and(|row| row.hash < hash) {
        *at += 1;
    }
    let mut held = 0u64;
    let mut end = *at;
    while rows.get(end).is_some_and(|row| row.hash == hash) {
        held += rows[end]
            .counts
            .iter()
            .map(|&count| u64::from(count))
            .sum::<u64>();
        end += 1;
    }
    held
}

/// One word's two doubling lanes against its own occurrences.
pub(super) fn doubled_word(
    corpus: &[&WordAggregate],
    row: &DoubleTally,
    held: u64,
    table: &TerminalTable,
    config: &JudgingConfig,
    out: &mut Vec<Pattern>,
) {
    let free_separated = row.separated_free(table);
    if row.bare == 0 && free_separated == 0 {
        return;
    }
    let total = held + u64::from(row.uncased);
    let Some((band, ceiling)) = entitled_words(total, config) else {
        return;
    };
    for (separated, count) in [(false, u64::from(row.bare)), (true, free_separated)] {
        let share = share_bp(count, total);
        if count == 0 || share >= ceiling {
            continue;
        }
        let key = PatternKey::Doubled {
            hash: row.hash,
            separated,
        };
        out.push(Pattern {
            glyph: ScalarKey::NONE,
            channel: Channel::Doubled,
            key,
            band: Some(band),
            numerator: saturate(count),
            denominator: saturate(total),
            share_bp: reported_share(count, total),
            books: word_books(corpus, ScalarKey::NONE, &key, table),
        });
    }
}

/// The casing channel over a corpus tally: rows of one hash are contiguous, so
/// one walk sums the free lanes of every `Before` the word was seen under.
pub(super) fn casing(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    table: &TerminalTable,
    config: &JudgingConfig,
    moved: Option<&MovedWords>,
    out: &mut Vec<Pattern>,
) {
    match moved {
        None => {
            for word in totals.by_word() {
                casing_word(corpus, word, table, config, out);
            }
        }
        Some(moved) => {
            let rows = totals.rows();
            let mut at = 0usize;
            for hash in moved.words() {
                if !seek(rows, &mut at, hash, |row| row.hash) {
                    continue;
                }
                let mut end = at;
                while rows.get(end).is_some_and(|row| row.hash == *hash) {
                    end += 1;
                }
                casing_word(corpus, &rows[at..end], table, config, out);
                at = end;
            }
        }
    }
}

/// One word's casing rows, whose lanes this sums over whichever `Before`s the
/// terminal table left free.
pub(super) fn casing_word(
    corpus: &[&WordAggregate],
    word: &[WordTally],
    table: &TerminalTable,
    config: &JudgingConfig,
    out: &mut Vec<Pattern>,
) {
    let mut free = [0u64; 4];
    for row in word {
        if !row.before().is_free(table) {
            continue;
        }
        for (lane, count) in free.iter_mut().zip(row.counts) {
            *lane += u64::from(count);
        }
    }
    let total: u64 = free.iter().sum();
    let Some((band, ceiling)) = entitled_words(total, config) else {
        return;
    };
    for (lane, form) in Form::JUDGED.iter().enumerate() {
        let count = free[lane];
        let share = share_bp(count, total);
        if count == 0 || share >= ceiling {
            continue;
        }
        let key = PatternKey::Casing {
            hash: word[0].hash,
            form: *form,
        };
        out.push(Pattern {
            glyph: ScalarKey::NONE,
            channel: Channel::Casing,
            key,
            band: Some(band),
            numerator: saturate(count),
            denominator: saturate(total),
            share_bp: reported_share(count, total),
            books: word_books(corpus, ScalarKey::NONE, &key, table),
        });
    }
}

/// The corpus's own word length distribution, occurrence-weighted, and the
/// words standing `word_length_sigma` deviations above it.
///
/// Only the long end: a short word is a word, and the tail this names is
/// names, loanwords, and compounds — which is why the channel ships off.
pub(super) fn word_length(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    config: &JudgingConfig,
    out: &mut Vec<Pattern>,
) {
    let Some(shape) = LengthShape::of(corpus) else {
        return;
    };
    let ceiling = shape.at(config.word_length_sigma);
    let occurrences = saturate(shape.occurrences);
    let Some((band, _)) = config.word_bands.band_for(occurrences) else {
        return;
    };
    for word in totals.by_word() {
        let count: u64 = word.iter().flat_map(|row| row.counts).map(u64::from).sum();
        if count < u64::from(config.word_support_floor) {
            continue;
        }
        let Some(len) = shape.len_of(word[0].hash) else {
            continue;
        };
        if f64::from(len) < ceiling {
            continue;
        }
        let key = PatternKey::WordLength {
            hash: word[0].hash,
            sigma: shape.sigma(len),
        };
        out.push(Pattern {
            glyph: ScalarKey::NONE,
            channel: Channel::WordLength,
            key,
            band: Some(band),
            numerator: saturate(count),
            denominator: occurrences,
            share_bp: share_bp(count, u64::from(occurrences)),
            books: word_books(corpus, ScalarKey::NONE, &key, &TerminalTable::default()),
        });
    }
}

/// Mean and standard deviation of word length over every occurrence the
/// corpus counted, from the same `len` byte the row carries, plus the length
/// of each word so the sweep below reads it once instead of probing 66 books
/// per candidate.
pub(super) struct LengthShape {
    occurrences: u64,
    mean: f64,
    deviation: f64,
    lengths: FxHashMap<u64, u8>,
}

impl LengthShape {
    /// `None` when nothing was counted; a corpus with no cased word has no
    /// length distribution to judge against either.
    fn of(corpus: &[&WordAggregate]) -> Option<Self> {
        let (mut occurrences, mut sum, mut squares) = (0u64, 0f64, 0f64);
        let mut lengths: FxHashMap<u64, u8> = FxHashMap::default();
        for book in corpus {
            for row in book.words() {
                let count = f64::from(saturate(row.total()));
                let len = f64::from(row.len);
                occurrences += row.total();
                sum += len * count;
                squares += len * len * count;
                lengths.entry(row.hash).or_insert(row.len);
            }
        }
        if occurrences == 0 {
            return None;
        }
        let n = occurrences as f64;
        let mean = sum / n;
        Some(Self {
            occurrences,
            mean,
            deviation: (squares / n - mean * mean).max(0.0).sqrt(),
            lengths,
        })
    }

    /// The length `sigma` whole deviations above the mean.
    fn at(&self, sigma: u8) -> f64 {
        self.mean + f64::from(sigma) * self.deviation
    }

    /// Whole deviations above the mean, saturating; a corpus whose words are
    /// all one length has no spread and answers `u8::MAX`.
    fn sigma(&self, len: u8) -> u8 {
        if self.deviation <= 0.0 {
            return u8::MAX;
        }
        let over = (f64::from(len) - self.mean) / self.deviation;
        if over >= f64::from(u8::MAX) {
            u8::MAX
        } else {
            over as u8
        }
    }

    /// The scalar count the corpus recorded for one word.
    fn len_of(&self, hash: u64) -> Option<u8> {
        self.lengths.get(&hash).copied()
    }
}

/// Books whose own counts hold part of this word row's numerator, saturating.
///
/// Recomputed from the aggregates rather than carried through the tally: a
/// word row's numerator sums the `Before`s the table left free, and which
/// those are is a judging decision the config may move.
pub(super) fn word_books(
    corpus: &[&WordAggregate],
    glyph: ScalarKey,
    key: &PatternKey,
    table: &TerminalTable,
) -> u8 {
    let touched = corpus
        .iter()
        .filter(|book| free_of(book, glyph, key, table) > 0)
        .count();
    u8::try_from(touched).unwrap_or(u8::MAX)
}

/// One book's contribution to a word row's numerator. `glyph` is the row's
/// own, which only [`Channel::LetterRun`] reads: its key carries the length
/// and the letter stays where a glyph belongs.
pub(crate) fn free_of(
    book: &WordAggregate,
    glyph: ScalarKey,
    key: &PatternKey,
    table: &TerminalTable,
) -> u64 {
    match *key {
        PatternKey::Casing { hash, form } => book
            .rows_for(hash)
            .iter()
            .filter(|row| row.before().is_free(table))
            .map(|row| u64::from(row.count_of(form)))
            .sum(),
        PatternKey::WordLength { hash, .. } => {
            book.rows_for(hash).iter().map(|row| row.total()).sum()
        }
        PatternKey::Doubled { hash, separated } => book.doubles_for(hash).map_or(0, |row| {
            if separated {
                row.free_separated(table)
            } else {
                row.count_of(false)
            }
        }),
        PatternKey::LetterRun { length } => book
            .letter_runs_for(glyph)
            .map_or(0, |lanes| u64::from(lanes[letter_run_lane(length)])),
        _ => 0,
    }
}

/// A word's band, or `None` when its free positions are under the word
/// support floor and it abstains.
pub(super) fn entitled_words(free: u64, config: &JudgingConfig) -> Option<(u8, u16)> {
    if free < u64::from(config.word_support_floor) {
        return None;
    }
    config.word_bands.band_for(saturate(free))
}
