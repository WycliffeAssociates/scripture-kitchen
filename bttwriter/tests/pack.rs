//! The pack writer against real git: only git can say a pack is right.
//!
//! ```text
//! history (62 saves, two book files)
//!   → PackWriter                      head, pack, idx
//!   → git fast-import, same history   head            the heads must be equal
//! git fsck --full --strict            on a repo holding only our pack
//! git index-pack                      its .idx must equal ours, byte for byte
//! git show <commit>:<file>            every file at every commit reads back
//! ```
//!
//! Needs the `git` command; a missing one fails the test.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use bttwriter::pack::{
    CommitFields, Kind, ObjectId, PackWriter, Signature, commit_body, tree_body,
};

struct Step {
    file: &'static str,
    text: String,
    who: Signature,
    message: String,
}

fn someone(n: i64) -> Signature {
    let device = n % 3 == 0;
    Signature {
        name: if device { "tsDesktop" } else { "Mwila Chanda" }.into(),
        email: if device {
            "tsdesktop@example.org"
        } else {
            "mwila@example.org"
        }
        .into(),
        at: 1_782_000_000 + n * 61,
        offset_minutes: if n % 2 == 0 { 120 } else { -300 },
    }
}

fn long_book(marker: &str) -> String {
    std::iter::once("\\id ACT".to_string())
        .chain(
            (1..=900)
                .map(|i| format!("\\v {i} Mu kale {marker} — ŋaŋa ẽ ẅ {}", "text ".repeat(16))),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

fn history() -> Vec<Step> {
    let mut steps = vec![
        Step {
            file: "45-ACT.usfm",
            text: long_book("a"),
            who: someone(0),
            message: "ACT 1:1\n".into(),
        },
        Step {
            file: "66-JUD.usfm",
            text: "\\id JUD\n\\v 1 Yuda".into(),
            who: someone(1),
            message: "JUD 1:1\n".into(),
        },
        Step {
            file: "45-ACT.usfm",
            text: long_book("a").replace("Mu kale a", &format!("Mu kale {}", "ẑ".repeat(300))),
            who: someone(2),
            message: "ACT 1:1, a long insert\n".into(),
        },
        Step {
            file: "66-JUD.usfm",
            text: "\\id JUD\n\\v 1 Yuda".into(),
            who: someone(3),
            message: "JUD, a save that changed nothing\n".into(),
        },
    ];
    for i in 0..60 {
        steps.push(Step {
            file: "66-JUD.usfm",
            text: format!("\\id JUD\n\\v 1 Yuda {}", "musha ".repeat(i + 1)),
            who: someone(4 + i as i64),
            message: format!("JUD 1:1, save {}\n", i + 1),
        });
    }
    steps
}

/// `git`, with no `GIT_*` from the environment leaking in.
fn git(args: &[&str]) -> Command {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars() {
        if name.starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command.args(args);
    command
}

fn run(command: &mut Command) -> Vec<u8> {
    let out = command.output().expect("git runs");
    assert!(
        out.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn work() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bttwriter-pack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// The same history through `git fast-import`: git's own head.
fn gits_head(steps: &[Step], dir: &Path) -> String {
    run(git(&["init", "-q", "--bare", path(dir)]).arg("--initial-branch=main"));
    let mut stream = Vec::new();
    let data = |text: &str, stream: &mut Vec<u8>| {
        stream.extend_from_slice(format!("data {}\n", text.len()).as_bytes());
        stream.extend_from_slice(text.as_bytes());
        stream.push(b'\n');
    };
    for (n, step) in steps.iter().enumerate() {
        let who = step.who.to_string();
        stream.extend_from_slice(
            format!(
                "commit refs/heads/main\nmark :{}\nauthor {who}\ncommitter {who}\n",
                n + 1
            )
            .as_bytes(),
        );
        data(&step.message, &mut stream);
        if n > 0 {
            stream.extend_from_slice(format!("from :{n}\n").as_bytes());
        }
        stream.extend_from_slice(format!("M 100644 inline {}\n", step.file).as_bytes());
        data(&step.text, &mut stream);
    }
    let mut child = git(&["-C", path(dir), "fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()
        .expect("git runs");
    child.stdin.take().unwrap().write_all(&stream).unwrap();
    assert!(child.wait().unwrap().success(), "fast-import");
    String::from_utf8(run(&mut git(&["-C", path(dir), "rev-parse", "main"])))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn git_reads_our_pack_exactly_as_written() {
    let steps = history();
    let work = work();

    let mut pack = PackWriter::new();
    let mut files: BTreeMap<String, ObjectId> = BTreeMap::new();
    let mut head: Option<ObjectId> = None;
    let mut commits = Vec::new();
    for step in &steps {
        let blob = pack.add(Kind::Blob, step.text.as_bytes(), Some(step.file));
        files.insert(step.file.to_string(), blob);
        let tree = pack.add(Kind::Tree, &tree_body(&files), Some("tree"));
        let parents: Vec<ObjectId> = head.into_iter().collect();
        let commit = pack.add(
            Kind::Commit,
            &commit_body(&CommitFields {
                tree,
                parents: &parents,
                author: &step.who,
                committer: &step.who,
                message: &step.message,
            }),
            None,
        );
        head = Some(commit);
        commits.push((commit, files.clone()));
    }
    let head = head.unwrap();
    let written = pack.finish();
    assert!(written.stats.deltas > 0);

    // Git builds the same commits from the same history.
    assert_eq!(
        head.to_string(),
        gits_head(&steps, &work.join("fast-import"))
    );

    // A repo holding only our pack passes git's strictest check.
    let ours = work.join("ours");
    run(git(&["init", "-q", "--bare", path(&ours)]).arg("--initial-branch=main"));
    let packs = ours.join("objects/pack");
    std::fs::write(packs.join(format!("{}.pack", written.name)), &written.pack).unwrap();
    std::fs::write(packs.join(format!("{}.idx", written.name)), &written.idx).unwrap();
    std::fs::write(ours.join("refs/heads/main"), format!("{head}\n")).unwrap();
    run(&mut git(&[
        "-C",
        path(&ours),
        "fsck",
        "--full",
        "--strict",
        "--no-progress",
    ]));

    // git's own index of our pack is our index.
    let again = work.join("reindexed");
    std::fs::create_dir_all(&again).unwrap();
    let copy = again.join("copy.pack");
    std::fs::write(&copy, &written.pack).unwrap();
    run(&mut git(&[
        "index-pack",
        "--strict",
        "-o",
        path(&again.join("copy.idx")),
        path(&copy),
    ]));
    assert!(
        std::fs::read(again.join("copy.idx")).unwrap() == written.idx,
        "idx differs from git's"
    );

    // Every file at every commit reads back.
    for (commit, files) in &commits {
        for (file, blob) in files {
            let shown = run(&mut git(&[
                "-C",
                path(&ours),
                "rev-parse",
                &format!("{commit}:{file}"),
            ]));
            assert_eq!(String::from_utf8(shown).unwrap().trim(), blob.to_string());
        }
    }
    let last = steps
        .iter()
        .rev()
        .find(|s| s.file == "45-ACT.usfm")
        .unwrap();
    let read = run(&mut git(&[
        "-C",
        path(&ours),
        "show",
        &format!("{head}:45-ACT.usfm"),
    ]));
    assert!(read == last.text.as_bytes());

    let _ = std::fs::remove_dir_all(&work);
}
