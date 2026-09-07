//! Where a firing pattern actually occurs: the retained text rescanned once
//! per book, one row per maximal run that still matches.
//!
//! ```text
//! firing(MRK's counts, table, &mut set)     → [0, 1]    // both glyphs are in MRK
//! locate("He said ,go. Yes`", [0..17], [(0, ',' placement prev=Space),
//!                                       (1, '`' rarity)], &mut out)
//!   → Site { span:  8..9,  headline: 0, reasons: PlacementBefore }
//!     Site { span: 16..17, headline: 1, reasons: Rarity }
//! ```
//!
//! A judged pattern is a corpus fact with no coordinates; this is the one step
//! that reads text to place it. The counts already decided *what* is anomalous,
//! so a rescan may only agree with them — every occurrence the walk counted for
//! a pattern's key is one this module finds, which is what
//! `tests/sites_agree_with_counts.rs` pins.
//!
//! The engine, the cursor's seam semantics, and the one-row-per-run rule:
//! sites.md.

use memchr::memmem::Finder;

use crate::judge::{Channel, Pattern, PatternIndex, PatternKey, Side, pool_of_key};
use crate::substrate::{BookAggregate, OuterClass, RUN_BUCKETS, ScalarKey, is_run_atom};
use crate::unicode::{atoms::widen_to_atoms, class_of};
use crate::words::word_around;
use crate::{Chapter, Reasons, TextRange};

/// One matching run in projected-book coordinates.
///
/// The whole run, even when only a placement on its first member fired: a
/// maximal bad run is one review row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Site {
    pub span: TextRange,
    /// The matched pattern with the finest channel, ties by table position.
    pub headline: PatternIndex,
    /// Every ladder rung any pattern matched in this run.
    pub reasons: Reasons,
}

/// The table positions whose glyph this book's own counts hold.
///
/// What [`locate`] scans for, and what a resident host keys a site cache on:
/// two publications whose firing set is the same for a book have the same
/// sites in it.
pub fn firing(book: &BookAggregate, patterns: &[Pattern], out: &mut Vec<PatternIndex>) {
    out.clear();
    for (index, pattern) in patterns.iter().enumerate() {
        // The table is the whole corpus's, and a word row names a hash rather
        // than a glyph — and a letter-run row names a letter this walk never
        // counted. Those are `words::Words`'s to place.
        if pattern.channel.judged_by_words() {
            continue;
        }
        let held = book
            .scalars()
            .binary_search_by_key(&pattern.glyph, |entry| entry.0)
            .is_ok();
        if held {
            out.push(PatternIndex::new(index as u16));
        }
    }
}

/// Every run of `text` matching any of `patterns`, in projected-book
/// coordinates, ordered by chapter and then by offset.
///
/// `patterns` is the book's own firing set from [`firing`], each row paired
/// with its position in the publication's table.
pub fn locate(
    text: &str,
    chapters: &[Chapter],
    patterns: &[(PatternIndex, Pattern)],
    out: &mut Vec<Site>,
) {
    let mut tally = Vec::new();
    locate_counted(text, chapters, patterns, out, &mut tally);
}

/// [`locate`], plus the matching occurrences behind each pattern,
/// index-aligned with `patterns`.
///
/// The oracle behind "sites agree with counts": an occurrence here is what the
/// walk counted for that pattern's key — a run for `RunShape`, an in-run
/// position for `ExactNeighbor`, an occurrence of the glyph otherwise.
pub fn locate_counted(
    text: &str,
    chapters: &[Chapter],
    patterns: &[(PatternIndex, Pattern)],
    out: &mut Vec<Site>,
    tally: &mut Vec<u64>,
) {
    tally.clear();
    tally.resize(patterns.len(), 0);
    if patterns.is_empty() {
        return;
    }
    let base = out.len();
    let needles = needles(patterns);
    let cursor = Cursor::new(text, chapters);
    let mut hits: Vec<(u32, u32)> = Vec::new();
    let mut atoms: Vec<(u32, ScalarKey)> = Vec::new();
    let mut matched: Vec<(usize, u64)> = Vec::new();

    for (index, chapter) in chapters.iter().enumerate() {
        let span = chapter.text();
        let slice = &text[span.from() as usize..span.to() as usize];
        hits.clear();
        for (slot, needle) in needles.iter().enumerate() {
            let slot = slot as u32;
            match &needle.finder {
                Some(finder) => hits.extend(
                    finder
                        .find_iter(slice.as_bytes())
                        .map(|at| (span.from() + at as u32, slot)),
                ),
                // The pooled digit key has no single literal, so its glyphs
                // come from one classifier pass over the chapter.
                None => hits.extend(
                    slice
                        .char_indices()
                        .filter(|(_, scalar)| class_of(*scalar).is_decimal_digit())
                        .map(|(at, _)| (span.from() + at as u32, slot)),
                ),
            }
        }
        // Distinct scalars have distinct UTF-8 encodings and none is a prefix
        // of another, so no two needles hit one offset and this order is total.
        hits.sort_unstable();

        // The search is per chapter, which is also the clip a run takes: a hit
        // outside every chapter is text a pass never sees, and searching the
        // whole book instead measured SLOWER, because the chapter lookup a hit
        // then needs costs more than the call it saves.
        let mut evaluated = span.from();
        for &(at, slot) in &hits {
            let glyph = needles[slot as usize].glyph;
            let scalar = text[at as usize..]
                .chars()
                .next()
                .expect("a hit lands on a scalar");
            if !is_run_atom(class_of(scalar)) {
                // A rostered letter or space, or a digit: not a run atom, so
                // it sites its own atom.
                let own = TextRange::new(at, at + scalar.len_utf8() as u32)
                    .expect("a scalar has a positive width");
                lone(
                    patterns,
                    &needles[slot as usize],
                    own,
                    index,
                    &cursor,
                    out,
                    tally,
                );
                continue;
            }
            if at < evaluated {
                continue;
            }
            let run = cursor.run_around(at);
            evaluated = run.to();
            atoms.clear();
            for (offset, scalar) in text[run.from() as usize..run.to() as usize].char_indices() {
                atoms.push((run.from() + offset as u32, key_of(scalar)));
            }
            debug_assert!(atoms.iter().any(|atom| atom.1 == glyph));

            matched.clear();
            for (position, &(_, key)) in atoms.iter().enumerate() {
                if atoms[..position].iter().any(|seen| seen.1 == key) {
                    continue;
                }
                let Some(needle) = needles.iter().find(|needle| needle.glyph == key) else {
                    continue;
                };
                for &slot in &needle.patterns {
                    let count = occurrences(&patterns[slot].1, key, &atoms, &cursor);
                    if count > 0 {
                        matched.push((slot, count));
                    }
                }
            }
            emit(patterns, &matched, run, index, &cursor, out, tally);
            // The follows lane credits the run's LAST atom, so only that one
            // can have handed a capital off — and its site is a different span
            // from this run's, which is why it is a row of its own.
            if let Some(&(terminal, key)) = atoms.last()
                && let Some(needle) = needles.iter().find(|needle| needle.glyph == key)
            {
                sentence_start(patterns, needle, terminal, &cursor, out, tally);
            }
        }
    }
    // A sentence-start row names the word AFTER its run, which may sit past
    // the next run's own site; the promised order is the span's.
    out[base..].sort_by_key(|site| (site.span.from(), site.span.to()));
}

/// The one channel whose site is not the run that matched it: the pattern is
/// glyph-side, but the reviewable thing is the lowercase word the glyph handed
/// off to, so the span is that word and the row carries it alone.
///
/// One row per lowercase handoff, which is exactly what the `follows` lane
/// counted — the run terminal's, whitespace ridden through and nothing else,
/// across a chapter seam as the fold's carry is.
fn sentence_start(
    patterns: &[(PatternIndex, Pattern)],
    needle: &Needle,
    terminal: u32,
    cursor: &Cursor<'_>,
    out: &mut Vec<Site>,
    tally: &mut [u64],
) {
    let Some(&slot) = needle
        .patterns
        .iter()
        .find(|&&slot| patterns[slot].1.channel == Channel::SentenceStart)
    else {
        return;
    };
    let Some((chapter, at, letter)) = cursor.handoff(terminal) else {
        return;
    };
    if !class_of(letter).is_lowercase() {
        return;
    }
    tally[slot] += 1;
    out.push(Site {
        span: cursor.word(at, chapter),
        headline: patterns[slot].0,
        reasons: Reasons::SENTENCE_START,
    });
}

/// A scalar that is not a run atom, sited on its own: only the two channels a
/// single atom can answer may name it.
///
/// [`Channel::Rarity`] names a rostered letter or space; [`Channel::Placement`]
/// names a digit, whose G0 pair the walk still counts. Run shape and exact
/// neighbour need a run, and a digit is in none.
#[allow(clippy::too_many_arguments)]
fn lone(
    patterns: &[(PatternIndex, Pattern)],
    needle: &Needle,
    own: TextRange,
    chapter: usize,
    cursor: &Cursor<'_>,
    out: &mut Vec<Site>,
    tally: &mut [u64],
) {
    let atoms = [(own.from(), needle.glyph)];
    let matched: Vec<(usize, u64)> = needle
        .patterns
        .iter()
        .filter(|&&slot| {
            matches!(
                patterns[slot].1.channel,
                Channel::Rarity | Channel::Placement
            )
        })
        .filter_map(|&slot| {
            let count = occurrences(&patterns[slot].1, needle.glyph, &atoms, cursor);
            (count > 0).then_some((slot, count))
        })
        .collect();
    emit(patterns, &matched, own, chapter, cursor, out, tally);
}

/// One row for the whole span, headlined by the finest matched channel and
/// carrying every rung as a reason.
fn emit(
    patterns: &[(PatternIndex, Pattern)],
    matched: &[(usize, u64)],
    span: TextRange,
    chapter: usize,
    cursor: &Cursor<'_>,
    out: &mut Vec<Site>,
    tally: &mut [u64],
) {
    let Some(&(first, _)) = matched.first() else {
        return;
    };
    let mut headline = first;
    let mut reasons = Reasons::default();
    for &(slot, count) in matched {
        tally[slot] += count;
        reasons = reasons.union(rung(&patterns[slot].1));
        let finer = (patterns[slot].1.channel, patterns[slot].0)
            < (patterns[headline].1.channel, patterns[headline].0);
        if finer {
            headline = slot;
        }
    }
    out.push(Site {
        span: cursor.widen(span, chapter),
        headline: patterns[headline].0,
        reasons,
    });
}

/// Which ladder rung a pattern belongs to; placement splits by side.
fn rung(pattern: &Pattern) -> Reasons {
    match pattern.key {
        PatternKey::ExactNeighbor(_) => Reasons::EXACT_NEIGHBOR,
        PatternKey::PooledNeighbor(_) => Reasons::POOLED_NEIGHBOR,
        PatternKey::RunShape { .. } => Reasons::RUN_SHAPE,
        PatternKey::Placement { side, .. } => match side {
            Side::Prev => Reasons::PLACEMENT_BEFORE,
            Side::Next => Reasons::PLACEMENT_AFTER,
        },
        PatternKey::Rarity => Reasons::RARITY,
        PatternKey::SentenceStart => Reasons::SENTENCE_START,
        // `firing` never lets one through: the word pass owns them.
        PatternKey::LetterRun { .. } => Reasons::LETTER_RUN,
        PatternKey::Casing { .. } => Reasons::CASING,
        PatternKey::WordLength { .. } => Reasons::WORD_LENGTH,
        PatternKey::Doubled { separated, .. } => {
            if separated {
                Reasons::DOUBLED_SEPARATED
            } else {
                Reasons::DOUBLED_BARE
            }
        }
    }
}

/// How many of this pattern's key the run holds — the same unit the walk
/// counted, so the two numbers may be compared directly.
fn occurrences(
    pattern: &Pattern,
    glyph: ScalarKey,
    atoms: &[(u32, ScalarKey)],
    cursor: &Cursor<'_>,
) -> u64 {
    match pattern.key {
        // A glyph's neighbours inside a run are `Nonletter`; only the run's
        // first and last members can see anything else.
        PatternKey::Placement { side, class } => atoms
            .iter()
            .filter(|atom| atom.1 == glyph)
            .filter(|atom| match side {
                Side::Prev => cursor.prev_outer(atom.0) == class,
                Side::Next => cursor.next_outer(atom.0) == class,
            })
            .count() as u64,
        PatternKey::RunShape { pure, bucket } => {
            let shape = (
                atoms.iter().all(|atom| atom.1 == glyph),
                atoms.len().min(RUN_BUCKETS) as u8,
            );
            u64::from(shape == (pure, bucket))
        }
        PatternKey::ExactNeighbor(neighbor) => atoms
            .windows(2)
            .filter(|pair| pair[0].1 == glyph && pair[1].1 == neighbor)
            .count() as u64,
        PatternKey::PooledNeighbor(pool) => atoms
            .windows(2)
            .filter(|pair| pair[0].1 == glyph && pool_of_key(pair[1].1) == pool)
            .count() as u64,
        PatternKey::Rarity => atoms.iter().filter(|atom| atom.1 == glyph).count() as u64,
        // Its site is the word after the run, never the run: `sentence_start`.
        PatternKey::SentenceStart => 0,
        PatternKey::Casing { .. }
        | PatternKey::WordLength { .. }
        | PatternKey::Doubled { .. }
        | PatternKey::LetterRun { .. } => 0,
    }
}

/// One distinct glyph to search for, and every pattern judged on it.
struct Needle {
    glyph: ScalarKey,
    /// `None` for the pooled digit key, which no single literal can find.
    finder: Option<Finder<'static>>,
    /// Positions into the caller's `patterns`.
    patterns: Vec<usize>,
}

/// One needle per distinct glyph. `memmem` and not `memchr` on a UTF-8 lead
/// byte: every Devanagari scalar shares `0xE0`, so a lead-byte filter verifies
/// every character (evidence.md, 2026-09-03).
fn needles(patterns: &[(PatternIndex, Pattern)]) -> Vec<Needle> {
    let mut out: Vec<Needle> = Vec::new();
    for (slot, (_, pattern)) in patterns.iter().enumerate() {
        match out.iter_mut().find(|needle| needle.glyph == pattern.glyph) {
            Some(needle) => needle.patterns.push(slot),
            None => out.push(Needle {
                glyph: pattern.glyph,
                finder: pattern.glyph.scalar().map(|scalar| {
                    let mut buffer = [0u8; 4];
                    Finder::new(scalar.encode_utf8(&mut buffer).as_bytes()).into_owned()
                }),
                patterns: vec![slot],
            }),
        }
    }
    out
}

/// The scalar as a count key, digits pooled (charter invariant 7).
fn key_of(scalar: char) -> ScalarKey {
    if class_of(scalar).is_decimal_digit() {
        ScalarKey::DIGITS
    } else {
        ScalarKey::of(scalar)
    }
}

// ── The cursor ──────────────────────────────────────────────────────────

/// The pattern language: what the walk saw either side of a scalar, and the
/// run it belonged to.
///
/// Pairs read across a masked chapter seam as one string and runs do not,
/// which is exactly `fold_book`'s semantics — a nonletter run abutting a `\c`
/// stays two runs, while the pair either side of the seam is resolved.
pub struct Cursor<'a> {
    text: &'a str,
    chapters: &'a [Chapter],
}

impl<'a> Cursor<'a> {
    pub fn new(text: &'a str, chapters: &'a [Chapter]) -> Self {
        Self { text, chapters }
    }

    /// The outer class before the scalar starting at `at`; `Edge` only at the
    /// start of the book.
    pub fn prev_outer(&self, at: u32) -> OuterClass {
        let Some(chapter) = self.chapter_at(at) else {
            return OuterClass::Edge;
        };
        let span = self.chapters[chapter].text();
        if let Some(scalar) = self.text[span.from() as usize..at as usize]
            .chars()
            .next_back()
        {
            return OuterClass::of(class_of(scalar));
        }
        // An empty chapter is not a neighbour; the fold passes the seam
        // through it untouched.
        for earlier in self.chapters[..chapter].iter().rev() {
            if let Some(scalar) = self.slice(earlier).chars().next_back() {
                return OuterClass::of(class_of(scalar));
            }
        }
        OuterClass::Edge
    }

    /// The outer class after the scalar starting at `at`; `Edge` only at the
    /// end of the book.
    pub fn next_outer(&self, at: u32) -> OuterClass {
        let Some(chapter) = self.chapter_at(at) else {
            return OuterClass::Edge;
        };
        let span = self.chapters[chapter].text();
        let mut rest = self.text[at as usize..span.to() as usize].chars();
        rest.next();
        if let Some(scalar) = rest.next() {
            return OuterClass::of(class_of(scalar));
        }
        for later in &self.chapters[chapter + 1..] {
            if let Some(scalar) = self.slice(later).chars().next() {
                return OuterClass::of(class_of(scalar));
            }
        }
        OuterClass::Edge
    }

    /// The letter a run terminal at `at` hands off to: the first non-whitespace
    /// scalar after it, with the chapter and offset holding it.
    ///
    /// Whitespace is ridden through and nothing else — a nonletter opens a new
    /// run and a mark clears the handoff, which is what the walk does when it
    /// drops `awaiting`. Across a seam the fold pairs a chapter's `open_follow`
    /// with the next one's `edge_case`, and a blank chapter passes the follow
    /// through, so the scan crosses a seam the same way the pair reads across
    /// one. `None` at the end of the book.
    pub fn handoff(&self, at: u32) -> Option<(usize, u32, char)> {
        let chapter = self.chapter_at(at)?;
        let span = self.chapters[chapter].text();
        let mut rest = self.text[at as usize..span.to() as usize].char_indices();
        rest.next();
        for (offset, scalar) in rest {
            if !class_of(scalar).is_whitespace() {
                return Some((chapter, at + offset as u32, scalar));
            }
        }
        for (later, held) in self.chapters.iter().enumerate().skip(chapter + 1) {
            let start = held.text().from();
            for (offset, scalar) in self.slice(held).char_indices() {
                if !class_of(scalar).is_whitespace() {
                    return Some((later, start + offset as u32, scalar));
                }
            }
        }
        None
    }

    /// The word holding the scalar at `at`, widened to atom edges: the same
    /// span the word lane draws, clipped to the chapter as that walk is.
    pub fn word(&self, at: u32, chapter: usize) -> TextRange {
        let start = self.chapters[chapter].text().from();
        let (from, to) = word_around(self.slice(&self.chapters[chapter]), at - start);
        let span = TextRange::new(from + start, to + start).expect("a word grows forward");
        self.widen(span, chapter)
    }

    /// The maximal run holding the scalar at `at`, clipped to its chapter;
    /// empty when that scalar is not a run atom — a letter, whitespace, or a
    /// digit.
    pub fn run_around(&self, at: u32) -> TextRange {
        let empty = TextRange::new(at, at).expect("an empty range is ordered");
        let Some(chapter) = self.chapter_at(at) else {
            return empty;
        };
        let span = self.chapters[chapter].text();
        let mut to = at;
        for (offset, scalar) in self.text[at as usize..span.to() as usize].char_indices() {
            if !is_run_atom(class_of(scalar)) {
                break;
            }
            to = at + offset as u32 + scalar.len_utf8() as u32;
        }
        if to == at {
            return empty;
        }
        let mut from = at;
        for scalar in self.text[span.from() as usize..at as usize].chars().rev() {
            if !is_run_atom(class_of(scalar)) {
                break;
            }
            from -= scalar.len_utf8() as u32;
        }
        TextRange::new(from, to).expect("a run grows outward from one scalar")
    }

    /// The span snapped out to atom edges, inside its own chapter: a chapter
    /// edge is an edge of text for a site, as it is for hygiene.
    fn widen(&self, span: TextRange, chapter: usize) -> TextRange {
        let start = self.chapters[chapter].text().from();
        let slice = self.slice(&self.chapters[chapter]);
        let local = TextRange::new(span.from() - start, span.to() - start)
            .expect("a chapter-relative range keeps its order");
        let wide = widen_to_atoms(slice, local);
        TextRange::new(wide.from() + start, wide.to() + start)
            .expect("a rebased range keeps its order")
    }

    fn slice(&self, chapter: &Chapter) -> &'a str {
        let span = chapter.text();
        &self.text[span.from() as usize..span.to() as usize]
    }

    /// The chapter holding `at`, or `None` for text outside every chapter —
    /// front matter a pass never sees.
    fn chapter_at(&self, at: u32) -> Option<usize> {
        let found = self
            .chapters
            .partition_point(|chapter| chapter.text().to() <= at);
        self.chapters
            .get(found)
            .filter(|chapter| chapter.text().from() <= at)
            .map(|_| found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapters(spans: &[(u32, u32)]) -> Vec<Chapter> {
        spans
            .iter()
            .enumerate()
            .map(|(at, &(from, to))| {
                Chapter::new(at as u16 + 1, TextRange::new(from, to).unwrap()).unwrap()
            })
            .collect()
    }

    fn whole(text: &str) -> Vec<Chapter> {
        chapters(&[(0, text.len() as u32)])
    }

    fn pattern(glyph: char, channel: Channel, key: PatternKey) -> Pattern {
        Pattern {
            glyph: ScalarKey::of(glyph),
            channel,
            key,
            band: (channel != Channel::Rarity).then_some(0),
            numerator: 1,
            denominator: 1,
            share_bp: 10_000,
            books: 1,
        }
    }

    fn placement(glyph: char, side: Side, class: OuterClass) -> Pattern {
        pattern(
            glyph,
            Channel::Placement,
            PatternKey::Placement { side, class },
        )
    }

    fn run_shape(glyph: char, pure: bool, bucket: u8) -> Pattern {
        pattern(
            glyph,
            Channel::RunShape,
            PatternKey::RunShape { pure, bucket },
        )
    }

    fn exact(glyph: char, neighbor: char) -> Pattern {
        pattern(
            glyph,
            Channel::ExactNeighbor,
            PatternKey::ExactNeighbor(ScalarKey::of(neighbor)),
        )
    }

    fn rarity(glyph: char) -> Pattern {
        pattern(glyph, Channel::Rarity, PatternKey::Rarity)
    }

    fn found(text: &str, chapters: &[Chapter], rows: &[Pattern]) -> Vec<(u32, u32, u16, u16)> {
        let table: Vec<(PatternIndex, Pattern)> = rows
            .iter()
            .enumerate()
            .map(|(at, row)| (PatternIndex::new(at as u16), *row))
            .collect();
        let mut out = Vec::new();
        locate(text, chapters, &table, &mut out);
        out.iter()
            .map(|site| {
                (
                    site.span.from(),
                    site.span.to(),
                    site.headline.get(),
                    site.reasons.bits(),
                )
            })
            .collect()
    }

    #[test]
    fn module_doc_example_is_exact() {
        let text = "He said ,go. Yes`";
        assert_eq!(
            found(
                text,
                &whole(text),
                &[placement(',', Side::Prev, OuterClass::Space), rarity('`'),]
            ),
            vec![
                (8, 9, 0, Reasons::PLACEMENT_BEFORE.bits()),
                (16, 17, 1, Reasons::RARITY.bits()),
            ]
        );
    }

    #[test]
    fn a_placement_pattern_sites_the_occurrence_it_names() {
        let text = "a, b ,c";
        // The comma at 5 is the one with a space before it.
        assert_eq!(
            found(
                text,
                &whole(text),
                &[placement(',', Side::Prev, OuterClass::Space)]
            ),
            vec![(5, 6, 0, Reasons::PLACEMENT_BEFORE.bits())]
        );
        // The comma at 1 is the one with a letter before it.
        assert_eq!(
            found(
                text,
                &whole(text),
                &[placement(',', Side::Prev, OuterClass::Letter)]
            ),
            vec![(1, 2, 0, Reasons::PLACEMENT_BEFORE.bits())]
        );
    }

    #[test]
    fn a_run_shape_pattern_sites_the_whole_run() {
        let text = "a,..,b c, d";
        assert_eq!(
            found(text, &whole(text), &[run_shape(',', false, 4)]),
            vec![(1, 5, 0, Reasons::RUN_SHAPE.bits())]
        );
    }

    #[test]
    fn an_exact_neighbor_pattern_sites_the_run_holding_the_pair() {
        let text = "a?. b?\" c?";
        assert_eq!(
            found(text, &whole(text), &[exact('?', '.')]),
            vec![(1, 3, 0, Reasons::EXACT_NEIGHBOR.bits())]
        );
    }

    #[test]
    fn a_run_matching_two_patterns_is_one_row_with_both_reason_bits() {
        let text = "a?. b";
        let rows = found(
            text,
            &whole(text),
            &[run_shape('?', false, 2), exact('?', '.')],
        );
        assert_eq!(
            rows,
            vec![(
                1,
                3,
                1,
                Reasons::RUN_SHAPE.union(Reasons::EXACT_NEIGHBOR).bits()
            )],
            "one row, headlined by the finer channel"
        );
    }

    /// Ruling 4: a pair reads across a masked `\\c` as the counts do.
    #[test]
    fn a_pair_across_a_masked_chapter_seam_is_found() {
        let text = "one,two";
        let split = chapters(&[(0, 4), (4, 7)]);
        assert_eq!(
            found(
                text,
                &split,
                &[placement(',', Side::Next, OuterClass::Letter)]
            ),
            vec![(3, 4, 0, Reasons::PLACEMENT_AFTER.bits())],
            "the comma ends chapter 1 and reads chapter 2's first letter"
        );
        assert_eq!(
            found(
                text,
                &split,
                &[placement(',', Side::Next, OuterClass::Edge)]
            ),
            Vec::new(),
            "only the book's own end is an edge"
        );
    }

    /// The same seam for a run: two chapters, two runs, two rows.
    #[test]
    fn a_run_abutting_a_masked_chapter_seam_is_two_runs() {
        let text = "a,,,,b";
        let split = chapters(&[(0, 3), (3, 6)]);
        assert_eq!(
            found(text, &whole(text), &[run_shape(',', true, 4)]),
            vec![(1, 5, 0, Reasons::RUN_SHAPE.bits())]
        );
        assert_eq!(
            found(text, &split, &[run_shape(',', true, 2)]),
            vec![
                (1, 3, 0, Reasons::RUN_SHAPE.bits()),
                (3, 5, 0, Reasons::RUN_SHAPE.bits()),
            ]
        );
    }

    #[test]
    fn a_rostered_letter_sites_its_own_atom() {
        let text = "he\u{301}llo z z";
        assert_eq!(
            found(text, &whole(text), &[rarity('z')]),
            vec![
                (8, 9, 0, Reasons::RARITY.bits()),
                (10, 11, 0, Reasons::RARITY.bits()),
            ]
        );
        // The mark rides its base, so a rare `e` widens to both scalars.
        assert_eq!(
            found(text, &whole(text), &[rarity('e')]),
            vec![(1, 4, 0, Reasons::RARITY.bits())]
        );
    }

    /// A digit is not a run atom, so the pooled key sites its own scalar and
    /// only placement can name it.
    #[test]
    fn digits_site_through_the_scan() {
        let text = "in 12,345 and \u{966}\u{967} too";
        let pooled = |side, class| Pattern {
            glyph: ScalarKey::DIGITS,
            ..placement('0', side, class)
        };
        assert_eq!(
            found(
                text,
                &whole(text),
                &[pooled(Side::Prev, OuterClass::Nonletter)]
            ),
            vec![(6, 7, 0, Reasons::PLACEMENT_BEFORE.bits())],
            "the one digit standing after the comma"
        );
        assert_eq!(
            found(text, &whole(text), &[pooled(Side::Prev, OuterClass::Space)]),
            vec![
                (3, 4, 0, Reasons::PLACEMENT_BEFORE.bits()),
                (14, 17, 0, Reasons::PLACEMENT_BEFORE.bits()),
            ],
            "one Latin and one Devanagari digit, each its own atom"
        );
        assert_eq!(
            found(
                text,
                &whole(text),
                &[Pattern {
                    glyph: ScalarKey::DIGITS,
                    ..run_shape('0', false, 6)
                }]
            ),
            Vec::new(),
            "a run-shape row on the pooled key, which the judge no longer emits"
        );
    }

    #[test]
    fn sites_are_widened_to_atom_edges() {
        // The virama joins the conjunct, so the run's own edge is not an atom
        // edge and the site widens over the whole cluster.
        let text = "\u{915}\u{94d}\u{937},";
        assert_eq!(
            found(text, &whole(text), &[rarity('\u{937}')]),
            vec![(0, 9, 0, Reasons::RARITY.bits())]
        );
    }

    #[test]
    fn a_book_without_the_glyph_reads_no_text() {
        let text = "a, b, c";
        let mut set = Vec::new();
        let counts = crate::substrate::fold_book(
            &[crate::ChapterObs {
                start: 0,
                obs: &crate::substrate::walk::walk("no punctuation here", &[]),
            }],
            &mut crate::substrate::Edge::default(),
        );
        firing(
            &counts,
            &[placement(',', Side::Prev, OuterClass::Letter)],
            &mut set,
        );
        assert!(set.is_empty(), "the counts say the comma is absent");
        // With an empty firing set the text is never consulted.
        assert_eq!(found(text, &whole(text), &[]), Vec::new());
    }

    #[test]
    fn the_cursor_reads_the_walks_neighbours_and_runs() {
        let text = "a, \u{301}b";
        let spans = whole(text);
        let cursor = Cursor::new(text, &spans);
        assert_eq!(cursor.prev_outer(1), OuterClass::Letter);
        assert_eq!(cursor.next_outer(1), OuterClass::Space);
        assert_eq!(cursor.prev_outer(0), OuterClass::Edge);
        // Glue reads as Letter rather than as its base, exactly as the walk
        // records it.
        assert_eq!(cursor.next_outer(2), OuterClass::Letter);
        assert_eq!(cursor.run_around(1), TextRange::new(1, 2).unwrap());
        assert_eq!(cursor.run_around(0), TextRange::new(0, 0).unwrap());
    }
}
