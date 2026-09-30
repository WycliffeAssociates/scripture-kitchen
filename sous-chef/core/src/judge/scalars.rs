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

    let (explained, runs, shapes) = learned(corpus, config);
    let runs = if config.channels.run_shape {
        RunIndex::new(runs)
    } else {
        RunIndex::default()
    };
    let placements = placement_evidence(corpus, &explained);
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
/// `?"'"` x2 and `!"'"` x1 pool with it as `T"'"` x30, sentence ends as one
///   they recur too; `.'?"` x1 is another order and does not
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Explained {
    /// Glyphs whose ExactNeighbor is entitled, ascending.
    leaders: Vec<ScalarKey>,
    /// Exact runs that recur, ascending: the run itself, or the run with
    /// every sentence-ending mark read as one, occurs at least
    /// `support_floor` times.
    clusters: Vec<Box<[ScalarKey]>>,
}

impl Explained {
    /// Everything the corpus's counts explain, before any row is judged.
    pub fn learn(corpus: &[&BookAggregate], config: &JudgingConfig) -> Self {
        learned(corpus, config).0
    }

    /// Whether the in-run pairs `glyph` leads are ExactNeighbor's to judge.
    pub fn leads(&self, glyph: ScalarKey) -> bool {
        self.leaders.binary_search(&glyph).is_ok()
    }

    /// Whether the occurrence at `at` in one run meets its `side` neighbour
    /// in an in-run pair whose leader's ExactNeighbor is entitled: that pair
    /// is ExactNeighbor's, so no coarser row counts or sites it.
    pub fn claimed<T>(
        &self,
        run: &[T],
        at: usize,
        side: Side,
        key: impl Fn(&T) -> ScalarKey,
    ) -> bool {
        let leader = match side {
            Side::Prev => at.checked_sub(1),
            Side::Next => (at + 1 < run.len()).then_some(at),
            // Letters on both sides make a run of one: no in-run pair.
            Side::Both => None,
        };
        leader.is_some_and(|leader| self.leads(key(&run[leader])))
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

/// [`Explained`], with the merged runs and the run evidence it is learned
/// from, which the judge reads next.
///
/// Recurrence comes from the merged runs first, since each run's `novel` tally
/// needs it; the leaders then come from the evidence's own position counts.
fn learned<'a>(
    corpus: &[&'a BookAggregate],
    config: &JudgingConfig,
) -> (Explained, MergedRuns<'a>, FxHashMap<ScalarKey, RunEvidence>) {
    let runs = merged_runs(corpus);
    let floor = u64::from(config.support_floor);
    let mut pooled: FxHashMap<Box<[ScalarKey]>, u64> = FxHashMap::default();
    for &(atoms, count) in &runs.runs {
        if let Some(key) = terminals_pooled(atoms) {
            *pooled.entry(key).or_default() += count;
        }
    }
    let mut explained = Explained {
        leaders: Vec::new(),
        clusters: runs
            .runs
            .iter()
            .filter(|&&(atoms, count)| {
                count >= floor
                    || terminals_pooled(atoms).is_some_and(|key| pooled[&key] >= floor)
            })
            .map(|&(atoms, _)| atoms.into())
            .collect(),
    };
    let shapes = run_evidence(corpus, &explained);
    if config.channels.exact_neighbor {
        explained.leaders = shapes
            .iter()
            .filter(|(_, evidence)| entitled(evidence.positions, config).is_some())
            .map(|(glyph, _)| *glyph)
            .collect();
    }
    explained.seal();
    (explained, runs, shapes)
}

/// The run with every `Pool::Terminal` atom read as one placeholder, order
/// kept, or `None` for a run holding no terminal, which pools with nothing.
///
/// ```text
/// ."'"  ?"'"  !"'"   -> T"'"      one convention
/// .'?"               -> T'T"      its own
/// ;'                 -> None      `;` is a separator
/// ```
fn terminals_pooled(atoms: &[ScalarKey]) -> Option<Box<[ScalarKey]>> {
    let terminal =
        |atom: &ScalarKey| atom.scalar().is_some_and(|c| pool_of(c) == Pool::Terminal);
    atoms.iter().any(terminal).then(|| {
        atoms
            .iter()
            .map(|atom| if terminal(atom) { ScalarKey::NONE } else { *atom })
            .collect()
    })
}

/// Every exact run's corpus count, and the runs holding each atom, so a
/// RunShape row reads its own runs and not the corpus's.
#[derive(Default)]
pub(super) struct RunIndex<'a> {
    runs: MergedRuns<'a>,
    /// Positions into `runs`, ascending.
    holding: FxHashMap<ScalarKey, Vec<u32>>,
}

impl<'a> RunIndex<'a> {
    fn new(runs: MergedRuns<'a>) -> Self {
        let mut holding: FxHashMap<ScalarKey, Vec<u32>> = FxHashMap::default();
        for (at, (atoms, _)) in runs.runs.iter().enumerate() {
            for (position, atom) in atoms.iter().enumerate() {
                if !atoms[..position].contains(atom) {
                    holding.entry(*atom).or_default().push(at as u32);
                }
            }
        }
        Self { runs, holding }
    }

    /// The runs holding `glyph`, ascending by atoms.
    fn holding(&self, glyph: ScalarKey) -> impl Iterator<Item = (&'a [ScalarKey], u64)> + '_ {
        self.holding
            .get(&glyph)
            .into_iter()
            .flatten()
            .map(|&at| self.runs.runs[at as usize])
    }

    /// The facing most occurrences of this exact run have, `None` for a run
    /// holding no directionless quote.
    fn facing(&self, atoms: &[ScalarKey]) -> Option<Facing> {
        dominant(self.runs.facings.get(atoms)?)
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
        let among = |kin: fn(ScalarKey, ScalarKey) -> bool| {
            most(
                scalars
                    .iter()
                    .filter(|(other, _)| kin(glyph, *other))
                    .map(|(other, tally)| (*other, tally.count)),
            )
        };
        let (usual, lookalike) = match among(rarity_lookalike) {
            Some(found) => (Some(found), true),
            None => (among(rarity_kin), false),
        };
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
                lookalike,
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
///
/// Then both sides at once: the occurrences with a letter on each side, a
/// mark inside a word, against the same denominator. Each side alone can be
/// common where the pair is not; where a side's `Letter` row fired, that row
/// already holds every occurrence inside a word, and the joint row is not
/// pushed.
///
/// ```text
/// '"' prev=Letter, next=Letter   each common
/// '"' both=Letter      1/12,014   blasp"heming               -> FIRES
/// ''' both=Letter  2,904/6,289   don't, brother's            -> silent
/// ',' next=Letter fired           ff,gg is already its site  -> not pushed
/// ```
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
    // Every occurrence inside a word also has a letter on each side, so a
    // side whose `Letter` row fired already names all of them.
    let rare = |count: u64| share_bp(count, marginals.denominator) < ceiling;
    let letter = OuterClass::Letter as usize;
    let inside = marginals.inside;
    if inside.count > 0
        && rare(inside.count)
        && Side::ALL
            .iter()
            .all(|side| !rare(marginals.sides[*side as usize][letter].count))
    {
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::Placement,
            key: PatternKey::Placement {
                side: Side::Both,
                class: OuterClass::Letter,
            },
            band: Some(band),
            numerator: saturate(inside.count),
            denominator: saturate(marginals.denominator),
            share_bp: reported_share(inside.count, marginals.denominator),
            books: inside.books(),
            usual: Usual::None,
        });
    }
}

/// One book whose rate of a placement key breaks from the other books'.
///
/// ```text
/// nya ',' prev=Space   1SA 1,183/1,526 = 7,752 bp   the other 61 books' median 31 bp
///   1,526 >= book_rate_min_uses 100, 7,752 >= book_rate_min_bp 4,000,
///   and >= 10 x 31                                    -> FIRES for 1SA
/// ```
///
/// Judged books hold the glyph at least `support_floor` times, and the channel
/// needs [`BOOK_RATE_MIN_BOOKS`] of them. Only a book holding it
/// `book_rate_min_uses` times may fire. The baseline leaves the book under
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
    let others = judged.len() - 1;
    let mut rates: Vec<u16> = Vec::with_capacity(judged.len());
    let mut sorted: Vec<u16> = Vec::with_capacity(judged.len());
    for side in Side::ALL {
        for class in OuterClass::ALL {
            if class == OuterClass::Edge {
                continue;
            }
            let count = |book: &BookCounts| book.sides[side as usize][class as usize];
            rates.clear();
            rates.extend(
                judged
                    .iter()
                    .map(|book| share_bp(count(book), book.occurrences)),
            );
            sorted.clear();
            for (at, book) in judged.iter().enumerate() {
                let rate = rates[at];
                if book.occurrences < u64::from(config.book_rate_min_uses)
                    || count(book) < floor
                    || rate < config.book_rate_min_bp
                {
                    continue;
                }
                if sorted.is_empty() {
                    sorted.extend_from_slice(&rates);
                    sorted.sort_unstable();
                }
                let baseline = median_without(&sorted, rate);
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
                        books: others as u32,
                    },
                });
            }
        }
    }
}

/// The middle rate of `sorted` with one `rate` left out, the mean of the two
/// middle ones for an even count. Which equal rate leaves makes no difference,
/// so one sort serves every book under test.
///
/// ```text
/// sorted [10, 20, 30, 40, 50], without 30 -> [10, 20, 40, 50] -> 30
/// ```
fn median_without(sorted: &[u16], rate: u16) -> u16 {
    let gap = sorted.partition_point(|&other| other < rate);
    debug_assert_eq!(sorted.get(gap), Some(&rate), "the rate is one of them");
    let at = |index: usize| sorted[if index < gap { index } else { index + 1 }];
    let len = sorted.len() - 1;
    let half = len / 2;
    if len % 2 == 1 {
        at(half)
    } else {
        ((u32::from(at(half - 1)) + u32::from(at(half))) / 2) as u16
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
        Side::Both => {}
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
    runs: &RunIndex<'_>,
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
            runs.holding(glyph)
                .filter(|&(atoms, _)| {
                    explained.recurs(atoms.iter().copied())
                        && shape_of(atoms, glyph) == Some((pure, bucket))
                })
                .map(|(atoms, _)| Box::<[ScalarKey]>::from(atoms)),
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
        for cluster in clusters(glyph, (pure, bucket), runs, explained) {
            out.push_cluster(Cluster {
                pattern: index,
                ..cluster
            });
        }
    }
}

/// The exact runs of one shape holding `glyph`, most frequent first: at most
/// [`Cluster::PER_ROW`], the novel ones the row counts first, with
/// [`Cluster::RECURRING_SLOTS`] kept for the recurring ones it does not.
fn clusters(
    glyph: ScalarKey,
    shape: (bool, u8),
    runs: &RunIndex<'_>,
    explained: &Explained,
) -> Vec<Cluster> {
    let recurs = |atoms: &[ScalarKey]| explained.recurs(atoms.iter().copied());
    let (mut recurring, mut novel): (Vec<_>, Vec<_>) = runs
        .holding(glyph)
        .filter(|(atoms, _)| shape_of(atoms, glyph) == Some(shape))
        .partition(|(atoms, _)| recurs(atoms));
    let order =
        |a: &(&[ScalarKey], u64), b: &(&[ScalarKey], u64)| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0));
    recurring.sort_by(order);
    novel.sort_by(order);
    let kept = Cluster::PER_ROW - recurring.len().min(Cluster::RECURRING_SLOTS);
    let novel_taken = novel.len().min(kept);
    let recurring_taken = recurring.len().min(Cluster::PER_ROW - novel_taken);
    let mut chosen: Vec<(&[ScalarKey], u64)> = novel[..novel_taken]
        .iter()
        .chain(&recurring[..recurring_taken])
        .copied()
        .collect();
    chosen.sort_by(order);
    chosen
        .into_iter()
        .map(|(atoms, count)| Cluster {
            pattern: PatternIndex::new(0),
            atoms: atoms[..atoms.len().min(Cluster::ATOMS)].into(),
            count: saturate(count),
            recurring: recurs(atoms),
            truncated: atoms.len() > Cluster::ATOMS,
            facing: runs.facing(atoms),
        })
        .collect()
}

/// Every exact run's corpus count, whatever its facing.
#[derive(Default)]
pub(super) struct MergedRuns<'a> {
    /// Ascending by atoms.
    runs: Vec<(&'a [ScalarKey], u64)>,
    /// A run holding a directionless quote, counted per [`Facing::index`].
    facings: FxHashMap<&'a [ScalarKey], [u64; Facing::COUNT]>,
}

/// [`MergedRuns`], summed over the corpus.
pub(super) fn merged_runs<'a>(corpus: &[&'a BookAggregate]) -> MergedRuns<'a> {
    let mut counts: FxHashMap<&[ScalarKey], u64> = FxHashMap::default();
    let mut facings: FxHashMap<&[ScalarKey], [u64; Facing::COUNT]> = FxHashMap::default();
    for book in corpus {
        for (atoms, facing, count) in book.faced_runs() {
            *counts.entry(atoms).or_default() += u64::from(count);
            if let Some(facing) = facing {
                facings.entry(atoms).or_default()[facing.index()] += u64::from(count);
            }
        }
    }
    let mut runs: Vec<_> = counts.into_iter().collect();
    runs.sort_unstable();
    MergedRuns { runs, facings }
}

/// The facing with the most occurrences; `Unknown` when two facings tie for
/// it, since split evidence names no direction; `None` when nothing was
/// counted.
///
/// ```text
/// Closing 3                 -> Closing
/// Closing 1, Unknown 1      -> Unknown     JER 3:19 `".'` beside NUM 21:14 `"...`
/// ```
fn dominant(counts: &[u64; Facing::COUNT]) -> Option<Facing> {
    let (facing, count) = most(
        Facing::ALL
            .into_iter()
            .map(|facing| (facing, counts[facing.index()])),
    )?;
    if count == 0 {
        return None;
    }
    let tied = counts.iter().filter(|&&other| other == count).count() > 1;
    Some(if tied { Facing::Unknown } else { facing })
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
        let facing = evidence.facing(neighbor);
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
                reversed: saturate(followed(shapes, neighbor, glyph, facing)),
                facing,
            },
        });
    }
}

/// In-run positions where `glyph` is followed by `neighbor`, and only in runs
/// facing `facing` when the pair holds a directionless quote.
///
/// ```text
/// ''' then '.'    3, all Closing      facing Closing
///   '.' then '''  974 Closing         reversed 974
/// '"' then '.'    NUM 21:14 `"...`    facing Opening
///   '.' then '"'  4,038, all Closing  reversed 0
/// ```
fn followed(
    shapes: &FxHashMap<ScalarKey, RunEvidence>,
    glyph: ScalarKey,
    neighbor: ScalarKey,
    facing: Option<Facing>,
) -> u64 {
    let Some(evidence) = shapes.get(&glyph) else {
        return 0;
    };
    match facing {
        Some(facing) => evidence
            .faced
            .binary_search_by_key(&neighbor, |entry| entry.0)
            .map_or(0, |at| evidence.faced[at].1[facing.index()]),
        None => evidence
            .neighbors
            .binary_search_by_key(&neighbor, |entry| entry.0)
            .map_or(0, |at| evidence.neighbors[at].1.count),
    }
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

/// Whether `other` may stand as a rare `glyph`'s usual: another mark of the
/// same named pool. [`Pool::Other`] holds letters, spaces and unlisted marks
/// alike, so neither it nor a letter has a kin to name.
pub(crate) fn rarity_kin(glyph: ScalarKey, other: ScalarKey) -> bool {
    let pool = pool_of_key(glyph);
    other != glyph
        && !matches!(pool, Pool::Other | Pool::Digit)
        && pool_of_key(other) == pool
        && is_mark(glyph)
        && is_mark(other)
}

/// Whether `other` may stand as a rare `glyph`'s lookalike usual: another
/// mark `confusables.txt` draws like it, whatever its pool (`'` for `’`).
pub(crate) fn rarity_lookalike(glyph: ScalarKey, other: ScalarKey) -> bool {
    match (glyph.scalar(), other.scalar()) {
        (Some(a), Some(b)) => is_mark(glyph) && is_mark(other) && look_alike(a, b),
        _ => false,
    }
}

/// Neither a letter, a space, a digit, nor the wire's U+0000.
fn is_mark(key: ScalarKey) -> bool {
    key != ScalarKey::NONE
        && key.scalar().is_some_and(|scalar| {
            let class = class_of(scalar);
            !class.is_alphabetic() && !class.is_whitespace() && !class.is_decimal_digit()
        })
}

/// The outer class a pair records on one side of its glyph; `side` is one
/// of [`Side::ALL`].
pub(super) const fn class_on(key: PairKey, side: Side) -> OuterClass {
    match side {
        Side::Prev => key.prev(),
        Side::Next | Side::Both => key.next(),
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
            let occurrences = placed_in(book, pattern.glyph, side, class);
            if class != OuterClass::Nonletter {
                return occurrences;
            }
            let judged: u64 = book
                .runs()
                .map(|(atoms, count)| {
                    let claimed = (0..atoms.len())
                        .filter(|&at| atoms[at] == pattern.glyph)
                        .filter(|&at| explained.claimed(atoms, at, side, |atom| *atom))
                        .count() as u64;
                    claimed * u64::from(count)
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
        PatternKey::BookRate { side, class, .. } => placed_in(book, pattern.glyph, side, class),
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

/// One book's occurrences of `glyph` with `class` on `side`, what Placement
/// and BookRate both count before Placement's in-run subtraction.
fn placed_in(book: &BookAggregate, glyph: ScalarKey, side: Side, class: OuterClass) -> u64 {
    book.pairs()
        .iter()
        .filter(|(key, _)| key.scalar() == glyph && side.sees(*key, class))
        .map(|(_, count)| u64::from(*count))
        .sum()
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
    /// Occurrences with a letter on both sides.
    inside: Tally,
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
    /// The followers whose pair with the glyph holds a directionless quote,
    /// counted per [`Facing::index`] of the runs holding the pair.
    faced: Vec<(ScalarKey, [u64; Facing::COUNT])>,
    pools: Vec<(Pool, Tally)>,
    /// Runs holding the glyph at all.
    runs: u64,
    /// Positions where the glyph is followed by another atom.
    positions: u64,
}

impl RunEvidence {
    /// The facing most of the glyph's pairs with `neighbor` have, `None` when
    /// neither is a directionless quote.
    fn facing(&self, neighbor: ScalarKey) -> Option<Facing> {
        let at = self
            .faced
            .binary_search_by_key(&neighbor, |entry| entry.0)
            .ok()?;
        dominant(&self.faced[at].1)
    }
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
        for &(key, counts) in aggregate.follows().iter().filter(|(key, _)| key.is_bare()) {
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
            if Side::Both.sees(key, OuterClass::Letter) {
                evidence.inside.add(count, book);
            }
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
            for (at, glyph) in atoms.iter().enumerate() {
                for side in Side::ALL {
                    if !explained.claimed(atoms, at, side, |atom| *atom) {
                        continue;
                    }
                    let left =
                        &mut nonletter.get_mut(glyph).expect("a run atom has pairs")[side as usize];
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
        for (atoms, facing, count) in aggregate.faced_runs() {
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
                if let Some(facing) = facing
                    && (is_directionless(pair[0]) || is_directionless(pair[1]))
                {
                    let at = match evidence.faced.iter().position(|entry| entry.0 == pair[1]) {
                        Some(at) => at,
                        None => {
                            evidence.faced.push((pair[1], [0; Facing::COUNT]));
                            evidence.faced.len() - 1
                        }
                    };
                    evidence.faced[at].1[facing.index()] += count;
                }
            }
        }
    }
    for evidence in out.values_mut() {
        evidence.shapes.sort_unstable_by_key(|entry| entry.0);
        evidence.neighbors.sort_unstable_by_key(|entry| entry.0);
        evidence.faced.sort_unstable_by_key(|entry| entry.0);
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

#[cfg(test)]
mod median_tests {
    use super::median_without;

    /// The brute force it replaces: copy the others, sort, take the middle.
    fn leave_one_out(rates: &[u16], at: usize) -> u16 {
        let mut others: Vec<u16> = rates[..at].iter().chain(&rates[at + 1..]).copied().collect();
        others.sort_unstable();
        let half = others.len() / 2;
        if others.len() % 2 == 1 {
            others[half]
        } else {
            ((u32::from(others[half - 1]) + u32::from(others[half])) / 2) as u16
        }
    }

    #[test]
    fn one_sort_gives_every_leave_one_out_median() {
        let mut state = 0x9e37_79b9_u32;
        for len in 2..40 {
            for _ in 0..50 {
                let rates: Vec<u16> = (0..len)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        (state % 12) as u16 * 900
                    })
                    .collect();
                let mut sorted = rates.clone();
                sorted.sort_unstable();
                for (at, &rate) in rates.iter().enumerate() {
                    assert_eq!(median_without(&sorted, rate), leave_one_out(&rates, at));
                }
            }
        }
    }
}
