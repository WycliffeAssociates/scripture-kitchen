//! Level 3: one verse's length against a declared source and its neighbours.
//!
//! ```text
//! judge_lengths(target, source, &LengthConfig::default(), &mut out)
//!   MRK 1:1  target 41 graphemes / source 40  ratio 1.02   z_book +0.3  → silent
//!   MRK 1:9  target  6 graphemes / source 44  ratio 0.14   z_book -8.7  → row
//!     LengthProportionality  book -8.7 Q8.8  project -6.1 Q8.8  over 1:9's span
//!   LUK (12 paired verses, under min_verses 50)
//!     book lane i16::MIN — unavailable — and the project lane judges alone
//! ```
//!
//! Not a [`ChapterPass`](crate::ChapterPass): it needs both sides and a
//! pairing, so it is a corpus-level step a host runs after the chapter passes,
//! from lengths both sides already retain. It reads no text.
//!
//! The claim is only "this verse's length is unusual relative to this declared
//! source and the surrounding paired verses" — never an omission, a
//! mistranslation, or a wrong language. Pairing failures are
//! [`AlignmentFact`]s and never rows. The algorithm, its defaults, and its
//! known limits: `sous-chef/rules/length-proportionality.md`.

use rustc_hash::FxHashMap;

use crate::alignment::pair_keys;
use crate::codec::{
    FindingKind, PresenceDigest, ProportionalityDigest, QuantizedDeviation, SourceCopyDigest,
};
use crate::presence::{self, PresenceRow};
use crate::source_copy::{self, SourceCopyRow, SourceWords};
use crate::substrate::VerseLength;
use crate::unicode::atoms::count_atoms;
use crate::{AlignmentFact, BookIndex, BookKey, Findings, ProjectedBook, TextRange, VerseKey};

/// Scale making a MAD stddev-equivalent under normality, so `z_long` and
/// `z_short` read in familiar z units.
const MAD_TO_SIGMA: f64 = 0.6745;

/// Strict deviations one side needs before its own MAD is trusted over the
/// pooled one.
///
/// Below it a side's MAD is measured from the very points it would judge: at
/// one deviation the "median" is that deviation, pinning its z at exactly
/// [`MAD_TO_SIGMA`] however extreme the ratio.
const SIDE_DATA_FLOOR: usize = 3;

/// What judging may vary without re-walking a chapter or re-pairing a book.
#[derive(Debug, Clone, Copy)]
pub struct LengthConfig {
    /// Standardized deviations above the book's median a verse must exceed.
    pub z_long: f32,
    /// Standardized deviations below it.
    pub z_short: f32,
    /// Paired units a scope needs before it judges at all.
    pub min_verses: u32,
    pub enabled: bool,
    /// Verses one side holds and the other does not, and paired target verses
    /// with no content. On: the facts already exist in the pairing, they cost
    /// no text walk, and versification difference alone keeps the volume to a
    /// handful of coalesced rows per book.
    pub presence: bool,
    /// Consecutive target words the paired source verse also holds. OFF: the
    /// run is the rule's only filter, and against a source in the same
    /// language family the tier fires about two rows per verse at any floor
    /// measured. A host that knows its source is unrelated turns it on.
    pub source_copy: bool,
    /// Consecutive words a run needs before it is a row. Below
    /// [`source_copy::MIN_RUN`] it reads as that floor: one shared word is
    /// not a run.
    pub source_copy_min_run: u32,
}

impl Default for LengthConfig {
    fn default() -> Self {
        Self {
            z_long: 3.5,
            z_short: 3.5,
            min_verses: 50,
            enabled: true,
            presence: true,
            source_copy: false,
            source_copy_min_run: 3,
        }
    }
}

/// Bitwise on the two thresholds, which is what makes [`LengthConfig`] — and
/// so the whole [`crate::JudgingConfig`] — `Eq`.
///
/// The question a config comparison asks is "would a re-judge produce the
/// identical verdict", not "are these numerically close", and bitwise
/// equality is reflexive whatever the payload.
impl PartialEq for LengthConfig {
    fn eq(&self, other: &Self) -> bool {
        self.z_long.to_bits() == other.z_long.to_bits()
            && self.z_short.to_bits() == other.z_short.to_bits()
            && self.min_verses == other.min_verses
            && self.enabled == other.enabled
            && self.presence == other.presence
            && self.source_copy == other.source_copy
            && self.source_copy_min_run == other.source_copy_min_run
    }
}

impl Eq for LengthConfig {}

/// One source verse: its key and its projected grapheme length, and nothing
/// else — a reference publishes no coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceVerse {
    key: VerseKey,
    graphemes: u32,
}

impl SourceVerse {
    pub const fn new(key: VerseKey, graphemes: u32) -> Self {
        Self { key, graphemes }
    }

    pub const fn key(self) -> VerseKey {
        self.key
    }

    pub const fn graphemes(self) -> u32 {
        self.graphemes
    }
}

/// One target book's rows, in producer order. The slice position is the
/// [`BookIndex`] a row names, as it is for [`crate::ChapterPass::judge`].
#[derive(Debug, Clone, Copy)]
pub struct TargetLengths<'a> {
    pub book: BookKey,
    pub verses: &'a [VerseLength],
    /// The projected text `verses` index into — what the source-copy walk
    /// reads, and the only text this step ever touches.
    pub text: &'a str,
}

/// One source book's rows, in producer order.
#[derive(Debug, Clone, Copy)]
pub struct SourceLengths<'a> {
    pub book: BookKey,
    pub verses: &'a [SourceVerse],
    /// Index-aligned word sets, or `None` from a producer that retained none;
    /// without them the source-copy lane abstains.
    pub words: Option<&'a SourceWords>,
}

/// Every keyed verse of a projected book as a source row.
///
/// The one place a source's lengths are counted, so an Onion reference and a
/// vref reference cannot disagree about what a grapheme is.
pub fn source_lengths(book: &impl ProjectedBook) -> Vec<SourceVerse> {
    let text = book.text();
    book.verses()
        .map(|verse| {
            let span = verse.text();
            SourceVerse::new(
                verse.key(),
                count_atoms(&text[span.from() as usize..span.to() as usize]),
            )
        })
        .collect()
}

/// What one paired judgement saw, beside the rows it pushed.
///
/// `units` and `facts` are structure, not evidence: a count of ratios is a
/// denominator a host may show, and a fact says why a key did not pair.
/// Versification shear is a separate, parked rule
/// (`rules/presence-shear.md`) and never becomes a finding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Paired {
    /// Units that produced a ratio, per target book in slice order.
    pub units: Vec<u32>,
    /// Why a key did not pair.
    pub facts: Vec<AlignmentFact>,
    /// The presence rows pushed, per target book in slice order, with the keys
    /// the wire lanes do not carry.
    pub presence: Vec<Vec<PresenceRow>>,
    /// The source-copy rows pushed, per target book in slice order.
    pub copies: Vec<Vec<SourceCopyRow>>,
    /// Target books whose paired source retained no word lane while
    /// [`LengthConfig::source_copy`] was on: their silence is a missing lane,
    /// not a clean result, so it is reported rather than published.
    pub wordless: Vec<BookKey>,
}

impl Paired {
    pub fn total(&self) -> u64 {
        self.units.iter().map(|count| u64::from(*count)).sum()
    }
}

/// One target book paired against its declared source: a ratio and the target
/// span a row would name per unit, plus the knob-free order statistics over
/// them.
///
/// A pure function of the two books' rows and the pairing law, which is what
/// lets a resident host key one under the two checksums and re-pair only the
/// books that moved (`galley/src/sous/expediter.md`).
#[derive(Debug, Clone, Default)]
pub struct PairedBook {
    /// Parallel to [`Self::spans`], and held apart from it so pooling every
    /// book's ratios for the project scope is a memcpy rather than a walk.
    ratios: Box<[f64]>,
    spans: Box<[TextRange]>,
    /// Coalesced presence rows, in span order. A pure function of the same two
    /// key lists the ratios come from, so one cache entry holds both.
    presence: Box<[PresenceRow]>,
    /// Maximal source-copy runs of at least [`source_copy::MIN_RUN`] words, in
    /// unit order. Empty when the caller passed no source word lane; the
    /// minimum run a ROW needs is a judge-time knob applied over these.
    copies: Box<[SourceCopyRow]>,
    spread: Spread,
}

impl PairedBook {
    /// Pairs one book and turns each unit into a ratio; empty and absent
    /// counterparts produce no ratio, and a bridge contributes exactly one.
    pub fn pair(
        book: BookKey,
        target: &[VerseLength],
        source: &[SourceVerse],
        facts: &mut Vec<AlignmentFact>,
    ) -> Self {
        Self::pair_with(book, target, source, None, facts)
    }

    /// [`pair`](Self::pair) with the source-copy lane: the target's projected
    /// text and the source's word sets, both or neither.
    ///
    /// Walking words is the one thing this step does that reads text, so it
    /// happens only when a caller hands both over — which is what makes the
    /// switch, unlike a threshold, worth invalidating a cache for.
    pub fn pair_with(
        book: BookKey,
        target: &[VerseLength],
        source: &[SourceVerse],
        copy: Option<(&str, &SourceWords)>,
        facts: &mut Vec<AlignmentFact>,
    ) -> Self {
        let target_keys: Vec<VerseKey> = target.iter().map(|row| row.key()).collect();
        let source_keys: Vec<VerseKey> = source.iter().map(|row| row.key()).collect();
        let mut ratios: Vec<f64> = Vec::with_capacity(target.len());
        let mut spans: Vec<TextRange> = Vec::with_capacity(target.len());
        let mut empties: Vec<(VerseKey, TextRange)> = Vec::new();
        let mut copies: Vec<SourceCopyRow> = Vec::new();
        // Reused per unit: a bridge's source constituents merge into one set,
        // a lone verse borrows its own.
        let mut merged: Vec<u32> = Vec::new();
        // This book's own facts, whatever the caller accumulated before it.
        let facts_start = facts.len();
        pair_keys(
            book,
            &target_keys,
            &source_keys,
            &mut |key, left, right| {
                if let Some((text, words)) = copy {
                    let set = match right {
                        [only] => words.verse(*only),
                        many => {
                            merged.clear();
                            for at in many {
                                merged.extend_from_slice(words.verse(*at));
                            }
                            merged.sort_unstable();
                            merged.dedup();
                            &merged
                        }
                    };
                    source_copy::unit_rows(text, target, left, set, &mut copies);
                }
                let long: u32 = left.iter().map(|at| target[*at].graphemes()).sum();
                let short: u32 = right.iter().map(|at| source[*at].graphemes()).sum();
                if long == 0 || short == 0 {
                    if long == 0 && short > 0 {
                        empties.push((key, bounding(target, left)));
                    }
                    return;
                }
                ratios.push(f64::from(long) / f64::from(short));
                spans.push(bounding(target, left));
            },
            facts,
        );
        // Pairing walks keys in target order, so units already follow the book.
        let spread = Spread::of(&ratios);
        copies.sort_by_key(|row| (row.span().from(), row.span().to()));
        Self {
            ratios: ratios.into(),
            spans: spans.into(),
            presence: presence::rows(target, &facts[facts_start..], &empties).into(),
            copies: copies.into(),
            spread,
        }
    }

    /// The presence rows this pairing found, in span order.
    pub fn presence(&self) -> &[PresenceRow] {
        &self.presence
    }

    /// Every maximal source-copy run this pairing found, in span order.
    pub fn copies(&self) -> &[SourceCopyRow] {
        &self.copies
    }

    /// Every unit's ratio, in target order.
    pub fn ratios(&self) -> &[f64] {
        &self.ratios
    }

    /// Units that produced a ratio — the count a [`Paired`] reports.
    pub fn count(&self) -> u32 {
        u32::try_from(self.ratios.len()).expect("a book holds under 4G verses")
    }

    /// Inline size plus every lane: what a resident cache pays for one book.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>()
            + size_of_val(&*self.ratios)
            + size_of_val(&*self.spans)
            + size_of_val(&*self.presence)
            + size_of_val(&*self.copies)
    }
}

/// The pooled order statistics over every paired book of one publication.
///
/// A function of the multiset of ratios alone — the order books contribute
/// them in cannot move a median — so a host whose books every one hit their
/// cache reuses one verbatim.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectSpread(Spread);

impl ProjectSpread {
    pub fn of(books: &[Option<&PairedBook>]) -> Self {
        let mut pooled: Vec<f64> =
            Vec::with_capacity(books.iter().flatten().map(|book| book.ratios.len()).sum());
        for book in books.iter().flatten() {
            pooled.extend_from_slice(&book.ratios);
        }
        Self(Spread::of(&pooled))
    }
}

/// Judges pairs a caller already holds against their own book and against the
/// whole paired project, and pushes a row for every unit either scope calls an
/// outlier.
///
/// Returns the units each book contributed, in slice order. `None` is a target
/// with no source book of its key: no ratios and no rows, which is the
/// contract and not an error.
pub fn judge_paired(
    books: &[Option<&PairedBook>],
    project: &ProjectSpread,
    config: &LengthConfig,
    out: &mut Findings,
) -> Vec<u32> {
    if !config.enabled && !config.presence && !config.source_copy {
        return vec![0; books.len()];
    }
    let min_run = config.source_copy_min_run.max(source_copy::MIN_RUN);
    let project = project.0.gated(config.min_verses);
    let mut counts: Vec<u32> = Vec::with_capacity(books.len());
    for (index, paired) in books.iter().enumerate() {
        let Some(paired) = paired else {
            counts.push(0);
            continue;
        };
        counts.push(paired.count());
        let book_idx = BookIndex::new(index).expect("a corpus indexes every book");
        let mut opened = false;
        if config.presence {
            for row in &*paired.presence {
                if !opened {
                    out.open_book(book_idx);
                    opened = true;
                }
                out.push(
                    row.span(),
                    FindingKind::Presence(
                        PresenceDigest::new(row.kind(), row.keys())
                            .expect("a coalesced row covers a key"),
                    ),
                )
                .expect("a presence span lies inside its own book");
            }
        }
        if config.source_copy {
            for row in &*paired.copies {
                if row.run() < min_run {
                    continue;
                }
                if !opened {
                    out.open_book(book_idx);
                    opened = true;
                }
                out.push(
                    row.span(),
                    FindingKind::SourceCopy(
                        SourceCopyDigest::new(row.run(), row.eligible())
                            .expect("a run is at least one word of its own unit"),
                    ),
                )
                .expect("a source-copy span lies inside its own book");
            }
        }
        if !config.enabled || paired.ratios.is_empty() {
            continue;
        }
        let book = paired.spread.gated(config.min_verses);
        for (ratio, span) in paired.ratios.iter().zip(paired.spans.iter()) {
            let book_z = side_z(*ratio, book);
            let project_z = side_z(*ratio, project);
            if !fires(book_z, config) && !fires(project_z, config) {
                continue;
            }
            if !opened {
                out.open_book(book_idx);
                opened = true;
            }
            // Both scopes ride every row; a scope that did not judge is the
            // wire's `i16::MIN`, which is also the under-`min_verses` flag.
            let (book_lane, book_clamped) = lane(book_z);
            let (project_lane, project_clamped) = lane(project_z);
            out.push(
                *span,
                FindingKind::LengthProportionality(ProportionalityDigest::new(
                    book_lane,
                    project_lane,
                    book_clamped || project_clamped,
                )),
            )
            .expect("a paired verse's span lies inside its own book");
        }
    }
    counts
}

/// Pairs every target book with the source book of the same [`BookKey`],
/// judges each paired verse's length ratio against its book and against the
/// whole paired project, and pushes a row for every unit either scope calls an
/// outlier.
///
/// A target book with no source book of its key is skipped whole — that is the
/// contract, not an error, and it produces no ratios and no facts.
pub fn judge_lengths(
    target: &[TargetLengths<'_>],
    source: &[SourceLengths<'_>],
    config: &LengthConfig,
    out: &mut Findings,
) -> Paired {
    let mut facts = Vec::new();
    if !config.enabled && !config.presence && !config.source_copy {
        return Paired {
            units: vec![0; target.len()],
            facts,
            presence: vec![Vec::new(); target.len()],
            copies: vec![Vec::new(); target.len()],
            wordless: Vec::new(),
        };
    }
    // First wins: a caller may present two files under one key, and the
    // choice has to be its order rather than a hash's.
    let mut sources: FxHashMap<BookKey, SourceLengths<'_>> = FxHashMap::default();
    for book in source {
        sources.entry(book.book).or_insert(*book);
    }

    let mut wordless: Vec<BookKey> = Vec::new();
    let books: Vec<Option<PairedBook>> = target
        .iter()
        .map(|book| {
            let rows = sources.get(&book.book)?;
            if config.source_copy && rows.words.is_none() {
                wordless.push(book.book);
            }
            let copy = rows
                .words
                .filter(|_| config.source_copy)
                .map(|words| (book.text, words));
            Some(PairedBook::pair_with(
                book.book,
                book.verses,
                rows.verses,
                copy,
                &mut facts,
            ))
        })
        .collect();
    let views: Vec<Option<&PairedBook>> = books.iter().map(Option::as_ref).collect();
    let project = ProjectSpread::of(&views);
    let presence = views
        .iter()
        .map(|book| match book.filter(|_| config.presence) {
            Some(book) => book.presence().to_vec(),
            None => Vec::new(),
        })
        .collect();
    let min_run = config.source_copy_min_run.max(source_copy::MIN_RUN);
    let copies = views
        .iter()
        .map(|book| match book.filter(|_| config.source_copy) {
            Some(book) => book
                .copies()
                .iter()
                .filter(|row| row.run() >= min_run)
                .copied()
                .collect(),
            None => Vec::new(),
        })
        .collect();
    Paired {
        units: judge_paired(&views, &project, config, out),
        facts,
        presence,
        copies,
        wordless,
    }
}

/// A bridge is ONE row over the bounding target range: its constituents may be
/// discontinuous, and the span is a navigation coordinate, not a claim that
/// the bytes between belong to it.
fn bounding(target: &[VerseLength], at: &[usize]) -> TextRange {
    let from = at
        .iter()
        .map(|at| target[*at].text().from())
        .min()
        .expect("a paired unit has a target row");
    let to = at
        .iter()
        .map(|at| target[*at].text().to())
        .max()
        .expect("a paired unit has a target row");
    TextRange::new(from, to).expect("a bounding range keeps its order")
}

/// A median with its two one-sided MADs, their sample sizes, and the pooled
/// symmetric MAD behind them. Knob-free: every gate is applied by
/// [`Spread::gated`].
#[derive(Debug, Clone, Copy, Default)]
struct Spread {
    count: usize,
    median: f64,
    /// Median of `x - median` over `x > median` — the long side.
    above: f64,
    /// Median of `median - x` over `x < median` — the short side.
    below: f64,
    n_above: usize,
    n_below: usize,
    /// Median of `|x - median|` over every point, the per-side fallback.
    symmetric: f64,
}

/// One gated unit: the median, plus each side's MAD where that side has
/// signal.
#[derive(Debug, Clone, Copy)]
struct Sides {
    median: f64,
    above: Option<f64>,
    below: Option<f64>,
}

impl Spread {
    /// The median is taken from the WHOLE sample; only the spread around it
    /// splits by side, because the short side is bounded by zero and the long
    /// side is open-ended.
    fn of(ratios: &[f64]) -> Self {
        if ratios.is_empty() {
            return Self::default();
        }
        let mut all = ratios.to_vec();
        let median = median_in_place(&mut all);
        let mut above: Vec<f64> = ratios
            .iter()
            .copied()
            .filter(|x| *x > median)
            .map(|x| x - median)
            .collect();
        let mut below: Vec<f64> = ratios
            .iter()
            .copied()
            .filter(|x| *x < median)
            .map(|x| median - x)
            .collect();
        let mut symmetric: Vec<f64> = ratios.iter().map(|x| (x - median).abs()).collect();
        Self {
            count: ratios.len(),
            median,
            n_above: above.len(),
            n_below: below.len(),
            above: median_or_zero(&mut above),
            below: median_or_zero(&mut below),
            symmetric: median_in_place(&mut symmetric),
        }
    }

    /// Two gates. The whole unit needs `min_verses` paired ratios — and at
    /// least one, whatever the floor. Then each side uses its own MAD only
    /// when it has [`SIDE_DATA_FLOOR`] strict deviations and that MAD is
    /// nonzero; otherwise it falls back to the pooled symmetric MAD, which is
    /// absent only when every ratio is identical and neither side should fire.
    fn gated(self, min_verses: u32) -> Option<Sides> {
        if self.count == 0 || self.count < min_verses as usize {
            return None;
        }
        let side = |n: usize, mad: f64| {
            if n >= SIDE_DATA_FLOOR && mad > 0.0 {
                Some(mad)
            } else {
                (self.symmetric > 0.0).then_some(self.symmetric)
            }
        };
        Some(Sides {
            median: self.median,
            above: side(self.n_above, self.above),
            below: side(self.n_below, self.below),
        })
    }
}

/// The signed z of `ratio` against one gated scope, or `None` when the scope
/// did not judge, the ratio sits exactly at the median, or its side abstained.
///
/// Negative means shorter than typical.
fn side_z(ratio: f64, scope: Option<Sides>) -> Option<f64> {
    let sides = scope?;
    if ratio > sides.median {
        Some(MAD_TO_SIGMA * (ratio - sides.median) / sides.above?)
    } else if ratio < sides.median {
        Some(MAD_TO_SIGMA * (ratio - sides.median) / sides.below?)
    } else {
        None
    }
}

/// A long-side z is held to `z_long` and a short-side z to `z_short`; neither
/// ever borrows the other's knob.
fn fires(z: Option<f64>, config: &LengthConfig) -> bool {
    z.is_some_and(|z| {
        let bar = if z >= 0.0 {
            config.z_long
        } else {
            config.z_short
        };
        z.abs() > f64::from(bar)
    })
}

/// One scope's wire lane: `None` where the scope did not judge, and whether
/// the value clamped.
fn lane(z: Option<f64>) -> (Option<QuantizedDeviation>, bool) {
    let Some(z) = z.filter(|z| z.is_finite()) else {
        return (None, false);
    };
    let scaled = (z * 256.0).round();
    let clamped = scaled > f64::from(i16::MAX) || scaled <= f64::from(i16::MIN);
    // `i16::MIN` is the unavailable sentinel, so the floor is one above it.
    let raw = scaled.clamp(f64::from(i16::MIN) + 1.0, f64::from(i16::MAX)) as i16;
    (
        Some(QuantizedDeviation::from_raw(raw).expect("clamped away from the sentinel")),
        clamped,
    )
}

/// The median of `v`, destructively.
fn median_in_place(v: &mut [f64]) -> f64 {
    let cmp = |a: &f64, b: &f64| a.partial_cmp(b).expect("a ratio is finite");
    let n = v.len();
    if n % 2 == 1 {
        let (_, mid, _) = v.select_nth_unstable_by(n / 2, cmp);
        *mid
    } else {
        let (low, mid, _) = v.select_nth_unstable_by(n / 2, cmp);
        let high = *mid;
        (low.iter().copied().fold(f64::NEG_INFINITY, f64::max) + high) / 2.0
    }
}

fn median_or_zero(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        median_in_place(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_side_under_the_floor_borrows_the_pooled_mad() {
        // Two deviations a side, both under the floor of three: each side
        // falls back to the pooled MAD rather than measuring itself from the
        // very points it would judge.
        let spread = Spread::of(&[1.0, 1.1, 1.2, 5.0]);
        let gated = spread.gated(0).unwrap();
        assert!(spread.symmetric > 0.0);
        assert_eq!(gated.above, Some(spread.symmetric));
        assert_eq!(gated.below, Some(spread.symmetric));
    }

    /// The collapse the floor exists to stop: a side judged by its own single
    /// deviation pins that deviation's z at exactly `MAD_TO_SIGMA`, however
    /// extreme the ratio behind it.
    #[test]
    fn a_lone_deviation_would_pin_its_own_z_at_the_scale_constant() {
        let spread = Spread::of(&[1.0, 1.0, 1.0, 1_000.0]);
        let sides = Sides {
            median: spread.median,
            above: Some(spread.above),
            below: None,
        };
        assert_eq!(side_z(1_000.0, Some(sides)), Some(MAD_TO_SIGMA));
        // The shipped gate refuses that side outright: the pooled MAD is zero
        // here, so nothing judges the long tail at all.
        assert_eq!(spread.gated(0).unwrap().above, None);
    }

    #[test]
    fn a_degenerate_sample_judges_nothing() {
        let gated = Spread::of(&[1.0; 8]).gated(0).unwrap();
        assert_eq!(gated.above, None);
        assert_eq!(gated.below, None);
        assert_eq!(side_z(1.0, Some(gated)), None);
    }

    #[test]
    fn a_scope_under_min_verses_does_not_judge() {
        assert!(Spread::of(&[1.0, 2.0, 3.0]).gated(50).is_none());
        assert!(Spread::of(&[]).gated(0).is_none());
    }

    #[test]
    fn the_lane_reserves_the_sentinel_and_reports_its_own_clamp() {
        let (value, clamped) = lane(Some(1.5));
        assert_eq!(value.unwrap().raw(), 0x0180);
        assert!(!clamped);

        let (value, clamped) = lane(Some(-4000.0));
        assert_eq!(value.unwrap().raw(), i16::MIN + 1);
        assert!(clamped);

        let (value, clamped) = lane(Some(4000.0));
        assert_eq!(value.unwrap().raw(), i16::MAX);
        assert!(clamped);

        assert_eq!(lane(None), (None, false));
    }

    #[test]
    fn each_side_answers_to_its_own_knob() {
        let long = LengthConfig {
            z_short: 100.0,
            ..LengthConfig::default()
        };
        assert!(fires(Some(4.0), &long));
        assert!(!fires(Some(-4.0), &long));
    }
}
