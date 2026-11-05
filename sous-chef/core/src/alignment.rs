//! Pairs target and source verse rows without depending on either producer.
//!
//! Alignment facts stay separate from analyzable units: a fact says why a row
//! did not pair, and is never a finding. Bridge units keep every constituent
//! range, so no measurement treats separated ranges as one contiguous span.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{BookKey, Corpus, ProjectedBook, TextRange, Verse, VerseKey};

/// One side of an aligned unit. Bridges can contain several independently
/// mappable ranges; callers may sum their lengths without collapsing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedSide {
    ranges: Vec<TextRange>,
}

impl AlignedSide {
    pub fn ranges(&self) -> &[TextRange] {
        &self.ranges
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

/// One target/source unit ready for a later measurement pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedUnit {
    book: BookKey,
    key: VerseKey,
    target: AlignedSide,
    source: AlignedSide,
}

impl AlignedUnit {
    pub fn book(&self) -> BookKey {
        self.book
    }

    pub fn key(&self) -> VerseKey {
        self.key
    }

    pub fn target(&self) -> &AlignedSide {
        &self.target
    }

    pub fn source(&self) -> &AlignedSide {
        &self.source
    }
}

/// Structural alignment information that must not be interpreted as a rule
/// finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentFact {
    TargetOnly {
        book: BookKey,
        key: VerseKey,
    },
    SourceOnly {
        book: BookKey,
        key: VerseKey,
    },
    AmbiguousDuplicate {
        book: BookKey,
        key: VerseKey,
    },
    PartialOverlap {
        book: BookKey,
        target: VerseKey,
        source: VerseKey,
    },
}

/// The complete alignment table for one target/source corpus pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alignment {
    units: Vec<AlignedUnit>,
    facts: Vec<AlignmentFact>,
}

impl Alignment {
    pub fn units(&self) -> &[AlignedUnit] {
        &self.units
    }

    pub fn facts(&self) -> &[AlignmentFact] {
        &self.facts
    }
}

/// Pair two caller-ordered corpora by [`BookKey`], accepting different
/// producer types and never using their array positions as alignment.
pub fn align<'target, 'source, T, S>(
    target: &Corpus<'target, T>,
    source: &Corpus<'source, S>,
) -> Alignment
where
    T: ProjectedBook,
    S: ProjectedBook,
{
    let mut units = Vec::new();
    let mut facts = Vec::new();
    let source_books: FxHashMap<_, _> = source
        .books()
        .iter()
        .map(|book| (book.key(), book))
        .collect();
    let target_keys: FxHashSet<_> = target.books().iter().map(ProjectedBook::key).collect();

    for target_book in target.books() {
        let key = target_book.key();
        let source_book = source_books.get(&key).copied();
        align_book(
            key,
            target_book.verses().collect(),
            source_book.map_or_else(Vec::new, |book| book.verses().collect()),
            &mut units,
            &mut facts,
        );
    }

    for source_book in source.books() {
        if !target_keys.contains(&source_book.key()) {
            align_book(
                source_book.key(),
                Vec::new(),
                source_book.verses().collect(),
                &mut units,
                &mut facts,
            );
        }
    }

    Alignment { units, facts }
}

#[derive(Clone, Copy)]
struct Row {
    key: VerseKey,
    text: TextRange,
}

impl From<Verse> for Row {
    fn from(verse: Verse) -> Self {
        Self {
            key: verse.key(),
            text: verse.text(),
        }
    }
}

fn align_book(
    book: BookKey,
    target: Vec<Verse>,
    source: Vec<Verse>,
    units: &mut Vec<AlignedUnit>,
    facts: &mut Vec<AlignmentFact>,
) {
    let target: Vec<Row> = target.into_iter().map(Row::from).collect();
    let source: Vec<Row> = source.into_iter().map(Row::from).collect();
    pair_keys(
        book,
        &keys_of(&target),
        &keys_of(&source),
        &mut |key, left, right| {
            units.push(unit(
                book,
                key,
                left.iter().map(|at| target[*at].text).collect(),
                right.iter().map(|at| source[*at].text).collect(),
            ));
        },
        facts,
    );
}

fn keys_of(rows: &[Row]) -> Vec<VerseKey> {
    rows.iter().map(|row| row.key).collect()
}

/// Pairs one book's verse keys: exact key plus occurrence ordinal, then a
/// bridge against the exact contiguous constituent run on the other side.
/// Everything left is a fact.
///
/// The pairing law is the aligned-unit contract's and nothing else's, so the
/// text-carrying [`align`] and the length-carrying
/// [`crate::proportionality`] cannot drift apart. `unit` is called with each
/// pair as positions into the two slices, never as an owned list: an ordinary
/// verse pairs one row with one row, and a whole Bible is 31k of them.
pub(crate) fn pair_keys(
    book: BookKey,
    target: &[VerseKey],
    source: &[VerseKey],
    unit: &mut impl FnMut(VerseKey, &[usize], &[usize]),
    facts: &mut Vec<AlignmentFact>,
) {
    let mut target_used = vec![false; target.len()];
    let mut source_used = vec![false; source.len()];
    let mut target_blocked = vec![false; target.len()];
    let mut source_blocked = vec![false; source.len()];

    let mut keys = Vec::new();
    let mut seen_keys = FxHashSet::default();
    for key in target.iter().chain(source.iter()).copied() {
        if seen_keys.insert(key) {
            keys.push(key);
        }
    }

    let mut target_by_key: FxHashMap<VerseKey, Vec<usize>> = FxHashMap::default();
    let mut source_by_key: FxHashMap<VerseKey, Vec<usize>> = FxHashMap::default();
    for (index, key) in target.iter().enumerate() {
        target_by_key.entry(*key).or_default().push(index);
    }
    for (index, key) in source.iter().enumerate() {
        source_by_key.entry(*key).or_default().push(index);
    }

    // Exact keys, including equal bridges, are the unambiguous fast path.
    // Pairing by zip preserves each producer's occurrence order.
    for key in keys.iter().copied() {
        // Borrowed, not cloned: the two index maps are distinct locals from
        // the used/blocked lanes, and a whole Bible is 31k keys.
        let (Some(target_indices), Some(source_indices)) =
            (target_by_key.get(&key), source_by_key.get(&key))
        else {
            continue;
        };
        if target_indices.len() != source_indices.len() {
            push_fact(facts, AlignmentFact::AmbiguousDuplicate { book, key });
            for index in target_indices {
                target_blocked[*index] = true;
            }
            for index in source_indices {
                source_blocked[*index] = true;
            }
            continue;
        }

        for (target_index, source_index) in target_indices.iter().zip(source_indices) {
            target_used[*target_index] = true;
            source_used[*source_index] = true;
            unit(key, &[*target_index], &[*source_index]);
        }
    }

    // A bridge may equal a contiguous run of ordinary verses on the other
    // side. Keep the run as a list of ranges; pair duplicate runs only when
    // their multiplicities are equal.
    for key in keys.iter().copied().filter(|key| key.first() < key.last()) {
        let target_indices: Vec<_> = target
            .iter()
            .enumerate()
            .filter(|(index, row)| **row == key && !target_used[*index] && !target_blocked[*index])
            .map(|(index, _)| index)
            .collect();
        if target_indices.is_empty() {
            continue;
        }
        let candidates = exact_sequences(source, key, &source_used, &source_blocked);
        if candidates.len() == target_indices.len() {
            for (target_index, candidate) in target_indices.into_iter().zip(candidates) {
                target_used[target_index] = true;
                for source_index in &candidate {
                    source_used[*source_index] = true;
                }
                unit(key, &[target_index], &candidate);
            }
        } else if !candidates.is_empty() {
            push_fact(facts, AlignmentFact::AmbiguousDuplicate { book, key });
            for index in target_indices {
                target_blocked[index] = true;
            }
            for candidate in candidates {
                for index in candidate {
                    source_blocked[index] = true;
                }
            }
        }
    }

    // The reverse bridge direction runs after the first pass. Used rows
    // cannot be consumed twice; a remaining intersection reports as partial
    // overlap below.
    for key in keys.iter().copied().filter(|key| key.first() < key.last()) {
        let source_indices: Vec<_> = source
            .iter()
            .enumerate()
            .filter(|(index, row)| **row == key && !source_used[*index] && !source_blocked[*index])
            .map(|(index, _)| index)
            .collect();
        if source_indices.is_empty() {
            continue;
        }
        let candidates = exact_sequences(target, key, &target_used, &target_blocked);
        if candidates.len() == source_indices.len() {
            for (source_index, candidate) in source_indices.into_iter().zip(candidates) {
                source_used[source_index] = true;
                for target_index in &candidate {
                    target_used[*target_index] = true;
                }
                unit(key, &candidate, &[source_index]);
            }
        } else if !candidates.is_empty() {
            push_fact(facts, AlignmentFact::AmbiguousDuplicate { book, key });
            for index in source_indices {
                source_blocked[index] = true;
            }
            for candidate in candidates {
                for index in candidate {
                    target_blocked[index] = true;
                }
            }
        }
    }

    for (target_index, target_key) in target.iter().copied().enumerate() {
        if target_used[target_index] || target_blocked[target_index] {
            continue;
        }
        let overlapping: Vec<_> = source
            .iter()
            .copied()
            .enumerate()
            .filter(|(source_index, source_key)| {
                !source_used[*source_index]
                    && !source_blocked[*source_index]
                    && overlaps(target_key, *source_key)
            })
            .map(|(_, key)| key)
            .collect();
        if overlapping.is_empty() {
            push_fact(
                facts,
                AlignmentFact::TargetOnly {
                    book,
                    key: target_key,
                },
            );
        } else {
            for source_key in overlapping {
                push_fact(
                    facts,
                    AlignmentFact::PartialOverlap {
                        book,
                        target: target_key,
                        source: source_key,
                    },
                );
            }
        }
    }

    for (source_index, source_key) in source.iter().copied().enumerate() {
        if source_used[source_index] || source_blocked[source_index] {
            continue;
        }
        let overlapping = target
            .iter()
            .copied()
            .enumerate()
            .any(|(target_index, target_key)| {
                !target_used[target_index]
                    && !target_blocked[target_index]
                    && overlaps(target_key, source_key)
            });
        if !overlapping {
            push_fact(
                facts,
                AlignmentFact::SourceOnly {
                    book,
                    key: source_key,
                },
            );
        }
    }
}

fn exact_sequences(
    rows: &[VerseKey],
    bridge: VerseKey,
    used: &[bool],
    blocked: &[bool],
) -> Vec<Vec<usize>> {
    let mut sequences = Vec::new();
    for start in 0..rows.len() {
        let mut sequence = Vec::new();
        let mut valid = true;
        for (offset, number) in (bridge.first()..=bridge.last()).enumerate() {
            let Some(index) = start.checked_add(offset) else {
                valid = false;
                break;
            };
            let Some(row) = rows.get(index).copied() else {
                valid = false;
                break;
            };
            if used[index]
                || blocked[index]
                || row.chapter() != bridge.chapter()
                || row.first() != number
                || row.last() != number
            {
                valid = false;
                break;
            }
            sequence.push(index);
        }
        if valid {
            sequences.push(sequence);
        }
    }
    sequences
}

fn overlaps(left: VerseKey, right: VerseKey) -> bool {
    left.chapter() == right.chapter()
        && left.first() <= right.last()
        && right.first() <= left.last()
}

fn unit(
    book: BookKey,
    key: VerseKey,
    target: Vec<TextRange>,
    source: Vec<TextRange>,
) -> AlignedUnit {
    AlignedUnit {
        book,
        key,
        target: AlignedSide { ranges: target },
        source: AlignedSide { ranges: source },
    }
}

/// How one book's verses correspond across two numberings: the target's and
/// its source's. A caller that knows both sides' versification schemes
/// supplies it (galley, from canon); sous-core knows no scheme.
///
/// Both directions answer in the other side's numbering and push nothing for
/// a verse with no counterpart. Verse 0 is the title slot before verse 1 (a
/// Psalm's superscription in a scheme that doesn't number it); no row is ever
/// keyed there, so an answer of verse 0 means "a title, not a verse".
pub trait Crosswalk {
    /// The source verses a target verse is, as `(chapter, verse)`.
    fn to_source(&self, chapter: u16, verse: u16, out: &mut Vec<(u16, u16)>);
    /// The target verses a source verse is.
    fn to_target(&self, chapter: u16, verse: u16, out: &mut Vec<(u16, u16)>);
}

/// [`pair_keys`] for two sides numbered differently: units are the connected
/// groups of rows, where a target row joins every source row covering a
/// verse it corresponds to (and the reverse).
///
/// ```text
/// target PSA 3:1 (eng)     ↔ source PSA 3:2 (org)               one-to-one, renumbered
/// target PSA 13:5, 13:6    ↔ source PSA 13:6                    a merge: one unit, two target ranges
/// target PSA 89:6 (rsc)    ↔ source PSA 90:5, 90:6              a split: one unit, two source ranges
/// target \v 1-2            ↔ source 2, 3 (each moved by one)    a bridge across a move
/// ```
///
/// The same callback contract as [`pair_keys`], so everything downstream
/// (lengths, presence, source copy) reads a unit the same way. A unit's key
/// is its target rows' hull. Facts differ in one way, on purpose: a source
/// row nothing in the target answers is `SourceOnly` keyed in the TARGET's
/// numbering, which is where presence places "missing" and what a translator
/// reads. One whose counterpart is a title (verse 0) or nothing at all is
/// dropped: the target lacks no verse there.
///
/// A key repeated on one side cannot be placed across numberings, so every
/// copy is blocked and reported `AmbiguousDuplicate`. Verses the crosswalk
/// sends to another book are ignored: pairing is per book.
pub(crate) fn pair_keys_across(
    book: BookKey,
    target: &[VerseKey],
    source: &[VerseKey],
    walk: &dyn Crosswalk,
    unit: &mut impl FnMut(VerseKey, &[usize], &[usize]),
    facts: &mut Vec<AlignmentFact>,
) {
    let blocked = |rows: &[VerseKey], facts: &mut Vec<AlignmentFact>| -> Vec<bool> {
        let mut count: FxHashMap<VerseKey, u32> = FxHashMap::default();
        for key in rows {
            *count.entry(*key).or_default() += 1;
        }
        rows.iter()
            .map(|key| {
                let repeated = count[key] > 1;
                if repeated {
                    push_fact(facts, AlignmentFact::AmbiguousDuplicate { book, key: *key });
                }
                repeated
            })
            .collect()
    };
    let target_blocked = blocked(target, facts);
    let source_blocked = blocked(source, facts);

    // (chapter, verse) → the unblocked rows covering it, per side.
    let cover = |rows: &[VerseKey], blocked: &[bool]| {
        let mut at: FxHashMap<(u16, u16), Vec<usize>> = FxHashMap::default();
        for (index, key) in rows.iter().enumerate() {
            if blocked[index] {
                continue;
            }
            for verse in key.first()..=key.last() {
                at.entry((key.chapter(), verse)).or_default().push(index);
            }
        }
        at
    };
    let target_at = cover(target, &target_blocked);
    let source_at = cover(source, &source_blocked);

    // Union-find over target rows (0..t) then source rows (t..).
    let t = target.len();
    let mut parent: Vec<usize> = (0..t + source.len()).collect();
    fn root(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    let join = |parent: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (root(parent, a), root(parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    };
    let mut there = Vec::new();
    for (index, key) in target.iter().enumerate() {
        if target_blocked[index] {
            continue;
        }
        for verse in key.first()..=key.last() {
            there.clear();
            walk.to_source(key.chapter(), verse, &mut there);
            for place in &there {
                for s in source_at.get(place).into_iter().flatten() {
                    join(&mut parent, index, t + s);
                }
            }
        }
    }
    for (index, key) in source.iter().enumerate() {
        if source_blocked[index] {
            continue;
        }
        for verse in key.first()..=key.last() {
            there.clear();
            walk.to_target(key.chapter(), verse, &mut there);
            for place in &there {
                for r in target_at.get(place).into_iter().flatten() {
                    join(&mut parent, *r, t + index);
                }
            }
        }
    }

    // Components, in the order of their first row (target rows come first,
    // so units follow the target book).
    let mut groups: FxHashMap<usize, (Vec<usize>, Vec<usize>)> = FxHashMap::default();
    let mut order: Vec<usize> = Vec::new();
    for node in 0..parent.len() {
        let blocked = if node < t {
            target_blocked[node]
        } else {
            source_blocked[node - t]
        };
        if blocked {
            continue;
        }
        let r = root(&mut parent, node);
        let group = groups.entry(r).or_insert_with(|| {
            order.push(r);
            (Vec::new(), Vec::new())
        });
        if node < t {
            group.0.push(node);
        } else {
            group.1.push(node - t);
        }
    }

    let mut missing: Vec<VerseKey> = Vec::new();
    for r in order {
        let (left, right) = &groups[&r];
        match (left.is_empty(), right.is_empty()) {
            (false, false) => unit(hull(target, left), left, right),
            (false, true) => {
                for index in left {
                    push_fact(
                        facts,
                        AlignmentFact::TargetOnly {
                            book,
                            key: target[*index],
                        },
                    );
                }
            }
            (true, false) => {
                for index in right {
                    let key = source[*index];
                    let mut answered: Vec<(u16, u16)> = Vec::new();
                    for verse in key.first()..=key.last() {
                        walk.to_target(key.chapter(), verse, &mut answered);
                    }
                    answered.retain(|(_, verse)| *verse > 0);
                    answered.sort_unstable();
                    answered.dedup();
                    // One fact per chapter the counterpart spans.
                    for chunk in answered.chunk_by(|a, b| a.0 == b.0) {
                        let (chapter, first, last) =
                            (chunk[0].0, chunk[0].1, chunk[chunk.len() - 1].1);
                        if let Ok(key) = VerseKey::new(chapter, first, last) {
                            missing.push(key);
                        }
                    }
                }
            }
            (true, true) => {}
        }
    }
    missing.sort_unstable_by_key(|key| (key.chapter(), key.first(), key.last()));
    for key in missing {
        push_fact(facts, AlignmentFact::SourceOnly { book, key });
    }
}

/// The span a unit's target rows cover, as one key: their hull within the
/// first row's chapter (a unit crossing a chapter keys by its first row).
fn hull(target: &[VerseKey], rows: &[usize]) -> VerseKey {
    let first = target[rows[0]];
    let (lo, hi) = rows
        .iter()
        .map(|at| target[*at])
        .filter(|key| key.chapter() == first.chapter())
        .fold((first.first(), first.last()), |(lo, hi), key| {
            (lo.min(key.first()), hi.max(key.last()))
        });
    VerseKey::new(first.chapter(), lo, hi).unwrap_or(first)
}

fn push_fact(facts: &mut Vec<AlignmentFact>, fact: AlignmentFact) {
    if !facts.contains(&fact) {
        facts.push(fact);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chapter;

    const TEXT: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    #[derive(Clone)]
    struct TestBook {
        key: BookKey,
        verses: Vec<Verse>,
    }

    impl ProjectedBook for TestBook {
        fn key(&self) -> BookKey {
            self.key
        }

        fn text(&self) -> &str {
            TEXT
        }

        fn chapters(&self) -> impl Iterator<Item = Chapter> {
            std::iter::once(Chapter::new(1, TextRange::new(0, TEXT.len() as u32).unwrap()).unwrap())
        }

        fn verses(&self) -> impl Iterator<Item = Verse> {
            self.verses.iter().copied()
        }
    }

    struct OtherBook(TestBook);

    impl ProjectedBook for OtherBook {
        fn key(&self) -> BookKey {
            self.0.key()
        }

        fn text(&self) -> &str {
            self.0.text()
        }

        fn chapters(&self) -> impl Iterator<Item = Chapter> {
            self.0.chapters()
        }

        fn verses(&self) -> impl Iterator<Item = Verse> {
            self.0.verses()
        }
    }

    fn verse(first: u16, last: u16, from: u32, to: u32) -> Verse {
        Verse::new(
            VerseKey::new(1, first, last).unwrap(),
            TextRange::new(from, to).unwrap(),
        )
    }

    fn test_book(key: [u8; 3], verses: &[(u16, u16, u32, u32)]) -> TestBook {
        TestBook {
            key: BookKey::new(key),
            verses: verses
                .iter()
                .map(|&(first, last, from, to)| verse(first, last, from, to))
                .collect(),
        }
    }

    fn other_book(key: [u8; 3], verses: &[(u16, u16, u32, u32)]) -> OtherBook {
        OtherBook(test_book(key, verses))
    }

    fn corpus<B: ProjectedBook>(books: &[B]) -> Corpus<'_, B> {
        Corpus::try_new(books).unwrap()
    }

    #[test]
    fn ordinary_rows_pair_by_key() {
        let target = [test_book(*b"MRK", &[(1, 1, 0, 3)])];
        let source = [other_book(*b"MRK", &[(1, 1, 4, 9)])];
        let result = align(&corpus(&target), &corpus(&source));

        assert_eq!(result.facts(), &[]);
        assert_eq!(result.units().len(), 1);
        assert_eq!(result.units()[0].key(), VerseKey::new(1, 1, 1).unwrap());
        assert_eq!(
            result.units()[0].target().ranges(),
            &[TextRange::new(0, 3).unwrap()]
        );
    }

    #[test]
    fn equal_bridges_pair_as_single_ranges() {
        let target = [test_book(*b"MRK", &[(2, 3, 0, 3)])];
        let source = [other_book(*b"MRK", &[(2, 3, 4, 9)])];
        let result = align(&corpus(&target), &corpus(&source));

        assert_eq!(result.units().len(), 1);
        assert_eq!(result.units()[0].target().len(), 1);
        assert_eq!(result.units()[0].source().len(), 1);
        assert!(result.facts().is_empty());
    }

    #[test]
    fn equal_duplicate_counts_pair_in_producer_order() {
        let target = [test_book(*b"MRK", &[(1, 1, 0, 1), (1, 1, 2, 3)])];
        let source = [other_book(*b"MRK", &[(1, 1, 4, 5), (1, 1, 6, 7)])];
        let result = align(&corpus(&target), &corpus(&source));

        assert_eq!(result.units().len(), 2);
        assert_eq!(
            result.units()[0].source().ranges()[0],
            TextRange::new(4, 5).unwrap()
        );
        assert_eq!(
            result.units()[1].source().ranges()[0],
            TextRange::new(6, 7).unwrap()
        );
        assert!(result.facts().is_empty());
    }

    #[test]
    fn unequal_duplicate_counts_are_ambiguous_without_prefix_pairs() {
        let target = [test_book(*b"MRK", &[(1, 1, 0, 1), (1, 1, 2, 3)])];
        let source = [other_book(*b"MRK", &[(1, 1, 4, 5)])];
        let result = align(&corpus(&target), &corpus(&source));

        assert!(result.units().is_empty());
        assert_eq!(
            result.facts(),
            &[AlignmentFact::AmbiguousDuplicate {
                book: BookKey::new(*b"MRK"),
                key: VerseKey::new(1, 1, 1).unwrap(),
            }]
        );
    }

    #[test]
    fn missing_units_are_facts_and_missing_books_report_each_key() {
        let target = [test_book(*b"MRK", &[(1, 1, 0, 1), (2, 2, 2, 3)])];
        let source = [
            other_book(*b"MRK", &[(1, 1, 4, 5), (3, 3, 6, 7)]),
            other_book(*b"GEN", &[(1, 1, 8, 9)]),
        ];
        let result = align(&corpus(&target), &corpus(&source));

        assert_eq!(result.units().len(), 1);
        assert!(result.facts().contains(&AlignmentFact::TargetOnly {
            book: BookKey::new(*b"MRK"),
            key: VerseKey::new(1, 2, 2).unwrap(),
        }));
        assert!(result.facts().contains(&AlignmentFact::SourceOnly {
            book: BookKey::new(*b"MRK"),
            key: VerseKey::new(1, 3, 3).unwrap(),
        }));
        assert!(result.facts().contains(&AlignmentFact::SourceOnly {
            book: BookKey::new(*b"GEN"),
            key: VerseKey::new(1, 1, 1).unwrap(),
        }));
    }

    #[test]
    fn bridge_coalesces_exact_constituents_in_either_direction() {
        let target = [test_book(*b"MRK", &[(2, 3, 0, 2)])];
        let source = [other_book(*b"MRK", &[(2, 2, 4, 5), (3, 3, 6, 8)])];
        let result = align(&corpus(&target), &corpus(&source));
        assert_eq!(result.units().len(), 1);
        assert_eq!(result.units()[0].source().ranges().len(), 2);
        assert!(result.facts().is_empty());

        let target = [test_book(*b"MRK", &[(2, 2, 0, 1), (3, 3, 2, 4)])];
        let source = [other_book(*b"MRK", &[(2, 3, 6, 9)])];
        let result = align(&corpus(&target), &corpus(&source));
        assert_eq!(result.units().len(), 1);
        assert_eq!(result.units()[0].target().ranges().len(), 2);
        assert!(result.facts().is_empty());
    }

    #[test]
    fn overlapping_but_incompatible_ranges_are_partial_overlap() {
        let target = [test_book(*b"MRK", &[(2, 3, 0, 2)])];
        let source = [other_book(*b"MRK", &[(3, 4, 4, 7)])];
        let result = align(&corpus(&target), &corpus(&source));

        assert!(result.units().is_empty());
        assert_eq!(
            result.facts(),
            &[AlignmentFact::PartialOverlap {
                book: BookKey::new(*b"MRK"),
                target: VerseKey::new(1, 2, 3).unwrap(),
                source: VerseKey::new(1, 3, 4).unwrap(),
            }]
        );
    }

    #[test]
    fn book_permutation_and_producer_type_do_not_change_keyed_units() {
        let target = [
            test_book(*b"MRK", &[(1, 1, 0, 1)]),
            test_book(*b"GEN", &[(1, 1, 2, 3)]),
        ];
        let source = [
            other_book(*b"GEN", &[(1, 1, 6, 7)]),
            other_book(*b"MRK", &[(1, 1, 4, 5)]),
        ];
        let result = align(&corpus(&target), &corpus(&source));
        let permuted_target = [target[1].clone(), target[0].clone()];
        let permuted_source = [
            other_book(*b"MRK", &[(1, 1, 4, 5)]),
            other_book(*b"GEN", &[(1, 1, 6, 7)]),
        ];
        let permuted = align(&corpus(&permuted_target), &corpus(&permuted_source));

        let normalize = |alignment: &Alignment| {
            let mut keyed: Vec<_> = alignment
                .units()
                .iter()
                .map(|unit| (unit.book().as_bytes(), unit.key()))
                .collect();
            keyed.sort_by_key(|entry| *entry);
            keyed
        };
        let keyed = normalize(&result);
        assert_eq!(
            keyed,
            vec![
                (*b"GEN", VerseKey::new(1, 1, 1).unwrap()),
                (*b"MRK", VerseKey::new(1, 1, 1).unwrap()),
            ]
        );
        assert_eq!(keyed, normalize(&permuted));
        assert!(result.facts().is_empty());
        assert!(permuted.facts().is_empty());
    }

    #[test]
    fn same_key_in_different_books_does_not_cross_pair() {
        let target = [test_book(*b"MRK", &[(1, 1, 0, 1)])];
        let source = [other_book(*b"GEN", &[(1, 1, 4, 5)])];
        let result = align(&corpus(&target), &corpus(&source));
        assert!(result.units().is_empty());
        assert_eq!(result.facts().len(), 2);
    }
}

#[cfg(test)]
mod across_tests {
    use super::*;

    const PSA: BookKey = BookKey::new(*b"PSA");

    /// A crosswalk given as explicit (target → source) pairs; a verse not
    /// listed keeps its number on the other side.
    struct Table(Vec<((u16, u16), (u16, u16))>);

    impl Crosswalk for Table {
        fn to_source(&self, chapter: u16, verse: u16, out: &mut Vec<(u16, u16)>) {
            let listed: Vec<_> = self
                .0
                .iter()
                .filter(|(t, _)| *t == (chapter, verse))
                .map(|(_, s)| *s)
                .collect();
            if listed.is_empty() && !self.0.iter().any(|(_, s)| *s == (chapter, verse)) {
                out.push((chapter, verse));
            }
            out.extend(listed);
        }
        fn to_target(&self, chapter: u16, verse: u16, out: &mut Vec<(u16, u16)>) {
            let listed: Vec<_> = self
                .0
                .iter()
                .filter(|(_, s)| *s == (chapter, verse))
                .map(|(t, _)| *t)
                .collect();
            if listed.is_empty() && !self.0.iter().any(|(t, _)| *t == (chapter, verse)) {
                out.push((chapter, verse));
            }
            out.extend(listed);
        }
    }

    fn key(chapter: u16, first: u16, last: u16) -> VerseKey {
        VerseKey::new(chapter, first, last).unwrap()
    }

    /// One unit's target keys and source keys.
    type Unit = (Vec<VerseKey>, Vec<VerseKey>);

    /// Every unit, and the facts.
    fn paired(
        target: &[VerseKey],
        source: &[VerseKey],
        walk: &dyn Crosswalk,
    ) -> (Vec<Unit>, Vec<AlignmentFact>) {
        let mut units = Vec::new();
        let mut facts = Vec::new();
        pair_keys_across(
            PSA,
            target,
            source,
            walk,
            &mut |_, left, right| {
                units.push((
                    left.iter().map(|at| target[*at]).collect(),
                    right.iter().map(|at| source[*at]).collect(),
                ));
            },
            &mut facts,
        );
        (units, facts)
    }

    #[test]
    fn with_nothing_moved_it_pairs_exactly_as_pair_keys() {
        let target = [key(1, 1, 1), key(1, 2, 3), key(1, 4, 4), key(1, 6, 6)];
        let source = [
            key(1, 1, 1),
            key(1, 2, 2),
            key(1, 3, 3),
            key(1, 4, 4),
            key(1, 5, 5),
        ];
        let (units, facts) = paired(&target, &source, &Table(vec![]));
        let mut plain_units = Vec::new();
        let mut plain_facts = Vec::new();
        pair_keys(
            PSA,
            &target,
            &source,
            &mut |_, left, right| {
                plain_units.push((
                    left.iter().map(|at| target[*at]).collect::<Vec<_>>(),
                    right.iter().map(|at| source[*at]).collect::<Vec<_>>(),
                ));
            },
            &mut plain_facts,
        );
        // The same units; pair_keys emits exact keys before bridges, this
        // walks the target in order, and nothing downstream reads the order.
        let (mut units, mut plain_units) = (units, plain_units);
        units.sort();
        plain_units.sort();
        assert_eq!(units, plain_units);
        assert_eq!(facts, plain_facts);
    }

    /// eng numbers Psalm 3's title as nothing and its verses 1-8; org
    /// numbers the title 1 and the verses 2-9. Every verse pairs with its
    /// counterpart, and org's title verse is no "missing" verse: the target
    /// has it as a `\d` title, not a verse.
    #[test]
    fn a_shift_by_one_pairs_every_verse_and_the_title_is_not_missing() {
        let mut walk = vec![((3, 0), (3, 1))];
        walk.extend((1..=8).map(|v| ((3, v), (3, v + 1))));
        let target: Vec<VerseKey> = (1..=8).map(|v| key(3, v, v)).collect();
        let source: Vec<VerseKey> = (1..=9).map(|v| key(3, v, v)).collect();
        let (units, facts) = paired(&target, &source, &Table(walk));
        assert_eq!(units.len(), 8);
        assert_eq!(units[0], (vec![key(3, 1, 1)], vec![key(3, 2, 2)]));
        assert!(facts.is_empty(), "{facts:?}");
    }

    #[test]
    fn a_merge_and_a_split_are_each_one_unit() {
        // Target 13:5 and 13:6 are both source 13:6; target 14:1 is source 14:1-2.
        let walk = Table(vec![
            ((13, 5), (13, 6)),
            ((13, 6), (13, 6)),
            ((14, 1), (14, 1)),
            ((14, 1), (14, 2)),
            ((14, 2), (14, 3)),
        ]);
        let target = [key(13, 5, 5), key(13, 6, 6), key(14, 1, 1), key(14, 2, 2)];
        let source = [key(13, 6, 6), key(14, 1, 1), key(14, 2, 2), key(14, 3, 3)];
        let (units, facts) = paired(&target, &source, &walk);
        assert_eq!(
            units,
            [
                (vec![key(13, 5, 5), key(13, 6, 6)], vec![key(13, 6, 6)]),
                (vec![key(14, 1, 1)], vec![key(14, 1, 1), key(14, 2, 2)]),
                (vec![key(14, 2, 2)], vec![key(14, 3, 3)]),
            ]
        );
        assert!(facts.is_empty(), "{facts:?}");
    }

    #[test]
    fn a_source_verse_the_target_lacks_is_missing_in_the_targets_numbering() {
        // Target numbers one behind the source; it lacks the source's 3:5,
        // which is its own 3:4.
        let walk = Table((1..=8).map(|v| ((3, v), (3, v + 1))).collect());
        let target = [key(3, 1, 1), key(3, 2, 2), key(3, 3, 3), key(3, 5, 5)];
        let source = [
            key(3, 2, 2),
            key(3, 3, 3),
            key(3, 4, 4),
            key(3, 5, 5),
            key(3, 6, 6),
        ];
        let (_, facts) = paired(&target, &source, &walk);
        assert_eq!(
            facts,
            [AlignmentFact::SourceOnly {
                book: PSA,
                key: key(3, 4, 4)
            }]
        );
    }

    #[test]
    fn a_repeated_key_is_blocked_rather_than_guessed() {
        let target = [key(1, 1, 1), key(1, 1, 1), key(1, 2, 2)];
        let source = [key(1, 1, 1), key(1, 2, 2)];
        let (units, facts) = paired(&target, &source, &Table(vec![]));
        assert_eq!(units, [(vec![key(1, 2, 2)], vec![key(1, 2, 2)])]);
        assert!(facts.contains(&AlignmentFact::AmbiguousDuplicate {
            book: PSA,
            key: key(1, 1, 1)
        }));
    }
}
