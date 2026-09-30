//! What a firing row IS: the claim, its channel, and its place in the table.
//!
//! ```text
//! Pattern { glyph: ',', channel: Placement, key: Placement { .. } }
//! pattern.share_bp    -> 12      12 parts in 10,000 of the denominator
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
    /// One book whose rate of a `Placement` key breaks from the median rate
    /// of the other books. The one channel that gates on dispersion.
    BookRate = 10,
}

impl Channel {
    pub const ALL: [Self; 11] = [
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
        Self::BookRate,
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
            Self::BookRate => "BookRate",
        }
    }
}

/// Which side of a glyph a placement distribution describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Side {
    Prev = 0,
    Next = 1,
    /// Both at once, with the class `Letter` only: a mark inside a word.
    Both = 2,
}

impl Side {
    /// The two single sides, each its own marginal distribution.
    pub const ALL: [Self; 2] = [Self::Prev, Self::Next];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Prev => "prev",
            Self::Next => "next",
            Self::Both => "both",
        }
    }

    /// Whether a pair sees `class` on this side, or on both for [`Self::Both`].
    pub const fn sees(self, key: PairKey, class: OuterClass) -> bool {
        let prev = key.prev() as u8 == class as u8;
        let next = key.next() as u8 == class as u8;
        match self {
            Self::Prev => prev,
            Self::Next => next,
            Self::Both => prev && next,
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
    /// `side` is `Both` only with `class` `Letter`: the glyph inside a word.
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
    /// A `Placement` key in one book: the book is part of the claim, so two
    /// books breaking the same way are two rows.
    BookRate {
        side: Side,
        class: OuterClass,
        book: BookIndex,
    },
}

/// Judged books [`Channel::BookRate`] needs: one under test and three to take
/// a median of.
pub const BOOK_RATE_MIN_BOOKS: u32 = 4;

/// What the corpus does instead of the row's claim, one variant per channel
/// that names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Usual {
    /// The row carries its own explanation.
    None,
    /// The class most common on this side, `Edge` excluded, and its count.
    Placement { class: OuterClass, count: u32 },
    /// The scalar that most often follows the glyph in a run, its count, and
    /// how often the row's pair occurs reversed. A pair holding a directionless
    /// quote carries the facing most of its occurrences have, and `reversed`
    /// counts only runs facing the same way: `'.` closing against `.'` closing.
    ExactNeighbor {
        neighbor: ScalarKey,
        count: u32,
        reversed: u32,
        facing: Option<Facing>,
    },
    /// The glyph's most common run shape and its runs.
    RunShape { pure: bool, bucket: u8, count: u32 },
    /// The most common mark `confusables.txt` draws like the glyph (`'` for
    /// `’`) and its count, with `lookalike` set; else the most common other
    /// mark of the glyph's pool. `None` when neither exists.
    Rarity {
        glyph: Option<ScalarKey>,
        count: u32,
        lookalike: bool,
    },
    /// The word's most common form in free positions and its count.
    Casing { form: Form, count: u32 },
    /// The median rate of the other judged books in basis points, and how
    /// many of them there are.
    BookRate { baseline_bp: u16, books: u32 },
}

/// One firing pattern: a glyph, the channel that convicted it, the fraction
/// behind the claim, and what is usual instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pattern {
    pub glyph: ScalarKey,
    pub channel: Channel,
    pub key: PatternKey,
    /// Staircase step index; `None` for `Rarity` and `BookRate`, which have
    /// no band.
    pub band: Option<u8>,
    pub numerator: u32,
    pub denominator: u32,
    /// `numerator * 10_000 / denominator`, saturating.
    pub share_bp: u16,
    /// Books whose own counts hold part of the numerator, saturating at 255.
    ///
    /// Dispersion is information: genre clusters punctuation legitimately, so
    /// only [`Channel::BookRate`] gates on it, and its rows name one book
    /// each. Books-possible is the publication's own `book_count`.
    /// [`books_touched`] recomputes it.
    pub books: u8,
    pub usual: Usual,
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
        if self.band.is_none() != matches!(self.channel, Channel::Rarity | Channel::BookRate) {
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
            PatternKey::BookRate { .. } => Channel::BookRate,
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
        match self.key {
            PatternKey::Placement {
                side: Side::Both,
                class,
            } if class != OuterClass::Letter => return Err("key"),
            PatternKey::BookRate {
                side: Side::Both, ..
            } => return Err("key"),
            _ => {}
        }
        // A word channel judges no scalar, so the glyph field carries its hash.
        if self.channel.is_word() && self.glyph != ScalarKey::NONE {
            return Err("glyph");
        }
        self.usual_fits().then_some(()).ok_or("usual")
    }

    fn inside_a_word(&self) -> bool {
        matches!(
            self.key,
            PatternKey::Placement {
                side: Side::Both,
                ..
            }
        )
    }

    /// The variant belongs to the channel, and every count fits the row.
    fn usual_fits(&self) -> bool {
        let within = |count: u32| count <= self.denominator;
        match (self.channel, self.usual) {
            (Channel::Placement, Usual::Placement { class, count }) => {
                !self.inside_a_word() && class != OuterClass::Edge && within(count)
            }
            // What is usual instead of "inside a word" is the rest of the
            // denominator, which the row already carries.
            (Channel::Placement, Usual::None) => self.inside_a_word(),
            (Channel::ExactNeighbor, Usual::ExactNeighbor { count, facing, .. }) => {
                let PatternKey::ExactNeighbor(neighbor) = self.key else {
                    return false;
                };
                within(count)
                    && facing.is_some()
                        == (is_directionless(self.glyph) || is_directionless(neighbor))
            }
            (Channel::RunShape, Usual::RunShape { bucket, count, .. }) => {
                (1..=RUN_BUCKETS as u8).contains(&bucket) && within(count)
            }
            (
                Channel::Rarity,
                Usual::Rarity {
                    glyph,
                    count,
                    lookalike,
                },
            ) => match glyph {
                None => count == 0 && !lookalike,
                Some(glyph) if lookalike => {
                    rarity_lookalike(self.glyph, glyph) && count > 0 && within(count)
                }
                Some(glyph) => rarity_kin(self.glyph, glyph) && count > 0 && within(count),
            },
            (Channel::Casing, Usual::Casing { form, count }) => {
                form != Form::Uncased && within(count)
            }
            (Channel::BookRate, Usual::BookRate { baseline_bp, books }) => {
                let PatternKey::BookRate { class, .. } = self.key else {
                    return false;
                };
                class != OuterClass::Edge
                    && baseline_bp <= 10_000
                    && books >= BOOK_RATE_MIN_BOOKS - 1
                    && self.books == 1
            }
            (
                Channel::PooledNeighbor
                | Channel::WordLength
                | Channel::Doubled
                | Channel::LetterRun
                | Channel::SentenceStart,
                Usual::None,
            ) => true,
            _ => false,
        }
    }
}

/// One exact run a `RunShape` row lists beside its claim.
///
/// ```text
/// '"' mixed len 4    ."'"  x27  recurring     ?"'"  x2  recurring     .'?"  x1
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    /// The `RunShape` row this run is of.
    pub pattern: PatternIndex,
    /// The run's first [`Cluster::ATOMS`] atoms at most.
    pub atoms: Box<[ScalarKey]>,
    /// Corpus occurrences of this exact run, saturating.
    pub count: u32,
    /// The run, or the run with its sentence-ending marks read as one,
    /// occurs at least `support_floor` times: a convention, outside the row's
    /// numerator.
    pub recurring: bool,
    /// The run was longer than [`Cluster::ATOMS`] atoms.
    pub truncated: bool,
    /// The facing most of its occurrences have; `None` for a run holding no
    /// directionless quote.
    pub facing: Option<Facing>,
}

impl Cluster {
    /// Atoms a cluster keeps.
    pub const ATOMS: usize = 16;
    /// Clusters one row lists.
    pub const PER_ROW: usize = 8;
    /// Of those, the slots recurring clusters get when there are more novel
    /// ones than fit; either side lends the other what it leaves unused.
    pub const RECURRING_SLOTS: usize = 3;

    /// Whether this cluster can belong to `row`: a `RunShape` row whose glyph
    /// and shape the atoms show, as far as a truncated run can show them.
    pub fn fits(&self, row: &Pattern) -> bool {
        let PatternKey::RunShape { pure, bucket } = row.key else {
            return false;
        };
        let stored = !self.atoms.is_empty()
            && self.atoms.len() <= Self::ATOMS
            && self.count > 0
            && self.atoms.iter().all(|atom| !atom.is_digits());
        if !stored {
            return false;
        }
        let quoted = self.atoms.iter().any(|atom| is_directionless(*atom));
        if self.truncated {
            // The tail it lost may hold the quote a facing is about.
            return self.atoms.len() == Self::ATOMS
                && usize::from(bucket) == RUN_BUCKETS
                && (self.facing.is_some() || !quoted);
        }
        shape_of(&self.atoms, row.glyph) == Some((pure, bucket)) && self.facing.is_some() == quoted
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

    /// Every row of a table a publication can NAME, each with its index.
    ///
    /// The tail past `u16::MAX` rows is left out rather than clamped into the
    /// last addressable index: a firing set that named it would site the wrong
    /// row, where a table that long is refused whole at encoding
    /// (`PatternCountOverflow`).
    pub fn over(table: &[Pattern]) -> impl Iterator<Item = (Self, &Pattern)> {
        table
            .iter()
            .take(usize::from(u16::MAX))
            .enumerate()
            .map(|(at, pattern)| (Self(at as u16), pattern))
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}
