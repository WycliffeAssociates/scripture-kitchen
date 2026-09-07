//! A judged pattern gets coordinates exactly once.
//!
//! ```text
//! book cache hit   -> replay(rows)                  no text read
//! hot book, one
//! chapter edited   -> site_by_chapter(keys)         one chapter walked
//! anything else    -> ChapterPass::locate           the book rescanned
//! ```
//!
//! A cached row keeps a [`PatternRef`], not a table index, so a publication
//! that renumbered the pattern table replays it under this publication's
//! numbering; spans are chapter-relative wherever a chapter owns them.

use rustc_hash::FxHashMap;
use sous_core::{
    Chapter, ChapterPass, ConventionDigest, FindingKind, Findings, PackedFinding, Pattern,
    PatternIndex, Reasons, TextRange, Verse,
};

use sous_core::BookIndex;

use super::keys::{ChapterSiteKey, PatternRef};
use super::{OnionBook, PublishError};
use crate::pantry::derived::Store;
use crate::pantry::{BookId, Pantry};

/// One located row without its text: a span plus whatever names the pattern.
#[derive(Debug, Clone, Copy)]
pub(super) struct SiteRow {
    from: u32,
    to: u32,
    kind: CachedKind,
}

/// A convention row keeps a content reference, so a renumbered table still
/// resolves it; anything else a `locate` pushed replays as it stands.
#[derive(Debug, Clone, Copy)]
pub(super) enum CachedKind {
    Convention {
        pattern: PatternRef,
        reasons: Reasons,
    },
    Other(FindingKind),
}

impl SiteRow {
    pub(super) fn of(row: &PackedFinding, table: &[Pattern]) -> Self {
        let kind = match row.kind() {
            FindingKind::Convention(digest) => CachedKind::Convention {
                pattern: PatternRef::of(&table[usize::from(digest.pattern().get())]),
                reasons: digest.reasons(),
            },
            other => CachedKind::Other(other),
        };
        Self {
            from: row.from(),
            to: row.to(),
            kind,
        }
    }

    /// The same row against a chapter's own start, so a chapter cached under
    /// one book's coordinates replays under another's.
    pub(super) fn rebased(self, start: u32) -> Self {
        debug_assert!(self.from >= start, "a chapter's row starts inside it");
        Self {
            from: self.from - start,
            to: self.to - start,
            ..self
        }
    }
}

/// One registered book's projected view, rebuilt from the products it already
/// retains. The text is borrowed, never copied.
pub(super) fn projection(pantry: &Pantry, id: &BookId) -> Result<OnionBook, PublishError> {
    let products = pantry.products(id).expect("the pantry listed this id");
    let text = products
        .text
        .ok_or_else(|| PublishError::NoText { id: id.clone() })?;
    OnionBook::from_parts(text, products.mask.clone(), products.toc.clone()).map_err(|error| {
        PublishError::InvalidBook {
            id: id.clone(),
            error,
        }
    })
}

/// One hot book's rows chapter by chapter: the book-wide members place theirs
/// as they always do, then every chapter either replays the rows its key
/// already names or is walked with the run of missing chapters around it.
///
/// Rows land in chapter order, so the sequence is the one a whole-book
/// [`ChapterPass::locate`] would have pushed. One walk per RUN, not per
/// chapter: reading a firing set is per book, and a keystroke leaves exactly
/// one chapter missing anyway.
///
/// Returns the chapters walked.
#[allow(clippy::too_many_arguments)]
pub(super) fn site_by_chapter<P: ChapterPass>(
    pass: &P,
    book: BookIndex,
    text: &str,
    chapters: &[Chapter],
    verses: &[Verse],
    aggregate: &P::Aggregate,
    keys: &[ChapterSiteKey],
    cache: &mut Store<ChapterSiteKey, Box<[SiteRow]>>,
    table: &[Pattern],
    resolver: &FxHashMap<PatternRef, PatternIndex>,
    out: &mut Findings,
) -> u64 {
    debug_assert_eq!(keys.len(), chapters.len(), "one key per chapter");
    pass.locate_book(book, text, chapters, verses, aggregate, out);
    let mut counts: Vec<u32> = Vec::new();
    let mut walked = 0;
    let mut at = 0;
    while at < keys.len() {
        if let Some(rows) = cache.get(&keys[at]) {
            replay(book, rows, chapters[at].text().from(), resolver, out);
            at += 1;
            continue;
        }
        let mut end = at + 1;
        while end < keys.len() && !cache.contains_key(&keys[end]) {
            end += 1;
        }
        counts.clear();
        let from = out.len();
        pass.locate_chapters(
            book,
            text,
            chapters,
            verses,
            at..end,
            aggregate,
            &mut counts,
            out,
        );
        assert_eq!(
            counts.len(),
            end - at,
            "a chapter-scoped pass counts every chapter it was given"
        );
        let mut row = from;
        for (offset, count) in counts.iter().enumerate() {
            let to = row + *count as usize;
            let start = chapters[at + offset].text().from();
            cache.insert(
                keys[at + offset],
                out.rows()[row..to]
                    .iter()
                    .map(|packed| SiteRow::of(packed, table).rebased(start))
                    .collect(),
            );
            row = to;
        }
        debug_assert_eq!(row, out.len(), "the counts cover every row pushed");
        walked += (end - at) as u64;
        at = end;
    }
    walked
}

/// Pushes cached rows back, each convention row under the pattern index THIS
/// publication gave its content, and each span moved forward by `start`.
///
/// `start` is zero for a book's own rows and the chapter's projected start for
/// rows cached per chapter. A row whose pattern the table no longer holds
/// cannot happen: the firing hash covers exactly the contents that produced
/// these rows.
pub(super) fn replay(
    book: BookIndex,
    rows: &[SiteRow],
    start: u32,
    resolver: &FxHashMap<PatternRef, PatternIndex>,
    out: &mut Findings,
) {
    out.open_book(book);
    for row in rows {
        let kind = match row.kind {
            CachedKind::Convention { pattern, reasons } => {
                let index = resolver[&pattern];
                FindingKind::Convention(ConventionDigest::new(index, reasons))
            }
            CachedKind::Other(kind) => kind,
        };
        let span = TextRange::new(row.from + start, row.to + start)
            .expect("a cached span keeps its order");
        out.push(span, kind)
            .expect("a replayed span lies inside the book it was found in");
    }
}
