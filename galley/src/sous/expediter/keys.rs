//! What makes two cached values interchangeable.
//!
//! ```text
//! the same projected chapter, the same schema  -> one ObservationKey
//! a renumbered table saying the same things    -> one TableHash
//! both sides of a pairing standing still       -> one PairKey
//! ```
//!
//! Every key here is content over position: a hash reads what a thing SAYS,
//! never where it sits, so a publication that renumbered a table or moved a
//! chapter still resolves to the value it already holds.

use sous_core::judge::{Channel, PatternKey};
use sous_core::substrate::ScalarKey;
use sous_core::{ChapterInput, ChapterPass, Pattern, PatternIndex, TerminalTable};
use xxhash_rust::xxh3::Xxh3Default;

use crate::pantry::RawChecksum;

/// One chapter's cache identity: xxh3-128 over its projected text, its rebased
/// verse rows, and the pass schema.
///
/// A markup-only edit moves the [`RawChecksum`] and not this, so the
/// observation is reused and only its coordinates are rebased. The chapter's
/// own address is absent, which is what lets identical chapters share one
/// entry — see `expediter.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservationKey([u8; 16]);

impl ObservationKey {
    pub(super) fn of<P: ChapterPass>(chapter: &ChapterInput<'_>) -> Self {
        let mut hasher = Xxh3Default::new();
        hasher.update(&(chapter.text.len() as u64).to_le_bytes());
        hasher.update(chapter.text.as_bytes());
        hasher.update(&(chapter.verses.len() as u64).to_le_bytes());
        for verse in chapter.verses {
            let key = verse.key();
            hasher.update(&key.chapter().to_le_bytes());
            hasher.update(&key.first().to_le_bytes());
            hasher.update(&key.last().to_le_bytes());
            hasher.update(&verse.text().from().to_le_bytes());
            hasher.update(&verse.text().to().to_le_bytes());
        }
        hasher.update(&P::SCHEMA.get().to_le_bytes());
        Self(hasher.digest128().to_be_bytes())
    }

    pub fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// What publication needs of one chapter without its text: whose observation,
/// and where to rebase it to.
///
/// No `ChapterKey`: a fold rebases by `start` and never reads an address.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChapterRow {
    pub(super) observation: ObservationKey,
    /// Projected-book offset the fold rebases chapter coordinates by.
    pub(super) start: u32,
}

/// One book's firing set by CONTENT: xxh3-128 over each pattern's glyph,
/// channel, and key, in table order.
///
/// Never over the indices — a publication renumbers the table whenever another
/// book's counts move a denominator, while what THIS book can be sited for is
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct FiringHash([u8; 16]);

impl FiringHash {
    pub(super) fn of(firing: &[PatternIndex], table: &[Pattern]) -> Self {
        let mut hasher = Xxh3Default::new();
        for index in firing {
            let pattern = &table[usize::from(index.get())];
            hasher.update(&pattern.glyph.raw().to_le_bytes());
            hasher.update(&[pattern.channel as u8]);
            hasher.update(&key_bytes(pattern.key));
        }
        Self(hasher.digest128().to_be_bytes())
    }
}

/// The whole pattern table by CONTENT: xxh3-128 over every row's glyph,
/// channel, and key, in table order.
///
/// What a book fires is a function of its own counts and these identities and
/// of nothing else — never of a numerator, which every keystroke anywhere in
/// the corpus moves — so two publications sharing this hash share every book's
/// firing set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TableHash([u8; 16]);

impl TableHash {
    pub(super) fn of(table: &[Pattern]) -> Self {
        let mut hasher = Xxh3Default::new();
        for pattern in table {
            hasher.update(&pattern.glyph.raw().to_le_bytes());
            hasher.update(&[pattern.channel as u8]);
            hasher.update(&key_bytes(pattern.key));
        }
        Self(hasher.digest128().to_be_bytes())
    }
}

/// The corpus evidence a chapter's word sites read beside its own text:
/// xxh3-128 over this publication's forcing glyphs, ascending.
///
/// Its own hash rather than a share of [`FiringHash`], because a firing set is
/// position-blind: the same rows fire while the terminal table decides
/// differently which of their occurrences are free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TerminalHash([u8; 16]);

impl TerminalHash {
    pub(super) fn of(table: Option<&TerminalTable>) -> Self {
        let mut hasher = Xxh3Default::new();
        // An absent table is not an empty one: a corpus may genuinely
        // capitalize after nothing.
        match table {
            None => hasher.update(&[0]),
            Some(table) => {
                hasher.update(&[1]);
                for glyph in table.forcing() {
                    hasher.update(&glyph.raw().to_le_bytes());
                }
            }
        }
        Self(hasher.digest128().to_be_bytes())
    }
}

/// One chapter's site identity: what it says, what its book fires, and what
/// the corpus's terminal table makes of that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ChapterSiteKey {
    pub(super) chapter: ObservationKey,
    pub(super) firing: FiringHash,
    pub(super) terminals: TerminalHash,
}

/// A pattern's identity across publications, which its table position is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PatternRef {
    glyph: ScalarKey,
    channel: Channel,
    key: PatternKey,
}

impl PatternRef {
    pub(super) fn of(pattern: &Pattern) -> Self {
        Self {
            glyph: pattern.glyph,
            channel: pattern.channel,
            key: pattern.key,
        }
    }
}

/// The `PatternKey` variant and its fields, flat, so the hash reads the claim
/// and not a pointer.
pub(super) fn key_bytes(key: PatternKey) -> [u8; 10] {
    let mut out = [0u8; 10];
    match key {
        PatternKey::ExactNeighbor(neighbor) => {
            out[0] = 0;
            out[1..5].copy_from_slice(&neighbor.raw().to_le_bytes());
        }
        PatternKey::RunShape { pure, bucket } => {
            out[..3].copy_from_slice(&[1, u8::from(pure), bucket]);
        }
        PatternKey::Placement { side, class } => {
            out[..3].copy_from_slice(&[2, side as u8, class as u8]);
        }
        PatternKey::Rarity => out[0] = 3,
        PatternKey::PooledNeighbor(pool) => out[..2].copy_from_slice(&[4, pool as u8]),
        PatternKey::Casing { hash, form } => {
            out[0] = 5;
            out[1] = form as u8;
            out[2..10].copy_from_slice(&hash.to_le_bytes());
        }
        PatternKey::WordLength { hash, sigma } => {
            out[0] = 6;
            out[1] = sigma;
            out[2..10].copy_from_slice(&hash.to_le_bytes());
        }
        PatternKey::Doubled { hash, separated } => {
            out[0] = 7;
            out[1] = u8::from(separated);
            out[2..10].copy_from_slice(&hash.to_le_bytes());
        }
        // The letter itself rides `glyph`, which the digest hashes beside
        // this; only the run length is the key's own.
        PatternKey::LetterRun { length } => out[..2].copy_from_slice(&[8, length]),
        // The glyph is the whole key; the digest hashes it beside this.
        PatternKey::SentenceStart => out[0] = 9,
    }
    out
}

/// What one book's pairing is a function of: its own raw checksum, the raw
/// checksum of the source book of its [`BookKey`], and whether the source-copy
/// words were walked. Both sides, because either moving is a different sample;
/// the walk, because a pairing made without it holds no runs and a knob that
/// only filters cached runs is not in here.
pub(super) type PairKey = (RawChecksum, RawChecksum, bool);
