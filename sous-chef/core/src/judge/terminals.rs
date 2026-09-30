//! What the corpus capitalizes after.
//!
//! ```text
//! merged_follows(corpus)  -> [(('.', bare), 4_112 upper / 4_190),
//!                             ((',', bare), 31 / 4_836),
//!                             ((',', quoted), 1_204 / 1_210)]
//! table.forcing()         -> [('.', bare), (',', quoted)]   // a bare comma is no terminal
//! table.mixed()           -> []
//! ```

use super::*;

// ── The terminal table ──────────────────────────────────────────────────

/// Which handoff contexts this corpus puts a capital after, learned from the
/// substrate's `follows` lane rather than listed.
///
/// ```text
/// learn(WA-en-ulb: ('.', bare) 33,353 of 33,359, (',', bare) 4,841 of 47,299,
///                  (',', quoted) 6,748 of 7,156, ('.', bracketed) 33 of 54)
///   at 8,000 bp   ('.', bare) 9,998 bp forces, (',', quoted) 9,429 bp forces,
///                 (',', bare) 1,023 bp and ('.', bracketed) 6,111 bp do not
/// ```
///
/// A context forces when the share of the cased letters it hands off to that
/// are uppercase reaches [`JudgingConfig::terminal_upper_share_bp`], on at
/// least `support_floor` handoffs. Each [`FollowKey`] is judged on its own
/// counts, so `, "` can force where `,` does not, and does not where a corpus
/// writes speech in lowercase; `.)` likewise, where a parenthetical ends
/// mid-sentence.
///
/// A context whose share lies strictly between
/// [`JudgingConfig::terminal_lower_share_bp`] and the upper share is MIXED:
/// the punctuation neither chose the capital nor left the word to choose, so
/// a word after it is not casing evidence either way.
///
/// ```text
/// ('.', bare)       9,998 bp   forces
/// ('.', bracketed)  6,111 bp   mixed    `Anakim.) Then the land` judges nothing
/// (',', bare)       1,023 bp   free     at or under 2,000
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalTable {
    forcing: Box<[FollowKey]>,
    mixed: Box<[FollowKey]>,
}

impl TerminalTable {
    /// The forcing contexts of a corpus-merged follow lane.
    pub fn learn(follows: &[(FollowKey, FollowCounts)], config: &JudgingConfig) -> Self {
        let mut forcing: Vec<FollowKey> = Vec::new();
        let mut mixed: Vec<FollowKey> = Vec::new();
        for (key, counts) in follows {
            let Some(share) = upper_share(*counts, config) else {
                continue;
            };
            if share >= config.terminal_upper_share_bp {
                forcing.push(*key);
            } else if share > config.terminal_lower_share_bp {
                mixed.push(*key);
            }
        }
        for keys in [&mut forcing, &mut mixed] {
            keys.sort_unstable();
            keys.dedup();
        }
        Self {
            forcing: forcing.into_boxed_slice(),
            mixed: mixed.into_boxed_slice(),
        }
    }

    /// Every forcing context, ascending.
    pub fn forcing(&self) -> &[FollowKey] {
        &self.forcing
    }

    /// Every mixed context, ascending.
    pub fn mixed(&self) -> &[FollowKey] {
        &self.mixed
    }

    pub fn forces(&self, key: FollowKey) -> bool {
        self.forcing.binary_search(&key).is_ok()
    }

    pub fn is_mixed(&self, key: FollowKey) -> bool {
        self.mixed.binary_search(&key).is_ok()
    }

    /// Whether the word after this context chose its own case: neither forced
    /// nor mixed.
    pub fn frees(&self, key: FollowKey) -> bool {
        !self.forces(key) && !self.is_mixed(key)
    }

    pub fn is_empty(&self) -> bool {
        self.forcing.is_empty() && self.mixed.is_empty()
    }
}

/// Entitlement and the share, in one place: the denominator is the cased
/// handoffs, so a context followed only by uncased letters decides nothing,
/// and one under `support_floor` of them is `None` and stays free.
fn upper_share(counts: FollowCounts, config: &JudgingConfig) -> Option<u16> {
    let upper = u64::from(counts.get(Case::Upper));
    let cased = upper + u64::from(counts.get(Case::Lower));
    (cased >= u64::from(config.support_floor)).then(|| share_bp(upper, cased))
}

/// Every book's follow lane merged into one, by key ascending.
pub fn merged_follows(corpus: &[&BookAggregate]) -> Vec<(FollowKey, FollowCounts)> {
    let mut out: Vec<(FollowKey, FollowCounts)> = Vec::new();
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

/// The rows of one lane the moved keys name, at or after `at`, or `None` when
/// the lane holds none of them.
///
/// Both sequences ascend, so the cursor only ever moves forward and the walk
/// is one tandem merge rather than a probe per key.
pub(super) fn seek<T, K: Ord>(
    rows: &[T],
    at: &mut usize,
    wanted: &K,
    key: impl Fn(&T) -> K,
) -> bool {
    while rows.get(*at).is_some_and(|row| key(row) < *wanted) {
        *at += 1;
    }
    rows.get(*at).is_some_and(|row| key(row) == *wanted)
}
