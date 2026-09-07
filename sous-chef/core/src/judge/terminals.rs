//! What the corpus capitalizes after.
//!
//! ```text
//! merged_follows(corpus)  -> [('.', 4_112 upper / 4_190), ('!', 96 / 98)]
//! table.forcing()         -> ['.', '!', '?']
//! ```
//!
//! A forcing glyph is corpus evidence, not a rule: the table says which
//! scalars this corpus treats as sentence enders, and the word channels read
//! it to decide which occurrences of a word are free to vary.

use super::*;

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
pub(super) fn forces_a_capital(counts: FollowCounts, config: &JudgingConfig) -> bool {
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
