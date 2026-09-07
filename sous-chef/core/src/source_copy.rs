//! Consecutive target words that all appear in the paired source verse.
//!
//! ```text
//! source MRK 1:1  "The beginning of the good news"
//!   hashes  {and? no}  sorted deduplicated u32, one set per verse
//! target MRK 1:1  "Mwanzo of the good news wa Yesu"
//!   words   Mwanzo  of  the  good  news  wa  Yesu
//!   present    no   yes yes  yes   yes   no   no
//!   run of 4, eligible 7   →  SourceCopyRow  span over `of the good news`
//! ```
//!
//! The claim is exactly that: these N consecutive words of the target verse
//! each appear, as the same exact scalar sequence, in the paired source verse.
//! It does not say untranslated — a run of shared names or a quotation is a
//! legitimate row, and the run itself is the evidence a reviewer reads. What
//! is not modelled, and the collision bound a 32-bit hash carries:
//! `sous-chef/rules/source-copy-residue.md`. The computation and its memory:
//! `source_copy.md`.

use xxhash_rust::xxh3::xxh3_64;

use crate::substrate::VerseLength;
use crate::words::for_each_word;
use crate::{ProjectedBook, TextRange};

/// Runs shorter than this are never cached, so `source_copy_min_run` stays a
/// judge-time knob rather than part of a pair cache's key. One shared word is
/// not a run under any reading of the claim.
pub const MIN_RUN: u32 = 2;

/// One word's wire-stable identity: the low 32 bits of xxh3-64 over its raw
/// UTF-8 bytes.
///
/// NOT case-folded. A paste preserves case, and case-insensitive matching is a
/// separate claim about a different question.
pub fn word_hash(word: &str) -> u32 {
    xxh3_64(word.as_bytes()) as u32
}

/// One source book's word sets: every verse's sorted deduplicated hashes, in
/// one flat lane with per-verse offsets.
///
/// Index-aligned with the `SourceVerse` rows [`crate::source_lengths`] counts
/// from the same book, so a caller holds one lane beside the other and never a
/// map. No positions, no strings, no token tape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceWords {
    hashes: Box<[u32]>,
    /// `(start, len)` into [`Self::hashes`], one per verse in TOC order.
    spans: Box<[(u32, u32)]>,
}

impl SourceWords {
    /// Every keyed verse of a projected book, hashed and set-ified.
    pub fn of(book: &impl ProjectedBook) -> Self {
        let text = book.text();
        let mut hashes: Vec<u32> = Vec::new();
        let mut spans: Vec<(u32, u32)> = Vec::new();
        for verse in book.verses() {
            let span = verse.text();
            let slice = &text[span.from() as usize..span.to() as usize];
            let start = hashes.len();
            for_each_word(slice, &[], |word| {
                hashes.push(word_hash(&slice[word.from as usize..word.to as usize]));
            });
            hashes[start..].sort_unstable();
            let len = dedup_len(&mut hashes[start..]);
            hashes.truncate(start + len);
            spans.push((
                u32::try_from(start).expect("a book holds under 4G words"),
                u32::try_from(len).expect("a verse holds under 4G words"),
            ));
        }
        hashes.shrink_to_fit();
        Self {
            hashes: hashes.into(),
            spans: spans.into(),
        }
    }

    /// One verse's set, in sorted order; empty for an index this book has no
    /// row for.
    pub fn verse(&self, index: usize) -> &[u32] {
        let Some((start, len)) = self.spans.get(index).copied() else {
            return &[];
        };
        &self.hashes[start as usize..(start + len) as usize]
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Inline size plus both lanes: what a resident host pays for one book.
    pub fn resident_bytes(&self) -> usize {
        size_of::<Self>() + size_of_val(&*self.hashes) + size_of_val(&*self.spans)
    }
}

/// Sorts-and-dedups in place, returning the surviving length.
fn dedup_len(sorted: &mut [u32]) -> usize {
    let mut write = 0;
    for read in 0..sorted.len() {
        if write == 0 || sorted[write - 1] != sorted[read] {
            sorted[write] = sorted[read];
            write += 1;
        }
    }
    write
}

/// One maximal run: the target span it covers, its length in words, and the
/// eligible target words of the unit it sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCopyRow {
    span: TextRange,
    run: u32,
    eligible: u32,
}

impl SourceCopyRow {
    /// First byte of the run's first word through the last byte of its last.
    pub const fn span(self) -> TextRange {
        self.span
    }

    /// Consecutive target words, all present in the paired source.
    pub const fn run(self) -> u32 {
        self.run
    }

    /// Words the whole paired unit offered — the denominator a reviewer reads
    /// the run against.
    pub const fn eligible(self) -> u32 {
        self.eligible
    }
}

/// Every maximal run of at least [`MIN_RUN`] words in one paired unit.
///
/// `left` is the unit's target rows as positions into `target`, and `source`
/// the sorted union of its source constituents' word sets. Run state carries
/// across a bridge's constituents: a bridge is one unit and one sentence.
///
/// A word the walk never emits — a run of digits with no letter, a verse
/// number or a year — is neither eligible nor a break in a run.
pub(crate) fn unit_rows(
    text: &str,
    target: &[VerseLength],
    left: &[usize],
    source: &[u32],
    out: &mut Vec<SourceCopyRow>,
) {
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    let mut open: Option<(u32, u32, u32)> = None;
    let mut eligible: u32 = 0;
    for at in left {
        let span = target[*at].text();
        let base = span.from();
        let slice = &text[span.from() as usize..span.to() as usize];
        for_each_word(slice, &[], |word| {
            eligible = eligible.saturating_add(1);
            let raw = &slice[word.from as usize..word.to as usize];
            if source.binary_search(&word_hash(raw)).is_err() {
                if let Some(run) = open.take() {
                    runs.push(run);
                }
                return;
            }
            match &mut open {
                Some(run) => {
                    run.1 = base + word.to;
                    run.2 += 1;
                }
                None => open = Some((base + word.from, base + word.to, 1)),
            }
        });
    }
    if let Some(run) = open.take() {
        runs.push(run);
    }
    for (from, to, run) in runs {
        if run < MIN_RUN {
            continue;
        }
        out.push(SourceCopyRow {
            span: TextRange::new(from, to).expect("a run grows forward"),
            run,
            eligible,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verse_set_is_sorted_and_deduplicated() {
        let mut hashes = vec![9, 1, 9, 4, 1];
        hashes.sort_unstable();
        let len = dedup_len(&mut hashes);
        assert_eq!(&hashes[..len], &[1, 4, 9]);
    }

    /// Case is part of the identity: a paste preserves it.
    #[test]
    fn the_hash_does_not_fold_case() {
        assert_ne!(word_hash("David"), word_hash("david"));
    }
}
