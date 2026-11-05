//! The walk's source over libgit2, behind the `git2` feature.
//!
//! ```text
//! let repo = git2::Repository::open_bare(path)?;
//! let head = repo.head()?.peel_to_commit()?.id();
//! read_history(&mut Git2Source::new(repo.odb()?), head.into(), &[], "owner/repo")
//! ```

use crate::pack::{Kind, ObjectId};
use crate::walk::ObjectSource;

pub struct Git2Source<'repo> {
    odb: git2::Odb<'repo>,
}

impl<'repo> Git2Source<'repo> {
    pub fn new(odb: git2::Odb<'repo>) -> Self {
        Git2Source { odb }
    }
}

impl From<git2::Oid> for ObjectId {
    fn from(oid: git2::Oid) -> Self {
        let mut bytes = [0u8; 20];
        bytes.copy_from_slice(oid.as_bytes());
        ObjectId(bytes)
    }
}

impl ObjectSource for Git2Source<'_> {
    type Error = git2::Error;

    fn read(&mut self, id: &ObjectId) -> Result<(Kind, Vec<u8>), git2::Error> {
        let object = self.odb.read(git2::Oid::from_bytes(&id.0)?)?;
        let kind = match object.kind() {
            git2::ObjectType::Commit => Kind::Commit,
            git2::ObjectType::Tree => Kind::Tree,
            git2::ObjectType::Blob => Kind::Blob,
            other => {
                return Err(git2::Error::from_str(&format!(
                    "{id} is a {other}, not a commit, tree or blob"
                )));
            }
        };
        Ok((kind, object.data().to_vec()))
    }
}
