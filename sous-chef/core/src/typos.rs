//! An on-demand reviewer action, not a channel: rare words one
//! Damerau-Levenshtein edit from a frequent word, grouped by the frequent
//! target they might be a slip of.
//!
//! ```text
//! typo_candidates(WordCounts { words: [
//!     WordEntry { text: "the",   count: 300, title_seen: false, refs: vec![] },
//!     WordEntry { text: "teh",   count: 1,   title_seen: false, refs: vec!["GEN 1:1".into()] },
//! ] }, &TypoConfig::default())
//!   → [ TypoGroup {
//!         target: ("the", 300),
//!         candidates: [("teh", 1, ["GEN 1:1"])],
//!     } ]
//! ```
//!
//! No BK-tree: every rare word's Damerau-Levenshtein-1 neighbourhood (one
//! insertion, deletion, substitution, or adjacent transposition of scalars)
//! is generated over the corpus's OWN scalar alphabet and each variant is
//! looked up by hash in the word→count table the caller already built. A
//! candidate is a rare word with at least one neighbour at or above the
//! frequent floor.
//!
//! Grouped by frequent TARGET, never a flat ranked list: a flat list ranked
//! by neighbour frequency collapses onto a handful of high-degree function
//! words on a short-word language (`evidence.md`, 2026-09-07 re-measure —
//! `will`, `for`, `that` swallowed English's top 25). A reviewer works one
//! frequent word's collision set at a time instead.
//!
//! `rare_word_candidates` is pure and holds no shared state, so a caller
//! (the CLI) may run it in parallel across rare words with `rayon` and then
//! call [`group_candidates`] once over the collected results —
//! [`typo_candidates`] itself is the serial reference composition of both,
//! kept for tests and small inputs. See `typos.md` and the two 2026-09-07
//! `evidence.md` rows this ported from `core/examples/edit_neighbors.rs`.

use std::collections::BTreeSet;

use rustc_hash::{FxHashMap, FxHashSet};

/// Plain knobs, none of them a `JudgingConfig` concern: this is not judging,
/// it never fires a wire row, and it runs only when a reviewer asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypoConfig {
    /// Fewer occurrences than this is a rare word: a typo candidate.
    pub rare_below: u32,
    /// At least this many occurrences makes a neighbour "frequent" — the
    /// word a typo would have meant.
    pub frequent_at_least: u32,
    /// A rare word must hold at least this many scalars to be scanned.
    pub min_scalars: usize,
    /// Skip a rare word that was ever seen in Title form anywhere in the
    /// corpus: a likely proper name, not a slip.
    pub skip_title: bool,
    /// Require the frequent neighbour to share the rare word's first
    /// scalar.
    pub shared_first: bool,
}

impl Default for TypoConfig {
    fn default() -> Self {
        Self {
            rare_below: 5,
            frequent_at_least: 200,
            min_scalars: 4,
            skip_title: true,
            shared_first: true,
        }
    }
}

/// One case-folded word's observed count, whether any occurrence stood in
/// Title form, and up to a few verse references the caller kept while
/// walking (typically the first occurrences, capped at three).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordEntry {
    pub text: String,
    pub count: u32,
    pub title_seen: bool,
    pub refs: Vec<String>,
}

/// A corpus's distinct case-folded words — the whole pure algorithm's input.
/// The caller builds this while walking the corpus text; nothing here knows
/// about USFM, Onion, or a book's coordinates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordCounts {
    pub words: Vec<WordEntry>,
}

/// A `WordCounts` lookup by text, built once and reused by every rare word's
/// neighbourhood scan: the hash lookup the module's doc promises, not a
/// BK-tree.
pub struct WordIndex<'w> {
    by_text: FxHashMap<&'w str, usize>,
}

impl<'w> WordIndex<'w> {
    pub fn build(words: &'w WordCounts) -> Self {
        Self {
            by_text: words
                .words
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.text.as_str(), index))
                .collect(),
        }
    }

    fn get(&self, text: &str) -> Option<usize> {
        self.by_text.get(text).copied()
    }
}

/// One rare word grouped under one frequent target it might be a slip of.
/// Candidates are sorted by count ascending, then text; groups are sorted
/// (by [`group_candidates`]) by candidate count descending, then target
/// text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypoGroup {
    pub target: (String, u32),
    pub candidates: Vec<(String, u32, Vec<String>)>,
}

/// The distinct scalars this corpus's words are built from — not a fixed
/// ASCII set, so an Ethiopic or Devanagari corpus gets its own alphabet.
pub fn alphabet_of(words: &WordCounts) -> Vec<char> {
    let mut set = BTreeSet::new();
    for entry in &words.words {
        set.extend(entry.text.chars());
    }
    set.into_iter().collect()
}

/// Whether `entry` is even worth generating a neighbourhood for, before any
/// neighbourhood is generated at all — a word this filters out costs
/// nothing.
pub fn is_rare_candidate(entry: &WordEntry, config: &TypoConfig) -> bool {
    entry.count < config.rare_below
        && !(config.skip_title && entry.title_seen)
        && entry.text.chars().count() >= config.min_scalars
}

/// Calls `visit` with every scalar string one Damerau-Levenshtein edit away
/// from `word`, over `alphabet`. Duplicates are possible (a substitution can
/// reproduce an insertion's result); the caller dedupes what it keeps.
fn edit_neighbors(word: &[char], alphabet: &[char], mut visit: impl FnMut(&[char])) {
    let mut buf: Vec<char> = Vec::with_capacity(word.len() + 1);

    // Deletions: one scalar removed.
    for i in 0..word.len() {
        buf.clear();
        buf.extend_from_slice(&word[..i]);
        buf.extend_from_slice(&word[i + 1..]);
        visit(&buf);
    }

    // Insertions: one scalar added at every gap.
    for i in 0..=word.len() {
        for &letter in alphabet {
            buf.clear();
            buf.extend_from_slice(&word[..i]);
            buf.push(letter);
            buf.extend_from_slice(&word[i..]);
            visit(&buf);
        }
    }

    // Substitutions: one scalar replaced by a different one.
    for i in 0..word.len() {
        for &letter in alphabet {
            if letter == word[i] {
                continue;
            }
            buf.clear();
            buf.extend_from_slice(&word[..i]);
            buf.push(letter);
            buf.extend_from_slice(&word[i + 1..]);
            visit(&buf);
        }
    }

    // Adjacent transpositions.
    for i in 0..word.len().saturating_sub(1) {
        if word[i] == word[i + 1] {
            continue; // swapping two equal scalars is not an edit
        }
        buf.clear();
        buf.extend_from_slice(word);
        buf.swap(i, i + 1);
        visit(&buf);
    }
}

/// Every distinct frequent word (as an index into `words.words`) that `word`
/// is one edit from, over `alphabet`, subject to `config`'s filters.
///
/// Pure and side-effect free: no shared state crosses one call to the next,
/// so a caller may run this in parallel across rare words (the CLI does,
/// with `rayon`) and fold the results with [`group_candidates`] afterward.
pub fn rare_word_candidates(
    word: &WordEntry,
    words: &WordCounts,
    index: &WordIndex<'_>,
    alphabet: &[char],
    config: &TypoConfig,
) -> Vec<usize> {
    let chars: Vec<char> = word.text.chars().collect();
    let Some(&first) = chars.first() else {
        return Vec::new();
    };
    let mut matched = FxHashSet::default();
    let mut out = Vec::new();
    let mut scratch = String::new();
    edit_neighbors(&chars, alphabet, |variant| {
        if config.shared_first && variant.first() != Some(&first) {
            return;
        }
        scratch.clear();
        scratch.extend(variant.iter());
        let Some(candidate_index) = index.get(&scratch) else {
            return;
        };
        let candidate = &words.words[candidate_index];
        if candidate.count >= config.frequent_at_least
            && candidate.text != word.text
            && matched.insert(candidate_index)
        {
            out.push(candidate_index);
        }
    });
    out
}

/// Groups every rare word's matches by frequent target: `rare` and `matches`
/// are parallel slices (`matches[i]` is `rare[i]`'s [`rare_word_candidates`]
/// result), so a caller may have computed `matches` serially or in parallel.
///
/// Groups are ordered by candidate count descending, then target text;
/// candidates within a group by count ascending, then text — the smallest,
/// most surprising slip first.
pub fn group_candidates(
    words: &WordCounts,
    rare: &[&WordEntry],
    matches: &[Vec<usize>],
) -> Vec<TypoGroup> {
    let mut groups: FxHashMap<usize, Vec<(String, u32, Vec<String>)>> = FxHashMap::default();
    for (word, targets) in rare.iter().zip(matches) {
        for &target_index in targets {
            groups.entry(target_index).or_default().push((
                word.text.clone(),
                word.count,
                word.refs.clone(),
            ));
        }
    }
    let mut out: Vec<TypoGroup> = groups
        .into_iter()
        .map(|(target_index, mut candidates)| {
            candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let target = &words.words[target_index];
            TypoGroup {
                target: (target.text.clone(), target.count),
                candidates,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.candidates
            .len()
            .cmp(&a.candidates.len())
            .then_with(|| a.target.0.cmp(&b.target.0))
    });
    out
}

/// The whole algorithm, serial: builds the alphabet and index once, scans
/// every rare word, and groups the result. The reference composition
/// [`rare_word_candidates`] and [`group_candidates`] exist to be split
/// across — this is what a caller with no reason to parallelize runs
/// directly, and what the unit tests below exercise.
pub fn typo_candidates(words: &WordCounts, config: &TypoConfig) -> Vec<TypoGroup> {
    let alphabet = alphabet_of(words);
    let index = WordIndex::build(words);
    let rare: Vec<&WordEntry> = words
        .words
        .iter()
        .filter(|entry| is_rare_candidate(entry, config))
        .collect();
    let matches: Vec<Vec<usize>> = rare
        .iter()
        .map(|word| rare_word_candidates(word, words, &index, &alphabet, config))
        .collect();
    group_candidates(words, &rare, &matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, count: u32) -> WordEntry {
        WordEntry {
            text: text.to_string(),
            count,
            title_seen: false,
            refs: Vec::new(),
        }
    }

    fn entry_with_refs(text: &str, count: u32, refs: &[&str]) -> WordEntry {
        WordEntry {
            text: text.to_string(),
            count,
            title_seen: false,
            refs: refs.iter().map(|r| r.to_string()).collect(),
        }
    }

    /// `the` x300 with `teh` x1: one group, one candidate. `the`/`teh` are
    /// 3 scalars, under the default 4-scalar floor, so this isolates the
    /// core match with the floor off; `a_short_rare_word_is_skipped_by_the_scalar_floor`
    /// below covers the floor itself under the default config.
    #[test]
    fn a_frequent_word_and_its_rare_transposition_form_one_group() {
        let words = WordCounts {
            words: vec![
                entry_with_refs("the", 300, &[]),
                entry_with_refs("teh", 1, &["GEN 1:1"]),
            ],
        };
        let config = TypoConfig {
            min_scalars: 0,
            ..TypoConfig::default()
        };
        let groups = typo_candidates(&words, &config);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].target, ("the".to_string(), 300));
        assert_eq!(groups[0].candidates.len(), 1);
        assert_eq!(groups[0].candidates[0].0, "teh");
        assert_eq!(groups[0].candidates[0].1, 1);
        assert_eq!(groups[0].candidates[0].2, vec!["GEN 1:1".to_string()]);
    }

    /// A rare word ever seen Title-cased is a likely proper name and is
    /// skipped: `Esek`/`esek` one edit from `seek`.
    #[test]
    fn a_title_cased_rare_word_is_skipped() {
        let mut esek = entry("esek", 1);
        esek.title_seen = true;
        let words = WordCounts {
            words: vec![esek, entry("seek", 213)],
        };
        let groups = typo_candidates(&words, &TypoConfig::default());
        assert!(groups.is_empty());
    }

    /// A rare word under the scalar floor is skipped: `fox` (3 scalars)
    /// against the default floor of 4.
    #[test]
    fn a_short_rare_word_is_skipped_by_the_scalar_floor() {
        let words = WordCounts {
            words: vec![entry("fox", 2), entry("for", 9353)],
        };
        let groups = typo_candidates(&words, &TypoConfig::default());
        assert!(groups.is_empty());
    }

    /// A frequent neighbour whose first scalar differs from the rare word's
    /// is skipped: `bother` against `other` shares no first letter.
    #[test]
    fn a_neighbour_with_a_different_first_scalar_is_skipped() {
        let words = WordCounts {
            words: vec![entry("bother", 1), entry("other", 300)],
        };
        let groups = typo_candidates(&words, &TypoConfig::default());
        assert!(groups.is_empty());
    }

    /// A rare word within one edit of two different frequent words appears
    /// in both of their groups: `boot` is one substitution from `boat` and
    /// one from `book`, both sharing its first scalar.
    #[test]
    fn a_rare_word_may_join_more_than_one_groups_candidates() {
        let words = WordCounts {
            words: vec![entry("boot", 1), entry("boat", 300), entry("book", 250)],
        };
        let groups = typo_candidates(&words, &TypoConfig::default());
        let targets: Vec<&str> = groups.iter().map(|g| g.target.0.as_str()).collect();
        assert_eq!(targets, vec!["boat", "book"]);
        for group in &groups {
            assert_eq!(group.candidates[0].0, "boot");
        }
    }

    /// The parallel-friendly split composes to the same result as the
    /// serial reference.
    #[test]
    fn split_composition_matches_the_serial_reference() {
        let words = WordCounts {
            words: vec![entry("mungu", 4423), entry("muungu", 4)],
        };
        let config = TypoConfig::default();
        let serial = typo_candidates(&words, &config);

        let alphabet = alphabet_of(&words);
        let index = WordIndex::build(&words);
        let rare: Vec<&WordEntry> = words
            .words
            .iter()
            .filter(|entry| is_rare_candidate(entry, &config))
            .collect();
        let matches: Vec<Vec<usize>> = rare
            .iter()
            .map(|word| rare_word_candidates(word, &words, &index, &alphabet, &config))
            .collect();
        let split = group_candidates(&words, &rare, &matches);

        assert_eq!(serial, split);
        assert_eq!(serial.len(), 1);
        assert_eq!(serial[0].target.0, "mungu");
    }
}
