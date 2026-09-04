use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("sous-cli-process-{suffix}-{id}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn stats_only_suppresses_debug_output_and_implies_stats() {
    let temp = TempDir::new();
    let book = temp.0.join("MRK.usfm");
    fs::write(&book, "\\id MRK\n\\c 1\n\\p\n\\v 1 Mark.\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--stats-only")
        .arg(&book)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("stats: mode=serial target_files=1 "));
}

/// The `--findings` rows a footnote fixture prints, pinned byte for byte
/// across the move to `sous_core::analyze`.
#[test]
fn findings_rows_are_unchanged_for_the_footnote_fixture() {
    let temp = TempDir::new();
    let book = temp.0.join("MRK.usfm");
    fs::write(
        &book,
        concat!(
            "\\id MRK\n",
            "\\c 1\n\\p\n",
            "\\v 1 Jesus \\f + \\ft note\\f* wept \\\\ here.\n",
            "\\v 2-3 Two and three.\0\n",
            "\\c 2\n\\p\n",
            "\\v 1 An \u{1f9c5}.\n",
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--findings")
        .arg(&book)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with("finding "))
        .collect();
    assert_eq!(
        rows,
        vec![
            "finding target[0] MRK 1:1-1:1 StrandedBackslash 14..16 run 2 raw [49..51]",
            "finding target[0] MRK 1:2-1:3 C0Control 37..38 run 1 raw [79..80]",
        ]
    );
}

/// `--findings` lists a pattern's sites under it, and `--report` writes the
/// same information as one self-contained page.
#[test]
fn sites_print_under_their_pattern_and_the_report_page_stands_alone() {
    let temp = TempDir::new();
    let book = temp.0.join("MRK.usfm");
    // Five commas attached to a letter and one after a space: the odd one out
    // fires placement, and its site is the comma itself.
    fs::write(
        &book,
        concat!(
            "\\id MRK\n\\c 1\n\\p\n",
            "\\v 1 aa, bb, cc, dd, ee, ff ,gg.\n",
        ),
    )
    .unwrap();
    let page = temp.0.join("sites.html");

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--findings")
        .arg("--report")
        .arg(&page)
        .arg(&book)
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let comma = stdout
        .lines()
        .position(|line| {
            line.starts_with("pattern[") && line.contains("U+002C ',' placement prev=Space")
        })
        .expect("the lone spaced comma fires");
    let site = stdout.lines().nth(comma + 1).expect("its site follows it");
    assert_eq!(site, "  site MRK 25..26");

    let rendered = fs::read_to_string(&page).unwrap();
    assert!(rendered.starts_with("<!doctype html>"));
    assert!(rendered.ends_with("</html>\n"));
    assert!(
        !rendered.contains("src=\"http") && !rendered.contains("href=\"http"),
        "the page fetches nothing"
    );
    assert!(
        rendered.contains("<mark>,</mark>"),
        "the span is highlighted"
    );
    assert!(rendered.contains("MRK 1:1-1:1"));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains(" sites to ")
    );
}
