use bento::Direction;
use bento::bttwriter::{Language, WriterProject};

use super::*;
use crate::pack::Signature;

const T: i64 = 1_600_000_000;
const NOW: i64 = 1_700_000_000;

fn history(book: &[u8; 3], saves: Vec<Save>) -> WriterHistory {
    WriterHistory {
        source: "owner/repo".into(),
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

fn save(n: u8, at: i64, changes: &[(&str, Option<&str>)]) -> Save {
    let who = Signature {
        name: "Mwila".into(),
        email: "m@example.org".into(),
        at,
        offset_minutes: 120,
    };
    Save {
        id: ObjectId([n; 20]),
        author: who.clone(),
        committer: who,
        message: format!("save {n}"),
        changes: changes
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.map(str::to_owned)))
            .collect(),
    }
}

fn opts() -> ReplayOptions {
    ReplayOptions {
        now: NOW,
        deltas: true,
    }
}

#[test]
fn a_save_that_leaves_the_rendering_as_it_was_is_no_commit() {
    let h = history(
        b"3JN",
        vec![
            save(1, T, &[("01/01.txt", Some("\\c 1 \\v 1 a"))]),
            // The same text again, and a chapter label equal to its number (not rendered).
            save(2, T + 1, &[("01/01.txt", Some("\\c 1 \\v 1 a"))]),
            save(3, T + 2, &[("01/title.txt", Some("1"))]),
            save(4, T + 3, &[("01/01.txt", Some("\\c 1 \\v 1 b"))]),
        ],
    );
    let r = replay(&[h], &opts()).unwrap();
    assert_eq!((r.commits, r.unchanged, r.estimated), (2, 2, 0));
    assert_eq!(
        r.books,
        [BookFile {
            book: BookKey::new(*b"3JN"),
            file: "65-3JN.usfm".into(),
            title: "3 John".into(),
        }]
    );
}

// Why: Sefer starts each book with no rendering at all, so its first save is
// always a commit, even one that renders like an empty book. The hub started
// from an empty string and could drop it, a different line of commits.
#[test]
fn a_books_first_save_is_always_a_commit() {
    let h = history(b"3JN", vec![save(1, T, &[("01/01.txt", None)])]);
    let r = replay(&[h], &opts()).unwrap();
    assert_eq!((r.commits, r.unchanged), (1, 0));
}

#[test]
fn nothing_to_commit_and_two_histories_of_one_book_are_errors() {
    assert_eq!(replay(&[], &opts()), Err(ReplayError::NothingChanged));
    let a = history(b"ROM", vec![save(1, T, &[("01/01.txt", Some("\\v 1 a"))])]);
    assert_eq!(
        replay(&[a.clone(), a], &opts()),
        Err(ReplayError::SameBook(BookKey::new(*b"ROM")))
    );
}

#[test]
fn estimated_dates_are_counted() {
    let h = history(
        b"3JN",
        vec![
            save(1, T, &[("01/01.txt", Some("\\v 1 a"))]),
            save(2, 946_684_800, &[("01/01.txt", Some("\\v 1 b"))]),
            save(3, T + 100, &[("01/01.txt", Some("\\v 1 c"))]),
        ],
    );
    assert_eq!(replay(&[h], &opts()).unwrap().estimated, 1);
}

#[test]
fn deltas_off_writes_every_object_whole() {
    let saves = (1..=20)
        .map(|n| {
            save(
                n,
                T + i64::from(n),
                &[(
                    "01/01.txt",
                    Some(&*format!("\\v 1 {}", "x".repeat(n.into()))),
                )],
            )
        })
        .collect::<Vec<_>>();
    let h = history(b"3JN", saves);
    let with = replay(std::slice::from_ref(&h), &opts()).unwrap();
    let without = replay(
        &[h],
        &ReplayOptions {
            now: NOW,
            deltas: false,
        },
    )
    .unwrap();
    assert_eq!(
        with.head, without.head,
        "deltas change the pack, not the commits"
    );
    assert!(with.stats.deltas > 0);
    assert_eq!(without.stats.deltas, 0);
}

// Why: Sefer holds a book's files in a JavaScript Map, and the template lets
// the later of two title files win. In a Map "later" is insertion order: a
// new path goes last, a deleted one comes back last. Sorted order would pick
// the other title and render different bytes.
#[test]
fn files_are_held_in_the_order_the_saves_wrote_them() {
    let h = history(
        b"3JN",
        vec![
            save(
                1,
                T,
                &[
                    ("01/title.txt", Some("Sura A")),
                    ("01/01.txt", Some("\\v 1 a")),
                ],
            ),
            save(2, T + 1, &[("01/TITLE.txt", Some("Sura B"))]),
        ],
    );
    assert!(h.usfm().contains("\\cl Sura B"), "{}", h.usfm());
    let mut later = h.clone();
    later.saves.push(save(3, T + 2, &[("01/title.txt", None)]));
    later
        .saves
        .push(save(4, T + 3, &[("01/title.txt", Some("Sura C"))]));
    assert!(later.usfm().contains("\\cl Sura C"), "{}", later.usfm());

    let mut files = Files::default();
    files.set("a", "1");
    files.set("b", "2");
    files.set("a", "3");
    assert_eq!(files.iter().collect::<Vec<_>>(), [("a", "3"), ("b", "2")]);
    files.delete("a");
    files.set("a", "4");
    assert_eq!(files.iter().collect::<Vec<_>>(), [("b", "2"), ("a", "4")]);
    assert_eq!(files.get("a"), Some("4"));
    files.delete("a");
    assert_eq!(files.get("a"), None);
}

#[test]
fn progress_counts_verses_after_each_save_at_its_estimated_time() {
    let h = history(
        b"3JN",
        vec![
            save(1, T, &[("01/01.txt", Some("\\v 1 a \\v 2-4 b"))]),
            save(2, 0, &[("01/05.txt", Some("\\v 5 c"))]),
            save(3, T + 10, &[("01/01.txt", None)]),
        ],
    );
    assert_eq!(h.progress(NOW), [(T, 4), (T + 9, 5), (T + 10, 1)]);
}

// Why: a book's title for a manifest is Sefer's `titleOf`, a regex over the
// rendered book: `/^\\h\s+(.+)$/m`, trimmed. Its corners (an all-space
// heading, `\h` with no space, a heading on a later line) decide the title.
#[test]
fn the_title_is_the_first_heading() {
    assert_eq!(h_title("\\id ROM\n\\h Roma \n\\mt1 Roma"), Some("Roma"));
    assert_eq!(h_title("\\hx Roma\n\\h Roma"), Some("Roma"));
    assert_eq!(h_title("\\h\u{FEFF}Roma"), Some("Roma"));
    assert_eq!(
        h_title("\\h   \n\\h Roma"),
        Some(""),
        "\\s+ gives (.+) its last space"
    );
    assert_eq!(
        h_title("\\h \n\\h Roma"),
        Some("Roma"),
        "one space: no match on that line"
    );
    assert_eq!(h_title("\\hRoma"), None);
}
