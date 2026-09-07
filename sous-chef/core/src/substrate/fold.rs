//! The book fold: merges chapter rows in order, resolving each seam.
//!
//! ```text
//! fold_book([row("one."), row(" Two")])
//!   → one BookAggregate, pairs/follows/runs merged across the seam
//! ```

use super::{
    BookAggregate, ChapterObs, ChapterRow, Edge, FollowCounts, OuterClass, PairKey, ScalarKey,
};

/// Merges one book's rows in order, resolving each chapter seam against the
/// previous chapter's trailing edge, and rebases the hygiene lane.
///
/// A nonletter run that straddles a masked `\c` stays two runs, as it does
/// for hygiene; every other lane reads as if the book were one string.
pub fn fold_book(book: &[ChapterObs<&ChapterRow>], carry: &mut Edge) -> BookAggregate {
    let mut out = BookAggregate::default();
    let mut scalar_scratch = Vec::new();
    let mut pair_scratch = Vec::new();
    let mut run_scratch = Vec::new();
    let mut follow_scratch = Vec::new();

    for chapter in book {
        let row = chapter.obs;
        out.chapters += 1;
        merge_counts(&mut out.scalars, &row.scalars, &mut scalar_scratch);
        merge_counts(&mut out.pairs, &row.pairs, &mut pair_scratch);
        merge_runs(&mut out.runs, row, &mut run_scratch);
        merge_follows(&mut out.follows, &row.follows, &mut follow_scratch);
        out.hygiene
            .extend(row.hygiene.iter().map(|site| site.rebased(chapter.start)));
        out.scalar_count += u64::from(row.scalar_count);
        out.word_count += u64::from(row.word_count);

        if row.scalar_count == 0 {
            // An empty chapter is not a neighbor; the carry passes through it.
            continue;
        }
        if carry.outer != OuterClass::Edge {
            resolve_seam(&mut out, *carry, row.lead);
        }

        let mut next = row.trail;
        if row.scalar_count == 1
            && carry.outer != OuterClass::Edge
            && let Some((key, _)) = next.open_pair
        {
            // Lead and trail are the same scalar: its `prev` was just fixed.
            next.open_pair = Some((key, carry.outer));
        }
        if next.blank {
            next.open_follow = carry.open_follow;
        }
        *carry = next;
    }
    out
}

/// One seam: the pair either side of it, the follow across it, and the word
/// the masked `\c` split in two.
fn resolve_seam(out: &mut BookAggregate, trail: Edge, lead: Edge) {
    if let Some((key, prev)) = trail.open_pair {
        bump(
            &mut out.pairs,
            PairKey::new(key, prev, OuterClass::Edge),
            -1,
        );
        bump(&mut out.pairs, PairKey::new(key, prev, lead.outer), 1);
    }
    if let Some((key, next)) = lead.open_pair {
        bump(
            &mut out.pairs,
            PairKey::new(key, OuterClass::Edge, next),
            -1,
        );
        bump(&mut out.pairs, PairKey::new(key, trail.outer, next), 1);
    }
    if let (Some(key), Some(case)) = (trail.open_follow, lead.edge_case) {
        let mut counts = FollowCounts::default();
        counts.0[case as usize] = 1;
        match out.follows.binary_search_by_key(&key, |entry| entry.0) {
            Ok(at) => out.follows[at].1.absorb(counts),
            Err(at) => out.follows.insert(at, (key, counts)),
        }
    }
    let joins = |class: OuterClass| matches!(class, OuterClass::Letter | OuterClass::Digit);
    if joins(trail.outer) && joins(lead.outer) {
        out.word_count -= 1;
    }
}

/// Adds `delta` to one sorted count, inserting or deleting the row as needed.
fn bump(counts: &mut Vec<(PairKey, u32)>, key: PairKey, delta: i32) {
    match counts.binary_search_by_key(&key, |entry| entry.0) {
        Ok(at) => {
            let next = i64::from(counts[at].1) + i64::from(delta);
            debug_assert!(
                next >= 0,
                "a seam fix never removes a count that is not there"
            );
            if next == 0 {
                counts.remove(at);
            } else {
                counts[at].1 = next as u32;
            }
        }
        Err(at) => {
            debug_assert!(
                delta > 0,
                "a seam fix never removes a count that is not there"
            );
            counts.insert(at, (key, delta.unsigned_abs()));
        }
    }
}

/// Sorted merge of two count lanes, `dst` taking the sum.
fn merge_counts<K: Ord + Copy>(
    dst: &mut Vec<(K, u32)>,
    src: &[(K, u32)],
    scratch: &mut Vec<(K, u32)>,
) {
    if src.is_empty() {
        return;
    }
    if dst.is_empty() {
        dst.extend_from_slice(src);
        return;
    }
    scratch.clear();
    scratch.reserve(dst.len() + src.len());
    let (mut left, mut right) = (0, 0);
    while left < dst.len() && right < src.len() {
        match dst[left].0.cmp(&src[right].0) {
            std::cmp::Ordering::Less => {
                scratch.push(dst[left]);
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                scratch.push(src[right]);
                right += 1;
            }
            std::cmp::Ordering::Equal => {
                scratch.push((dst[left].0, dst[left].1 + src[right].1));
                left += 1;
                right += 1;
            }
        }
    }
    scratch.extend_from_slice(&dst[left..]);
    scratch.extend_from_slice(&src[right..]);
    std::mem::swap(dst, scratch);
}

fn merge_follows(
    dst: &mut Vec<(ScalarKey, FollowCounts)>,
    src: &[(ScalarKey, FollowCounts)],
    scratch: &mut Vec<(ScalarKey, FollowCounts)>,
) {
    if src.is_empty() {
        return;
    }
    if dst.is_empty() {
        dst.extend_from_slice(src);
        return;
    }
    scratch.clear();
    let (mut left, mut right) = (0, 0);
    while left < dst.len() && right < src.len() {
        match dst[left].0.cmp(&src[right].0) {
            std::cmp::Ordering::Less => {
                scratch.push(dst[left]);
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                scratch.push(src[right]);
                right += 1;
            }
            std::cmp::Ordering::Equal => {
                let mut counts = dst[left].1;
                counts.absorb(src[right].1);
                scratch.push((dst[left].0, counts));
                left += 1;
                right += 1;
            }
        }
    }
    scratch.extend_from_slice(&dst[left..]);
    scratch.extend_from_slice(&src[right..]);
    std::mem::swap(dst, scratch);
}

fn merge_runs(
    dst: &mut Vec<(Box<[ScalarKey]>, u32)>,
    row: &ChapterRow,
    scratch: &mut Vec<(Box<[ScalarKey]>, u32)>,
) {
    if row.runs.is_empty() {
        return;
    }
    scratch.clear();
    let mut left = 0;
    let mut src = row.runs();
    let mut next = src.next();
    while left < dst.len() {
        let Some((atoms, count)) = next else { break };
        match dst[left].0.as_ref().cmp(atoms) {
            std::cmp::Ordering::Less => {
                scratch.push(std::mem::take(&mut dst[left]));
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                scratch.push((atoms.into(), count));
                next = src.next();
            }
            std::cmp::Ordering::Equal => {
                let mut held = std::mem::take(&mut dst[left]);
                held.1 += count;
                scratch.push(held);
                left += 1;
                next = src.next();
            }
        }
    }
    for entry in &mut dst[left..] {
        scratch.push(std::mem::take(entry));
    }
    while let Some((atoms, count)) = next {
        scratch.push((atoms.into(), count));
        next = src.next();
    }
    std::mem::swap(dst, scratch);
}
