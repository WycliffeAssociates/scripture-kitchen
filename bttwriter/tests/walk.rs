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

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use bttwriter::pack::{Kind, ObjectId};
use bttwriter::walk::{ObjectSource, WriterHistory, read_history};

const REPOS: &[&str] = &[
    "kpass_tmf_3jn_text_reg",
    "bahasatech.indotimur_kps-x-kaigar_3jn_text_reg",
    "chunga_major_bem-x-ngumbo_2ch_text_reg",
    "andriatiana_wa_msh_job_text_udb",
    "bahasatech.indotimur_saw_isa_text_reg",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(dir)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn clone(repo: &str) -> PathBuf {
    let bundle = fixtures().join("repos").join(format!("{repo}.bundle"));
    assert!(bundle.is_file(), "missing {}", bundle.display());
    let dir = std::env::temp_dir().join(format!("bttwriter-walk-{}-{repo}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let status = Command::new("git")
        .args(["clone", "--bare", "--quiet"])
        .arg(&bundle)
        .arg(&dir)
        .status()
        .expect("git runs");
    assert!(status.success(), "clone {repo}");
    dir
}

/// `git cat-file --batch`, kept open: one request per object.
struct CliSource {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl CliSource {
    fn new(dir: &Path) -> CliSource {
        let mut child = Command::new("git")
            .arg("--git-dir")
            .arg(dir)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("git runs");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        CliSource {
            child,
            stdin,
            stdout,
        }
    }
}

impl Drop for CliSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ObjectSource for CliSource {
    type Error = String;

    fn read(&mut self, id: &ObjectId) -> Result<(Kind, Vec<u8>), String> {
        writeln!(self.stdin, "{id}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        let mut header = String::new();
        self.stdout
            .read_line(&mut header)
            .map_err(|e| e.to_string())?;
        let parts: Vec<&str> = header.split_whitespace().collect();
        let [_, kind, size] = parts[..] else {
            return Err(format!("{id}: {header}"));
        };
        let kind = match kind {
            "commit" => Kind::Commit,
            "tree" => Kind::Tree,
            "blob" => Kind::Blob,
            other => return Err(format!("{id} is a {other}")),
        };
        let size: usize = size.parse().map_err(|_| header.clone())?;
        let mut body = vec![0; size + 1];
        self.stdout
            .read_exact(&mut body)
            .map_err(|e| e.to_string())?;
        body.pop();
        Ok((kind, body))
    }
}

fn head(dir: &Path) -> ObjectId {
    ObjectId::from_hex(git(dir, &["rev-parse", "HEAD"]).trim()).unwrap()
}

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

fn walk(dir: &Path, repo: &str) -> WriterHistory {
    let source = repo.replacen('_', "/", 1);
    read_history(&mut CliSource::new(dir), head(dir), &[], &source)
        .unwrap_or_else(|e| panic!("{repo}: {e}"))
}

#[test]
fn five_histories_rebuild_head_and_agree_with_git() {
    for repo in REPOS {
        let dir = clone(repo);
        let history = walk(&dir, repo);

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
            let via_git2 = read_history(&mut source, head(&dir), &[], &history.source).unwrap();
            assert_eq!(
                via_git2, history,
                "{repo}: libgit2 and the git command disagree"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
