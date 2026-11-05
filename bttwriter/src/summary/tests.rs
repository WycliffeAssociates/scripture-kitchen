use super::*;

fn ch(path: &str, text: &str) -> (String, Option<String>) {
    (path.to_owned(), Some(text.to_owned()))
}

fn nothing_before(_: &str) -> Option<&'static str> {
    None
}

// Why: the subject line is the only intent a replayed commit carries, and a
// different byte is a different commit id from Sefer's. Bridges, chunks with
// no verse marker, titles and the "+N more" cut are where a port slips.
#[test]
fn summaries_say_what_changed() {
    let rom = |changes: &[(String, Option<String>)]| summary_of("ROM", changes, nothing_before);
    assert_eq!(
        rom(&[ch("05/01.txt", "\\v 1 a \\v 2-3 b \\v 11 c")]),
        "ROM 5:1–11"
    );
    assert_eq!(rom(&[ch("05/01.txt", "\\v 4 one")]), "ROM 5:4");
    assert_eq!(
        rom(&[ch("05/01.txt", "no marker")]),
        "ROM 5 (no verse number)"
    );
    assert_eq!(rom(&[ch("05/title.txt", "Chapter 5")]), "ROM titles");
    assert_eq!(rom(&[ch("front/title.txt", "Roma")]), "ROM titles");
    assert_eq!(
        rom(&[
            ch("06/04.txt", "\\v 4 x \\v 6 y"),
            ch("05/01.txt", "\\v 1 x"),
            ch("05/title.txt", "")
        ]),
        "ROM 5:1, 6:4–6; titles"
    );
    let many: Vec<_> = (1..=8)
        .map(|i| ch(&format!("01/{i:02}.txt"), &format!("\\v {i} x")))
        .collect();
    assert_eq!(
        summary_of("TIT", &many, nothing_before),
        "TIT 1:1, 1:2, 1:3, 1:4, 1:5, 1:6, +2 more"
    );
}

#[test]
fn a_deleted_chunk_is_named_from_its_text_before() {
    let gone = [("05/01.txt".to_owned(), None)];
    let before = |path: &str| (path == "05/01.txt").then_some("\\v 1 a \\v 2 b");
    assert_eq!(summary_of("ROM", &gone, before), "ROM 5:1–2");
    assert_eq!(
        summary_of("ROM", &gone, nothing_before),
        "ROM 5 (no verse number)"
    );
}

// Why: Sefer's regex is `/\\v\s+(\d+)(?:-(\d+))?/g`: JavaScript whitespace
// (U+FEFF counts), ASCII digits, a bridge only with digits after the dash.
#[test]
fn verse_markers_follow_seferss_regex() {
    let found = |chunk: &str| verse_markers(chunk).collect::<Vec<_>>();
    assert_eq!(found("\\v 1 \\v 2-3 \\v\t4"), [(1, 1), (2, 3), (4, 4)]);
    assert_eq!(found("\\va 1 \\vp 2 \\v1 \\v x \\v"), []);
    assert_eq!(
        found("\\v\u{FEFF}7"),
        [(7, 7)],
        "U+FEFF is JavaScript whitespace"
    );
    assert_eq!(found("\\v\u{85}7"), [], "U+0085 isn't");
    assert_eq!(found("\\v 5- \\v 6-x \\v 7--8"), [(5, 5), (6, 6), (7, 7)]);
    assert_eq!(found("\\v ٣"), [], "digits are ASCII");
    assert_eq!(found("\\v 007"), [(7, 7)]);
    assert_eq!(
        verses_of(5, "\\v 9-3"),
        "5:9–3",
        "lowest start to highest end, as Sefer"
    );
}

#[test]
fn verse_counts_count_a_bridge_as_its_verses() {
    assert_eq!(verse_count("\\v 1 a \\v 2-4 b"), 4);
    assert_eq!(verse_count("\\v 9-3"), 1);
    assert_eq!(verse_count("no marker"), 0);
}
