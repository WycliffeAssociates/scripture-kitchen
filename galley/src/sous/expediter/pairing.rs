//! A target's comparison against its source is a pure function of both sides'
//! rows.
//!
//! ```text
//! both checksums unmoved, the lane unmoved  -> the retained PairedBook
//! the target edited                         -> that one book re-paired
//! the source withdrawn                      -> the whole cache dropped
//! ```
//!
//! The key is both raw checksums and whether the source-copy words were
//! walked, so nothing here has to be invalidated: a side that moved simply
//! misses.

use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::{
    AlignmentFact, BookKey, ChapterPass, Findings, PairedBook, ProjectSpread, ProjectedBook,
    SourceVerse, SourceWords, judge_paired,
};

use super::keys::PairKey;
use super::siting::projection;
use super::{OnionBook, PublishError};
use crate::pantry::derived::Store;
use crate::pantry::{BookId, Pantry, RawChecksum};

/// Pairs every target against the reference of its [`BookKey`] and judges the
/// ratios, filling `views` with any projection the source-copy walk had to
/// build. Returns the books paired and the references that kept no word lane.
#[allow(clippy::too_many_arguments)]
pub(super) fn pair_and_judge<P: ChapterPass>(
    pass: &P,
    config: &P::Config,
    pantry: &Pantry,
    books: &[(BookId, BookKey)],
    references: &[(BookId, BookKey)],
    checksums: &[RawChecksum],
    corpus: &[&P::Aggregate],
    paired: &mut Store<PairKey, PairedBook>,
    project: &mut Option<(Vec<PairKey>, ProjectSpread)>,
    views: &mut [Option<OnionBook>],
    findings: &mut Findings,
) -> Result<(u64, u64), PublishError> {
    let (mut pairings, mut wordless) = (0, 0);
    // Then the source comparison, from lengths both sides already
    // retain. Every Target pairs with the Reference of the same
    // `BookKey`; a Target with none gets no ratios and no rows,
    // which is the contract and not an error. The pairing facts
    // are alignment structure, never findings, so they are
    // dropped here — a host that wants them runs the cold path.
    //
    // Only a book whose own checksum or whose source's moved is
    // paired again: the ratios, their order statistics, and the
    // presence rows are a pure function of both sides' rows
    // (`expediter.md`).
    // All three channels, the same set `judge_paired` gates on: a
    // host running source-copy alone still pairs.
    let lengths = pass
        .length_config(config)
        .filter(|lengths| lengths.enabled || lengths.presence || lengths.source_copy);
    // First wins: a caller may present two files under one key, and
    // the choice has to be its order rather than a hash's.
    type Source<'a> = (RawChecksum, &'a [SourceVerse], Option<&'a SourceWords>);
    let mut sources: FxHashMap<BookKey, Source<'_>> = FxHashMap::default();
    let copying = lengths.is_some_and(|lengths| lengths.source_copy);
    if lengths.is_some() {
        for (id, key) in references {
            let Some(verses) = pantry.reference_lengths(id) else {
                continue;
            };
            let checksum = pantry.checksum(id).expect("the pantry listed this id");
            let words = copying.then(|| pantry.reference_words(id)).flatten();
            sources.entry(*key).or_insert((checksum, verses, words));
        }
    }
    match lengths.filter(|_| !sources.is_empty()) {
        Some(lengths) => {
            let mut facts: Vec<AlignmentFact> = Vec::new();
            let mut keys: Vec<PairKey> = Vec::with_capacity(books.len());
            let mut slots: Vec<Option<PairKey>> = Vec::with_capacity(books.len());
            for (index, ((id, key), aggregate)) in books.iter().zip(corpus).enumerate() {
                let Some((source, verses, words)) = sources.get(key) else {
                    slots.push(None);
                    continue;
                };
                if copying && words.is_none() {
                    wordless += 1;
                }
                let walked = copying && words.is_some();
                let entry = (checksums[index], *source, walked);
                if let Entry::Vacant(slot) = paired.entry(entry) {
                    facts.clear();
                    // The one text read on this path, and only for
                    // a book whose own side or whose source moved:
                    // the target's projection, rebuilt from the
                    // products it already retains.
                    if walked && views[index].is_none() {
                        views[index] = Some(projection(pantry, id)?);
                    }
                    let copy = walked
                        .then(|| views[index].as_ref().zip(*words))
                        .flatten()
                        .map(|(view, words)| (view.text(), words));
                    slot.insert(PairedBook::pair_with(
                        *key,
                        pass.verse_lengths(aggregate),
                        verses,
                        copy,
                        &mut facts,
                    ));
                    pairings += 1;
                }
                keys.push(entry);
                slots.push(Some(entry));
            }
            let hit = matches!(&*project, Some((seen, _)) if *seen == keys);
            if !hit {
                // Sweep by live keys: an entry no target names has
                // no book on either side any more.
                let live: FxHashSet<PairKey> = keys.iter().copied().collect();
                if paired.len() > live.len() {
                    paired.keep_live(|key| live.contains(key));
                }
            }
            let rows: Vec<Option<&PairedBook>> = slots
                .iter()
                .map(|slot| slot.map(|key| &paired[&key]))
                .collect();
            let spread = match (hit, &*project) {
                (true, Some((_, spread))) => *spread,
                _ => {
                    let spread = ProjectSpread::of(&rows);
                    *project = Some((keys, spread));
                    spread
                }
            };
            judge_paired(&rows, &spread, &lengths, findings);
        }
        // No source, or the lane switched off: the cache is a
        // whole corpus of ratios, and nothing is left to key it.
        None => {
            paired.clear();
            *project = None;
        }
    }
    Ok((pairings, wordless))
}
