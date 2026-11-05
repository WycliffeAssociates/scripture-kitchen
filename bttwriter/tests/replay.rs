//! Replay over the five real Writer projects: GOLDEN.
//!
//! ```text
//! each fixture's history, replayed alone        head == Sefer's replayWriterHistories at 326f02c8
//! four books (one per book), replayed together  head == Sefer's, interleaved the same way
//! the four-book pack + idx                      git fsck --strict accepts it; git index-pack writes our idx;
//!                                               each book file at head == that history's usfm()
//! two made-up books, one with a reset clock     git log reads the order, messages and dates we meant
//! ```
//!
//! Sefer's ids hold where Sefer and the kitchen agree: a history with no
//! estimated date (Sefer doesn't estimate). `fixtures/sefer-heads.test.ts`
//! is how they were made. Needs the `git` command; a missing one, or a
//! missing fixture, fails.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use bento::Direction;
use bento::bttwriter::{Language, WriterProject};
use bttwriter::pack::{ObjectId, Signature};
use bttwriter::{ReplayOptions, Replayed, Save, WriterHistory, replay};
use mise::books::BookKey;

use common::{REPOS, clone, history_of};

/// Sefer's head for each fixture replayed alone, then for every fixture but
/// the second 3 John replayed together.
const SEFER_HEADS: &[(&str, &str, usize, usize)] = &[
    (
        "kpass_tmf_3jn_text_reg",
        "350ebd4bbf30573ec4b0f253b239261476f3dece",
        3,
        0,
    ),
    (
        "bahasatech.indotimur_kps-x-kaigar_3jn_text_reg",
        "710cac6f2a667b1bb57af3a4db7d979b4ef6549f",
        16,
        1,
    ),
    (
        "chunga_major_bem-x-ngumbo_2ch_text_reg",
        "cc2a00c7e2c86a2bc3a11e35f5be004776efdb18",
        332,
        5,
    ),
    (
        "andriatiana_wa_msh_job_text_udb",
        "ffe1509052ab8fd71dba2135fea9a367f10a55a3",
        1130,
        36,
    ),
    (
        "bahasatech.indotimur_saw_isa_text_reg",
        "903023b30335d7d1ddf3c232f524bcd714ba1793",
        19,
        0,
    ),
];
const SEFER_FOUR_BOOKS: (&str, usize, usize) =
    ("c9f5bd89dc8e082bed8f05a5aa80e70fdf70de8d", 1484, 41);

/// 2026-11-05: later than every fixture save, so no fixture date is "in the future".
const NOW: i64 = 1_793_836_800;

fn opts() -> ReplayOptions {
    ReplayOptions {
        now: NOW,
        deltas: true,
    }
}

/// A bare repo holding `replayed`'s pack, with `main` at its head.
fn unpacked(replayed: &Replayed, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bttwriter-replay-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    run(Command::new("git")
        .args(["init", "--quiet", "--bare"])
        .arg(&dir));
    let packs = dir.join("objects/pack");
    std::fs::write(
        packs.join(format!("{}.pack", replayed.name)),
        &replayed.pack,
    )
    .unwrap();
    std::fs::write(packs.join(format!("{}.idx", replayed.name)), &replayed.idx).unwrap();
    git(
        &dir,
        &["update-ref", "refs/heads/main", &replayed.head.to_string()],
    );
    git(&dir, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    dir
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = run(Command::new("git").arg("--git-dir").arg(dir).args(args));
    String::from_utf8(out).unwrap()
}

fn run(command: &mut Command) -> Vec<u8> {
    for (name, _) in std::env::vars() {
        if name.starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let out = command.output().expect("git runs");
    assert!(
        out.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

#[test]
fn each_fixture_replays_to_seferss_head_and_four_books_together_do_too() {
    let mut histories: Vec<WriterHistory> = Vec::new();
    for (repo, sefer_head, commits, unchanged) in SEFER_HEADS {
        assert!(REPOS.contains(repo));
        let dir = clone(repo);
        let history = history_of(&dir, repo);
        let _ = std::fs::remove_dir_all(&dir);
        let replayed = replay(std::slice::from_ref(&history), &opts()).unwrap();
        assert_eq!(
            replayed.estimated, 0,
            "{repo}: a bad clock; Sefer's id doesn't apply"
        );
        assert_eq!(
            (
                replayed.head.to_string().as_str(),
                replayed.commits,
                replayed.unchanged
            ),
            (*sefer_head, *commits, *unchanged),
            "{repo}: not Sefer's line of commits"
        );
        histories.push(history);
    }

    histories.remove(1); // the second 3 John: one history per book
    let replayed = replay(&histories, &opts()).unwrap();
    let (sefer_head, commits, unchanged) = SEFER_FOUR_BOOKS;
    assert_eq!(
        (
            replayed.head.to_string().as_str(),
            replayed.commits,
            replayed.unchanged
        ),
        (sefer_head, commits, unchanged),
        "four books: not Sefer's interleaving"
    );
    let files: Vec<&str> = replayed.books.iter().map(|b| b.file.as_str()).collect();
    assert_eq!(
        files,
        ["65-3JN.usfm", "14-2CH.usfm", "18-JOB.usfm", "23-ISA.usfm"]
    );
    assert!(
        replayed.stats.deltas > replayed.stats.objects / 2,
        "{:?}",
        replayed.stats
    );

    // git reads it as its own: every object, the index byte for byte, each book at head.
    let dir = unpacked(&replayed, "four");
    git(&dir, &["fsck", "--full", "--strict", "--no-dangling"]);
    let pack = dir
        .join("objects/pack")
        .join(format!("{}.pack", replayed.name));
    let theirs =
        std::env::temp_dir().join(format!("bttwriter-replay-{}-four.idx", std::process::id()));
    run(Command::new("git")
        .arg("index-pack")
        .arg("-o")
        .arg(&theirs)
        .arg(&pack));
    assert!(
        std::fs::read(&theirs).unwrap() == replayed.idx,
        "git's idx differs from ours"
    );
    for (book, history) in replayed.books.iter().zip(&histories) {
        let at_head = git(&dir, &["cat-file", "blob", &format!("main:{}", book.file)]);
        assert!(
            at_head == history.usfm(),
            "{}: head isn't the book's last save",
            book.file
        );
    }
    let count = git(&dir, &["rev-list", "--count", "main"]);
    assert_eq!(count.trim(), commits.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&theirs);
}

fn history(book: &[u8; 3], source: &str, saves: Vec<Save>) -> WriterHistory {
    WriterHistory {
        source: source.into(),
        project: WriterProject {
            book: BookKey::new(*book),
            language: Language {
                id: "tmf".into(),
                name: None,
                direction: Direction::Ltr,
            },
            resource: "reg".into(),
            sources: Vec::new(),
            translators: Vec::new(),
        },
        project_name: None,
        saves,
    }
}

fn save(n: u8, at: i64, path: &str, text: &str) -> Save {
    let who = Signature {
        name: "Mwila Chanda".into(),
        email: "mwila@example.org".into(),
        at,
        offset_minutes: 120,
    };
    Save {
        id: ObjectId([n; 20]),
        author: who.clone(),
        committer: Signature { at: at + 5, ..who },
        message: format!("2019-03-0{n}T10:22:31"),
        changes: vec![(path.into(), Some(text.into()))],
    }
}

// Why: the order of the line and each commit's message and dates are what a
// reader of the consolidated log sees, and the clock trailer is the only
// record of what a reset device said. Read back by git, not by us.
#[test]
fn git_reads_the_order_messages_and_dates_we_meant() {
    const T: i64 = 1_600_000_000;
    let rom = history(
        b"ROM",
        "kpass/tmf_rom_text_reg",
        vec![
            save(1, T, "05/01.txt", "\\c 5 \\v 1 a \\v 2-3 b"),
            // A reset battery: estimated to a second before the next good save.
            save(2, 946_684_800, "05/04.txt", "\\v 4 c"),
            save(3, T + 300, "front/title.txt", "Roma"),
        ],
    );
    let tit = history(
        b"TIT",
        "kpass/tmf_tit_text_reg",
        vec![
            save(4, T + 100, "01/01.txt", "\\c 1 \\v 1 d"),
            save(5, T + 300, "01/02.txt", "\\v 2 e"), // ties ROM's third: ROM goes first
        ],
    );
    let replayed = replay(&[rom, tit], &opts()).unwrap();
    assert_eq!(
        (replayed.commits, replayed.unchanged, replayed.estimated),
        (5, 0, 1)
    );
    let titles: Vec<&str> = replayed.books.iter().map(|b| b.title.as_str()).collect();
    assert_eq!(titles, ["Roma", "Titus"]);

    let dir = unpacked(&replayed, "made-up");
    git(&dir, &["fsck", "--full", "--strict", "--no-dangling"]);
    let log = git(&dir, &["log", "--reverse", "--format=%at %ct %ai|%s"]);
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        [
            "1600000000 1600000005 2020-09-13 14:26:40 +0200|ROM 5:1–3",
            "1600000100 1600000105 2020-09-13 14:28:20 +0200|TIT 1:1",
            "1600000299 1600000304 2020-09-13 14:31:39 +0200|ROM 5:4",
            "1600000300 1600000305 2020-09-13 14:31:40 +0200|ROM titles",
            "1600000300 1600000305 2020-09-13 14:31:40 +0200|TIT 1:2",
        ]
    );
    let estimated = git(&dir, &["log", "--format=%B", "-1", "main~2"]);
    assert_eq!(
        estimated,
        format!(
            "ROM 5:4\n\nWriter-Message: 2019-03-02T10:22:31\nWriter-Source: kpass/tmf_rom_text_reg@{}\nWriter-Clock: 2000-01-01 (device clock; date estimated)\n\n",
            ObjectId([2; 20])
        )
    );
    let files = git(&dir, &["ls-tree", "--name-only", "main"]);
    assert_eq!(files, "46-ROM.usfm\n57-TIT.usfm\n");
    let _ = std::fs::remove_dir_all(&dir);
}
