//! One row per habit: hygiene rows grouped by (book or project, class,
//! placement), and the deletions that remove a group's runs.
//!
//! ```text
//! JHN  NoBreakSpace    BesideSpace     ×8157  fix: delete each run
//! JHN  NoBreakSpace    BetweenLetters  ×12    fix: keep each run's first scalar
//! —    MisplacedFormat BesideSpace     ×235k  fix: delete each ZWNJ, never the letter its span starts on
//! JHN  ReplacementChar Other           ×3     no fix
//! ```
//!
//! Every fix is a deletion, applied back to front. The row says where its
//! run sits in the span (the site's anchor), and the one-unit bit makes `run`
//! a UTF-16 length, so in a UTF-16 publication a run at either end of its
//! span is exact. In a UTF-8 publication only an `Exact` removal is.
//! The table and its reasons: `galley/sous-messages.md`, "Hygiene fixes".

use crate::corpus::{CoordinateSpace, CorpusSnapshot, CorpusWireError};
use crate::{Anchor, BookIndex, FindingKind, HygieneClass, HygieneDigest, Placement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Book,
    Project,
}

/// `book` is `None` in project scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupKey {
    pub book: Option<BookIndex>,
    pub class: HygieneClass,
    pub placement: Placement,
}

/// One published hygiene row: its book and position in that book's section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HygieneRow {
    pub book: BookIndex,
    pub row: u32,
    pub from: u32,
    pub to: u32,
    pub digest: HygieneDigest,
}

/// `onion::Edit`'s shape with an empty insert, in the publication's
/// coordinates: a host builds `Edit { from, to, insert: FixStr::EMPTY }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deletion {
    pub book: BookIndex,
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneGroup {
    key: GroupKey,
    space: CoordinateSpace,
    rows: Vec<HygieneRow>,
}

impl HygieneGroup {
    pub const fn key(&self) -> GroupKey {
        self.key
    }

    pub fn count(&self) -> usize {
        self.rows.len()
    }

    pub fn rows(&self) -> &[HygieneRow] {
        &self.rows
    }

    /// The deletions for every row that has one, last first within a book.
    /// `None` when no row of the group has a fix.
    pub fn fix(&self) -> Option<Vec<Deletion>> {
        let mut out: Vec<Deletion> = self
            .rows
            .iter()
            .filter_map(|row| deletion(row, self.space))
            .collect();
        if out.is_empty() {
            return None;
        }
        out.sort_by(|a, b| a.book.cmp(&b.book).then(b.from.cmp(&a.from)));
        Some(out)
    }
}

/// Every hygiene row of the publication, grouped, in key order.
pub fn hygiene_groups(
    snapshot: &CorpusSnapshot<'_>,
    scope: Scope,
) -> Result<Vec<HygieneGroup>, CorpusWireError> {
    let mut groups: Vec<HygieneGroup> = Vec::new();
    for index in 0..snapshot.len() {
        let book = snapshot
            .book(BookIndex::new(index).expect("the directory indexes every book"))
            .expect("an index under len names a book");
        for row in 0..book.len() {
            let finding = book.at(row)?;
            let FindingKind::Hygiene(digest) = finding.kind() else {
                continue;
            };
            let key = GroupKey {
                book: (scope == Scope::Book).then(|| book.index()),
                class: digest.class(),
                placement: digest.site().placement(),
            };
            let row = HygieneRow {
                book: book.index(),
                row: row as u32,
                from: finding.from(),
                to: finding.to(),
                digest,
            };
            match groups.binary_search_by(|group| group.key.cmp(&key)) {
                Ok(at) => groups[at].rows.push(row),
                Err(at) => groups.insert(
                    at,
                    HygieneGroup {
                        key,
                        space: snapshot.coordinate_space(),
                        rows: vec![row],
                    },
                ),
            }
        }
    }
    Ok(groups)
}

#[derive(Clone, Copy)]
enum Action {
    Remove,
    KeepOne,
}

/// The fix table. A class or placement it does not name has no fix.
fn action(digest: HygieneDigest) -> Option<Action> {
    use HygieneClass::*;
    use Placement::*;
    let site = digest.site();
    match (digest.class(), site.placement()) {
        (C0Control | Delete | Noncharacter, _) => Some(Action::Remove),
        (NoBreakSpace | MisplacedFormat, _) if site.lone_is_silent() => Some(Action::KeepOne),
        (NoBreakSpace, BesideSpace | AtEdge | Alone) => Some(Action::Remove),
        (MisplacedFormat, BesideSpace | BesidePunctuation | AtEdge | Alone) => Some(Action::Remove),
        _ => None,
    }
}

fn deletion(row: &HygieneRow, space: CoordinateSpace) -> Option<Deletion> {
    let digest = row.digest;
    if digest.saturated() {
        return None;
    }
    let action = action(digest)?;
    let site = digest.site();
    let units = (space == CoordinateSpace::Utf16 && site.one_unit()).then_some(digest.run());
    let (from, to) = match (action, site.anchor()) {
        (_, Anchor::Neither) => return None,
        (Action::Remove, Anchor::Exact) => (row.from, row.to),
        (Action::Remove, Anchor::AtStart) => (row.from, row.from + units?),
        (Action::Remove, Anchor::AtEnd) => (row.to - units?, row.to),
        (Action::KeepOne, Anchor::Exact | Anchor::AtStart) => (row.from + 1, row.from + units?),
        (Action::KeepOne, Anchor::AtEnd) => (row.to - units? + 1, row.to),
    };
    (from < to).then_some(Deletion {
        book: row.book,
        from,
        to,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{PublicationBook, SnapshotId, encode_to_corpus_buffer};
    use crate::{BookKey, HygieneSite, PackedFinding};

    fn row(from: u32, to: u32, class: HygieneClass, run: u32, site: HygieneSite) -> PackedFinding {
        let digest = HygieneDigest::new(class, run, site).unwrap();
        PackedFinding::new(
            from,
            to,
            BookIndex::new(0).unwrap(),
            FindingKind::Hygiene(digest),
            &[100],
        )
        .unwrap()
    }

    fn publication(space: CoordinateSpace, rows: &[PackedFinding]) -> Vec<u8> {
        let book = PublicationBook::new(BookKey::new(*b"GEN"), "gen.usfm", 100, rows);
        encode_to_corpus_buffer(SnapshotId::new([0; 16]), space, &[book], &[], &[], &[]).unwrap()
    }

    fn deletions(group: &HygieneGroup) -> Option<Vec<(u32, u32)>> {
        group
            .fix()
            .map(|all| all.iter().map(|d| (d.from, d.to)).collect())
    }

    #[test]
    fn a_group_deletes_each_run_and_never_the_letter_its_span_starts_on() {
        let zwnj = HygieneSite::new(Placement::BesideSpace, Anchor::AtEnd, true, false);
        let pair = HygieneSite::new(Placement::BetweenLetters, Anchor::Exact, true, true);
        let lost = HygieneSite::new(Placement::Other, Anchor::Exact, true, false);
        let rows = [
            row(3, 5, HygieneClass::MisplacedFormat, 1, zwnj),
            row(10, 13, HygieneClass::NoBreakSpace, 3, pair),
            row(20, 21, HygieneClass::ReplacementChar, 1, lost),
            row(30, 32, HygieneClass::MisplacedFormat, 1, zwnj),
        ];
        let buffer = publication(CoordinateSpace::Utf16, &rows);
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        let groups = hygiene_groups(&snapshot, Scope::Book).unwrap();
        let keys: Vec<_> = groups
            .iter()
            .map(|g| (g.key().class, g.key().placement, g.count()))
            .collect();
        assert_eq!(
            keys,
            [
                (HygieneClass::ReplacementChar, Placement::Other, 1),
                (HygieneClass::MisplacedFormat, Placement::BesideSpace, 2),
                (HygieneClass::NoBreakSpace, Placement::BetweenLetters, 1),
            ]
        );
        assert_eq!(deletions(&groups[0]), None);
        assert_eq!(deletions(&groups[1]), Some(vec![(31, 32), (4, 5)]));
        assert_eq!(deletions(&groups[2]), Some(vec![(11, 13)]));
        let rows: Vec<u32> = groups[1].rows().iter().map(|row| row.row).collect();
        assert_eq!(rows, [0, 3]);
    }

    #[test]
    fn a_utf8_publication_fixes_only_a_run_that_is_its_span() {
        let at_end = HygieneSite::new(Placement::BesideSpace, Anchor::AtEnd, true, false);
        let exact = HygieneSite::new(Placement::BesideSpace, Anchor::Exact, true, false);
        let rows = [
            row(3, 8, HygieneClass::MisplacedFormat, 1, at_end),
            row(10, 12, HygieneClass::NoBreakSpace, 1, exact),
        ];
        let buffer = publication(CoordinateSpace::Utf8, &rows);
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        let groups = hygiene_groups(&snapshot, Scope::Project).unwrap();
        assert_eq!(groups[0].key().book, None);
        assert_eq!(deletions(&groups[0]), None);
        assert_eq!(deletions(&groups[1]), Some(vec![(10, 12)]));
    }
}
