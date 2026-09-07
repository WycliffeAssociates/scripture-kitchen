//! One byte ceiling, and every resident byte attributed to a tier.
//!
//! ```text
//! pantry.budget().ceiling()   -> 16_777_216
//! pantry.tally()              -> Tally { pinned: 4_512_003,
//!                                        hot: 0,
//!                                        rebuildable: 6_210_944 }
//! tally.total() == pantry.resident_bytes()      always, with no residual
//! ```
//!
//! THE CEILING BITES IN ONE PLACE: chunk products, the rebuildable tier's
//! LRU. Every other tier is counted and reported, never evicted; enforcing
//! them is a later measured slice.

/// Where a resident byte sits, which is what it costs to lose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// A `Target`'s text and its book products. Publication rescans that text
    /// to place a finding, so losing it is losing a coordinate: never evicted.
    Pinned,
    /// The hot set's chapter rows — the price of re-mapping one chapter
    /// instead of a whole book. Evicted last.
    Hot,
    /// Chunk products and derived values: content-addressed, so a lost entry
    /// is a miss and never a wrong answer. Evicted by weight and recency.
    Rebuildable,
}

/// The declared ceiling on resident bytes.
///
/// A ceiling is what makes a session-long cache safe: wasm linear memory grows
/// and never shrinks, so a high-water mark is permanent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    ceiling: usize,
}

impl Budget {
    pub const fn new(ceiling: usize) -> Self {
        Self { ceiling }
    }

    pub const fn ceiling(self) -> usize {
        self.ceiling
    }

    /// Whether this many bytes of evictable weight is over the ceiling.
    pub const fn over(self, bytes: usize) -> bool {
        bytes > self.ceiling
    }
}

/// Resident bytes by tier — the accounting half, with no residual: a
/// `Tally`'s total is the same number `resident_bytes()` reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub pinned: usize,
    pub hot: usize,
    pub rebuildable: usize,
}

impl Tally {
    pub fn add(&mut self, tier: Tier, bytes: usize) {
        match tier {
            Tier::Pinned => self.pinned += bytes,
            Tier::Hot => self.hot += bytes,
            Tier::Rebuildable => self.rebuildable += bytes,
        }
    }

    pub fn total(self) -> usize {
        self.pinned + self.hot + self.rebuildable
    }
}

impl core::ops::Add for Tally {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            pinned: self.pinned + other.pinned,
            hot: self.hot + other.hot,
            rebuildable: self.rebuildable + other.rebuildable,
        }
    }
}
