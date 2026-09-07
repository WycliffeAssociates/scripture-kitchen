//! The book fold: chapter rows merged by hash, and nothing else.
//!
//! ```text
//! fold_book([row("David went."), row("David wept.")])
//!   → hash(david) free [Lower 0, Title 1, Upper 0, Mixed 0] forced 1
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
        rows.extend(chapter.obs.words().iter().map(|word| WordTotal {
            hash: word.hash,
            free: [
                u32::from(word.free[0]),
                u32::from(word.free[1]),
                u32::from(word.free[2]),
                u32::from(word.free[3]),
            ],
            forced: u32::from(word.forced),
            len: word.len,
        }));
    }
    rows.sort_unstable_by_key(|row| row.hash);
    let mut out: Vec<WordTotal> = Vec::with_capacity(rows.len());
    for row in rows {
        match out.last_mut() {
            Some(last) if last.hash == row.hash => {
                for (slot, count) in last.free.iter_mut().zip(row.free) {
                    *slot = slot.saturating_add(count);
                }
                last.forced = last.forced.saturating_add(row.forced);
            }
            _ => out.push(row),
        }
    }
    WordAggregate::new(out, cased)
}
