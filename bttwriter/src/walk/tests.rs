use std::collections::BTreeMap;

use super::*;
use crate::pack::{CommitFields, commit_body};

const MANIFEST: &str = r#"{"target_language": {"id": "bem"}, "project": {"id": "lev", "name": "Leviticus"}, "resource": {"id": "reg"}}"#;

/// An in-memory git: objects by id.
#[derive(Default)]
struct Repo {
    objects: FxHashMap<ObjectId, (Kind, Vec<u8>)>,
    at: i64,
}

#[derive(Debug)]
struct Missing;

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no such object")
    }
}

impl ObjectSource for Repo {
    type Error = Missing;
    fn read(&mut self, id: &ObjectId) -> Result<(Kind, Vec<u8>), Missing> {
        self.objects.get(id).cloned().ok_or(Missing)
    }
}

impl Repo {
    fn put(&mut self, kind: Kind, body: Vec<u8>) -> ObjectId {
        let id = ObjectId::of(kind, &body);
        self.objects.insert(id, (kind, body));
        id
    }

    /// A tree from `path → bytes`, folders included.
    fn tree(&mut self, files: &[(&str, &[u8])]) -> ObjectId {
        let mut here: BTreeMap<String, (&str, ObjectId)> = BTreeMap::new();
        let mut folders: BTreeMap<String, Vec<(&str, &[u8])>> = BTreeMap::new();
        for (path, bytes) in files {
            match path.split_once('/') {
                Some((folder, rest)) => folders
                    .entry(folder.to_string())
                    .or_default()
                    .push((rest, bytes)),
                None => {
                    let id = self.put(Kind::Blob, bytes.to_vec());
                    here.insert(path.to_string(), ("100644", id));
                }
            }
        }
        for (folder, inside) in folders {
            let id = self.tree(&inside);
            here.insert(folder, ("40000", id));
        }
        // Git sorts a folder as if its name ended in `/`.
        let mut entries: Vec<(String, &str, ObjectId)> = here
            .into_iter()
            .map(|(name, (mode, id))| (name, mode, id))
            .collect();
        entries.sort_by_key(|(name, mode, _)| {
            if *mode == "40000" {
                format!("{name}/")
            } else {
                name.clone()
            }
        });
        let mut body = Vec::new();
        for (name, mode, id) in entries {
            body.extend_from_slice(format!("{mode} {name}\0").as_bytes());
            body.extend_from_slice(&id.0);
        }
        self.put(Kind::Tree, body)
    }

    fn commit(&mut self, files: &[(&str, &[u8])], parents: &[ObjectId], message: &str) -> ObjectId {
        let tree = self.tree(files);
        self.at += 60;
        let who = Signature {
            name: "tsDesktop".into(),
            email: "t@example.org".into(),
            at: self.at,
            offset_minutes: 120,
        };
        let body = commit_body(&CommitFields {
            tree,
            parents,
            author: &who,
            committer: &who,
            message,
        });
        self.put(Kind::Commit, body)
    }
}

fn walk(repo: &mut Repo, head: ObjectId, shallow: &[ObjectId]) -> WriterHistory {
    read_history(repo, head, shallow, "chunga_major/bem_lev_text_reg").unwrap()
}

/// Every save's changes applied in order: the files at the end.
fn replayed(history: &WriterHistory) -> BTreeMap<String, Option<String>> {
    let mut files = BTreeMap::new();
    for save in &history.saves {
        for (path, text) in &save.changes {
            files.insert(path.clone(), text.clone());
        }
    }
    files
}

// Why: Bemba Leviticus had a side branch that deleted chapter 15's first
// chunk, merged back keeping the line's files. Replaying every commit lost
// 15:1–5; the first-parent line, each merge as what it brought, must end
// where HEAD is.
#[test]
fn a_merged_side_branch_does_not_lose_chunks() {
    let mut repo = Repo::default();
    let m = MANIFEST.as_bytes();
    let base = repo.commit(
        &[
            ("manifest.json", m),
            ("15/01.txt", b"\\c 15 \\v 1 one"),
            ("15/06.txt", b"\\v 6 six"),
        ],
        &[],
        "base\n",
    );
    let side = repo.commit(
        &[("manifest.json", m), ("15/06.txt", b"\\v 6 six")],
        &[base],
        "side\n",
    );
    let line = repo.commit(
        &[
            ("manifest.json", m),
            ("15/01.txt", b"\\c 15 \\v 1 one"),
            ("15/06.txt", b"\\v 6 six, edited"),
        ],
        &[base],
        "line\n",
    );
    let merge = repo.commit(
        &[
            ("manifest.json", m),
            ("15/01.txt", b"\\c 15 \\v 1 one"),
            ("15/06.txt", b"\\v 6 six, edited"),
        ],
        &[line, side],
        "merge\n",
    );
    let history = walk(&mut repo, merge, &[]);
    let files = replayed(&history);
    assert_eq!(files["15/01.txt"].as_deref(), Some("\\c 15 \\v 1 one"));
    assert_eq!(files["15/06.txt"].as_deref(), Some("\\v 6 six, edited"));
    // base and line changed text; the merge brought nothing to the line.
    assert_eq!(
        history
            .saves
            .iter()
            .map(|s| s.message.as_str())
            .collect::<Vec<_>>(),
        ["base", "line"]
    );
    assert_eq!(history.project.book.to_string(), "LEV");
    assert_eq!(history.project_name.as_deref(), Some("Leviticus"));
    assert_eq!(history.source, "chunga_major/bem_lev_text_reg");
}

#[test]
fn saves_are_oldest_first_with_only_text_changes() {
    let mut repo = Repo::default();
    let m = MANIFEST.as_bytes();
    let a = repo.commit(
        &[("manifest.json", m), ("01/01.txt", b"\\c 1 \\v 1 a")],
        &[],
        "2019-01-01 first\nmore\n",
    );
    let b = repo.commit(
        &[
            ("manifest.json", m),
            ("01/01.txt", b"\\c 1 \\v 1 a"),
            ("LICENSE.md", b"x"),
        ],
        &[a],
        "license only\n",
    );
    let c = repo.commit(
        &[
            ("manifest.json", m),
            ("01/01.txt", b"\\c 1 \\v 1 b"),
            ("01/title.txt", b"Sura 1"),
            ("LICENSE.md", b"x"),
        ],
        &[b],
        "edit\n",
    );
    let history = walk(&mut repo, c, &[]);
    let summary: Vec<(&str, Vec<&str>)> = history
        .saves
        .iter()
        .map(|s| {
            (
                s.message.as_str(),
                s.changes.iter().map(|(p, _)| p.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("2019-01-01 first", vec!["01/01.txt"]),
            ("edit", vec!["01/01.txt", "01/title.txt"])
        ]
    );
    assert_eq!(history.saves[0].author.offset_minutes, 120);
    assert!(history.saves[0].author.at < history.saves[1].author.at);
}

#[test]
fn deletions_and_undecodable_files_read_as_none() {
    let mut repo = Repo::default();
    let m = MANIFEST.as_bytes();
    let a = repo.commit(
        &[
            ("manifest.json", m),
            ("01/01.txt", b"\\v 1"),
            ("01/02.txt", b"\\v 2"),
        ],
        &[],
        "a\n",
    );
    let b = repo.commit(
        &[
            ("manifest.json", m),
            ("01/01.txt", b"\xFF\xFE"),
            ("02/01.txt", b"\\c 2"),
        ],
        &[a],
        "b\n",
    );
    let history = walk(&mut repo, b, &[]);
    let files = replayed(&history);
    assert_eq!(files["01/01.txt"], None);
    assert_eq!(files["01/02.txt"], None);
    assert_eq!(files["02/01.txt"].as_deref(), Some("\\c 2"));
}

#[test]
fn a_file_that_becomes_a_folder_is_deleted_and_its_contents_added() {
    let mut repo = Repo::default();
    let m = MANIFEST.as_bytes();
    let a = repo.commit(
        &[
            ("manifest.json", m),
            ("01", b"oops"),
            ("02/01.txt", b"\\c 2"),
        ],
        &[],
        "a\n",
    );
    let b = repo.commit(
        &[
            ("manifest.json", m),
            ("01/01.txt", b"\\c 1"),
            ("02/01.txt", b"\\c 2"),
        ],
        &[a],
        "b\n",
    );
    let history = walk(&mut repo, b, &[]);
    // `01` isn't a text path, so only the new chunk shows.
    assert_eq!(
        history.saves[1].changes,
        [("01/01.txt".to_string(), Some("\\c 1".to_string()))]
    );
}

#[test]
fn a_shallow_commit_is_a_root() {
    let mut repo = Repo::default();
    let m = MANIFEST.as_bytes();
    let a = repo.commit(&[("manifest.json", m), ("01/01.txt", b"one")], &[], "a\n");
    let b = repo.commit(&[("manifest.json", m), ("01/01.txt", b"two")], &[a], "b\n");
    // `a` isn't in the repo at all, as in a shallow clone.
    repo.objects.remove(&a);
    let history = walk(&mut repo, b, &[b]);
    assert_eq!(history.saves.len(), 1);
    assert_eq!(
        history.saves[0].changes,
        [("01/01.txt".to_string(), Some("two".to_string()))]
    );
}

#[test]
fn no_manifest_or_no_book_is_not_a_project() {
    let mut repo = Repo::default();
    let a = repo.commit(&[("01/01.txt", b"one")], &[], "a\n");
    let err = read_history(&mut repo, a, &[], "x/y").unwrap_err();
    assert!(
        matches!(err, ReadHistoryError::Walk(WalkError::NotAProject(_))),
        "{err}"
    );
    let b = repo.commit(
        &[
            ("manifest.json", br#"{"project": {"id": "obs"}}"#),
            ("01/01.txt", b"one"),
        ],
        &[],
        "b\n",
    );
    let err = read_history(&mut repo, b, &[], "x/y").unwrap_err();
    assert!(
        matches!(err, ReadHistoryError::Walk(WalkError::NotAProject(_))),
        "{err}"
    );
}

#[test]
fn a_missing_object_is_the_sources_error() {
    let mut repo = Repo::default();
    let a = repo.commit(
        &[
            ("manifest.json", MANIFEST.as_bytes()),
            ("01/01.txt", b"one"),
        ],
        &[],
        "a\n",
    );
    let b = repo.commit(
        &[
            ("manifest.json", MANIFEST.as_bytes()),
            ("01/01.txt", b"two"),
        ],
        &[a],
        "b\n",
    );
    repo.objects.remove(&a);
    assert!(matches!(
        read_history(&mut repo, b, &[], "x/y"),
        Err(ReadHistoryError::Source(Missing))
    ));
}

#[test]
fn the_machine_asks_in_rounds_and_ignores_extras() {
    let mut repo = Repo::default();
    let m = MANIFEST.as_bytes();
    let a = repo.commit(&[("manifest.json", m), ("01/01.txt", b"one")], &[], "a\n");
    let b = repo.commit(&[("manifest.json", m), ("01/01.txt", b"two")], &[a], "b\n");
    let mut walk = HistoryWalk::new(b, &[]);
    assert_eq!(walk.wants(), [b]);
    assert!(matches!(
        HistoryWalk::new(b, &[]).finish("x/y"),
        Err(WalkError::Unfinished)
    ));
    // Give everything at once: only what's wanted is taken, round by round.
    let everything: Vec<(ObjectId, Kind, Vec<u8>)> = repo
        .objects
        .iter()
        .map(|(id, (kind, bytes))| (*id, *kind, bytes.clone()))
        .collect();
    let mut rounds = 0;
    while !walk.wants().is_empty() {
        walk.give(everything.clone()).unwrap();
        rounds += 1;
    }
    // b, a, the root trees, the chapter trees, the blobs.
    assert_eq!(rounds, 5);
    assert_eq!(walk.finish("x/y").unwrap().saves.len(), 2);
}

#[test]
fn a_wrong_kind_is_malformed() {
    let mut repo = Repo::default();
    let a = repo.commit(&[("manifest.json", MANIFEST.as_bytes())], &[], "a\n");
    let mut walk = HistoryWalk::new(a, &[]);
    assert!(matches!(
        walk.give([(a, Kind::Blob, b"x".to_vec())]),
        Err(WalkError::Malformed { .. })
    ));
}

#[test]
fn signatures_parse_as_git_writes_them() {
    let sig = parse_signature("Mwila Chanda <mwila@example.org> 1782000000 -0530").unwrap();
    assert_eq!(
        (
            sig.name.as_str(),
            sig.email.as_str(),
            sig.at,
            sig.offset_minutes
        ),
        ("Mwila Chanda", "mwila@example.org", 1_782_000_000, -330)
    );
    // The email is the last `<…>`; an empty name is kept empty.
    let sig = parse_signature("a <b> c <d@e> 5 +0100").unwrap();
    assert_eq!(
        (sig.name.as_str(), sig.email.as_str(), sig.offset_minutes),
        ("a <b> c", "d@e", 60)
    );
    let sig = parse_signature(" <tsdesktop@x> 0 +0000").unwrap();
    assert_eq!(sig.name, "");
    assert_eq!(parse_signature("no email 5 +0000"), None);
    assert_eq!(parse_signature("a <b> notanumber +0000"), None);
    // A zone four bytes long but not four characters: no zone, not a panic.
    assert_eq!(
        parse_signature("A <a@b> 1700000000 +0\u{e9}0")
            .unwrap()
            .offset_minutes,
        0
    );
    // Round trip with the writer's Display.
    let written = Signature {
        name: "x".into(),
        email: "y@z".into(),
        at: 7,
        offset_minutes: -45,
    };
    assert_eq!(parse_signature(&written.to_string()), Some(written));
}
