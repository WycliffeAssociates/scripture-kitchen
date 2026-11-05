//! What the integration tests share: the five fixture repos, cloned with
//! `git` and read through `git cat-file --batch`.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use bttwriter::pack::{Kind, ObjectId};
use bttwriter::walk::{ObjectSource, WriterHistory, read_history};

pub const REPOS: &[&str] = &[
    "kpass_tmf_3jn_text_reg",
    "bahasatech.indotimur_kps-x-kaigar_3jn_text_reg",
    "chunga_major_bem-x-ngumbo_2ch_text_reg",
    "andriatiana_wa_msh_job_text_udb",
    "bahasatech.indotimur_saw_isa_text_reg",
];

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub fn git(dir: &Path, args: &[&str]) -> String {
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

pub fn clone(repo: &str) -> PathBuf {
    let bundle = fixtures().join("repos").join(format!("{repo}.bundle"));
    assert!(bundle.is_file(), "missing {}", bundle.display());
    // Unique per call: `cargo test` runs a binary's tests as threads of one
    // process, and two of them may clone the same repo at once.
    static CLONES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CLONES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("bttwriter-walk-{}-{n}-{repo}", std::process::id()));
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
pub struct CliSource {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl CliSource {
    pub fn new(dir: &Path) -> CliSource {
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

pub fn head(dir: &Path) -> ObjectId {
    ObjectId::from_hex(git(dir, &["rev-parse", "HEAD"]).trim()).unwrap()
}

/// `owner/repo` for a fixture named `owner_repo`. Two owners hold an underscore.
pub fn source_of(repo: &str) -> String {
    let owner = ["chunga_major", "andriatiana_wa"]
        .into_iter()
        .find(|o| repo.starts_with(&format!("{o}_")))
        .unwrap_or_else(|| repo.split('_').next().unwrap());
    format!("{owner}/{}", &repo[owner.len() + 1..])
}

/// The fixture's history, read through `git cat-file`.
pub fn history_of(dir: &Path, repo: &str) -> WriterHistory {
    read_history(&mut CliSource::new(dir), head(dir), &[], &source_of(repo))
        .unwrap_or_else(|e| panic!("{repo}: {e}"))
}
