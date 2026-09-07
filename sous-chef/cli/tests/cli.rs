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

/// `--findings` lists a pattern's sites under it.
#[test]
fn sites_print_under_their_pattern() {
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

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--findings")
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
}

/// Sixty verses of one chapter, verse `i` as long as `length(i)` says: a
/// sample big enough for a median and a MAD to mean anything.
fn sized(code: &str, count: usize, length: impl Fn(usize) -> usize) -> String {
    let mut text = format!("\\id {code}\n\\c 1\n\\p\n");
    for verse in 0..count {
        text.push_str(&format!(
            "\\v {} {}\n",
            verse + 1,
            "a".repeat(length(verse))
        ));
    }
    text
}

/// A declared source is read as USFM or as a vref stream, and either way one
/// verse the source disagrees with gets a `length` row, both scopes printed,
/// with the alignment facts beside it as counts and never as rows.
#[test]
fn a_declared_source_prints_length_rows_and_unpaired_counts() {
    let temp = TempDir::new();
    let target = temp.0.join("MRK.usfm");
    fs::write(
        &target,
        sized(
            "MRK",
            60,
            |verse| if verse == 59 { 20 } else { 40 + verse % 7 },
        ),
    )
    .unwrap();

    // The same keys at a constant length, once as USFM and once as vref, with
    // one verse the target does not have so an unpaired key is reported.
    let usfm_source = temp.0.join("source.usfm");
    fs::write(&usfm_source, sized("MRK", 61, |_| 40)).unwrap();
    let vref_source = temp.0.join("source.txt");
    let mut rows = String::new();
    for verse in 1..=61 {
        rows.push_str(&format!("MRK 1:{verse}\t{}\n", "a".repeat(40)));
    }
    fs::write(&vref_source, &rows).unwrap();

    for source in [&usfm_source, &vref_source] {
        let output = Command::new(env!("CARGO_BIN_EXE_sous"))
            .arg("--stats-only")
            .arg("--findings")
            .arg("--source")
            .arg(source)
            .arg(&target)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", source.display());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let lengths: Vec<&str> = stdout
            .lines()
            .filter(|line| line.starts_with("length "))
            .collect();
        assert_eq!(lengths.len(), 1, "{}: {stdout}", source.display());
        assert!(
            lengths[0].starts_with("length target[0] MRK 1:60 ratio 0.5"),
            "{}",
            lengths[0]
        );
        assert!(lengths[0].contains("z_book -"), "{}", lengths[0]);
        assert!(
            stdout
                .contains("unpaired MRK target-only 0 source-only 1 ambiguous 0 partial-overlap 0"),
            "the facts are counts, not rows: {stdout}"
        );
    }
}

/// A target with no source declared says nothing about length at all.
#[test]
fn no_source_means_no_length_rows() {
    let temp = TempDir::new();
    let target = temp.0.join("MRK.usfm");
    fs::write(&target, sized("MRK", 60, |verse| 40 + verse % 7)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--stats-only")
        .arg("--findings")
        .arg(&target)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("length target["));
    assert!(!stdout.contains("unpaired "));
}

/// `--report` writes the v1 inventory page as one self-contained file, its
/// `const CORPORA` literal a valid, balanced JSON array.
#[test]
fn report_writes_a_self_contained_inventory_page() {
    let temp = TempDir::new();
    let book = temp.0.join("MRK.usfm");
    fs::write(
        &book,
        concat!(
            "\\id MRK\n\\c 1\n\\p\n",
            "\\v 1 aa, bb, cc, dd, ee, ff ,gg.\n",
        ),
    )
    .unwrap();
    let page = temp.0.join("inventory.html");

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--report")
        .arg(&page)
        .arg(&book)
        .output()
        .unwrap();
    assert!(output.status.success());

    let rendered = fs::read_to_string(&page).unwrap();
    assert!(rendered.starts_with("<!doctype html>"));
    assert!(
        !rendered.contains("src=\"http") && !rendered.contains("href=\"http"),
        "the page fetches nothing"
    );
    assert!(rendered.contains("Character by character"));
    assert!(
        rendered.contains("Capitalization") && rendered.contains(r#""cap":["#),
        "the word tab and its data are present"
    );
    assert!(
        rendered.contains("Verse length") && rendered.contains(r#""len":[]"#),
        "the source tab is present and empty with no source declared"
    );
    assert!(rendered.contains("MRK.usfm"), "the corpus name appears");
    assert!(rendered.contains("U+002C"), "the comma's code appears");
    assert!(
        !rendered.contains("Tune the judge"),
        "the removed tab is gone"
    );
    assert!(
        !rendered.contains("@@CORPORA@@"),
        "the placeholder was substituted"
    );

    let marker = "const CORPORA = ";
    let start = rendered.find(marker).expect("the data literal") + marker.len();
    let end = start
        + rendered[start..]
            .find(";\n")
            .expect("the literal is terminated");
    let json = &rendered[start..end];
    assert!(json.starts_with('['), "one array of corpus records");
    // No serde_json dependency in this workspace: check the brackets balance
    // instead of parsing, skipping bracket-shaped bytes inside JSON strings.
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for c in json.chars() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            _ => {}
        }
        assert!(depth >= 0, "brackets never close before they open");
    }
    assert!(!in_string, "every string literal is closed");
    assert_eq!(depth, 0, "the JSON literal's brackets balance");

    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains(" inventory (")
    );
}

/// A glyph rare enough to fire only `PatternKey::Rarity` still lists a
/// sample on its own card, even though its one occurrence sits in a run a
/// commoner glyph's pattern would headline (`sites.rs`'s "one row per run,
/// headlined by the finest match" — the report must not inherit that as "one
/// glyph's card per run").
#[test]
fn rarity_only_glyph_inside_a_headlined_run_still_lists_a_sample() {
    let temp = TempDir::new();
    let book = temp.0.join("MRK.usfm");
    fs::write(
        &book,
        concat!(
            "\\id MRK\n\\c 1\n\\p\n",
            "\\v 1 One sentence ends here.\n",
            "\\v 2 Another one ends here.\n",
            "\\v 3 A third one ends here.\n",
            "\\v 4 A fourth one ends here.\n",
            "\\v 5 A fifth one ends here.\n",
            "\\v 6 A sixth one ends here.]\n",
        ),
    )
    .unwrap();
    let page = temp.0.join("inventory.html");

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--report")
        .arg(&page)
        .arg(&book)
        .output()
        .unwrap();
    assert!(output.status.success());

    let rendered = fs::read_to_string(&page).unwrap();
    // `]` occurs once in the whole book, under the default rarity floor of 5;
    // its run is `.]`, which the far commoner `.` headlines. `]`'s own card
    // still carries at least one sample.
    let glyph_at = rendered.find(r#""g":"]""#).expect("the ] glyph has a card");
    let rarity_at = rendered[glyph_at..]
        .find(r#""rarity":"#)
        .map(|offset| glyph_at + offset)
        .expect("the glyph card carries a rarity field");
    let field = &rendered[rarity_at..rarity_at + 120];
    assert!(
        field.starts_with(r#""rarity":{"flag":true,"samples":[["#),
        "a rarity-only glyph still lists a sample: {field}"
    );
}
