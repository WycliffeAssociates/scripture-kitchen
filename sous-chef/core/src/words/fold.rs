//! The book fold: chapter rows merged by hash, and nothing else.
//!
//! ```text
//! fold_book([row("David went."), row("David wept.")])
//!   → hash(david) Start [Lower 0, Title 2, Upper 0, Mixed 0]
//! ```
//!
//! A word never crosses a masked `\c` — the chapter seam is an edge of text
//! for a word exactly as it is for a run — so there is no seam state here and
//! the fold is a plain merge. That rules the doubles lane too: a pair the seam
//! splits was never counted, so there is nothing to carry. Book coordinates
//! never enter: the row counts.

use super::{
    ChapterObs, DoubleTotal, LETTER_RUN_LANES, ScalarKey, WordAggregate, WordRow, WordTotal,
    merge_glyph_lane,
};

/// Merges one book's rows in order. Order is irrelevant to the result, which
/// is what makes a cached row and a fresh one indistinguishable.
pub fn fold_book(book: &[ChapterObs<&WordRow>]) -> WordAggregate {
    let mut rows: Vec<WordTotal> = Vec::new();
    let mut doubles: Vec<DoubleTotal> = Vec::new();
    let mut runs: Vec<(ScalarKey, [u32; LETTER_RUN_LANES])> = Vec::new();
    let mut cased = false;
    for chapter in book {
        cased |= chapter.obs.cased();
        runs.extend(
            chapter
                .obs
                .letter_runs()
                .iter()
                .map(|&(letter, lanes)| (letter, lanes.map(u32::from))),
        );
        rows.extend(chapter.obs.words().iter().map(|word| {
            WordTotal::new(
                word.hash,
                word.before().raw(),
                [
                    u32::from(word.counts[0]),
                    u32::from(word.counts[1]),
                    u32::from(word.counts[2]),
                    u32::from(word.counts[3]),
                ],
                word.len,
            )
        }));
        doubles.extend(chapter.obs.doubles().iter().map(|row| {
            DoubleTotal {
                hash: row.hash,
                uncased: u32::from(row.uncased),
                bare: u32::from(row.bare),
                separated: row
                    .separated
                    .iter()
                    .map(|&(glyph, count)| (glyph, u32::from(count)))
                    .collect(),
            }
        }));
    }
    rows.sort_unstable_by_key(|row| (row.hash, row.before_raw()));
    let mut out: Vec<WordTotal> = Vec::with_capacity(rows.len());
    for row in rows {
        match out.last_mut() {
            Some(last) if (last.hash, last.before_raw()) == (row.hash, row.before_raw()) => {
                for (slot, count) in last.counts.iter_mut().zip(row.counts) {
                    *slot = slot.saturating_add(count);
                }
            }
            _ => out.push(row),
        }
    }

    doubles.sort_unstable_by_key(|row| row.hash);
    let mut lane: Vec<DoubleTotal> = Vec::with_capacity(doubles.len());
    for row in doubles {
        match lane.last_mut() {
            Some(last) if last.hash == row.hash => {
                last.uncased = last.uncased.saturating_add(row.uncased);
                last.bare = last.bare.saturating_add(row.bare);
                last.separated = merge_glyph_lane(&last.separated, &row.separated, true);
            }
            _ => lane.push(row),
        }
    }

    runs.sort_unstable_by_key(|row| row.0);
    let mut letters: Vec<(ScalarKey, [u32; LETTER_RUN_LANES])> = Vec::with_capacity(runs.len());
    for row in runs {
        match letters.last_mut() {
            Some(last) if last.0 == row.0 => {
                for (slot, count) in last.1.iter_mut().zip(row.1) {
                    *slot = slot.saturating_add(count);
                }
            }
            _ => letters.push(row),
        }
    }

    // Reserved for every pre-merge row; a book merges to a third of that, and
    // this aggregate is what the resident cache keeps.
    out.shrink_to_fit();
    lane.shrink_to_fit();
    letters.shrink_to_fit();
    WordAggregate::new(out, lane, letters, cased)
}
