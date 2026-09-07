//! Which verse keys one side of a pairing does not hold, and which it holds
//! empty.
//!
//! ```text
//! target  MRK 1:1  1:2       1:4        source  MRK 1:1 1:2 1:3 1:4  1:5 1:6
//!         "..."    ""        "..."                                    (absent
//!                                                                     from target)
//!   Missing  key 1:3, 1 key,  span 1:2's end .. 1:2's end
//!   Missing  keys 1:5-1:6, 2 keys, span 1:4's end .. 1:4's end
//!   Empty    key 1:2, 1 key,  span 1:2
//! ```
//!
//! Three statements about keys and counts, never about translation quality.
//! A row says HOW MANY consecutive keys it covers; a consumer reads WHICH from
//! the gap in its own table of contents around the published span. What is not
//! claimed, and why an ambiguous duplicate or a partial-overlap bridge stays a
//! fact instead: `sous-chef/rules/presence-shear.md`. The computation, the
//! coalescing law, and where each span lands: `presence.md`.

use rustc_hash::FxHashMap;

use crate::codec::PresenceKind;
use crate::substrate::VerseLength;
use crate::{AlignmentFact, TextRange, VerseKey};

/// One coalesced presence row: its kind, the first key it covers, how many
/// consecutive keys, and the target-side span a consumer navigates to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresenceRow {
    kind: PresenceKind,
    key: VerseKey,
    keys: u32,
    span: TextRange,
}

impl PresenceRow {
    pub const fn kind(self) -> PresenceKind {
        self.kind
    }

    /// The first key the row covers; the rest follow it consecutively.
    pub const fn key(self) -> VerseKey {
        self.key
    }

    /// Keys covered, a bridge counting as one. Never zero.
    pub const fn keys(self) -> u32 {
        self.keys
    }

    /// `Extra` and `Empty` bound their target verses; `Missing` is zero-length
    /// at the point the absent keys would be inserted.
    pub const fn span(self) -> TextRange {
        self.span
    }
}

/// Turns one book's pairing leftovers into coalesced rows, in span order.
///
/// `facts` is exactly what this book's pairing pushed, `empties` the paired
/// units whose target side counted no graphemes beside a source that counted
/// some. A pure function of both: no text is read.
pub(crate) fn rows(
    target: &[VerseLength],
    facts: &[AlignmentFact],
    empties: &[(VerseKey, TextRange)],
) -> Vec<PresenceRow> {
    if facts.is_empty() && empties.is_empty() {
        return Vec::new();
    }
    // A key may repeat in one book, so a span is the bound over its rows.
    let mut spans: FxHashMap<VerseKey, TextRange> = FxHashMap::default();
    for row in target {
        widen(&mut spans, row.key(), row.text());
    }

    let mut missing: Vec<VerseKey> = Vec::new();
    let mut extra: Vec<(VerseKey, TextRange)> = Vec::new();
    for fact in facts {
        match *fact {
            AlignmentFact::SourceOnly { key, .. } => missing.push(key),
            AlignmentFact::TargetOnly { key, .. } => {
                if let Some(span) = spans.get(&key) {
                    extra.push((key, *span));
                }
            }
            // A duplicate the pairing could not disambiguate, and a bridge
            // overlapping an incompatible one, are facts and not rows.
            AlignmentFact::AmbiguousDuplicate { .. } | AlignmentFact::PartialOverlap { .. } => {}
        }
    }

    let mut out = Vec::new();
    coalesce(PresenceKind::Extra, extra, &mut out);
    coalesce(PresenceKind::Empty, empties.to_vec(), &mut out);
    if !missing.is_empty() {
        let ordered = insertion_points(target);
        let placed: Vec<(VerseKey, TextRange)> = missing
            .into_iter()
            .map(|key| (key, insertion_point(&ordered, key)))
            .collect();
        coalesce(PresenceKind::Missing, placed, &mut out);
    }
    out.sort_by_key(|row| (row.span.from(), row.span.to(), row.kind as u8));
    out
}

/// Keyed target rows in key order, the one lane an insertion point reads.
fn insertion_points(target: &[VerseLength]) -> Vec<(VerseKey, TextRange)> {
    let mut ordered: Vec<(VerseKey, TextRange)> =
        target.iter().map(|row| (row.key(), row.text())).collect();
    ordered.sort_by_key(|(key, span)| (*key, span.from()));
    ordered
}

/// Where an absent key would go: after the last target verse of its chapter
/// that precedes it, before the chapter's first verse when nothing precedes,
/// and at the end of the target book when the whole chapter is absent.
fn insertion_point(ordered: &[(VerseKey, TextRange)], key: VerseKey) -> TextRange {
    let chapter: Vec<&(VerseKey, TextRange)> = ordered
        .iter()
        .filter(|(row, _)| row.chapter() == key.chapter())
        .collect();
    let at = match chapter.iter().rev().find(|(row, _)| *row < key) {
        Some((_, span)) => span.to(),
        None => match chapter.first() {
            Some((_, span)) => span.from(),
            None => ordered.last().map_or(0, |(_, span)| span.to()),
        },
    };
    TextRange::new(at, at).expect("a zero-length range keeps its order")
}

/// Consecutive keys of one chapter become one row: `3`, `4`, `5` is a single
/// statement, and so is a whole absent chapter.
fn coalesce(kind: PresenceKind, keyed: Vec<(VerseKey, TextRange)>, out: &mut Vec<PresenceRow>) {
    if keyed.is_empty() {
        return;
    }
    // One entry per key, whatever the producer's row order: a key repeated on
    // the target side is still one statement about that key.
    let mut spans: FxHashMap<VerseKey, TextRange> = FxHashMap::default();
    for (key, span) in keyed {
        widen(&mut spans, key, span);
    }
    let mut ordered: Vec<(VerseKey, TextRange)> = spans.into_iter().collect();
    ordered.sort_by_key(|(key, _)| *key);

    let mut run: Option<(PresenceRow, VerseKey)> = None;
    for (key, span) in ordered {
        let continues = run.as_ref().is_some_and(|(_, last)| {
            last.chapter() == key.chapter() && last.last().checked_add(1) == Some(key.first())
        });
        if continues {
            let (row, last) = run.as_mut().expect("the run continues one that exists");
            row.keys += 1;
            row.span = TextRange::new(
                row.span.from().min(span.from()),
                row.span.to().max(span.to()),
            )
            .expect("a bounding range keeps its order");
            *last = key;
            continue;
        }
        if let Some((row, _)) = run.replace((
            PresenceRow {
                kind,
                key,
                keys: 1,
                span,
            },
            key,
        )) {
            out.push(row);
        }
    }
    if let Some((row, _)) = run {
        out.push(row);
    }
}

fn widen(spans: &mut FxHashMap<VerseKey, TextRange>, key: VerseKey, span: TextRange) {
    spans
        .entry(key)
        .and_modify(|seen| {
            *seen = TextRange::new(seen.from().min(span.from()), seen.to().max(span.to()))
                .expect("a bounding range keeps its order");
        })
        .or_insert(span);
}
