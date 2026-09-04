//! Corpus-level judging over the substrate's counts: one row per firing
//! pattern, no sites.
//!
//! ```text
//! judge([&MRK, &GEN], &JudgingConfig::default(), out)
//!   ',' occurs 9,812 times, 12 of them before a letter
//!     → Placement { side: Next, class: Letter }   12/9,812   12 bp   band 3
//!   '?' opens 403 in-run pairs, 3 of them before '.'
//!     → ExactNeighbor('.')                         3/403     74 bp   band 2
//!   ',..,' once against 600 lone commas
//!     → RunShape { pure: false, bucket: 4 }        1/601     16 bp   band 2
//!   '`' occurs once in 48,213 scalars
//!     → Rarity                                     1/48,213
//! ```
//!
//! Config reaches judging and nothing else, so a host re-judges without
//! remapping a chapter or refolding a book. The ladder, the entitlement rule,
//! and the emission order: judge.md.

use rustc_hash::FxHashMap;

use crate::pass::Findings;
use crate::substrate::{BookAggregate, OuterClass, PairKey, RUN_BUCKETS, ScalarKey};
use crate::unicode::class_of;

// ── The config ──────────────────────────────────────────────────────────

/// One rung of the fraction staircase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BandStep {
    /// The largest denominator this rung covers; the last rung is `u32::MAX`.
    pub up_to: u32,
    /// The minority share eligible for review, in basis points.
    pub share_bp: u16,
}

/// The fraction bands of `rules/character-inventory.md`, in basis points so
/// 0.3% is exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Staircase {
    /// `(denominator up to, share in basis points)`, ascending; the last
    /// entry's bound is `u32::MAX`.
    pub steps: [BandStep; 5],
}

impl Staircase {
    /// Rungs in the staircase; a band index names one of them.
    pub const STEPS: usize = 5;

    /// 25% up to 10, 10% up to 100, 3% up to 1,000, 1% up to 10,000, 0.3%
    /// above.
    pub const DEFAULT_STEPS: [BandStep; 5] = [
        BandStep {
            up_to: 10,
            share_bp: 2_500,
        },
        BandStep {
            up_to: 100,
            share_bp: 1_000,
        },
        BandStep {
            up_to: 1_000,
            share_bp: 300,
        },
        BandStep {
            up_to: 10_000,
            share_bp: 100,
        },
        BandStep {
            up_to: u32::MAX,
            share_bp: 30,
        },
    ];

    /// `None` unless the bounds ascend, the last is `u32::MAX`, and every
    /// share is a legal basis-point value.
    pub fn new(steps: [BandStep; 5]) -> Option<Self> {
        if steps.windows(2).any(|pair| pair[0].up_to >= pair[1].up_to) {
            return None;
        }
        if steps[4].up_to != u32::MAX || steps.iter().any(|step| step.share_bp > 10_000) {
            return None;
        }
        Some(Self { steps })
    }

    /// The rung a denominator falls in, as `(step index, share in bp)`.
    pub fn band_for(self, denominator: u32) -> Option<(u8, u16)> {
        if denominator == 0 {
            return None;
        }
        self.steps
            .iter()
            .position(|step| denominator <= step.up_to)
            .map(|at| (at as u8, self.steps[at].share_bp))
    }
}

impl Default for Staircase {
    fn default() -> Self {
        Self {
            steps: Self::DEFAULT_STEPS,
        }
    }
}

/// Whether letters join the rarity roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LetterRoster {
    /// Rostered unless the corpus is logographic or too small to judge.
    #[default]
    Auto,
    Always,
    Never,
}

/// Per-channel enable bits; all on by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channels {
    pub placement: bool,
    pub run_shape: bool,
    pub exact_neighbor: bool,
    pub rarity: bool,
}

impl Default for Channels {
    fn default() -> Self {
        Self {
            placement: true,
            run_shape: true,
            exact_neighbor: true,
            rarity: true,
        }
    }
}

/// Everything judging may vary. Plain fields: this is exported through
/// wasm-bindgen later and mirrored nowhere by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgingConfig {
    /// A channel whose denominator is below this abstains.
    pub support_floor: u32,
    pub bands: Staircase,
    /// A scalar seen fewer times than this across the corpus is rostered.
    pub rarity_floor: u32,
    /// Letters leave the roster when the corpus uses more distinct letters
    /// than this, unless `letters` overrides.
    pub letter_roster_bound: u32,
    /// Letters join the roster only when the corpus holds at least this many
    /// letters: in a ten-verse draft `q`, `x`, `z` are rare by sample size,
    /// not by convention. Nonletters have no such floor.
    pub letter_roster_min_letters: u32,
    pub letters: LetterRoster,
    pub channels: Channels,
}

impl Default for JudgingConfig {
    fn default() -> Self {
        Self {
            support_floor: 5,
            bands: Staircase::default(),
            rarity_floor: 5,
            letter_roster_bound: 500,
            letter_roster_min_letters: 5_000,
            letters: LetterRoster::default(),
            channels: Channels::default(),
        }
    }
}

// ── The pattern ─────────────────────────────────────────────────────────

/// One evidence channel. The discriminants run finest grain first, which is
/// the order a glyph's own rows are emitted in; `Rarity` rows come first of
/// all, ahead of every glyph (judge.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Channel {
    /// G3: the exact nonletter that follows a glyph inside its run.
    ExactNeighbor = 0,
    /// G2: reserved for D3's pooled neighbor categories; never emitted.
    PooledNeighbor = 1,
    /// G1: the shape of the runs a glyph appears in.
    RunShape = 2,
    /// G0: the outer class one side of a glyph.
    Placement = 3,
    /// The absolute-rarity roster, which is a list and not a claim.
    Rarity = 4,
}

impl Channel {
    pub const ALL: [Self; 5] = [
        Self::ExactNeighbor,
        Self::PooledNeighbor,
        Self::RunShape,
        Self::Placement,
        Self::Rarity,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::ExactNeighbor => "ExactNeighbor",
            Self::PooledNeighbor => "PooledNeighbor",
            Self::RunShape => "RunShape",
            Self::Placement => "Placement",
            Self::Rarity => "Rarity",
        }
    }
}

/// Which side of a glyph a placement distribution describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Side {
    Prev = 0,
    Next = 1,
}

impl Side {
    pub const ALL: [Self; 2] = [Self::Prev, Self::Next];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Prev => "prev",
            Self::Next => "next",
        }
    }
}

/// What a channel counted, one variant per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PatternKey {
    /// The scalar that follows the glyph inside a run.
    ExactNeighbor(ScalarKey),
    /// Whether every atom is the glyph, and the run's length bucket `1..=6`.
    RunShape {
        pure: bool,
        bucket: u8,
    },
    Placement {
        side: Side,
        class: OuterClass,
    },
    Rarity,
}

/// One firing pattern: a glyph, the channel that convicted it, and the
/// fraction behind the claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pattern {
    pub glyph: ScalarKey,
    pub channel: Channel,
    pub key: PatternKey,
    /// Staircase step index; `None` for `Rarity`, which has no band.
    pub band: Option<u8>,
    pub numerator: u32,
    pub denominator: u32,
    /// `numerator * 10_000 / denominator`, saturating.
    pub share_bp: u16,
}

impl Pattern {
    /// Refuses a row whose fields disagree with each other, returning the
    /// offending field's name. The wire's own byte-level checks (flags,
    /// reserved, key nibbles) stay in `decode_pattern`; this is what a typed
    /// `Pattern` can express and a corrupted round trip cannot fake.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.band.is_none() != (self.channel == Channel::Rarity) {
            return Err("band");
        }
        if self.channel == Channel::PooledNeighbor {
            return Err("channel");
        }
        if self.numerator > self.denominator {
            return Err("numerator");
        }
        if self.share_bp != share_bp(u64::from(self.numerator), u64::from(self.denominator)) {
            return Err("share_bp");
        }
        if let PatternKey::RunShape { bucket, .. } = self.key
            && !(1..=RUN_BUCKETS as u8).contains(&bucket)
        {
            return Err("key");
        }
        Ok(())
    }
}

/// A pattern's position in the publication's pattern table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PatternIndex(u16);

impl PatternIndex {
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

// ── The judge ───────────────────────────────────────────────────────────

/// Every pattern the corpus's counts support, pushed in emission order.
pub(crate) fn judge_corpus(corpus: &[&BookAggregate], config: &JudgingConfig, out: &mut Findings) {
    let scalars = merged_scalars(corpus);
    let total_scalars: u64 = corpus.iter().map(|book| book.scalar_count()).sum();
    if config.channels.rarity {
        roster(&scalars, total_scalars, config, out);
    }

    let pairs = merged_pairs(corpus);
    let runs = merged_runs(corpus);
    let shapes = run_evidence(&runs);
    for group in pairs.chunk_by(|a, b| a.0.scalar() == b.0.scalar()) {
        let glyph = group[0].0.scalar();
        let evidence = shapes.get(&glyph);
        if config.channels.exact_neighbor
            && let Some(evidence) = evidence
        {
            neighbors(glyph, evidence, config, out);
        }
        if config.channels.run_shape
            && let Some(evidence) = evidence
        {
            run_shapes(glyph, evidence, config, out);
        }
        if config.channels.placement {
            placement(glyph, group, config, out);
        }
    }
}

/// Every scalar under `rarity_floor`, letters included when the corpus is
/// alphabetic enough for a letter roster to mean anything.
fn roster(
    scalars: &[(ScalarKey, u64)],
    total_scalars: u64,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let letters = letters_are_rostered(scalars, config);
    for &(glyph, count) in scalars {
        if glyph.is_digits() || count >= u64::from(config.rarity_floor) {
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
            numerator: saturate(count),
            denominator: saturate(total_scalars),
            share_bp: share_bp(count, total_scalars),
        });
    }
}

/// A logographic corpus has thousands of letters used once, and a ten-verse
/// draft has `q`, `x`, `z` used once by sample size; both abstain.
fn letters_are_rostered(scalars: &[(ScalarKey, u64)], config: &JudgingConfig) -> bool {
    match config.letters {
        LetterRoster::Always => true,
        LetterRoster::Never => false,
        LetterRoster::Auto => {
            let mut distinct = 0u32;
            let mut total = 0u64;
            for &(glyph, count) in scalars {
                if is_letter(glyph) {
                    distinct += 1;
                    total += count;
                }
            }
            distinct <= config.letter_roster_bound
                && total >= u64::from(config.letter_roster_min_letters)
        }
    }
}

/// G0: the outer class either side of one glyph, each side its own marginal
/// distribution over all of that glyph's occurrences.
fn placement(
    glyph: ScalarKey,
    group: &[(PairKey, u64)],
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let denominator: u64 = group.iter().map(|entry| entry.1).sum();
    let Some((band, ceiling)) = entitled(denominator, config) else {
        return;
    };
    let mut sides = [[0u64; OuterClass::ALL.len()]; 2];
    for &(key, count) in group {
        sides[Side::Prev as usize][key.prev() as usize] += count;
        sides[Side::Next as usize][key.next() as usize] += count;
    }
    for side in Side::ALL {
        for class in OuterClass::ALL {
            let count = sides[side as usize][class as usize];
            let share = share_bp(count, denominator);
            if count == 0 || share >= ceiling {
                continue;
            }
            out.push_pattern(Pattern {
                glyph,
                channel: Channel::Placement,
                key: PatternKey::Placement { side, class },
                band: Some(band),
                numerator: saturate(count),
                denominator: saturate(denominator),
                share_bp: share,
            });
        }
    }
}

/// G1: the shape of the runs one glyph appears in, against every run that
/// holds it.
fn run_shapes(
    glyph: ScalarKey,
    evidence: &RunEvidence,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let Some((band, ceiling)) = entitled(evidence.runs, config) else {
        return;
    };
    for &((pure, bucket), count) in &evidence.shapes {
        let share = share_bp(count, evidence.runs);
        if share >= ceiling {
            continue;
        }
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::RunShape,
            key: PatternKey::RunShape { pure, bucket },
            band: Some(band),
            numerator: saturate(count),
            denominator: saturate(evidence.runs),
            share_bp: share,
        });
    }
}

/// G3: what follows the glyph inside a run, against every position where
/// something does.
fn neighbors(glyph: ScalarKey, evidence: &RunEvidence, config: &JudgingConfig, out: &mut Findings) {
    let Some((band, ceiling)) = entitled(evidence.positions, config) else {
        return;
    };
    for &(neighbor, count) in &evidence.neighbors {
        let share = share_bp(count, evidence.positions);
        if share >= ceiling {
            continue;
        }
        out.push_pattern(Pattern {
            glyph,
            channel: Channel::ExactNeighbor,
            key: PatternKey::ExactNeighbor(neighbor),
            band: Some(band),
            numerator: saturate(count),
            denominator: saturate(evidence.positions),
            share_bp: share,
        });
    }
}

/// A channel's band, or `None` when its denominator is under the support
/// floor and it abstains.
fn entitled(denominator: u64, config: &JudgingConfig) -> Option<(u8, u16)> {
    if denominator < u64::from(config.support_floor) {
        return None;
    }
    config.bands.band_for(saturate(denominator))
}

// ── Corpus totals ───────────────────────────────────────────────────────

/// One glyph's run history: which shapes hold it, and what follows it inside
/// them.
#[derive(Default)]
struct RunEvidence {
    /// `((pure, length bucket), count)`.
    shapes: Vec<((bool, u8), u64)>,
    neighbors: Vec<(ScalarKey, u64)>,
    /// Runs holding the glyph at all.
    runs: u64,
    /// Positions where the glyph is followed by another atom.
    positions: u64,
}

/// Every run's contribution to every glyph it holds, in one pass.
fn run_evidence(runs: &[(&[ScalarKey], u64)]) -> FxHashMap<ScalarKey, RunEvidence> {
    let mut out: FxHashMap<ScalarKey, RunEvidence> = FxHashMap::default();
    let mut seen: Vec<ScalarKey> = Vec::new();
    for &(atoms, count) in runs {
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
            bump(&mut evidence.shapes, (pure, bucket), count);
        }
        for pair in atoms.windows(2) {
            let evidence = out.entry(pair[0]).or_default();
            evidence.positions += count;
            bump(&mut evidence.neighbors, pair[1], count);
        }
    }
    for evidence in out.values_mut() {
        evidence.shapes.sort_unstable_by_key(|entry| entry.0);
        evidence.neighbors.sort_unstable_by_key(|entry| entry.0);
    }
    out
}

/// Linear: a glyph holds a handful of shapes and a handful of neighbors.
fn bump<K: PartialEq>(counts: &mut Vec<(K, u64)>, key: K, count: u64) {
    match counts.iter_mut().find(|entry| entry.0 == key) {
        Some(entry) => entry.1 += count,
        None => counts.push((key, count)),
    }
}

fn merged_scalars(corpus: &[&BookAggregate]) -> Vec<(ScalarKey, u64)> {
    coalesce(
        corpus
            .iter()
            .flat_map(|book| book.scalars().iter().map(|&(key, count)| (key, count)))
            .collect(),
    )
}

fn merged_pairs(corpus: &[&BookAggregate]) -> Vec<(PairKey, u64)> {
    coalesce(
        corpus
            .iter()
            .flat_map(|book| book.pairs().iter().map(|&(key, count)| (key, count)))
            .collect(),
    )
}

fn merged_runs<'a>(corpus: &[&'a BookAggregate]) -> Vec<(&'a [ScalarKey], u64)> {
    coalesce(
        corpus
            .iter()
            .flat_map(|book| book.runs().map(|(atoms, count)| (atoms, u64::from(count))))
            .collect(),
    )
}

/// Sorts and sums; each book's lane is already sorted, so this is a merge the
/// sort does for us over 66 short vectors.
fn coalesce<K: Ord + Copy, C: Into<u64> + Copy>(mut rows: Vec<(K, C)>) -> Vec<(K, u64)> {
    rows.sort_unstable_by_key(|row| row.0);
    let mut out: Vec<(K, u64)> = Vec::with_capacity(rows.len());
    for (key, count) in rows {
        match out.last_mut() {
            Some(last) if last.0 == key => last.1 += count.into(),
            _ => out.push((key, count.into())),
        }
    }
    out
}

fn is_letter(glyph: ScalarKey) -> bool {
    glyph
        .scalar()
        .is_some_and(|scalar| class_of(scalar).is_alphabetic())
}

/// Shares are computed in `u64` so a corpus of ten million scalars cannot
/// wrap on the way to a wire width.
fn share_bp(numerator: u64, denominator: u64) -> u16 {
    if denominator == 0 {
        return 0;
    }
    (numerator.saturating_mul(10_000) / denominator).min(10_000) as u16
}

fn saturate(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_staircase_is_the_rule_documents_table() {
        let bands = Staircase::default();
        assert_eq!(bands.band_for(0), None);
        assert_eq!(bands.band_for(1), Some((0, 2_500)));
        assert_eq!(bands.band_for(10), Some((0, 2_500)));
        assert_eq!(bands.band_for(11), Some((1, 1_000)));
        assert_eq!(bands.band_for(100), Some((1, 1_000)));
        assert_eq!(bands.band_for(1_000), Some((2, 300)));
        assert_eq!(bands.band_for(10_000), Some((3, 100)));
        assert_eq!(bands.band_for(10_001), Some((4, 30)));
        assert_eq!(bands.band_for(u32::MAX), Some((4, 30)));
    }

    #[test]
    fn a_staircase_whose_bounds_do_not_ascend_is_refused() {
        let mut steps = Staircase::DEFAULT_STEPS;
        steps.swap(1, 2);
        assert_eq!(Staircase::new(steps), None);
        let mut open = Staircase::DEFAULT_STEPS;
        open[4].up_to = 10_000_000;
        assert_eq!(Staircase::new(open), None);
        assert_eq!(
            Staircase::new(Staircase::DEFAULT_STEPS),
            Some(Staircase::default())
        );
    }

    /// A ten-million-count corpus stays inside its wire widths.
    #[test]
    fn a_ten_million_count_corpus_does_not_wrap() {
        assert_eq!(share_bp(10_000_000, 10_000_000), 10_000);
        assert_eq!(share_bp(1, 10_000_000), 0);
        assert_eq!(share_bp(3_000, 10_000_000), 3);
        assert_eq!(share_bp(u64::MAX, 1), 10_000);
        assert_eq!(saturate(u64::from(u32::MAX) + 1), u32::MAX);
        assert_eq!(saturate(10_000_000), 10_000_000);
    }
}
