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
use crate::substrate::{BookAggregate, Case, FollowCounts, OuterClass, RUN_BUCKETS, ScalarKey};
use crate::unicode::{Pool, class_of, pool_of};
use crate::words::{Form, WordAggregate, WordTotals};

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

    /// The glyph staircase at a tenth of its shares, which is what the fleet
    /// sweep put word casing at the glyph channels' own volume: p50 11 rows
    /// per corpus against their p50 10 (evidence.md, W3).
    pub const WORD_STEPS: [BandStep; 5] = [
        BandStep {
            up_to: 10,
            share_bp: 250,
        },
        BandStep {
            up_to: 100,
            share_bp: 100,
        },
        BandStep {
            up_to: 1_000,
            share_bp: 30,
        },
        BandStep {
            up_to: 10_000,
            share_bp: 10,
        },
        BandStep {
            up_to: u32::MAX,
            share_bp: 3,
        },
    ];

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

/// Whether the doubled-word channel judges this corpus.
///
/// The same shape as [`LetterRoster`], and for the same reason: `Auto` is a
/// measurement about the corpus — how much of its vocabulary doubles — and a
/// host that knows better overrides it either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoublesPolicy {
    /// Judged unless the corpus doubles productively.
    #[default]
    Auto,
    Always,
    Never,
}

/// Per-channel enable bits. All on by default except `pooled_neighbor`: a
/// pool's share is never under a member's, so every G2 row rides beside its
/// G3 rows and adds a coarser sentence, not a finding. A host that wants the
/// grouped statement turns it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channels {
    pub placement: bool,
    pub run_shape: bool,
    pub exact_neighbor: bool,
    pub pooled_neighbor: bool,
    pub rarity: bool,
    pub casing: bool,
    /// Long words against the corpus's own length distribution. Off: names
    /// and loanwords are the long tail, and they are not slips.
    pub word_length: bool,
    /// A word written twice in a row. On: a low-volume, cheap claim, and a
    /// language that doubles productively recuses itself corpus-wide rather
    /// than through the band ([`DoublesPolicy`]).
    pub doubled: bool,
}

impl Default for Channels {
    fn default() -> Self {
        Self {
            placement: true,
            run_shape: true,
            exact_neighbor: true,
            pooled_neighbor: false,
            rarity: true,
            casing: true,
            word_length: false,
            doubled: true,
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
    /// A word judged on fewer free positions than this abstains.
    pub word_support_floor: u32,
    /// The minority share that flags a word's case form:
    /// [`Staircase::WORD_STEPS`], the glyph ladder at a tenth. Word casing
    /// fires at twenty times glyph volume under shared bands, so the shares
    /// are its own (`rules/word-conventions.md`).
    pub word_bands: Staircase,
    /// The share of a glyph's handoffs that must be uppercase before the
    /// corpus is held to capitalize after it, in basis points.
    pub terminal_upper_share_bp: u16,
    /// Whole standard deviations above the corpus's mean word length that a
    /// word must reach before [`Channel::WordLength`] names it.
    pub word_length_sigma: u8,
    /// The share of a corpus's distinct words that may appear doubled before
    /// doubling is held to be productive in this language and
    /// [`Channel::Doubled`] abstains for the whole corpus, in basis points.
    pub doubles_productive_bp: u16,
    pub doubles: DoublesPolicy,
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
            word_support_floor: 20,
            word_bands: Staircase::new(Staircase::WORD_STEPS).expect("the word bounds ascend"),
            terminal_upper_share_bp: 8_000,
            word_length_sigma: 4,
            doubles_productive_bp: 300,
            doubles: DoublesPolicy::default(),
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
    /// G2: the pool the atom following a glyph inside its run falls in.
    PooledNeighbor = 1,
    /// G1: the shape of the runs a glyph appears in.
    RunShape = 2,
    /// G0: the outer class one side of a glyph.
    Placement = 3,
    /// The absolute-rarity roster, which is a list and not a claim.
    Rarity = 4,
    /// One case-folded word's minority form in free positions. It judges no
    /// scalar, so its `glyph` field is [`ScalarKey::NONE`] and the wire
    /// carries the word hash in its place.
    Casing = 5,
    /// One case-folded word far longer than the corpus's own words. It judges
    /// no scalar either, and carries the word hash the same way.
    WordLength = 6,
    /// One case-folded word written twice in a row, adjacent or separated by
    /// a nonletter run. It judges no scalar either.
    Doubled = 7,
}

impl Channel {
    pub const ALL: [Self; 8] = [
        Self::ExactNeighbor,
        Self::PooledNeighbor,
        Self::RunShape,
        Self::Placement,
        Self::Rarity,
        Self::Casing,
        Self::WordLength,
        Self::Doubled,
    ];

    /// Whether the channel's `glyph` field carries a word hash instead of a
    /// scalar. The three word channels do; every substrate channel does not.
    pub const fn is_word(self) -> bool {
        matches!(self, Self::Casing | Self::WordLength | Self::Doubled)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::ExactNeighbor => "ExactNeighbor",
            Self::PooledNeighbor => "PooledNeighbor",
            Self::RunShape => "RunShape",
            Self::Placement => "Placement",
            Self::Rarity => "Rarity",
            Self::Casing => "Casing",
            Self::WordLength => "WordLength",
            Self::Doubled => "Doubled",
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
    /// The pool the scalar that follows the glyph inside a run belongs to.
    PooledNeighbor(Pool),
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
    /// The case-folded word, and the form that is its minority.
    Casing {
        hash: u64,
        form: Form,
    },
    /// The case-folded word, and how many whole standard deviations its length
    /// stands above the corpus mean, saturating.
    WordLength {
        hash: u64,
        sigma: u8,
    },
    /// The case-folded word, and whether a nonletter run stood between the
    /// two occurrences. The two are separate claims with separate
    /// denominators, so they are separate keys.
    Doubled {
        hash: u64,
        separated: bool,
    },
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
    /// Books whose own counts hold part of the numerator, saturating at 255.
    ///
    /// Dispersion is information, never a judgement: genre clusters
    /// punctuation legitimately, so nothing gates on it. Books-possible is the
    /// publication's own `book_count`. [`books_touched`] recomputes it.
    pub books: u8,
}

impl Pattern {
    /// The word hash a word channel's row carries, `None` on every other.
    pub const fn word_hash(&self) -> Option<u64> {
        match self.key {
            PatternKey::Casing { hash, .. }
            | PatternKey::WordLength { hash, .. }
            | PatternKey::Doubled { hash, .. } => Some(hash),
            _ => None,
        }
    }

    /// Refuses a row whose fields disagree with each other, returning the
    /// offending field's name. The wire's own byte-level checks (flags,
    /// reserved, key nibbles) stay in `decode_pattern`; this is what a typed
    /// `Pattern` can express and a corrupted round trip cannot fake.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.band.is_none() != (self.channel == Channel::Rarity) {
            return Err("band");
        }
        if self.numerator > self.denominator {
            return Err("numerator");
        }
        if self.share_bp != share_bp(u64::from(self.numerator), u64::from(self.denominator)) {
            return Err("share_bp");
        }
        if self.books == 0 && self.numerator > 0 {
            return Err("books");
        }
        if let PatternKey::RunShape { bucket, .. } = self.key
            && !(1..=RUN_BUCKETS as u8).contains(&bucket)
        {
            return Err("key");
        }
        let keyed = match self.key {
            PatternKey::Casing { .. } => Channel::Casing,
            PatternKey::WordLength { .. } => Channel::WordLength,
            PatternKey::Doubled { .. } => Channel::Doubled,
            PatternKey::ExactNeighbor(_) => Channel::ExactNeighbor,
            PatternKey::PooledNeighbor(_) => Channel::PooledNeighbor,
            PatternKey::RunShape { .. } => Channel::RunShape,
            PatternKey::Placement { .. } => Channel::Placement,
            PatternKey::Rarity => Channel::Rarity,
        };
        if keyed != self.channel {
            return Err("channel");
        }
        if let PatternKey::Casing { form, .. } = self.key
            && form == Form::Uncased
        {
            return Err("key");
        }
        // A word channel judges no scalar, so the glyph field carries its hash.
        if self.channel.is_word() && self.glyph != ScalarKey::NONE {
            return Err("glyph");
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

    let placements = placement_evidence(corpus);
    let shapes = run_evidence(corpus);
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
    }
}

/// Every scalar under `rarity_floor`, letters included when the corpus is
/// alphabetic enough for a letter roster to mean anything.
fn roster(
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
            share_bp: share_bp(tally.count, total_scalars),
            books: tally.books(),
        });
    }
}

/// A logographic corpus has thousands of letters used once, and a ten-verse
/// draft has `q`, `x`, `z` used once by sample size; both abstain.
fn letters_are_rostered(scalars: &[(ScalarKey, Tally)], config: &JudgingConfig) -> bool {
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
fn placement(
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
                share_bp: share,
                books: tally.books(),
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
            share_bp: share,
            books: tally.books(),
        });
    }
}

/// G3: what follows the glyph inside a run, against every position where
/// something does.
fn neighbors(glyph: ScalarKey, evidence: &RunEvidence, config: &JudgingConfig, out: &mut Findings) {
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
            share_bp: share,
            books: tally.books(),
        });
    }
}

/// G2: which pool follows the glyph inside a run, against the same positions
/// G3 counts — so a pair too thin to name exactly can still be named by kind.
fn pooled_neighbors(
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
            share_bp: share,
            books: tally.books(),
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

// ── Dispersion ──────────────────────────────────────────────────────────

/// A numerator under construction: the running sum, and how many books have
/// contributed to it.
///
/// Books arrive in `BookIndex` order, so a distinct count needs the last
/// contributor and not a set.
#[derive(Clone, Copy, Default)]
struct Tally {
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
fn numerator_in(book: &BookAggregate, pattern: &Pattern) -> u64 {
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
        // A word row is judged over word aggregates, which these are not;
        // `words::free_in` is its oracle.
        PatternKey::Casing { .. } | PatternKey::WordLength { .. } | PatternKey::Doubled { .. } => 0,
    }
}

// ── Corpus totals ───────────────────────────────────────────────────────

/// One glyph's G0 marginals: the denominator both sides share, and a tally
/// per side and outer class.
#[derive(Default)]
struct PlacementEvidence {
    denominator: u64,
    sides: [[Tally; OuterClass::ALL.len()]; Side::ALL.len()],
}

/// One glyph's run history: which shapes hold it, and what follows it inside
/// them.
#[derive(Default)]
struct RunEvidence {
    /// `((pure, length bucket), tally)`.
    shapes: Vec<((bool, u8), Tally)>,
    neighbors: Vec<(ScalarKey, Tally)>,
    pools: Vec<(Pool, Tally)>,
    /// Runs holding the glyph at all.
    runs: u64,
    /// Positions where the glyph is followed by another atom.
    positions: u64,
}

/// Every book's pairs into one glyph-keyed table, ascending by glyph.
fn placement_evidence(corpus: &[&BookAggregate]) -> Vec<(ScalarKey, PlacementEvidence)> {
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
fn run_evidence(corpus: &[&BookAggregate]) -> FxHashMap<ScalarKey, RunEvidence> {
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
fn bump<K: PartialEq>(counts: &mut Vec<(K, Tally)>, key: K, count: u64, book: u32) {
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
fn merged_scalars(corpus: &[&BookAggregate]) -> Vec<(ScalarKey, Tally)> {
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

fn is_letter(glyph: ScalarKey) -> bool {
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

fn saturate(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

// ── The terminal table ──────────────────────────────────────────────────

/// Which glyphs this corpus puts a capital after, learned from the substrate's
/// `follows` lane rather than listed.
///
/// ```text
/// learn(en_ulb: '.' upper 33,332 of 33,338 cased, ',' upper 4,836 of 47,291)
///   at 8,000 bp   '.' forces (9,998 bp), ',' does not (1,022 bp)
/// ```
///
/// A glyph forces when the share of the cased letters it hands off to that are
/// uppercase reaches [`JudgingConfig::terminal_upper_share_bp`], on at least
/// `support_floor` handoffs. So the danda and `።` force wherever a corpus
/// writes them that way, and a comma forces in a corpus that reports speech
/// after one — no ASCII allow-list, and no rule per script.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalTable {
    forcing: Box<[ScalarKey]>,
}

impl TerminalTable {
    /// The forcing glyphs of a corpus-merged follow lane.
    pub fn learn(follows: &[(ScalarKey, FollowCounts)], config: &JudgingConfig) -> Self {
        let mut forcing: Vec<ScalarKey> = follows
            .iter()
            .filter(|(_, counts)| forces_a_capital(*counts, config))
            .map(|(key, _)| *key)
            .collect();
        forcing.sort_unstable();
        forcing.dedup();
        Self {
            forcing: forcing.into_boxed_slice(),
        }
    }

    /// Every forcing glyph, ascending.
    pub fn forcing(&self) -> &[ScalarKey] {
        &self.forcing
    }

    pub fn forces(&self, glyph: ScalarKey) -> bool {
        self.forcing.binary_search(&glyph).is_ok()
    }

    pub fn is_empty(&self) -> bool {
        self.forcing.is_empty()
    }
}

/// Entitlement and the share, in one place: the denominator is the cased
/// handoffs, so a glyph followed only by uncased letters decides nothing.
fn forces_a_capital(counts: FollowCounts, config: &JudgingConfig) -> bool {
    let upper = u64::from(counts.get(Case::Upper));
    let cased = upper + u64::from(counts.get(Case::Lower));
    cased >= u64::from(config.support_floor)
        && share_bp(upper, cased) >= config.terminal_upper_share_bp
}

/// Every book's follow lane merged into one, by key ascending.
pub fn merged_follows(corpus: &[&BookAggregate]) -> Vec<(ScalarKey, FollowCounts)> {
    let mut out: Vec<(ScalarKey, FollowCounts)> = Vec::new();
    for book in corpus {
        for (key, counts) in book.follows() {
            match out.binary_search_by_key(key, |entry| entry.0) {
                Ok(at) => out[at].1.absorb(*counts),
                Err(at) => out.insert(at, (*key, *counts)),
            }
        }
    }
    out
}

// ── The word channels ───────────────────────────────────────────────────

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
    if config.channels.casing {
        casing(corpus, totals, table, config, out);
    }
    if config.channels.word_length {
        word_length(corpus, totals, config, out);
    }
    if config.channels.doubled && judges_doubles(totals, table, config) {
        doubled(corpus, totals, table, config, out);
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
fn judges_doubles(totals: &WordTotals, table: &TerminalTable, config: &JudgingConfig) -> bool {
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
fn doubled(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    table: &TerminalTable,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    let mut cased = totals.by_word().peekable();
    for row in totals.doubles() {
        let free_separated = row.separated_free(table);
        if row.bare == 0 && free_separated == 0 {
            continue;
        }
        while cased.peek().is_some_and(|word| word[0].hash < row.hash) {
            cased.next();
        }
        // The two lanes partition a word's occurrences: a cased occurrence is
        // in the casing lane, an uncased one is counted here.
        let held: u64 = cased
            .peek()
            .filter(|word| word[0].hash == row.hash)
            .map_or(0, |word| {
                word.iter().flat_map(|row| row.counts).map(u64::from).sum()
            });
        let total = held + u64::from(row.uncased);
        let Some((band, ceiling)) = entitled_words(total, config) else {
            continue;
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
            out.push_pattern(Pattern {
                glyph: ScalarKey::NONE,
                channel: Channel::Doubled,
                key,
                band: Some(band),
                numerator: saturate(count),
                denominator: saturate(total),
                share_bp: share,
                books: word_books(corpus, &key, table),
            });
        }
    }
}

/// The casing channel over a corpus tally: rows of one hash are contiguous, so
/// one walk sums the free lanes of every `Before` the word was seen under.
fn casing(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    table: &TerminalTable,
    config: &JudgingConfig,
    out: &mut Findings,
) {
    for word in totals.by_word() {
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
            continue;
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
            out.push_pattern(Pattern {
                glyph: ScalarKey::NONE,
                channel: Channel::Casing,
                key,
                band: Some(band),
                numerator: saturate(count),
                denominator: saturate(total),
                share_bp: share,
                books: word_books(corpus, &key, table),
            });
        }
    }
}

/// The corpus's own word length distribution, occurrence-weighted, and the
/// words standing `word_length_sigma` deviations above it.
///
/// Only the long end: a short word is a word, and the tail this names is
/// names, loanwords, and compounds — which is why the channel ships off.
fn word_length(
    corpus: &[&WordAggregate],
    totals: &WordTotals,
    config: &JudgingConfig,
    out: &mut Findings,
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
        out.push_pattern(Pattern {
            glyph: ScalarKey::NONE,
            channel: Channel::WordLength,
            key,
            band: Some(band),
            numerator: saturate(count),
            denominator: occurrences,
            share_bp: share_bp(count, u64::from(occurrences)),
            books: word_books(corpus, &key, &TerminalTable::default()),
        });
    }
}

/// Mean and standard deviation of word length over every occurrence the
/// corpus counted, from the same `len` byte the row carries, plus the length
/// of each word so the sweep below reads it once instead of probing 66 books
/// per candidate.
struct LengthShape {
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
fn word_books(corpus: &[&WordAggregate], key: &PatternKey, table: &TerminalTable) -> u8 {
    let touched = corpus
        .iter()
        .filter(|book| free_of(book, key, table) > 0)
        .count();
    u8::try_from(touched).unwrap_or(u8::MAX)
}

/// One book's contribution to a word row's numerator.
pub(crate) fn free_of(book: &WordAggregate, key: &PatternKey, table: &TerminalTable) -> u64 {
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
        _ => 0,
    }
}

/// A word's band, or `None` when its free positions are under the word
/// support floor and it abstains.
fn entitled_words(free: u64, config: &JudgingConfig) -> Option<(u8, u16)> {
    if free < u64::from(config.word_support_floor) {
        return None;
    }
    config.word_bands.band_for(saturate(free))
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

    /// `books_touched` recounts dispersion from the retained aggregates; the
    /// merge that produced each numerator must agree with it row for row.
    #[test]
    fn books_touched_is_the_oracle_for_the_merge_time_count() {
        use crate::pass::{ChapterObs, Findings};
        use crate::substrate::{Edge, fold_book, walk};

        let texts = [
            format!("{}c;d ,,, 12,345", "a; b ".repeat(40)),
            "e;f ... `rare` ;; 7,8 quiz".to_string(),
            "no punctuation at all in this one".to_string(),
            "x; y; z, w ,, q?. r?\" s".to_string(),
        ];
        let rows: Vec<_> = texts.iter().map(|text| walk::walk(text)).collect();
        let aggregates: Vec<BookAggregate> = rows
            .iter()
            .map(|obs| fold_book(&[ChapterObs { start: 0, obs }], &mut Edge::default()))
            .collect();
        let views: Vec<&BookAggregate> = aggregates.iter().collect();
        let config = JudgingConfig {
            support_floor: 1,
            letters: LetterRoster::Always,
            ..JudgingConfig::default()
        };
        let mut findings = Findings::new(texts.iter().map(|text| text.len() as u32).collect());
        judge_corpus(&views, &config, &mut findings);
        assert!(
            findings.patterns().len() > 20,
            "the sample must judge something: {} rows",
            findings.patterns().len()
        );
        let mut dispersed = 0;
        for pattern in findings.patterns() {
            assert_eq!(books_touched(&views, pattern), pattern.books, "{pattern:?}");
            dispersed += usize::from(pattern.books > 1);
        }
        assert!(dispersed > 0, "no pattern reached two books");
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
