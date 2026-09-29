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

    let explained = Explained::learn(corpus, config);
    let runs = if config.channels.run_shape {
        merged_runs(corpus)
    } else {
        Vec::new()
    };
    let placements = placement_evidence(corpus, &explained);
    let shapes = run_evidence(corpus, &explained);
    let handoffs = follow_evidence(corpus);
    let per_book = if config.channels.book_rate {
        book_evidence(corpus)
    } else {
        FxHashMap::default()
    };
    let other = OtherSide {
        explained: &explained,
        shapes: &shapes,
        numbers: placements.numbers,
    };
    let mut cited = Explained::default();
    for (glyph, marginals) in &placements.rows {
        let evidence = shapes.get(glyph);
        if config.channels.exact_neighbor
            && let Some(evidence) = evidence
        {
            neighbors(*glyph, evidence, &shapes, config, out);
        }
        if config.channels.pooled_neighbor
            && let Some(evidence) = evidence
        {
            pooled_neighbors(*glyph, evidence, config, out);
        }
        if config.channels.run_shape
            && let Some(evidence) = evidence
        {
            run_shapes(*glyph, evidence, &runs, &explained, config, &mut cited, out);
        }
        if config.channels.placement {
            placement(*glyph, marginals, &other, config, &mut cited, out);
        }
        if config.channels.sentence_start
            && let Some(handoffs) = handoffs.get(glyph)
        {
            sentence_start(*glyph, handoffs, config, out);
        }
        if let Some(books) = per_book.get(glyph) {
            book_rates(*glyph, books, config, out);
        }
    }
    cited.seal();
    out.set_explained(cited);
}

/// Occurrences a finer judgment already accounts for, so a coarser row
/// neither counts nor sites them.
///
/// ```text
/// ')' opens 900 in-run pairs, 30 of them before ','
///   ')' leads: its ExactNeighbor is entitled
///   ', prev=Nonletter' drops the 30 `),` from its numerator and its sites
/// `."'"` occurs 27 times, at least support_floor
///   it recurs: '" mixed len 4' drops those 27 runs and their sites
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Explained {
    /// Glyphs whose ExactNeighbor is entitled, ascending.
    leaders: Vec<ScalarKey>,
    /// Exact runs occurring at least `support_floor` times, ascending.
    clusters: Vec<Box<[ScalarKey]>>,
}

impl Explained {
    /// Everything the corpus's counts explain, before any row is judged.
    pub fn learn(corpus: &[&BookAggregate], config: &JudgingConfig) -> Self {
        let mut out = Self::default();
        if config.channels.exact_neighbor {
            let mut positions: FxHashMap<ScalarKey, u64> = FxHashMap::default();
            for book in corpus {
                for (atoms, count) in book.runs() {
                    for pair in atoms.windows(2) {
                        *positions.entry(pair[0]).or_default() += u64::from(count);
                    }
                }
            }
            out.leaders = positions
                .into_iter()
                .filter(|&(_, positions)| entitled(positions, config).is_some())
                .map(|(glyph, _)| glyph)
                .collect();
        }
        let mut runs: FxHashMap<&[ScalarKey], u64> = FxHashMap::default();
        for book in corpus {
            for (atoms, count) in book.runs() {
                *runs.entry(atoms).or_default() += u64::from(count);
            }
        }
        out.clusters = runs
            .into_iter()
            .filter(|&(_, count)| count >= u64::from(config.support_floor))
            .map(|(atoms, _)| atoms.into())
            .collect();
        out.seal();
        out
    }

    /// Whether the in-run pairs `glyph` leads are ExactNeighbor's to judge.
    pub fn leads(&self, glyph: ScalarKey) -> bool {
        self.leaders.binary_search(&glyph).is_ok()
    }

    /// Whether this exact run is a convention by recurrence.
    pub fn recurs(&self, atoms: impl Iterator<Item = ScalarKey> + Clone) -> bool {
        self.clusters
            .binary_search_by(|cluster| cluster.iter().copied().cmp(atoms.clone()))
            .is_ok()
    }

    pub fn leaders(&self) -> &[ScalarKey] {
        &self.leaders
    }

    pub fn clusters(&self) -> &[Box<[ScalarKey]>] {
        &self.clusters
    }

    fn seal(&mut self) {
        self.leaders.sort_unstable();
        self.leaders.dedup();
        self.clusters.sort_unstable();
        self.clusters.dedup();
    }
}

/// What a Placement row is judged against from the class it touches.
pub(super) struct OtherSide<'a> {
    explained: &'a Explained,
    shapes: &'a FxHashMap<ScalarKey, RunEvidence>,
    /// Digit occurrences that end a number, then that start one, by side.
    numbers: [u64; 2],
}

/// The mirror of the terminal table, on the same counts: a glyph this corpus
/// almost always capitalizes after, and the handoffs where it did not.
///
/// Only a glyph's bare handoffs: the row names a glyph and not a context, and
/// the quoted context mixes openings with closings (`?" he said`).
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
        usual: Usual::None,
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
        let pool = pool_of_key(glyph);
        let usual = most(
            scalars
                .iter()
                .filter(|(other, _)| ![glyph, ScalarKey::NONE, ScalarKey::DIGITS].contains(other))
                .filter(|(other, _)| pool_of_key(*other) == pool)
                .map(|(other, tally)| (*other, tally.count)),
        );
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::Rarity,
            key: PatternKey::Rarity,
            band: None,
            numerator: saturate(tally.count),
            denominator: saturate(total_scalars),
            share_bp: reported_share(tally.count, total_scalars),
            books: tally.books(),
            usual: Usual::Rarity {
                glyph: usual.map(|(other, _)| other),
                count: usual.map_or(0, |(_, count)| saturate(count)),
            },
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
///
/// A row unusual from the glyph's side is then judged from the class's:
/// a `Nonletter` row keeps only the occurrences no entitled leader judges,
/// and a `Digit` row ordinary among number ends (or starts) is silent.
pub(super) fn placement(
    glyph: ScalarKey,
    marginals: &PlacementEvidence,
    other: &OtherSide<'_>,
    config: &JudgingConfig,
    cited: &mut Explained,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(marginals.denominator, config) else {
        return;
    };
    for side in Side::ALL {
        let (usual, usual_count) = most(
            OuterClass::ALL
                .into_iter()
                .filter(|class| *class != OuterClass::Edge)
                .map(|class| (class, marginals.sides[side as usize][class as usize].count)),
        )
        .expect("four classes are not Edge");
        for class in OuterClass::ALL {
            if class == OuterClass::Edge {
                continue;
            }
            let tally = marginals.sides[side as usize][class as usize];
            let share = share_bp(tally.count, marginals.denominator);
            if tally.count == 0 || share >= ceiling {
                continue;
            }
            let tally = match class {
                OuterClass::Nonletter => marginals.unexplained[side as usize],
                OuterClass::Digit
                    if ordinary(tally.count, other.numbers[side as usize], config) =>
                {
                    continue;
                }
                _ => tally,
            };
            if tally.count == 0 {
                continue;
            }
            if class == OuterClass::Nonletter {
                cite_leaders(glyph, side, other, cited);
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
                usual: Usual::Placement {
                    class: usual,
                    count: saturate(usual_count),
                },
            });
        }
    }
}

/// One book whose rate of a placement key breaks from the other books'.
///
/// ```text
/// nya ',' prev=Space   1SA 1,183/1,526 = 7,752 bp   the other 42 books' median 30 bp
///   7,752 >= book_rate_min_bp 1,000 and >= 10 x 30   -> FIRES for 1SA
/// ```
///
/// Judged books hold the glyph at least `support_floor` times, and the channel
/// needs [`BOOK_RATE_MIN_BOOKS`] of them. The baseline leaves the book under
/// test out, so a dominant book cannot pull it toward itself.
pub(super) fn book_rates(
    glyph: ScalarKey,
    books: &[BookCounts],
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let floor = u64::from(config.support_floor);
    let judged: Vec<&BookCounts> = books
        .iter()
        .filter(|book| book.occurrences >= floor)
        .collect();
    if judged.len() < BOOK_RATE_MIN_BOOKS as usize {
        return;
    }
    let mut others: Vec<u16> = Vec::with_capacity(judged.len() - 1);
    for side in Side::ALL {
        for class in OuterClass::ALL {
            if class == OuterClass::Edge {
                continue;
            }
            let count = |book: &BookCounts| book.sides[side as usize][class as usize];
            let rates: Vec<u16> = judged
                .iter()
                .map(|book| share_bp(count(book), book.occurrences))
                .collect();
            for (at, book) in judged.iter().enumerate() {
                let rate = rates[at];
                if count(book) < floor || rate < config.book_rate_min_bp {
                    continue;
                }
                others.clear();
                others.extend(rates[..at].iter().chain(&rates[at + 1..]));
                let baseline = median(&mut others);
                if u32::from(rate) < u32::from(config.book_rate_ratio) * u32::from(baseline.max(1))
                {
                    continue;
                }
                out.push_pattern(Pattern {
                    glyph,
                    channel: Channel::BookRate,
                    key: PatternKey::BookRate {
                        side,
                        class,
                        book: BookIndex::new(book.book as usize)
                            .expect("a corpus indexes every book"),
                    },
                    band: None,
                    numerator: saturate(count(book)),
                    denominator: saturate(book.occurrences),
                    share_bp: reported_share(count(book), book.occurrences),
                    books: 1,
                    usual: Usual::BookRate {
                        baseline_bp: baseline,
                        books: others.len() as u32,
                    },
                });
            }
        }
    }
}

/// The middle rate, the mean of the two middle ones for an even count.
fn median(rates: &mut [u16]) -> u16 {
    rates.sort_unstable();
    let half = rates.len() / 2;
    if rates.len() % 2 == 1 {
        rates[half]
    } else {
        ((u32::from(rates[half - 1]) + u32::from(rates[half])) / 2) as u16
    }
}

/// `(pure, length bucket)` of a run holding `glyph`, or `None` without it.
pub(crate) fn shape_of(atoms: &[ScalarKey], glyph: ScalarKey) -> Option<(bool, u8)> {
    atoms.contains(&glyph).then(|| {
        (
            atoms.iter().all(|atom| *atom == glyph),
            atoms.len().min(RUN_BUCKETS) as u8,
        )
    })
}

/// Records the leaders a firing `Nonletter` row's sites skip.
fn cite_leaders(glyph: ScalarKey, side: Side, other: &OtherSide<'_>, cited: &mut Explained) {
    match side {
        Side::Prev => cited.leaders.extend(
            other
                .shapes
                .iter()
                .filter(|(leader, _)| other.explained.leads(**leader))
                .filter(|(_, evidence)| {
                    evidence
                        .neighbors
                        .binary_search_by_key(&glyph, |entry| entry.0)
                        .is_ok()
                })
                .map(|(leader, _)| *leader),
        ),
        Side::Next => {
            if other.explained.leads(glyph) {
                cited.leaders.push(glyph);
            }
        }
    }
}

/// Whether `numerator / denominator` is at or over the band ceiling, on a
/// denominator entitled to say so.
pub(super) fn ordinary(numerator: u64, denominator: u64, config: &JudgingConfig) -> bool {
    entitled(denominator, config)
        .is_some_and(|(_, ceiling)| share_bp(numerator, denominator) >= ceiling)
}

/// G1: the shape of the runs one glyph appears in, against every run that
/// holds it.
///
/// A row unusual by shape keeps only its runs that do not recur exactly, and
/// lists its clusters beside it.
pub(super) fn run_shapes(
    glyph: ScalarKey,
    evidence: &RunEvidence,
    runs: &[(&[ScalarKey], u64)],
    explained: &Explained,
    config: &JudgingConfig,
    cited: &mut Explained,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(evidence.runs, config) else {
        return;
    };
    let ((usual_pure, usual_bucket), usual_count) = most(
        evidence
            .shapes
            .iter()
            .map(|&(shape, tally)| (shape, tally.count)),
    )
    .expect("an entitled glyph sits in a run");
    for &((pure, bucket), tally) in &evidence.shapes {
        let share = share_bp(tally.count, evidence.runs);
        if share >= ceiling {
            continue;
        }
        let Some(&(_, tally)) = evidence
            .novel
            .iter()
            .find(|entry| entry.0 == (pure, bucket))
        else {
            continue;
        };
        cited.clusters.extend(
            explained
                .clusters
                .iter()
                .filter(|atoms| shape_of(atoms, glyph) == Some((pure, bucket)))
                .cloned(),
        );
        let index = out.push_pattern(Pattern {
            glyph,
            channel: Channel::RunShape,
            key: PatternKey::RunShape { pure, bucket },
            band: Some(band),
            numerator: saturate(tally.count),
            denominator: saturate(evidence.runs),
            share_bp: reported_share(tally.count, evidence.runs),
            books: tally.books(),
            usual: Usual::RunShape {
                pure: usual_pure,
                bucket: usual_bucket,
                count: saturate(usual_count),
            },
        });
        for cluster in clusters(glyph, (pure, bucket), runs, config) {
            out.push_cluster(Cluster {
                pattern: index,
                ..cluster
            });
        }
    }
}

/// The exact runs of one shape holding `glyph`, most frequent first: the
/// novel ones the row counts, then the recurring ones it does not, at most
/// [`Cluster::PER_ROW`] and never fewer recurring than
/// [`Cluster::RECURRING_SLOTS`] while any are left.
fn clusters(
    glyph: ScalarKey,
    shape: (bool, u8),
    runs: &[(&[ScalarKey], u64)],
    config: &JudgingConfig,
) -> Vec<Cluster> {
    let (mut recurring, mut novel): (Vec<_>, Vec<_>) = runs
        .iter()
        .filter(|(atoms, _)| shape_of(atoms, glyph) == Some(shape))
        .partition(|(_, count)| *count >= u64::from(config.support_floor));
    let order = |a: &&(&[ScalarKey], u64), b: &&(&[ScalarKey], u64)| {
        b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0))
    };
    recurring.sort_by(order);
    novel.sort_by(order);
    let kept = Cluster::PER_ROW - recurring.len().min(Cluster::RECURRING_SLOTS);
    let novel_taken = novel.len().min(kept);
    let recurring_taken = recurring.len().min(Cluster::PER_ROW - novel_taken);
    let mut chosen: Vec<&(&[ScalarKey], u64)> = novel[..novel_taken]
        .iter()
        .chain(&recurring[..recurring_taken])
        .copied()
        .collect();
    chosen.sort_by(order);
    chosen
        .into_iter()
        .map(|&(atoms, count)| Cluster {
            pattern: PatternIndex::new(0),
            atoms: atoms[..atoms.len().min(Cluster::ATOMS)].into(),
            count: saturate(count),
            recurring: count >= u64::from(config.support_floor),
            truncated: atoms.len() > Cluster::ATOMS,
        })
        .collect()
}

/// Every exact run's corpus count, ascending by atoms.
pub(super) fn merged_runs<'a>(corpus: &[&'a BookAggregate]) -> Vec<(&'a [ScalarKey], u64)> {
    let mut counts: FxHashMap<&[ScalarKey], u64> = FxHashMap::default();
    for book in corpus {
        for (atoms, count) in book.runs() {
            *counts.entry(atoms).or_default() += u64::from(count);
        }
    }
    let mut out: Vec<_> = counts.into_iter().collect();
    out.sort_unstable();
    out
}

/// G3: what follows the glyph inside a run, against every position where
/// something does.
pub(super) fn neighbors(
    glyph: ScalarKey,
    evidence: &RunEvidence,
    shapes: &FxHashMap<ScalarKey, RunEvidence>,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(evidence.positions, config) else {
        return;
    };
    let (usual, usual_count) = most(
        evidence
            .neighbors
            .iter()
            .map(|&(neighbor, tally)| (neighbor, tally.count)),
    )
    .expect("an entitled glyph is followed in a run");
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
            usual: Usual::ExactNeighbor {
                neighbor: usual,
                count: saturate(usual_count),
                reversed: saturate(followed(shapes, neighbor, glyph)),
            },
        });
    }
}

/// In-run positions where `glyph` is followed by `neighbor`.
fn followed(
    shapes: &FxHashMap<ScalarKey, RunEvidence>,
    glyph: ScalarKey,
    neighbor: ScalarKey,
) -> u64 {
    shapes
        .get(&glyph)
        .and_then(|evidence| {
            evidence
                .neighbors
                .binary_search_by_key(&neighbor, |entry| entry.0)
                .ok()
                .map(|at| evidence.neighbors[at].1.count)
        })
        .unwrap_or(0)
}

/// The key with the largest count, the smallest key on a tie.
pub(super) fn most<K: Copy + Ord>(counts: impl Iterator<Item = (K, u64)>) -> Option<(K, u64)> {
    counts.fold(None, |best, (key, count)| match best {
        Some((held, most)) if most > count || (most == count && held < key) => best,
        _ => Some((key, count)),
    })
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
            usual: Usual::None,
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

/// The outer class a pair records on one side of its glyph.
pub(super) const fn class_on(key: PairKey, side: Side) -> OuterClass {
    match side {
        Side::Prev => key.prev(),
        Side::Next => key.next(),
    }
}

/// Books whose own counts hold part of `pattern`'s numerator, saturating at
/// 255 — the dispersion on the row, recomputed from the retained aggregates.
///
/// The judge counts this during the merge that produces the numerator; this
/// is the same number for any pattern a host holds, and the oracle that merge
/// is tested against. Books-possible is the publication's `book_count`.
pub fn books_touched(corpus: &[&BookAggregate], pattern: &Pattern, config: &JudgingConfig) -> u8 {
    let explained = Explained::learn(corpus, config);
    let touched = corpus
        .iter()
        .enumerate()
        .filter(|(index, _)| match pattern.key {
            PatternKey::BookRate { book, .. } => usize::from(book.get()) == *index,
            _ => true,
        })
        .filter(|(_, book)| numerator_in(book, pattern, &explained) > 0)
        .count();
    u8::try_from(touched).unwrap_or(u8::MAX)
}

/// One book's own contribution to a pattern's numerator, in the unit that
/// channel counts.
pub(super) fn numerator_in(book: &BookAggregate, pattern: &Pattern, explained: &Explained) -> u64 {
    match pattern.key {
        PatternKey::Placement { side, class } => {
            let occurrences: u64 = book
                .pairs()
                .iter()
                .filter(|(key, _)| key.scalar() == pattern.glyph)
                .filter(|(key, _)| class == class_on(*key, side))
                .map(|(_, count)| u64::from(*count))
                .sum();
            if class != OuterClass::Nonletter {
                return occurrences;
            }
            let judged: u64 = book
                .runs()
                .map(|(atoms, count)| {
                    let pairs = atoms
                        .windows(2)
                        .filter(|pair| explained.leads(pair[0]))
                        .filter(|pair| {
                            pattern.glyph
                                == match side {
                                    Side::Prev => pair[1],
                                    Side::Next => pair[0],
                                }
                        })
                        .count() as u64;
                    pairs * u64::from(count)
                })
                .sum();
            occurrences - judged
        }
        PatternKey::RunShape { pure, bucket } => book
            .runs()
            .filter(|(atoms, _)| shape_of(atoms, pattern.glyph) == Some((pure, bucket)))
            .filter(|(atoms, _)| !explained.recurs(atoms.iter().copied()))
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
        // The row's own book only; [`books_touched`] reads the index.
        PatternKey::BookRate { side, class, .. } => book
            .pairs()
            .iter()
            .filter(|(key, _)| key.scalar() == pattern.glyph && class == class_on(*key, side))
            .map(|(_, count)| u64::from(*count))
            .sum(),
        PatternKey::SentenceStart => book
            .follows()
            .iter()
            .find(|(key, _)| *key == FollowKey::new(pattern.glyph, false))
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
    /// The `Nonletter` tally per side, less the pairs an entitled leader judges.
    unexplained: [Tally; Side::ALL.len()],
}

/// Every glyph's marginals, and the digit edges a `Digit` row is judged against.
pub(super) struct PlacementTable {
    /// Ascending by glyph.
    rows: Vec<(ScalarKey, PlacementEvidence)>,
    /// Digit occurrences that end a number, then that start one, by side.
    numbers: [u64; Side::ALL.len()],
}

/// One glyph's run history: which shapes hold it, and what follows it inside
/// them.
#[derive(Default)]
pub(super) struct RunEvidence {
    /// `((pure, length bucket), tally)`.
    shapes: Vec<((bool, u8), Tally)>,
    /// The same, over runs whose exact sequence does not recur.
    novel: Vec<((bool, u8), Tally)>,
    neighbors: Vec<(ScalarKey, Tally)>,
    pools: Vec<(Pool, Tally)>,
    /// Runs holding the glyph at all.
    runs: u64,
    /// Positions where the glyph is followed by another atom.
    positions: u64,
}

/// One glyph's placement counts in one book, what [`book_rates`] compares.
pub(super) struct BookCounts {
    book: u32,
    occurrences: u64,
    sides: [[u64; OuterClass::ALL.len()]; Side::ALL.len()],
}

/// Every glyph's per-book placement counts, books ascending.
pub(super) fn book_evidence(corpus: &[&BookAggregate]) -> FxHashMap<ScalarKey, Vec<BookCounts>> {
    let mut out: FxHashMap<ScalarKey, Vec<BookCounts>> = FxHashMap::default();
    for (book, aggregate) in corpus.iter().enumerate() {
        let book = book as u32;
        for &(key, count) in aggregate.pairs() {
            let books = out.entry(key.scalar()).or_default();
            if books.last().is_none_or(|last| last.book != book) {
                books.push(BookCounts {
                    book,
                    occurrences: 0,
                    sides: Default::default(),
                });
            }
            let counts = books.last_mut().expect("just pushed");
            counts.occurrences += u64::from(count);
            for side in Side::ALL {
                counts.sides[side as usize][class_on(key, side) as usize] += u64::from(count);
            }
        }
    }
    out
}

/// One glyph's handoffs: the merged counts, and the books holding part of the
/// lowercase lane, which is the numerator [`Channel::SentenceStart`] reports.
#[derive(Default)]
pub(super) struct FollowEvidence {
    counts: FollowCounts,
    lower: Tally,
}

/// Every book's bare handoffs into one glyph-keyed table.
///
/// [`merged_follows`] answers the same question without dispersion, and the
/// terminal table is all it needs; this one carries the `Tally` a row does.
pub(super) fn follow_evidence(corpus: &[&BookAggregate]) -> FxHashMap<ScalarKey, FollowEvidence> {
    let mut out: FxHashMap<ScalarKey, FollowEvidence> = FxHashMap::default();
    for (book, aggregate) in corpus.iter().enumerate() {
        let book = book as u32;
        for &(key, counts) in aggregate.follows().iter().filter(|(key, _)| !key.quoted()) {
            let evidence = out.entry(key.glyph()).or_default();
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
///
/// An in-run pair is a `Nonletter` pair on both members, so the subtraction
/// never goes below zero.
pub(super) fn placement_evidence(
    corpus: &[&BookAggregate],
    explained: &Explained,
) -> PlacementTable {
    let mut out: FxHashMap<ScalarKey, PlacementEvidence> = FxHashMap::default();
    let mut numbers = [0u64; Side::ALL.len()];
    let mut nonletter: FxHashMap<ScalarKey, [u64; Side::ALL.len()]> = FxHashMap::default();
    for (book, aggregate) in corpus.iter().enumerate() {
        let book = book as u32;
        nonletter.clear();
        for &(key, count) in aggregate.pairs() {
            let count = u64::from(count);
            let evidence = out.entry(key.scalar()).or_default();
            evidence.denominator += count;
            for side in Side::ALL {
                let class = class_on(key, side);
                evidence.sides[side as usize][class as usize].add(count, book);
                if class == OuterClass::Nonletter {
                    nonletter.entry(key.scalar()).or_default()[side as usize] += count;
                }
            }
            if key.scalar().is_digits() {
                if key.next() != OuterClass::Digit {
                    numbers[Side::Prev as usize] += count;
                }
                if key.prev() != OuterClass::Digit {
                    numbers[Side::Next as usize] += count;
                }
            }
        }
        for (atoms, count) in aggregate.runs() {
            let count = u64::from(count);
            for pair in atoms.windows(2).filter(|pair| explained.leads(pair[0])) {
                for (glyph, side) in [(pair[1], Side::Prev), (pair[0], Side::Next)] {
                    let left = &mut nonletter.get_mut(&glyph).expect("a run atom has pairs")
                        [side as usize];
                    debug_assert!(*left >= count, "an in-run pair is a Nonletter pair");
                    *left -= count;
                }
            }
        }
        for (glyph, counts) in &nonletter {
            let evidence = out.get_mut(glyph).expect("counted above");
            for side in Side::ALL {
                if counts[side as usize] > 0 {
                    evidence.unexplained[side as usize].add(counts[side as usize], book);
                }
            }
        }
    }
    let mut rows: Vec<(ScalarKey, PlacementEvidence)> = out.into_iter().collect();
    rows.sort_unstable_by_key(|row| row.0);
    PlacementTable { rows, numbers }
}

/// Every run's contribution to every glyph it holds, book by book so the
/// numerators carry their dispersion.
pub(super) fn run_evidence(
    corpus: &[&BookAggregate],
    explained: &Explained,
) -> FxHashMap<ScalarKey, RunEvidence> {
    let mut out: FxHashMap<ScalarKey, RunEvidence> = FxHashMap::default();
    let mut seen: Vec<ScalarKey> = Vec::new();
    for (book, aggregate) in corpus.iter().enumerate() {
        let book = book as u32;
        for (atoms, count) in aggregate.runs() {
            let count = u64::from(count);
            let recurs = explained.recurs(atoms.iter().copied());
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
                if !recurs {
                    bump(&mut evidence.novel, (pure, bucket), count, book);
                }
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
