//! What a firing row IS: the claim, its channel, and its place in the table.
//!
//! ```text
//! Pattern { glyph: ',', channel: Placement, key: Placement { .. } }
//! pattern.share_bp()  -> 12      12 parts in 10,000 of the denominator
//! PatternIndex::at(3) -> the fourth row of this publication's table
//! ```
//!
//! An index is a position in ONE publication's table; the glyph, channel and
//! key are the identity that survives a renumbering.

use super::*;

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
    /// One letter repeated more times in a row than this corpus repeats it.
    /// The word walk feeds it, but the key IS a scalar: the glyph field holds
    /// the folded letter and the key byte the run length.
    LetterRun = 8,
    /// A lowercase letter after a glyph this corpus almost always capitalizes
    /// after. The mirror of [`Self::Casing`], on the same `follows` lane: that
    /// asks what form a WORD wears in a free position, this what case a GLYPH
    /// hands off to.
    SentenceStart = 9,
}

impl Channel {
    pub const ALL: [Self; 10] = [
        Self::ExactNeighbor,
        Self::PooledNeighbor,
        Self::RunShape,
        Self::Placement,
        Self::Rarity,
        Self::Casing,
        Self::WordLength,
        Self::Doubled,
        Self::LetterRun,
        Self::SentenceStart,
    ];

    /// Whether the channel's `glyph` field carries a word hash instead of a
    /// scalar. The three hash-keyed word channels do; every other does not,
    /// `LetterRun` included.
    pub const fn is_word(self) -> bool {
        matches!(self, Self::Casing | Self::WordLength | Self::Doubled)
    }

    /// Whether [`crate::Words`] owns the channel — judging it, claiming it in
    /// `firing`, and siting it. `LetterRun` keys a real scalar and still comes
    /// out of the word walk, so this is wider than [`Self::is_word`].
    pub const fn judged_by_words(self) -> bool {
        self.is_word() || matches!(self, Self::LetterRun)
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
            Self::LetterRun => "LetterRun",
            Self::SentenceStart => "SentenceStart",
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
    /// How long the run of one letter was, `LETTER_RUN_MIN..=LETTER_RUN_MAX`,
    /// the last saturating. The letter is the row's own glyph.
    LetterRun {
        length: u8,
    },
    /// The glyph is the row's own; the claim needs nothing else, so the key is
    /// a unit and the wire's key byte is zero.
    SentenceStart,
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
            PatternKey::LetterRun { .. } => Channel::LetterRun,
            PatternKey::SentenceStart => Channel::SentenceStart,
        };
        if keyed != self.channel {
            return Err("channel");
        }
        if let PatternKey::Casing { form, .. } = self.key
            && form == Form::Uncased
        {
            return Err("key");
        }
        if let PatternKey::LetterRun { length } = self.key
            && !(LETTER_RUN_MIN..=LETTER_RUN_MAX).contains(&length)
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

    /// A table position, saturating. A table past `u16::MAX` rows cannot be
    /// named on the wire and the encoder refuses it, so the clamp surfaces as
    /// that refusal rather than as a truncated index naming the wrong row.
    pub fn at(index: usize) -> Self {
        Self(u16::try_from(index).unwrap_or(u16::MAX))
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}
