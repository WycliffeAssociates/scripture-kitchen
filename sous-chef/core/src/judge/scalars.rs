//! The substrate channels: what a corpus does with one scalar, judged against
//! what it does with the rest.
//!
//! ```text
//! ',' before a letter    12/9,812      12 bp  -> Placement
//! '?' before '.'          3/403        74 bp  -> ExactNeighbor
//! ',..,' against commas   1/601        16 bp  -> RunShape
//! '`' in 48,213 scalars   1/48,213            -> Rarity
//! ```
//!
//! Every row here is a share of a denominator the corpus itself supplies:
//! nothing is judged against an outside expectation.

use super::*;

// ── The judge ───────────────────────────────────────────────────────────

/// Every pattern the corpus's counts support, pushed in emission order.
pub(crate) fn judge_corpus(corpus: &[&BookAggregate], config: &JudgingConfig, out: &mut Findings) {
    let scalars = merged_scalars(corpus);
    let total_scalars: u64 = corpus.iter().map(|book| book.scalar_count()).sum();
    if config.channels.rarity {
        roster(&scalars, total_scalars, config, out);
    }

    let placements = placement_evidence(corpus);
    let shapes = run_evidence(corpus);
    let handoffs = follow_evidence(corpus);
    for (glyph, marginals) in &placements {
        let evidence = shapes.get(glyph);
        if config.channels.exact_neighbor
            && let Some(evidence) = evidence
        {
            neighbors(*glyph, evidence, config, out);
        }
        if config.channels.pooled_neighbor
            && let Some(evidence) = evidence
        {
            pooled_neighbors(*glyph, evidence, config, out);
        }
        if config.channels.run_shape
            && let Some(evidence) = evidence
        {
            run_shapes(*glyph, evidence, config, out);
        }
        if config.channels.placement {
            placement(*glyph, marginals, config, out);
        }
        if config.channels.sentence_start
            && let Some(handoffs) = handoffs.get(glyph)
        {
            sentence_start(*glyph, handoffs, config, out);
        }
    }
}

/// The mirror of the terminal table, on the same counts: a glyph this corpus
/// almost always capitalizes after, and the handoffs where it did not.
///
/// `upper / (upper + lower)` decides whether the glyph speaks at all; the row
/// then reports the lowercase handoffs against the cased ones, so the fraction
/// a reviewer reads is the exception's own. A glyph followed only by uncased
/// letters decides nothing, exactly as [`TerminalTable`] has it — the
/// denominator is the cased handoffs and not every handoff. The band is the
/// glyph staircase over that denominator and is cosmetic: the threshold above
/// is the whole firing rule.
pub(super) fn sentence_start(
    glyph: ScalarKey,
    handoffs: &FollowEvidence,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let upper = u64::from(handoffs.counts.get(Case::Upper));
    let lower = u64::from(handoffs.counts.get(Case::Lower));
    let cased = upper + lower;
    let Some((band, _)) = entitled(cased, config) else {
        return;
    };
    if lower == 0 || share_bp(upper, cased) < config.sentence_start_upper_bp {
        return;
    }
    out.push_pattern(Pattern {
        glyph,
        channel: Channel::SentenceStart,
        key: PatternKey::SentenceStart,
        band: Some(band),
        numerator: saturate(lower),
        denominator: saturate(cased),
        share_bp: reported_share(lower, cased),
        books: handoffs.lower.books(),
    });
}

/// Every scalar under `rarity_floor`, letters included when the corpus is
/// alphabetic enough for a letter roster to mean anything.
pub(super) fn roster(
    scalars: &[(ScalarKey, Tally)],
    total_scalars: u64,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let letters = letters_are_rostered(scalars, config);
    for &(glyph, tally) in scalars {
        if glyph.is_digits() || tally.count >= u64::from(config.rarity_floor) {
            continue;
        }
        if is_letter(glyph) && !letters {
            continue;
        }
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::Rarity,
            key: PatternKey::Rarity,
            band: None,
            numerator: saturate(tally.count),
            denominator: saturate(total_scalars),
            share_bp: reported_share(tally.count, total_scalars),
            books: tally.books(),
        });
    }
}

/// A logographic corpus has thousands of letters used once, and a ten-verse
/// draft has `q`, `x`, `z` used once by sample size; both abstain.
pub(super) fn letters_are_rostered(scalars: &[(ScalarKey, Tally)], config: &JudgingConfig) -> bool {
    match config.letters {
        LetterRoster::Always => true,
        LetterRoster::Never => false,
        LetterRoster::Auto => {
            let mut distinct = 0u32;
            let mut total = 0u64;
            for &(glyph, tally) in scalars {
                if is_letter(glyph) {
                    distinct += 1;
                    total += tally.count;
                }
            }
            distinct <= config.letter_roster_bound
                && total >= u64::from(config.letter_roster_min_letters)
        }
    }
}

/// G0: the outer class either side of one glyph, each side its own marginal
/// distribution over all of that glyph's occurrences.
///
/// `Edge` counts in the denominator and fires nothing: a book boundary is a
/// fact about the file, and the glyph is judged by its other side.
pub(super) fn placement(
    glyph: ScalarKey,
    marginals: &PlacementEvidence,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(marginals.denominator, config) else {
        return;
    };
    for side in Side::ALL {
        for class in OuterClass::ALL {
            if class == OuterClass::Edge {
                continue;
            }
            let tally = marginals.sides[side as usize][class as usize];
            let share = share_bp(tally.count, marginals.denominator);
            if tally.count == 0 || share >= ceiling {
                continue;
            }
            out.push_pattern(Pattern {
                glyph,
                channel: Channel::Placement,
                key: PatternKey::Placement { side, class },
                band: Some(band),
                numerator: saturate(tally.count),
                denominator: saturate(marginals.denominator),
                share_bp: reported_share(tally.count, marginals.denominator),
                books: tally.books(),
            });
        }
    }
}

/// G1: the shape of the runs one glyph appears in, against every run that
/// holds it.
pub(super) fn run_shapes(
    glyph: ScalarKey,
    evidence: &RunEvidence,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(evidence.runs, config) else {
        return;
    };
    for &((pure, bucket), tally) in &evidence.shapes {
        let share = share_bp(tally.count, evidence.runs);
        if share >= ceiling {
            continue;
        }
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::RunShape,
            key: PatternKey::RunShape { pure, bucket },
            band: Some(band),
            numerator: saturate(tally.count),
            denominator: saturate(evidence.runs),
            share_bp: reported_share(tally.count, evidence.runs),
            books: tally.books(),
        });
    }
}

/// G3: what follows the glyph inside a run, against every position where
/// something does.
pub(super) fn neighbors(
    glyph: ScalarKey,
    evidence: &RunEvidence,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(evidence.positions, config) else {
        return;
    };
    for &(neighbor, tally) in &evidence.neighbors {
        let share = share_bp(tally.count, evidence.positions);
        if share >= ceiling {
            continue;
        }
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::ExactNeighbor,
            key: PatternKey::ExactNeighbor(neighbor),
            band: Some(band),
            numerator: saturate(tally.count),
            denominator: saturate(evidence.positions),
            share_bp: reported_share(tally.count, evidence.positions),
            books: tally.books(),
        });
    }
}

/// G2: which pool follows the glyph inside a run, against the same positions
/// G3 counts — so a pair too thin to name exactly can still be named by kind.
pub(super) fn pooled_neighbors(
    glyph: ScalarKey,
    evidence: &RunEvidence,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(evidence.positions, config) else {
        return;
    };
    for &(pool, tally) in &evidence.pools {
        let share = share_bp(tally.count, evidence.positions);
        if share >= ceiling {
            continue;
        }
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::PooledNeighbor,
            key: PatternKey::PooledNeighbor(pool),
            band: Some(band),
            numerator: saturate(tally.count),
            denominator: saturate(evidence.positions),
            share_bp: reported_share(tally.count, evidence.positions),
            books: tally.books(),
        });
    }
}

/// A channel's band, or `None` when its denominator is under the support
/// floor and it abstains.
pub(super) fn entitled(denominator: u64, config: &JudgingConfig) -> Option<(u8, u16)> {
    if denominator < u64::from(config.support_floor) {
        return None;
    }
    config.bands.band_for(saturate(denominator))
}

// ── Dispersion ──────────────────────────────────────────────────────────

/// A numerator under construction: the running sum, and how many books have
/// contributed to it.
///
/// Books arrive in `BookIndex` order, so a distinct count needs the last
/// contributor and not a set.
#[derive(Clone, Copy, Default)]
pub(super) struct Tally {
    count: u64,
    books: u32,
    /// One past the last contributing book's index; 0 before the first.
    last: u32,
}

impl Tally {
    fn add(&mut self, count: u64, book: u32) {
        self.count += count;
        if self.last != book + 1 {
            self.books += 1;
            self.last = book + 1;
        }
    }

    /// Saturating: the wire lane is a `u8` and the canon is 66 books.
    const fn books(self) -> u8 {
        if self.books > u8::MAX as u32 {
            u8::MAX
        } else {
            self.books as u8
        }
    }
}

/// The pool of one run atom.
///
/// [`ScalarKey::DIGITS`] answers [`Pool::Digit`] for completeness: a digit
/// breaks a run and joins none, so it never reaches here as an atom.
pub(crate) fn pool_of_key(key: ScalarKey) -> Pool {
    match key.scalar() {
        Some(scalar) => pool_of(scalar),
        None => Pool::Digit,
    }
}

/// Books whose own counts hold part of `pattern`'s numerator, saturating at
/// 255 — the dispersion on the row, recomputed from the retained aggregates.
///
/// The judge counts this during the merge that produces the numerator; this
/// is the same number for any pattern a host holds, and the oracle that merge
/// is tested against. Books-possible is the publication's `book_count`.
pub fn books_touched(corpus: &[&BookAggregate], pattern: &Pattern) -> u8 {
    let touched = corpus
        .iter()
        .filter(|book| numerator_in(book, pattern) > 0)
        .count();
    u8::try_from(touched).unwrap_or(u8::MAX)
}

/// One book's own contribution to a pattern's numerator, in the unit that
/// channel counts.
pub(super) fn numerator_in(book: &BookAggregate, pattern: &Pattern) -> u64 {
    match pattern.key {
        PatternKey::Placement { side, class } => book
            .pairs()
            .iter()
            .filter(|(key, _)| key.scalar() == pattern.glyph)
            .filter(|(key, _)| {
                class
                    == match side {
                        Side::Prev => key.prev(),
                        Side::Next => key.next(),
                    }
            })
            .map(|(_, count)| u64::from(*count))
            .sum(),
        PatternKey::RunShape { pure, bucket } => book
            .runs()
            .filter(|(atoms, _)| atoms.contains(&pattern.glyph))
            .filter(|(atoms, _)| {
                (
                    atoms.iter().all(|atom| *atom == pattern.glyph),
                    atoms.len().min(RUN_BUCKETS) as u8,
                ) == (pure, bucket)
            })
            .map(|(_, count)| u64::from(count))
            .sum(),
        PatternKey::ExactNeighbor(neighbor) => book
            .runs()
            .map(|(atoms, count)| {
                let pairs = atoms
                    .windows(2)
                    .filter(|pair| pair[0] == pattern.glyph && pair[1] == neighbor)
                    .count() as u64;
                pairs * u64::from(count)
            })
            .sum(),
        PatternKey::PooledNeighbor(pool) => book
            .runs()
            .map(|(atoms, count)| {
                let pairs = atoms
                    .windows(2)
                    .filter(|pair| pair[0] == pattern.glyph && pool_of_key(pair[1]) == pool)
                    .count() as u64;
                pairs * u64::from(count)
            })
            .sum(),
        PatternKey::Rarity => book
            .scalars()
            .iter()
            .find(|(key, _)| *key == pattern.glyph)
            .map_or(0, |(_, count)| u64::from(*count)),
        PatternKey::SentenceStart => book
            .follows()
            .iter()
            .find(|(key, _)| *key == pattern.glyph)
            .map_or(0, |(_, counts)| u64::from(counts.get(Case::Lower))),
        // A word row is judged over word aggregates, which these are not;
        // `words::free_in` is its oracle.
        PatternKey::Casing { .. }
        | PatternKey::WordLength { .. }
        | PatternKey::Doubled { .. }
        | PatternKey::LetterRun { .. } => 0,
    }
}

// ── Corpus totals ───────────────────────────────────────────────────────

/// One glyph's G0 marginals: the denominator both sides share, and a tally
/// per side and outer class.
#[derive(Default)]
pub(super) struct PlacementEvidence {
    denominator: u64,
    sides: [[Tally; OuterClass::ALL.len()]; Side::ALL.len()],
}

/// One glyph's run history: which shapes hold it, and what follows it inside
/// them.
#[derive(Default)]
pub(super) struct RunEvidence {
    /// `((pure, length bucket), tally)`.
    shapes: Vec<((bool, u8), Tally)>,
    neighbors: Vec<(ScalarKey, Tally)>,
    pools: Vec<(Pool, Tally)>,
    /// Runs holding the glyph at all.
    runs: u64,
    /// Positions where the glyph is followed by another atom.
    positions: u64,
}

/// One glyph's handoffs: the merged counts, and the books holding part of the
/// lowercase lane, which is the numerator [`Channel::SentenceStart`] reports.
#[derive(Default)]
pub(super) struct FollowEvidence {
    counts: FollowCounts,
    lower: Tally,
}

/// Every book's follow lane into one glyph-keyed table.
///
/// [`merged_follows`] answers the same question without dispersion, and the
/// terminal table is all it needs; this one carries the `Tally` a row does.
pub(super) fn follow_evidence(corpus: &[&BookAggregate]) -> FxHashMap<ScalarKey, FollowEvidence> {
    let mut out: FxHashMap<ScalarKey, FollowEvidence> = FxHashMap::default();
    for (book, aggregate) in corpus.iter().enumerate() {
        let book = book as u32;
        for &(key, counts) in aggregate.follows() {
            let evidence = out.entry(key).or_default();
            evidence.counts.absorb(counts);
            let lower = u64::from(counts.get(Case::Lower));
            if lower > 0 {
                evidence.lower.add(lower, book);
            }
        }
    }
    out
}

/// Every book's pairs into one glyph-keyed table, ascending by glyph.
pub(super) fn placement_evidence(corpus: &[&BookAggregate]) -> Vec<(ScalarKey, PlacementEvidence)> {
    let mut out: FxHashMap<ScalarKey, PlacementEvidence> = FxHashMap::default();
    for (book, aggregate) in corpus.iter().enumerate() {
        let book = book as u32;
        for &(key, count) in aggregate.pairs() {
            let count = u64::from(count);
            let evidence = out.entry(key.scalar()).or_default();
            evidence.denominator += count;
            evidence.sides[Side::Prev as usize][key.prev() as usize].add(count, book);
            evidence.sides[Side::Next as usize][key.next() as usize].add(count, book);
        }
    }
    let mut rows: Vec<(ScalarKey, PlacementEvidence)> = out.into_iter().collect();
    rows.sort_unstable_by_key(|row| row.0);
    rows
}

/// Every run's contribution to every glyph it holds, book by book so the
/// numerators carry their dispersion.
pub(super) fn run_evidence(corpus: &[&BookAggregate]) -> FxHashMap<ScalarKey, RunEvidence> {
    let mut out: FxHashMap<ScalarKey, RunEvidence> = FxHashMap::default();
    let mut seen: Vec<ScalarKey> = Vec::new();
    for (book, aggregate) in corpus.iter().enumerate() {
        let book = book as u32;
        for (atoms, count) in aggregate.runs() {
            let count = u64::from(count);
            seen.clear();
            for atom in atoms {
                if seen.contains(atom) {
                    continue;
                }
                seen.push(*atom);
                let pure = atoms.iter().all(|other| other == atom);
                let bucket = atoms.len().min(RUN_BUCKETS) as u8;
                let evidence = out.entry(*atom).or_default();
                evidence.runs += count;
                bump(&mut evidence.shapes, (pure, bucket), count, book);
            }
            for pair in atoms.windows(2) {
                let evidence = out.entry(pair[0]).or_default();
                evidence.positions += count;
                bump(&mut evidence.neighbors, pair[1], count, book);
                bump(&mut evidence.pools, pool_of_key(pair[1]), count, book);
            }
        }
    }
    for evidence in out.values_mut() {
        evidence.shapes.sort_unstable_by_key(|entry| entry.0);
        evidence.neighbors.sort_unstable_by_key(|entry| entry.0);
        evidence.pools.sort_unstable_by_key(|entry| entry.0);
    }
    out
}

/// Linear: a glyph holds a handful of shapes and a handful of neighbors.
pub(super) fn bump<K: PartialEq>(counts: &mut Vec<(K, Tally)>, key: K, count: u64, book: u32) {
    match counts.iter_mut().find(|entry| entry.0 == key) {
        Some(entry) => entry.1.add(count, book),
        None => {
            let mut tally = Tally::default();
            tally.add(count, book);
            counts.push((key, tally));
        }
    }
}

/// The census, summed across books and carrying how many held each scalar.
///
/// Each book's lane is already sorted; sorting the concatenation by
/// `(key, book)` makes one pass enough and keeps the book order a `Tally`
/// needs.
pub(super) fn merged_scalars(corpus: &[&BookAggregate]) -> Vec<(ScalarKey, Tally)> {
    let mut rows: Vec<(ScalarKey, u32, u32)> = corpus
        .iter()
        .enumerate()
        .flat_map(|(book, aggregate)| {
            aggregate
                .scalars()
                .iter()
                .map(move |&(key, count)| (key, book as u32, count))
        })
        .collect();
    rows.sort_unstable();
    let mut out: Vec<(ScalarKey, Tally)> = Vec::with_capacity(rows.len());
    for (key, book, count) in rows {
        match out.last_mut() {
            Some(last) if last.0 == key => last.1.add(u64::from(count), book),
            _ => {
                let mut tally = Tally::default();
                tally.add(u64::from(count), book);
                out.push((key, tally));
            }
        }
    }
    out
}

pub(super) fn is_letter(glyph: ScalarKey) -> bool {
    glyph
        .scalar()
        .is_some_and(|scalar| class_of(scalar).is_alphabetic())
}

/// Shares are computed in `u64` so a corpus of ten million scalars cannot
/// wrap on the way to a wire width.
pub(crate) fn share_bp(numerator: u64, denominator: u64) -> u16 {
    if denominator == 0 {
        return 0;
    }
    (numerator.saturating_mul(10_000) / denominator).min(10_000) as u16
}

pub(super) fn saturate(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The share a row REPORTS, from the pair as the row carries it.
///
/// A count past `u32::MAX` clamps on the way to the wire, and
/// [`Pattern::validate`] recomputes the share from the clamped pair; the raw
/// share is still what decides whether the row fires at all.
pub(super) fn reported_share(numerator: u64, denominator: u64) -> u16 {
    share_bp(
        u64::from(saturate(numerator)),
        u64::from(saturate(denominator)),
    )
}
