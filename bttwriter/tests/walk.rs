//! The walk over five real BTT Writer projects: GOLDEN.
//!
//! ```text
//! fixtures/repos/<owner>_<repo>.bundle   cloned with `git`, read through `git cat-file --batch`
//!   every save applied in order          == the files at HEAD: stitched, == fixtures/stitched/
//!   each save's changed paths            == `git diff-tree` against its first parent, text paths only
//!   (--features git2) Git2Source         == the same history, field for field
//! ```
//!
//! Needs the `git` command; a missing one, or a missing fixture, fails.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use bttwriter::pack::ObjectId;
#[cfg(feature = "git2")]
use bttwriter::walk::read_history;

use common::{REPOS, clone, fixtures, git, history_of};

/// What git says each first-parent commit changed, text paths only, oldest
/// first: one `git log`, each commit against its first parent (merges too).
fn gits_saves(dir: &Path) -> Vec<(ObjectId, Vec<String>)> {
    let log = git(
        dir,
        &[
            "log",
            "--first-parent",
            "--diff-merges=first-parent",
            "--reverse",
            "--no-renames",
            "--name-only",
            "--format=%x01%H",
        ],
    );
    log.split('\u{1}')
        .filter(|entry| !entry.trim().is_empty())
        .filter_map(|entry| {
            let mut lines = entry.lines();
            let commit = ObjectId::from_hex(lines.next()?.trim()).unwrap();
            let mut paths: Vec<String> = lines
                .filter(|p| bttwriter::is_text_path(p))
                .map(str::to_string)
                .collect();
            paths.sort();
            paths.dedup();
            (!paths.is_empty()).then_some((commit, paths))
        })
        .collect()
}

#[test]
fn five_histories_rebuild_head_and_agree_with_git() {
    for repo in REPOS {
        let dir = clone(repo);
        let history = history_of(&dir, repo);

        // Every save, applied in order, is the book at HEAD.
        let mut files: BTreeMap<String, String> = BTreeMap::new();
        for save in &history.saves {
            for (path, text) in &save.changes {
                match text {
                    Some(text) => files.insert(path.clone(), text.clone()),
                    None => files.remove(path),
                };
            }
        }
        let tree = bttwriter::tree_from_files(files.iter().map(|(p, t)| (p.as_str(), t.as_str())));
        let book = history.project.book;
        let usfm = bttwriter::book_to_usfm(&tree, book, &book.to_string());
        let expected =
            std::fs::read_to_string(fixtures().join("stitched").join(format!("{repo}.usfm")))
                .unwrap();
        assert!(
            usfm == expected,
            "{repo}: the replayed saves don't rebuild HEAD"
        );

        // The same saves, changing the same files, as git's own diff says.
        let ours: Vec<(ObjectId, Vec<String>)> = history
            .saves
            .iter()
            .map(|save| {
                let mut paths: Vec<String> = save.changes.iter().map(|(p, _)| p.clone()).collect();
                paths.sort();
                (save.id, paths)
            })
            .collect();
        assert_eq!(ours, gits_saves(&dir), "{repo}");
        assert!(!history.saves.is_empty(), "{repo}");

        #[cfg(feature = "git2")]
        {
            let git2_repo = git2::Repository::open_bare(&dir).unwrap();
            let mut source = bttwriter::Git2Source::new(git2_repo.odb().unwrap());
            let via_git2 =
                read_history(&mut source, common::head(&dir), &[], &history.source).unwrap();
            assert_eq!(
                via_git2, history,
                "{repo}: libgit2 and the git command disagree"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
