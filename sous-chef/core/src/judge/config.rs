//! The knobs judging reads, and nothing else reads.
//!
//! ```text
//! JudgingConfig::default().shared.bands -> five rungs, 2,500 bp down to 30 bp
//! config.channels.casing = false       -> the casing lane publishes nothing
//! ```
//!
//! Config reaches judging alone: moving a knob re-judges, and never remaps a
//! chapter or refolds a book.

use xxhash_rust::xxh3::Xxh3Default;

use super::*;

// ── The config ──────────────────────────────────────────────────────────

/// One rung of the fraction staircase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BandStep {
    /// The largest denominator this rung covers; the last rung is `u32::MAX`.
    pub up_to: u32,
    /// The minority share eligible for review, in basis points.
    pub share_bp: u16,
}

/// The fraction bands of `rules/character-inventory.md`, in basis points so
/// 0.3% is exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// A word written twice in a row. On: a language that doubles
    /// productively recuses itself corpus-wide ([`DoublesPolicy`]).
    pub doubled: bool,
    /// A letter repeated longer than this corpus ever repeats it. On: a
    /// handful of rows per corpus, and the denominator is the letter's own
    /// repeat history, so no script needs a rule of its own.
    pub letter_runs: bool,
    /// A lowercase letter after a glyph the corpus almost always capitalizes
    /// after. On: the bar is high enough that every exception is worth a look,
    /// and the tier fires at the glyph channels' own volume.
    pub sentence_start: bool,
    /// One book whose rate of a placement key breaks from the other books'
    /// median. On: it is silent until four books each hold the glyph at
    /// least `shared.floor` times.
    pub book_rate: bool,
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
            letter_runs: true,
            sentence_start: true,
            book_rate: true,
        }
    }
}

/// A denominator floor and the fraction staircase above it: what entitles a
/// row to speak, and the share under which it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ladder {
    /// A denominator below this abstains.
    pub floor: u32,
    pub bands: Staircase,
}

impl Ladder {
    /// The rung a denominator falls in, as `(step index, share in bp)`, or
    /// `None` when it is under the floor and abstains.
    pub fn entitled(self, denominator: u64) -> Option<(u8, u16)> {
        if denominator < u64::from(self.floor) {
            return None;
        }
        self.bands.band_for(saturate(denominator))
    }
}

/// [`Channel::Rarity`]'s roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RarityConfig {
    /// A scalar seen fewer times than this across the corpus is rostered.
    pub floor: u32,
    /// Letters leave the roster when the corpus uses more distinct letters
    /// than this, unless `letters` overrides.
    pub letter_roster_bound: u32,
    /// Letters join the roster only when the corpus holds at least this many
    /// letters: in a ten-verse draft `q`, `x`, `z` are rare by sample size,
    /// not by convention. Nonletters have no such floor.
    pub letter_roster_min_letters: u32,
    pub letters: LetterRoster,
}

/// [`Channel::Casing`]'s terminal table: which glyphs force the capital after
/// them. The table is data to every channel that reads it; these two shares
/// are what build it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CasingConfig {
    /// The share of a glyph's handoffs that must be uppercase before the
    /// corpus is held to capitalize after it, in basis points.
    pub terminal_upper_share_bp: u16,
    /// The share at or under which a glyph's handoffs leave the capital to
    /// the word, in basis points. Between this and `terminal_upper_share_bp`
    /// a context is mixed and Casing judges nothing after it.
    pub terminal_lower_share_bp: u16,
}

/// [`Channel::SentenceStart`]'s bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SentenceStartConfig {
    /// The share of a glyph's cased handoffs that must be uppercase before
    /// every lowercase one is reviewable, in basis points.
    ///
    /// A separate knob from `terminal_upper_share_bp` because the two answer
    /// different questions on the same counts: 80% decides whether a capital
    /// after this glyph was the punctuation's doing, so a word there is no
    /// evidence about the word; 98% decides whether a lowercase letter after
    /// it is an exception worth reading. A glyph can force at 80% and say
    /// nothing here.
    pub upper_bp: u16,
}

/// [`Channel::WordLength`]'s ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WordLengthConfig {
    /// Whole standard deviations above the corpus's mean word length that a
    /// word must reach before it is named.
    pub sigma: u8,
}

/// [`Channel::Doubled`]'s recusal and its two lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DoubledConfig {
    /// The share of a corpus's distinct words that may appear doubled before
    /// doubling is held to be productive in this language and the channel
    /// abstains for the whole corpus, in basis points.
    pub productive_bp: u16,
    pub policy: DoublesPolicy,
    /// Judges `the the`: a doubling with only whitespace between.
    pub bare: bool,
    /// Judges `Moses, Moses`: a doubling with punctuation between. Off by
    /// default: vocatives and genealogy chains are most of its volume.
    pub separated: bool,
}

/// [`Channel::BookRate`]'s three dials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BookRateConfig {
    /// How many times the other judged books' median rate a book's rate must
    /// reach before it is named.
    pub ratio: u16,
    /// The least rate, in basis points, a book must reach before it is named.
    pub min_bp: u16,
    /// The least number of times a book must hold the glyph before it may be
    /// named. The median still reads every book holding it `shared.floor`
    /// times.
    pub min_uses: u32,
}

/// Everything judging may vary: what several checks share, then one struct
/// per check that has settings of its own. A check reads its own struct and
/// the shared ladders it needs, never another check's. Plain fields: this is
/// exported through wasm-bindgen later and mirrored nowhere by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JudgingConfig {
    /// The glyph channels' ladder. Its floor is also the habit count Doubled,
    /// BookRate and the terminal table read.
    pub shared: Ladder,
    pub rarity: RarityConfig,
    /// The word channels' ladder: [`Staircase::WORD_STEPS`], the glyph ladder
    /// at a tenth. Word casing fires at twenty times glyph volume under shared
    /// bands, so the shares are its own (`rules/word-conventions.md`). A word
    /// judged on fewer free positions than its floor abstains.
    pub words: Ladder,
    pub casing: CasingConfig,
    pub sentence_start: SentenceStartConfig,
    pub word_length: WordLengthConfig,
    pub doubled: DoubledConfig,
    pub book_rate: BookRateConfig,
    /// The source-compared lane's own checks. Judged by
    /// [`crate::proportionality::judge_lengths`], which is a corpus-level step
    /// beside the chapter passes rather than one of them; a resident host
    /// reaches it through [`crate::ChapterPass::length_config`].
    pub lengths: LengthConfig,
    pub channels: Channels,
}

impl Default for JudgingConfig {
    fn default() -> Self {
        Self {
            shared: Ladder {
                floor: 5,
                bands: Staircase::default(),
            },
            rarity: RarityConfig {
                floor: 5,
                letter_roster_bound: 500,
                letter_roster_min_letters: 5_000,
                letters: LetterRoster::default(),
            },
            words: Ladder {
                floor: 20,
                bands: Staircase::new(Staircase::WORD_STEPS).expect("the word bounds ascend"),
            },
            casing: CasingConfig {
                terminal_upper_share_bp: 8_000,
                terminal_lower_share_bp: 2_000,
            },
            sentence_start: SentenceStartConfig { upper_bp: 9_800 },
            word_length: WordLengthConfig { sigma: 4 },
            doubled: DoubledConfig {
                productive_bp: 300,
                policy: DoublesPolicy::default(),
                bare: true,
                separated: false,
            },
            book_rate: BookRateConfig {
                ratio: 10,
                min_bp: 4_000,
                min_uses: 100,
            },
            lengths: LengthConfig::default(),
            channels: Channels::default(),
        }
    }
}

/// The whole config as one number, for a publication identity a host hashes.
///
/// Every field, never a chosen few: a knob left out of the stamp is two
/// publications a consumer cannot tell apart.
pub fn config_stamp(config: &JudgingConfig) -> u64 {
    let mut hasher = Portable(Xxh3Default::new());
    core::hash::Hash::hash(config, &mut hasher);
    core::hash::Hasher::finish(&hasher)
}

/// xxh3 with every number written at a fixed width, little-endian first.
///
/// A derived `Hash` writes an array's length as a `usize` and an integer in
/// native order — 8 bytes here, 4 in wasm — and a publication's identity may
/// not depend on which target computed it.
struct Portable(Xxh3Default);

impl core::hash::Hasher for Portable {
    fn finish(&self) -> u64 {
        core::hash::Hasher::finish(&self.0)
    }

    fn write(&mut self, bytes: &[u8]) {
        core::hash::Hasher::write(&mut self.0, bytes);
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes());
    }

    /// The one the derive reaches for on its own, as an array's length prefix.
    fn write_usize(&mut self, value: usize) {
        self.write(&(value as u64).to_le_bytes());
    }
}
