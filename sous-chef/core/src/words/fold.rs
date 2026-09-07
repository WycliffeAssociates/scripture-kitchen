//! The book fold: chapter rows merged by hash, and nothing else.
//!
//! ```text
//! fold_book([row("David went."), row("David wept.")])
//!   → hash(david) Start [Lower 0, Title 2, Upper 0, Mixed 0]
//! ```
//!
//! A word never crosses a masked `\c` — the chapter seam is an edge of text
//! for a word exactly as it is for a run — so there is no seam state here and
//! the fold is a plain merge. Book coordinates never enter: the row counts.

use super::{ChapterObs, WordAggregate, WordRow, WordTotal};

/// Merges one book's rows in order. Order is irrelevant to the result, which
/// is what makes a cached row and a fresh one indistinguishable.
pub fn fold_book(book: &[ChapterObs<&WordRow>]) -> WordAggregate {
    let mut rows: Vec<WordTotal> = Vec::new();
    let mut cased = false;
    for chapter in book {
        cased |= chapter.obs.cased();
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
    // Reserved for every pre-merge row; a book merges to a third of that, and
    // this aggregate is what the resident cache keeps.
    out.shrink_to_fit();
    WordAggregate::new(out, cased)
}
