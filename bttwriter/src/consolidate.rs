//! Writer histories → one consolidated project: the replayed line, and a
//! closing commit with its manifests.
//!
//! ```text
//! consolidate(&histories, &opts, &project, &[Burrito, Rc], &closing)
//!   s1 ─ t1 ─ s2 ─ … ─ s9 ─ C            C: every book file + metadata.json + manifest.yaml,
//!                                         by bento::create_bible from the books at their last save
//!   → Consolidated { replayed: Replayed { head: C, pack, idx, … }, manifests: [(name, bytes), …] }
//! ```
//!
//! Everything is in the one pack, so a caller writes the pack and its index,
//! points a branch at `head`, and has a project. The closing commit is the
//! caller's words (author, message): it's the one commit the migration itself
//! makes. [`replay()`](crate::replay()) is this without the closing commit.

use bento::{CreateError, Format, NewBibleProject, NewBook, create_bible};

use crate::pack::{CommitFields, Kind, PackWriter, Signature, commit_body, tree_body};
use crate::replay::{ReplayError, ReplayOptions, Replayed, line};
use crate::walk::WriterHistory;

/// Who makes the closing commit, and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closing {
    pub author: Signature,
    pub committer: Signature,
    /// As stored: end it with `\n`.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consolidated {
    /// `head` is the closing commit.
    pub replayed: Replayed,
    /// The manifests the closing commit holds, by file name, as written.
    pub manifests: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsolidateError {
    Replay(ReplayError),
    /// The manifests couldn't be written (a bad repository path or date).
    Manifest(CreateError),
}

impl std::fmt::Display for ConsolidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsolidateError::Replay(e) => write!(f, "{e}"),
            ConsolidateError::Manifest(e) => write!(f, "the manifests: {e}"),
        }
    }
}

impl std::error::Error for ConsolidateError {}

/// Replay `histories`, then commit the project's manifests on top.
///
/// `project` says everything about the project but its books (its `books`
/// are replaced): those are the replay's, each with its title (its `\h`) and
/// its text at its last save, so the Burrito's checksums are those files'.
pub fn consolidate(
    histories: &[WriterHistory],
    opts: &ReplayOptions,
    project: &NewBibleProject,
    formats: &[Format],
    closing: &Closing,
) -> Result<Consolidated, ConsolidateError> {
    let mut writer = PackWriter::new();
    let mut line = line(histories, opts, &mut writer).map_err(ConsolidateError::Replay)?;
    let project = NewBibleProject {
        books: line
            .books
            .iter()
            .map(|(file, usfm)| NewBook {
                book: file.book,
                title: file.title.clone(),
                bytes: usfm.clone().into_bytes(),
            })
            .collect(),
        ..project.clone()
    };
    let manifests = create_bible(&project, formats).map_err(ConsolidateError::Manifest)?;
    for (name, bytes) in &manifests {
        let blob = writer.add(Kind::Blob, bytes, None);
        line.tree.insert(name.clone(), blob);
    }
    let root = writer.add(Kind::Tree, &tree_body(&line.tree), None);
    let body = commit_body(&CommitFields {
        tree: root,
        parents: &[line.head],
        author: &closing.author,
        committer: &closing.committer,
        message: &closing.message,
    });
    let head = writer.add(Kind::Commit, &body, None);
    Ok(Consolidated {
        replayed: line.finish(writer, Some(head)),
        manifests,
    })
}
