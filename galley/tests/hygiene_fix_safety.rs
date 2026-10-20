//! Every hygiene fix is certain. Publish, apply every deletion `fix_row`
//! returns, republish: each fixed row is gone, no (book, class, placement)
//! group gains a row, and every deleted scalar is one of its row's class, so
//! no letter and no marker is ever lost. A row with no fix may still go when
//! a neighbour's fix restores its context: fa_opv's `ت‌\u{7f}ها` loses its
//! DEL, and the ZWNJ is then a joiner between letters.
//!
//! Instrument: VOLUME, both test tiers, each corpus one project: onion's
//! `testData/exampleCorpora` as written, and sous's `corpora/*.txt` written
//! out as USFM (`\id`, `\c`, `\p`, `\v` per vref line) so it publishes the
//! way Sefer's books do. And SHAPES, one book holding a case of every
//! fix-table row through the real scan, the conjunct and a footnote split
//! included.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mise::unicode::class_of;
use sous_core::hygiene::groups::{GroupKey, Scope, fix_row, hygiene_groups};
use sous_core::{Brigade, CorpusSnapshot, HygieneClass};
use usfm_galley::Role;
use usfm_galley::sous::Expediter;

const BUDGET: usize = 1 << 30;

#[derive(Debug, Default, PartialEq, Eq)]
struct Outcome {
    rows: usize,
    groups: usize,
    fixed_rows: usize,
    deleted_scalars: usize,
    rows_after: usize,
    cleared: usize,
}

fn publish(books: &[(String, String)]) -> Vec<u8> {
    let mut sous = Expediter::new(Brigade::default(), BUDGET);
    for (id, text) in books {
        sous.update(id.as_str(), Role::Target, text)
            .expect("a target");
    }
    sous.publish().expect("a publication")
}

fn counts(buffer: &[u8]) -> BTreeMap<GroupKey, usize> {
    let snapshot = CorpusSnapshot::open(buffer).unwrap();
    hygiene_groups(&snapshot, Scope::Book)
        .unwrap()
        .iter()
        .map(|group| (group.key(), group.count()))
        .collect()
}

fn member(class: HygieneClass, c: char) -> bool {
    match class {
        HygieneClass::C0Control => (c as u32) < 0x20 && c != '\t' && c != '\n',
        HygieneClass::Delete => c == '\u{7f}',
        HygieneClass::NoBreakSpace => c == '\u{a0}',
        HygieneClass::MisplacedFormat => class_of(c).is_format(),
        HygieneClass::Noncharacter => class_of(c).is_noncharacter(),
        other => panic!("{other:?} has no fix"),
    }
}

/// Applies every fix and checks the republication against the first.
fn fix_and_republish(books: &[(String, String)]) -> Outcome {
    let before = publish(books);
    let snapshot = CorpusSnapshot::open(&before).unwrap();
    let groups = hygiene_groups(&snapshot, Scope::Book).unwrap();
    let mut outcome = Outcome {
        groups: groups.len(),
        ..Outcome::default()
    };
    let mut fixed: BTreeMap<GroupKey, usize> = BTreeMap::new();
    let mut texts: Vec<Vec<u16>> = books
        .iter()
        .map(|(_, text)| text.encode_utf16().collect())
        .collect();
    let mut deletions: Vec<Vec<(u32, u32, HygieneClass)>> = vec![Vec::new(); books.len()];
    for group in &groups {
        outcome.rows += group.count();
        for row in group.rows() {
            let Some(deletion) = fix_row(row, snapshot.coordinate_space()) else {
                continue;
            };
            *fixed.entry(group.key()).or_default() += 1;
            outcome.fixed_rows += 1;
            let id = snapshot.book(deletion.book).expect("a published book").id();
            let book = books
                .iter()
                .position(|(name, _)| name == id)
                .expect("a registered id");
            deletions[book].push((deletion.from, deletion.to, row.digest.class()));
        }
    }
    for (book, list) in deletions.iter_mut().enumerate() {
        list.sort_by_key(|deletion| core::cmp::Reverse(deletion.0));
        let text = &mut texts[book];
        let mut floor = u32::MAX;
        for &(from, to, class) in list.iter() {
            assert!(
                to <= floor,
                "{}: deletions overlap at {from}..{to}",
                books[book].0
            );
            floor = from;
            let removed = String::from_utf16(&text[from as usize..to as usize])
                .unwrap_or_else(|_| panic!("{}: {from}..{to} splits a scalar", books[book].0));
            for c in removed.chars() {
                assert!(
                    member(class, c),
                    "{}: {from}..{to} deletes U+{:04X}, which is no {class:?}",
                    books[book].0,
                    c as u32
                );
                outcome.deleted_scalars += 1;
            }
            text.drain(from as usize..to as usize);
        }
    }
    let fixed_books: Vec<(String, String)> = books
        .iter()
        .zip(&texts)
        .map(|((id, _), text)| (id.clone(), String::from_utf16(text).expect("whole scalars")))
        .collect();
    let after = counts(&publish(&fixed_books));
    outcome.rows_after = after.values().sum();
    for (key, count) in counts(&before) {
        let most = count - fixed.get(&key).copied().unwrap_or(0);
        let left = after.get(&key).copied().unwrap_or(0);
        assert!(left <= most, "{key:?}: {left} rows after, at most {most}");
        outcome.cleared += most - left;
    }
    for key in after.keys() {
        assert!(
            counts(&before).contains_key(key),
            "{key:?} appears only after the fix"
        );
    }
    outcome
}

fn corpus(dir: &PathBuf) -> Vec<(String, String)> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "usfm" || ext == "SFM")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "{} holds books", dir.display());
    // A book with no `\id` line is one galley refuses to register.
    paths
        .iter()
        .map(|path| {
            let id = path.file_name().unwrap().to_string_lossy().into_owned();
            (id, std::fs::read_to_string(path).unwrap())
        })
        .filter(|(_, text)| text.lines().any(|line| line.starts_with("\\id ")))
        .collect()
}

#[test]
fn every_fix_on_the_test_tier_deletes_only_its_rows() {
    let tier = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testData/exampleCorpora");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&tier)
        .unwrap_or_else(|error| panic!("{}: {error}", tier.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    assert_eq!(dirs.len(), 4, "the tier's four corpora");
    for dir in &dirs {
        let outcome = fix_and_republish(&corpus(dir));
        eprintln!(
            "{}: {outcome:?}",
            dir.file_name().unwrap().to_string_lossy()
        );
    }
}

/// `BOOK C:V<TAB>text` lines as one USFM book per book code, in file order.
fn vref_as_usfm(path: &PathBuf) -> Vec<(String, String)> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut books: Vec<(String, String)> = Vec::new();
    let mut chapter = String::new();
    for line in text.lines() {
        let Some((reference, verse)) = line.split_once('\t') else {
            continue;
        };
        let Some((book, cv)) = reference.split_once(' ') else {
            continue;
        };
        let Some((c, v)) = cv.split_once(':') else {
            continue;
        };
        if books.last().is_none_or(|(id, _)| id != book) {
            books.push((book.to_owned(), format!("\\id {book}\n")));
            chapter.clear();
        }
        let usfm = &mut books.last_mut().expect("a book").1;
        if chapter != c {
            chapter = c.to_owned();
            usfm.push_str(&format!("\\c {c}\n\\p\n"));
        }
        usfm.push_str(&format!("\\v {v} {verse}\n"));
    }
    assert!(!books.is_empty(), "{} holds vref lines", path.display());
    books
}

#[test]
fn every_fix_on_the_vref_tier_deletes_only_its_rows() {
    let tier = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpora");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&tier)
        .unwrap_or_else(|error| panic!("{}: {error}", tier.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 8, "the sous tier's eight corpora");
    for path in &paths {
        let outcome = fix_and_republish(&vref_as_usfm(path));
        eprintln!(
            "{}: {outcome:?}",
            path.file_stem().unwrap().to_string_lossy()
        );
    }
}

#[test]
fn every_fix_table_case_fixes_through_galley() {
    let text = concat!(
        "\\id GEN\n\\c 1\n\\p\n",
        "\\v 1 word \u{a0}next and\u{a0}\u{a0}two \u{62a}\u{200c} \u{62a} ",
        "\u{915}\u{94d}\u{200c} x a\u{200b}\u{200b}b \u{1780}\u{200b} \u{1781} ",
        "in the\u{feff} beginning a\u{feff}b 7\u{feff}8 x\u{0}\u{0}y a\u{7f}. ",
        "\u{62a}\u{200c}\u{200c}\u{62a} a \u{e0001} b a\u{1fffe}\u{301}b \u{a0}\u{301}x ",
        "\u{fffd} q\u{fdd0}r.\n",
        "\\v 2 word\u{a0}\\f + \\ft note\\f*\u{a0} next.\n",
    );
    let outcome = fix_and_republish(&[("GEN.usfm".to_owned(), text.to_owned())]);
    // Fixed: NBSP beside a space, the NBSP pair, the ZWNJ after a letter, the
    // ZWSP pair, the ZWSP and the BOM beside a space, the NULs, the DEL, the
    // ZWNJ pair, the astral tag, the NBSP before a mark, the BMP noncharacter.
    // Kept: the conjunct's ZWNJ, the BOMs between letters and between
    // digits, the astral noncharacter a mark widened, the split NBSP pair,
    // the free mark after the NBSP and U+FFFD.
    assert_eq!(
        outcome,
        Outcome {
            rows: 19,
            groups: 10,
            fixed_rows: 12,
            deleted_scalars: 13,
            rows_after: 7,
            cleared: 0,
        }
    );
}
