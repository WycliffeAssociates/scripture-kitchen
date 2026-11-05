//! A Writer repo's git objects → its saves, oldest first.
//!
//! ```text
//! let mut walk = HistoryWalk::new(head, &shallow);
//! while !walk.wants().is_empty() {
//!     let objects = fetch(walk.wants());          // the caller's git: isomorphic-git, libgit2, `git`
//!     walk.give(objects)?;
//! }
//! let history = walk.finish("owner/repo")?;       // WriterHistory { project, saves: [Save { changes, … }] }
//! ```
//!
//! The walk does no I/O. It says which objects it wants and is given them, so a
//! caller whose git reads asynchronously (isomorphic-git in a browser) can await
//! between rounds. [`read_history`] is the loop for a caller with a synchronous
//! [`ObjectSource`].
//!
//! Which saves: HEAD's first-parent line, each commit against its first parent.
//! A side branch is never replayed commit by commit; a merge brings what it
//! changed to the line. (Replaying every commit applied a side branch's
//! deletions to the one text being rebuilt, and a merge that kept the line's
//! files never put them back.) A commit listed as shallow is a root. Only the
//! book's text files count ([`is_text_path`]); a save that changed none of them
//! is no save. A file that won't decode reads as deleted, as the template
//! leaves out a file it can't read.
//!
//! Rounds: one per commit on the line (each names the next), one per tree
//! depth, and one for every changed file at once.

use rustc_hash::{FxHashMap, FxHashSet};

use bento::bttwriter::WriterProject;

use crate::pack::{Kind, ObjectId, Signature};
use crate::stitch::{decode_text, is_text_path, js_trim};

/// One save that changed the book's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Save {
    pub id: ObjectId,
    pub author: Signature,
    pub committer: Signature,
    /// The message's first line, trimmed: a timestamp, in practice.
    pub message: String,
    /// The text files it changed, with their new text (`None`: deleted, or not
    /// text), in the order Sefer's walk lists them.
    pub changes: Vec<(String, Option<String>)>,
}

/// A Writer book project's history, oldest save first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterHistory {
    /// `owner/repo`, or the folder's name: where `Writer-Source` says a save came from.
    pub source: String,
    /// From `manifest.json` at HEAD.
    pub project: WriterProject,
    /// The manifest's `project.name` (`3 John`), when it has one.
    pub project_name: Option<String>,
    pub saves: Vec<Save>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkError {
    /// An object that isn't the kind it should be, or won't parse as one.
    Malformed { id: ObjectId, why: String },
    /// `finish` was called before every wanted object was given.
    Unfinished,
    /// HEAD has no `manifest.json`, or it doesn't name a book, language and resource.
    NotAProject(String),
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalkError::Malformed { id, why } => write!(f, "object {id}: {why}"),
            WalkError::Unfinished => write!(f, "the walk still wants objects"),
            WalkError::NotAProject(why) => write!(f, "not a Writer book project: {why}"),
        }
    }
}

impl std::error::Error for WalkError {}

/// What a commit says, as far as the walk needs.
struct Commit {
    id: ObjectId,
    tree: ObjectId,
    first_parent: Option<ObjectId>,
    author: Signature,
    committer: Signature,
    message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Tree,
    File,
    /// A submodule: neither a file nor a folder of the project.
    Gitlink,
}

struct TreeEntry {
    name: String,
    /// As written (`100644`, `40000`): a change of mode alone is a change.
    mode: Vec<u8>,
    kind: EntryKind,
    id: ObjectId,
}

/// A changed path, with its place in the order Sefer's walk lists a save's
/// changes: depth first, each folder's names as the union of the old tree's
/// (in tree order) and then the new tree's, and a folder's contents before a
/// file of the same name. Replay holds files in that insertion order, as
/// Sefer's `Map` does, and the template's "the later title file wins" reads it.
struct Change {
    order: Vec<u32>,
    path: String,
    blob: Option<ObjectId>,
}

/// Two trees to compare, at `prefix`, for the save at `save`.
struct Pair {
    save: usize,
    /// Where this pair's changes fall in the save's list: see [`Change`].
    order: Vec<u32>,
    before: Option<ObjectId>,
    after: Option<ObjectId>,
    prefix: String,
}

enum Phase {
    /// Walking the first-parent line back from HEAD.
    Commits {
        next: ObjectId,
    },
    /// Comparing each commit's tree with its first parent's.
    Trees,
    /// Reading the changed files.
    Blobs,
    Done,
}

/// The walk, as a state machine: [`wants`](Self::wants), [`give`](Self::give),
/// until it wants nothing, then [`finish`](Self::finish).
pub struct HistoryWalk {
    phase: Phase,
    shallow: FxHashSet<ObjectId>,
    /// Newest first while walking; reversed when the line ends.
    line: Vec<Commit>,
    trees: FxHashMap<ObjectId, Vec<TreeEntry>>,
    blobs: FxHashMap<ObjectId, Vec<u8>>,
    pending: Vec<Pair>,
    /// Per save on the line: the paths it changed and their new blob.
    changes: Vec<Vec<Change>>,
    manifest: Option<ObjectId>,
    wants: Vec<ObjectId>,
}

impl HistoryWalk {
    pub fn new(head: ObjectId, shallow: &[ObjectId]) -> HistoryWalk {
        HistoryWalk {
            phase: Phase::Commits { next: head },
            shallow: shallow.iter().copied().collect(),
            line: Vec::new(),
            trees: FxHashMap::default(),
            blobs: FxHashMap::default(),
            pending: Vec::new(),
            changes: Vec::new(),
            manifest: None,
            wants: vec![head],
        }
    }

    /// The objects to fetch next. Empty when the walk is done.
    pub fn wants(&self) -> &[ObjectId] {
        &self.wants
    }

    /// Hand over fetched objects, as `(id, kind, bytes)`. Objects it didn't
    /// ask for are ignored; ones it asked for and wasn't given stay wanted.
    pub fn give(
        &mut self,
        objects: impl IntoIterator<Item = (ObjectId, Kind, Vec<u8>)>,
    ) -> Result<(), WalkError> {
        let wanted: FxHashSet<ObjectId> = self.wants.iter().copied().collect();
        for (id, kind, bytes) in objects {
            if !wanted.contains(&id) {
                continue;
            }
            match (&self.phase, kind) {
                (Phase::Commits { next }, Kind::Commit) if *next == id => {
                    let commit = parse_commit(id, &bytes)?;
                    let parent = commit.first_parent.filter(|_| !self.shallow.contains(&id));
                    self.line.push(commit);
                    match parent {
                        Some(parent) => self.phase = Phase::Commits { next: parent },
                        None => self.start_trees(),
                    }
                }
                (Phase::Trees, Kind::Tree) => {
                    let entries = parse_tree(id, &bytes)?;
                    self.trees.insert(id, entries);
                }
                (Phase::Blobs, Kind::Blob) => {
                    self.blobs.insert(id, bytes);
                }
                _ => {
                    return Err(WalkError::Malformed {
                        id,
                        why: format!("a {kind:?} where the walk wanted something else"),
                    });
                }
            }
        }
        self.advance();
        Ok(())
    }

    /// The history, once the walk wants nothing more.
    pub fn finish(self, source: &str) -> Result<WriterHistory, WalkError> {
        if !matches!(self.phase, Phase::Done) {
            return Err(WalkError::Unfinished);
        }
        let manifest = self
            .manifest
            .and_then(|id| self.blobs.get(&id))
            .ok_or_else(|| WalkError::NotAProject("no manifest.json at HEAD".into()))?;
        let manifest = bento::bttwriter::read(manifest)
            .map_err(|e| WalkError::NotAProject(format!("manifest.json: {e}")))?;
        let repo = source.rsplit('/').next();
        let project = manifest.project(repo).ok_or_else(|| {
            WalkError::NotAProject("manifest.json names no book, language and resource".into())
        })?;
        let project_name = manifest.project.and_then(|p| p.name);

        let mut saves = Vec::new();
        for (commit, mut changed) in self.line.into_iter().zip(self.changes) {
            if changed.is_empty() {
                continue;
            }
            changed.sort_by(|a, b| a.order.cmp(&b.order));
            let changes = changed
                .into_iter()
                .map(|Change { path, blob, .. }| {
                    let text = blob
                        .and_then(|id| self.blobs.get(&id))
                        .and_then(|bytes| decode_text(bytes));
                    (path, text)
                })
                .collect();
            saves.push(Save {
                id: commit.id,
                author: commit.author,
                committer: commit.committer,
                message: commit.message,
                changes,
            });
        }
        Ok(WriterHistory {
            source: source.to_owned(),
            project,
            project_name,
            saves,
        })
    }

    /// The line is complete: oldest first, and compare each commit's tree
    /// with its first parent's (the one before it on the line).
    fn start_trees(&mut self) {
        self.line.reverse();
        self.changes = self.line.iter().map(|_| Vec::new()).collect();
        let mut before = None;
        for (save, commit) in self.line.iter().enumerate() {
            self.pending.push(Pair {
                save,
                order: Vec::new(),
                before,
                after: Some(commit.tree),
                prefix: String::new(),
            });
            before = Some(commit.tree);
        }
        self.phase = Phase::Trees;
    }

    /// Do everything the objects in hand allow, then say what's wanted next.
    fn advance(&mut self) {
        if let Phase::Trees = self.phase {
            loop {
                let ready: Vec<Pair>;
                (ready, self.pending) = std::mem::take(&mut self.pending)
                    .into_iter()
                    .partition(|pair| self.has_tree(pair.before) && self.has_tree(pair.after));
                if ready.is_empty() {
                    break;
                }
                for pair in ready {
                    self.compare(pair);
                }
            }
            if self.pending.is_empty() {
                self.phase = Phase::Blobs;
            }
        }
        let wants: Vec<ObjectId> = match &self.phase {
            Phase::Commits { next } => vec![*next],
            Phase::Trees => {
                let mut seen = FxHashSet::default();
                self.pending
                    .iter()
                    .flat_map(|pair| [pair.before, pair.after])
                    .flatten()
                    .filter(|id| !self.trees.contains_key(id) && seen.insert(*id))
                    .collect()
            }
            Phase::Blobs => {
                let mut seen = FxHashSet::default();
                self.changes
                    .iter()
                    .flatten()
                    .filter_map(|change| change.blob)
                    .chain(self.manifest)
                    .filter(|id| !self.blobs.contains_key(id) && seen.insert(*id))
                    .collect()
            }
            Phase::Done => Vec::new(),
        };
        if matches!(self.phase, Phase::Blobs) && wants.is_empty() {
            self.phase = Phase::Done;
        }
        self.wants = wants;
    }

    fn has_tree(&self, id: Option<ObjectId>) -> bool {
        id.is_none_or(|id| self.trees.contains_key(&id))
    }

    /// One level of a tree diff: a changed folder is compared one level
    /// deeper, a changed file is recorded if it's the book's text. A name that
    /// was a file and is now a folder (or the reverse) is both.
    fn compare(&mut self, pair: Pair) {
        let empty = Vec::new();
        let before = pair.before.map_or(&empty, |id| &self.trees[&id]);
        let after = pair.after.map_or(&empty, |id| &self.trees[&id]);
        let find = |entries: &'_ [TreeEntry], name: &str| -> Option<(EntryKind, ObjectId)> {
            entries
                .iter()
                .find(|e| e.name == name)
                .map(|e| (e.kind, e.id))
        };
        let same = |name: &str| {
            let was = before.iter().find(|e| e.name == name);
            let now = after.iter().find(|e| e.name == name);
            match (was, now) {
                (Some(a), Some(b)) => a.id == b.id && a.mode == b.mode,
                _ => false,
            }
        };
        let mut names: Vec<&str> = before.iter().map(|e| e.name.as_str()).collect();
        for entry in after {
            if !names.contains(&entry.name.as_str()) {
                names.push(&entry.name);
            }
        }
        let mut deeper = Vec::new();
        let mut changed = Vec::new();
        for (index, name) in (0u32..).zip(names) {
            if same(name) {
                continue;
            }
            let was = find(before, name);
            let now = find(after, name);
            let path = format!("{}{name}", pair.prefix);
            let tree_of = |side: Option<(EntryKind, ObjectId)>| {
                side.filter(|(kind, _)| *kind == EntryKind::Tree)
                    .map(|(_, id)| id)
            };
            let file_of = |side: Option<(EntryKind, ObjectId)>| {
                side.filter(|(kind, _)| *kind == EntryKind::File)
                    .map(|(_, id)| id)
            };
            if tree_of(was).is_some() || tree_of(now).is_some() {
                deeper.push(Pair {
                    save: pair.save,
                    order: [pair.order.as_slice(), &[index, 0]].concat(),
                    before: tree_of(was),
                    after: tree_of(now),
                    prefix: format!("{path}/"),
                });
            }
            let blob = match (file_of(was), file_of(now)) {
                (_, Some(blob)) => Some(blob),
                (Some(_), None) => None,
                (None, None) => continue,
            };
            if is_text_path(&path) {
                changed.push(Change {
                    order: [pair.order.as_slice(), &[index, 1]].concat(),
                    path,
                    blob,
                });
            }
        }
        if pair.prefix.is_empty() && pair.save + 1 == self.line.len() {
            self.manifest = find(after, "manifest.json")
                .filter(|(kind, _)| *kind == EntryKind::File)
                .map(|(_, id)| id);
        }
        self.changes[pair.save].extend(changed);
        self.pending.extend(deeper);
    }
}

/// A synchronous git: what [`read_history`] drives the walk with.
pub trait ObjectSource {
    type Error;
    fn read(&mut self, id: &ObjectId) -> Result<(Kind, Vec<u8>), Self::Error>;
}

#[derive(Debug)]
pub enum ReadHistoryError<E> {
    Source(E),
    Walk(WalkError),
}

impl<E: std::fmt::Display> std::fmt::Display for ReadHistoryError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadHistoryError::Source(e) => write!(f, "reading the repo: {e}"),
            ReadHistoryError::Walk(e) => e.fmt(f),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for ReadHistoryError<E> {}

/// The walk, driven to the end over a synchronous source.
pub fn read_history<S: ObjectSource>(
    objects: &mut S,
    head: ObjectId,
    shallow: &[ObjectId],
    source: &str,
) -> Result<WriterHistory, ReadHistoryError<S::Error>> {
    let mut walk = HistoryWalk::new(head, shallow);
    while !walk.wants().is_empty() {
        let fetched = walk
            .wants()
            .iter()
            .map(|id| objects.read(id).map(|(kind, bytes)| (*id, kind, bytes)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(ReadHistoryError::Source)?;
        walk.give(fetched).map_err(ReadHistoryError::Walk)?;
    }
    walk.finish(source).map_err(ReadHistoryError::Walk)
}

fn malformed(id: ObjectId, why: &str) -> WalkError {
    WalkError::Malformed {
        id,
        why: why.to_owned(),
    }
}

/// `tree <id>`, `parent <id>`…, `author …`, `committer …`, other headers
/// (with their continuation lines), a blank line, the message.
fn parse_commit(id: ObjectId, bytes: &[u8]) -> Result<Commit, WalkError> {
    let (head, message) = match bytes.windows(2).position(|w| w == b"\n\n") {
        Some(at) => (&bytes[..at], &bytes[at + 2..]),
        None => (bytes, &b""[..]),
    };
    let head = String::from_utf8_lossy(head);
    let mut tree = None;
    let mut first_parent = None;
    let mut author = None;
    let mut committer = None;
    for line in head.lines() {
        if let Some(hex) = line.strip_prefix("tree ") {
            tree = ObjectId::from_hex(hex.trim());
        } else if let Some(hex) = line.strip_prefix("parent ") {
            if first_parent.is_none() {
                first_parent = ObjectId::from_hex(hex.trim());
            }
        } else if let Some(sig) = line.strip_prefix("author ") {
            author = parse_signature(sig);
        } else if let Some(sig) = line.strip_prefix("committer ") {
            committer = parse_signature(sig);
        }
    }
    let message = String::from_utf8_lossy(message);
    Ok(Commit {
        id,
        tree: tree.ok_or_else(|| malformed(id, "a commit with no tree"))?,
        first_parent,
        author: author.ok_or_else(|| malformed(id, "a commit with no author"))?,
        committer: committer.ok_or_else(|| malformed(id, "a commit with no committer"))?,
        // As Sefer reads it: `message.trim().split("\n")[0]`.
        message: js_trim(&message)
            .split('\n')
            .next()
            .unwrap_or("")
            .to_owned(),
    })
}

/// `Name <email> 1700000000 +0200`. The name may hold anything; the email is
/// the last `<…>`.
fn parse_signature(text: &str) -> Option<Signature> {
    let open = text.rfind('<')?;
    let close = open + text[open..].find('>')?;
    let name = text[..open].trim().to_owned();
    let email = text[open + 1..close].to_owned();
    let mut rest = text[close + 1..].split_whitespace();
    let at: i64 = rest.next()?.parse().ok()?;
    let zone = rest.next().unwrap_or("+0000");
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let digits = zone.trim_start_matches(['+', '-']);
    let offset_minutes = if digits.len() == 4 {
        let hours: i32 = digits[..2].parse().ok()?;
        let minutes: i32 = digits[2..].parse().ok()?;
        sign * (hours * 60 + minutes)
    } else {
        0
    };
    Some(Signature {
        name,
        email,
        at,
        offset_minutes,
    })
}

/// `<mode> <name>\0<20-byte id>`, repeated.
fn parse_tree(id: ObjectId, bytes: &[u8]) -> Result<Vec<TreeEntry>, WalkError> {
    let mut entries = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let space = rest
            .iter()
            .position(|&b| b == b' ')
            .ok_or_else(|| malformed(id, "a tree entry with no mode"))?;
        let nul = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| malformed(id, "a tree entry with no name"))?;
        if nul < space || rest.len() < nul + 21 {
            return Err(malformed(id, "a truncated tree entry"));
        }
        let mode = &rest[..space];
        let name = String::from_utf8_lossy(&rest[space + 1..nul]).into_owned();
        let mut oid = [0u8; 20];
        oid.copy_from_slice(&rest[nul + 1..nul + 21]);
        let kind = match mode {
            b"40000" | b"040000" => EntryKind::Tree,
            b"160000" => EntryKind::Gitlink,
            _ => EntryKind::File,
        };
        entries.push(TreeEntry {
            name,
            mode: mode.to_vec(),
            kind,
            id: ObjectId(oid),
        });
        rest = &rest[nul + 21..];
    }
    Ok(entries)
}

#[cfg(test)]
mod tests;
