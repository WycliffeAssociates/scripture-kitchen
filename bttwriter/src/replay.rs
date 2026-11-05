//! Writer histories → one line of commits, one book file each, in one pack.
//!
//! ```text
//! histories: ROM [s1 s2 s3], TIT [t1 t2]          replay(&histories, &ReplayOptions { now, deltas: true })
//!   by author time, a book never reordered    →   s1 ─ t1 ─ s2 ─ t2 ─ s3   (a new root; head = s3)
//!   each commit's tree                            46-ROM.usfm, 57-TIT.usfm: every book written so far
//!   each commit's message                         ROM 5:1–11; titles
//!
//!                                                 Writer-Message: 2019-03-04T10:22:31
//!                                                 Writer-Source: owner/repo@<save id>
//!                                                 Writer-Clock: 2000-01-01 (device clock; date estimated)
//! ```
//!
//! Every save that changed a book's text becomes a commit holding the whole
//! book file as the template renders it after that save, with the save's own
//! author, committer, date and zone. A save that leaves the rendering as it
//! was is no commit (`unchanged`). The `Writer-Clock` trailer appears only on
//! a save whose date was estimated ([`crate::clock`]).
//!
//! Ported from Sefer's `replayWriterHistories` (`writerHistory.ts` at
//! `326f02c8`), plus the hub's clock fix: for a history with no bad clock the
//! commit ids are Sefer's (`tests/replay.rs` pins them). A book's files are
//! held in the order the saves first wrote them, as Sefer's `Map` holds them.
//!
//! Nothing is committed on top: [`Replayed::head`] is the last save. A
//! manifest, an assembly commit or a merge is the caller's.
//!
//! # Threads are the caller's
//!
//! Replay is one sequential line (each commit names the one before), and it
//! runs once per migration, so this crate starts no threads. What does
//! parallelize is reading: each book's history is independent. A native
//! caller with many books can read them on its own threads, one source per
//! thread, and hand the results to `replay`:
//!
//! ```ignore
//! use rayon::prelude::*;
//!
//! let histories = repos
//!     .par_iter()
//!     .map(|(path, source)| {
//!         let repo = git2::Repository::open_bare(path)?;
//!         let head = repo.head()?.peel_to_commit()?.id().into();
//!         let mut objects = bttwriter::Git2Source::new(repo.odb()?);
//!         Ok(bttwriter::read_history(&mut objects, head, &[], source)?)
//!     })
//!     .collect::<Result<Vec<_>, Box<dyn std::error::Error + Send + Sync>>>()?;
//! let replayed = bttwriter::replay(&histories, &ReplayOptions { now, deltas: true })?;
//! ```
//!
//! Measured on four books (1,484 commits), rendering is about a quarter of
//! replay's time and packing the rest, so threads inside replay would save
//! little.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use mise::books::BookKey;

use crate::clock::{Clock, clocks, day_of};
use crate::pack::{
    CommitFields, Kind, ObjectId, PackStats, PackWriter, Signature, commit_body, tree_body,
};
use crate::stitch::{book_to_usfm, is_js_space, js_trim, tree_from_files};
use crate::summary::{summary_of, verse_count};
use crate::walk::{Save, WriterHistory};

/// A device clock later than `now` plus this counts as wrong.
const CLOCK_SLACK: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayOptions {
    /// Unix seconds. A device clock later than a day past this can't be right.
    pub now: i64,
    /// Store each book's versions as deltas against the one before (git's
    /// own packs do). Off: every object whole, a pack several times larger.
    pub deltas: bool,
}

/// One book file the replay wrote: what a manifest needs to list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookFile {
    pub book: BookKey,
    /// `46-ROM.usfm`
    pub file: String,
    /// The book's `\h`, as rendered at its last save, else its English name.
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replayed {
    /// The last replayed commit. Nothing is committed on top of it: a
    /// manifest, an assembly commit or a merge is the caller's.
    pub head: ObjectId,
    /// → `.git/objects/pack/<name>.pack`
    pub pack: Vec<u8>,
    /// → `.git/objects/pack/<name>.idx` (v2)
    pub idx: Vec<u8>,
    /// `pack-<sha of the pack>`
    pub name: String,
    /// The books written, in the order of `histories`.
    pub books: Vec<BookFile>,
    pub commits: usize,
    /// Saves that changed no rendered text: no commit for them.
    pub unchanged: usize,
    /// Commits whose date was estimated (they carry a `Writer-Clock` trailer).
    pub estimated: usize,
    pub stats: PackStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// No save changed any book's rendering: there is nothing to commit.
    NothingChanged,
    /// Two histories of one book: they'd write one file from two projects.
    SameBook(BookKey),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::NothingChanged => write!(f, "no save changed any book"),
            ReplayError::SameBook(book) => write!(f, "two histories of {book}"),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Every history's saves as one line of commits, oldest first by author time
/// (estimated where the device's clock can't be right), as a new root. A
/// book's own saves never reorder: a save's place is never earlier than the
/// one before it in its book. Ties go to the earlier history, then the
/// earlier save.
pub fn replay(histories: &[WriterHistory], opts: &ReplayOptions) -> Result<Replayed, ReplayError> {
    let mut writer = PackWriter::new();
    let line = line(histories, opts, &mut writer)?;
    Ok(line.finish(writer, None))
}

/// The replayed line, before its pack is finished: what [`crate::consolidate()`]
/// commits a manifest on top of.
pub(crate) struct Line {
    pub head: ObjectId,
    /// The last commit's tree: every book file.
    pub tree: BTreeMap<String, ObjectId>,
    /// Each book written, with its rendering at its last save.
    pub books: Vec<(BookFile, String)>,
    pub commits: usize,
    pub unchanged: usize,
    pub estimated: usize,
}

impl Line {
    /// Finish the pack. `head` is the line's own, or a commit added on top.
    pub fn finish(self, writer: PackWriter, head: Option<ObjectId>) -> Replayed {
        let finished = writer.finish();
        Replayed {
            head: head.unwrap_or(self.head),
            pack: finished.pack,
            idx: finished.idx,
            name: finished.name,
            books: self.books.into_iter().map(|(file, _)| file).collect(),
            commits: self.commits,
            unchanged: self.unchanged,
            estimated: self.estimated,
            stats: finished.stats,
        }
    }
}

pub(crate) fn line(
    histories: &[WriterHistory],
    opts: &ReplayOptions,
    writer: &mut PackWriter,
) -> Result<Line, ReplayError> {
    let latest = opts.now.saturating_add(CLOCK_SLACK);
    let mut books: Vec<Book<'_>> = Vec::with_capacity(histories.len());
    for history in histories {
        let book = history.project.book;
        if books.iter().any(|b| b.key == book) {
            return Err(ReplayError::SameBook(book));
        }
        books.push(Book::new(history, latest));
    }

    let mut events: Vec<(i64, usize, usize)> = Vec::new();
    for (index, book) in books.iter().enumerate() {
        let mut key = i64::MIN;
        for (save, clock) in book.clocks.iter().enumerate() {
            key = key.max(clock.author_at);
            events.push((key, index, save));
        }
    }
    events.sort_unstable();

    fn lane(deltas: bool, name: &str) -> Option<&str> {
        deltas.then_some(name)
    }
    let mut tree: BTreeMap<String, ObjectId> = BTreeMap::new();
    let mut head: Option<ObjectId> = None;
    let (mut commits, mut unchanged, mut estimated) = (0, 0, 0);
    for (_, index, save_index) in events {
        let book = &mut books[index];
        let save = &book.history.saves[save_index];
        let clock = book.clocks[save_index];
        let Some((usfm, summary)) = book.apply(save) else {
            unchanged += 1;
            continue;
        };
        let blob = writer.add(Kind::Blob, usfm.as_bytes(), lane(opts.deltas, &book.file));
        tree.insert(book.file.clone(), blob);
        let root = writer.add(Kind::Tree, &tree_body(&tree), lane(opts.deltas, "tree"));
        let trailer = clock.device_at.map_or_else(String::new, |at| {
            format!(
                "Writer-Clock: {} (device clock; date estimated)\n",
                day_of(at)
            )
        });
        let message = format!(
            "{summary}\n\nWriter-Message: {}\nWriter-Source: {}@{}\n{trailer}",
            save.message, book.history.source, save.id
        );
        let parents: Vec<ObjectId> = head.into_iter().collect();
        let body = commit_body(&CommitFields {
            tree: root,
            parents: &parents,
            author: &at(&save.author, clock.author_at),
            committer: &at(&save.committer, clock.committer_at),
            message: &message,
        });
        head = Some(writer.add(Kind::Commit, &body, None));
        commits += 1;
        estimated += usize::from(clock.device_at.is_some());
    }

    let head = head.ok_or(ReplayError::NothingChanged)?;
    let books = books
        .into_iter()
        .filter_map(|book| {
            let rendered = book.rendered?;
            let title = h_title(&rendered).unwrap_or(book.fallback_title).to_owned();
            Some((
                BookFile {
                    book: book.key,
                    file: book.file,
                    title,
                },
                rendered,
            ))
        })
        .collect();
    Ok(Line {
        head,
        tree,
        books,
        commits,
        unchanged,
        estimated,
    })
}

fn at(who: &Signature, at: i64) -> Signature {
    Signature { at, ..who.clone() }
}

/// One book mid-replay.
struct Book<'h> {
    history: &'h WriterHistory,
    key: BookKey,
    file: String,
    fallback_title: &'static str,
    clocks: Vec<Clock>,
    files: Files,
    /// The rendering at the last commit; `None` before the first.
    rendered: Option<String>,
}

impl<'h> Book<'h> {
    fn new(history: &'h WriterHistory, latest: i64) -> Book<'h> {
        let key = history.project.book;
        Book {
            history,
            key,
            file: canon::usfm_file_name(key).unwrap_or_else(|| format!("{key}.usfm")),
            fallback_title: fallback_title(key),
            clocks: clocks(&history.saves, latest),
            files: Files::default(),
            rendered: None,
        }
    }

    /// Apply a save. The new rendering and what the save did, or `None` when
    /// the rendering is as it was.
    fn apply(&mut self, save: &Save) -> Option<(String, String)> {
        // A deleted chunk's verses are named from its text before the save.
        let gone: Vec<(&str, String)> = save
            .changes
            .iter()
            .filter(|(_, text)| text.is_none())
            .filter_map(|(path, _)| Some((path.as_str(), self.files.get(path)?.to_owned())))
            .collect();
        for (path, text) in &save.changes {
            match text {
                Some(text) => self.files.set(path, text),
                None => self.files.delete(path),
            }
        }
        let usfm = book_to_usfm(
            &tree_from_files(self.files.iter()),
            self.key,
            self.fallback_title,
        );
        if self.rendered.as_ref() == Some(&usfm) {
            return None;
        }
        self.rendered = Some(usfm.clone());
        let summary = summary_of(&self.key.to_string(), &save.changes, |path| {
            gone.iter()
                .find(|(p, _)| *p == path)
                .map(|(_, text)| text.as_str())
        });
        Some((usfm, summary))
    }
}

/// Canon's English name for a book: the template's title when
/// `front/title.txt` says nothing, as Sefer's `fallbackTitle`.
fn fallback_title(book: BookKey) -> &'static str {
    canon::book(book).map_or("", |b| b.english)
}

/// The first `\h` line's text: `/^\\h\s+(.+)$/m`, trimmed, as Sefer's
/// `titleOf` reads a rendered book.
fn h_title(usfm: &str) -> Option<&str> {
    usfm.split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .find_map(|line| {
            let rest = line.strip_prefix("\\h")?;
            let text = rest.trim_start_matches(is_js_space);
            match (rest.len() - text.len(), text.is_empty()) {
                (0, _) => None,
                (_, false) => Some(text),
                // All space: `\s+` gives back its last character to `(.+)`.
                (_, true) => rest.chars().count().gt(&1).then_some(""),
            }
        })
        .map(js_trim)
}

/// A book's files as a JavaScript `Map` holds them: a new path goes last, a
/// changed one keeps its place, a deleted one comes back last.
#[derive(Default)]
struct Files {
    slots: Vec<Option<(String, String)>>,
    at: FxHashMap<String, usize>,
}

impl Files {
    fn get(&self, path: &str) -> Option<&str> {
        let slot = self.slots[*self.at.get(path)?].as_ref()?;
        Some(&slot.1)
    }

    fn set(&mut self, path: &str, text: &str) {
        match self.at.get(path) {
            Some(&i) => {
                if let Some(slot) = &mut self.slots[i] {
                    text.clone_into(&mut slot.1);
                }
            }
            None => {
                self.at.insert(path.to_owned(), self.slots.len());
                self.slots.push(Some((path.to_owned(), text.to_owned())));
            }
        }
    }

    fn delete(&mut self, path: &str) {
        if let Some(i) = self.at.remove(path) {
            self.slots[i] = None;
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.slots
            .iter()
            .flatten()
            .map(|(p, t)| (p.as_str(), t.as_str()))
    }
}

impl WriterHistory {
    /// Each save's times, estimated where the device's clock can't be right
    /// (see [`crate::clock`]); `now` is Unix seconds.
    pub fn clocks(&self, now: i64) -> Vec<Clock> {
        clocks(&self.saves, now.saturating_add(CLOCK_SLACK))
    }

    /// Verses present after each save, as `(author time, verses)`, oldest
    /// first: raw verse-marker counts, a bridge `1-3` counted as three.
    /// Scaling against a versification is the caller's.
    pub fn progress(&self, now: i64) -> Vec<(i64, u64)> {
        let mut per_chunk: FxHashMap<&str, u64> = FxHashMap::default();
        let mut total: u64 = 0;
        self.saves
            .iter()
            .zip(self.clocks(now))
            .map(|(save, clock)| {
                for (path, text) in &save.changes {
                    let now = text.as_deref().map_or(0, verse_count);
                    let was = per_chunk.insert(path, now).unwrap_or(0);
                    total = total - was + now;
                }
                (clock.author_at, total)
            })
            .collect()
    }

    /// The book's title at its last save (its `\h`), else canon's English name.
    pub fn title(&self) -> String {
        let usfm = self.usfm();
        h_title(&usfm)
            .unwrap_or(fallback_title(self.project.book))
            .to_owned()
    }

    /// The book at its last save, as replay renders it.
    pub fn usfm(&self) -> String {
        let mut files = Files::default();
        for save in &self.saves {
            for (path, text) in &save.changes {
                match text {
                    Some(text) => files.set(path, text),
                    None => files.delete(path),
                }
            }
        }
        let book = self.project.book;
        book_to_usfm(&tree_from_files(files.iter()), book, fallback_title(book))
    }
}

#[cfg(test)]
mod tests;
