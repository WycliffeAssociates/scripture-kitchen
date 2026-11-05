//! Five real BTT Writer projects, stitched: GOLDEN.
//!
//! ```text
//! fixtures/repos/<owner>_<repo>.bundle      the project's git history, as `git bundle` writes it
//! fixtures/stitched/<owner>_<repo>.usfm     the book at HEAD, as the hub's port of Sefer's
//!                                           template stitched it (byte-identical to Sefer)
//! ```
//!
//! Each bundle is cloned with the `git` command and read at HEAD. A missing
//! bundle, a missing expectation or a missing `git` fails the test; nothing is
//! skipped.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mise::books::BookKey;

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

/// `…_3jn_text_reg` → `3JN`: Writer names a project `{language}_{book}_text_{resource}`.
fn book_of(repo: &str) -> BookKey {
    let parts: Vec<&str> = repo.split('_').collect();
    let at = parts
        .iter()
        .position(|p| *p == "text")
        .expect("a Writer text repo name");
    let code = parts[at - 1].to_uppercase();
    BookKey::new(code.as_bytes().try_into().expect("a three-letter book"))
}

fn git(args: &[&str], dir: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("--git-dir").arg(dir);
    }
    command.args(args);
    command
}

/// A bare clone of the bundle in a fresh temp dir.
fn clone(bundle: &Path, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bttwriter-golden-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let status = git(&["clone", "--bare", "--quiet"], None)
        .arg(bundle)
        .arg(&dir)
        .status()
        .expect("git runs");
    assert!(status.success(), "git clone {}", bundle.display());
    dir
}

/// Every file at HEAD, as `(path, bytes)`.
fn files_at_head(git_dir: &Path) -> Vec<(String, Vec<u8>)> {
    let listing = git(&["ls-tree", "-r", "--name-only", "HEAD"], Some(git_dir))
        .output()
        .expect("git runs");
    assert!(listing.status.success(), "ls-tree {}", git_dir.display());
    let paths: Vec<String> = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();

    let mut child = git(&["cat-file", "--batch"], Some(git_dir))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("git runs");
    let mut stdin = child.stdin.take().unwrap();
    let requests: String = paths.iter().map(|p| format!("HEAD:{p}\n")).collect();
    let writer = std::thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut files = Vec::new();
    for path in paths {
        let mut header = String::new();
        out.read_line(&mut header).unwrap();
        let size: usize = header
            .split_whitespace()
            .nth(2)
            .unwrap_or_else(|| panic!("{path}: {header}"))
            .parse()
            .unwrap();
        let mut body = vec![0; size + 1];
        out.read_exact(&mut body).unwrap();
        body.pop();
        files.push((path, body));
    }
    writer.join().unwrap().unwrap();
    assert!(child.wait().unwrap().success());
    files
}

#[test]
fn five_writer_projects_stitch_to_the_bytes_sefer_writes() {
    for repo in REPOS {
        let bundle = fixtures().join("repos").join(format!("{repo}.bundle"));
        let expected_path = fixtures().join("stitched").join(format!("{repo}.usfm"));
        assert!(bundle.is_file(), "missing {}", bundle.display());
        let expected = std::fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));

        let git_dir = clone(&bundle, repo);
        let files = files_at_head(&git_dir);
        let _ = std::fs::remove_dir_all(&git_dir);

        let book = book_of(repo);
        let fallback = book.to_string();
        let usfm = bttwriter::stitch(
            files
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
            book,
            &fallback,
        );
        assert!(
            usfm == expected,
            "{repo}: stitched bytes differ from {}",
            expected_path.display()
        );
    }
}
