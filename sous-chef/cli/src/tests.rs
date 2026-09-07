//! Instrument: SHAPES — hand-built inputs and a temp directory, chosen so
//! one flag is under test at a time.

use crate::load::paths_for_input;
use crate::typos::{format_typo_report, typo_word_counts};
use crate::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const MRK: &str = "\\id MRK\n\\c 1\n\\p\n\\v 1 Mark.\n";
const GEN: &str = "\\id GEN\n\\c 1\n\\p\n\\v 1 Genesis.\n";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("sous-cli-{suffix}-{id}"));
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
fn file_input_is_supported_and_case_insensitive() {
    let temp = TempDir::new();
    let path = temp.0.join("book.USFM");
    fs::write(&path, MRK).unwrap();
    assert_eq!(paths_for_input(&path).unwrap(), vec![path]);
}

#[test]
fn directory_input_filters_immediate_files_and_sorts_paths() {
    let temp = TempDir::new();
    fs::write(temp.0.join("b.USFM"), MRK).unwrap();
    fs::write(temp.0.join("a.sfm"), GEN).unwrap();
    fs::write(temp.0.join("ignore.txt"), "").unwrap();
    fs::create_dir(temp.0.join("nested.usfm")).unwrap();
    fs::write(temp.0.join("nested.usfm").join("c.usfm"), MRK).unwrap();

    let paths = paths_for_input(&temp.0).unwrap();
    let names: Vec<_> = paths
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a.sfm", "b.USFM"]);
}

#[test]
fn unsupported_file_and_empty_directory_fail_clearly() {
    let temp = TempDir::new();
    let unsupported = temp.0.join("book.txt");
    fs::write(&unsupported, MRK).unwrap();
    assert!(paths_for_input(&unsupported).is_err());
    assert!(paths_for_input(&temp.0).is_err());
}

#[test]
fn serial_and_parallel_loading_preserve_order_and_semantics() {
    let temp = TempDir::new();
    fs::write(temp.0.join("b.usfm"), MRK).unwrap();
    fs::write(temp.0.join("a.sfm"), GEN).unwrap();
    let serial = load_input(&temp.0, false).unwrap();
    let parallel = load_input(&temp.0, true).unwrap();
    assert_eq!(serial.paths, parallel.paths);
    let serial_corpus = Corpus::try_new(&serial.books).unwrap();
    let parallel_corpus = Corpus::try_new(&parallel.books).unwrap();
    let serial_keys: Vec<_> = serial_corpus.iter().map(|(_, book)| book.key()).collect();
    let parallel_keys: Vec<_> = parallel_corpus.iter().map(|(_, book)| book.key()).collect();
    assert_eq!(serial_keys, parallel_keys);
}

#[test]
fn file_file_alignment_uses_book_key_and_target_only_mode_has_no_source() {
    let temp = TempDir::new();
    let target_path = temp.0.join("target.usfm");
    let source_path = temp.0.join("source.sfm");
    fs::write(&target_path, MRK).unwrap();
    fs::write(&source_path, MRK).unwrap();
    let target = load_input(&target_path, false).unwrap();
    let source = load_input(&source_path, false).unwrap();
    let target_corpus = Corpus::try_new(&target.books).unwrap();
    let source_corpus = Corpus::try_new(&source.books).unwrap();
    let alignment = align(&target_corpus, &source_corpus);
    assert_eq!(alignment.units().len(), 1);
    assert!(alignment.facts().is_empty());
    assert!(source_corpus.index_of(target.books[0].key()).is_some());
}

#[test]
fn independently_ordered_directories_align_by_book_key() {
    let temp = TempDir::new();
    let target_dir = temp.0.join("target");
    let source_dir = temp.0.join("source");
    fs::create_dir(&target_dir).unwrap();
    fs::create_dir(&source_dir).unwrap();
    fs::write(target_dir.join("a-MRK.usfm"), MRK).unwrap();
    fs::write(target_dir.join("b-GEN.usfm"), GEN).unwrap();
    fs::write(source_dir.join("a-GEN.usfm"), GEN).unwrap();
    fs::write(source_dir.join("b-MRK.usfm"), MRK).unwrap();

    let target = load_input(&target_dir, false).unwrap();
    let source = load_input(&source_dir, true).unwrap();
    let target_corpus = Corpus::try_new(&target.books).unwrap();
    let source_corpus = Corpus::try_new(&source.books).unwrap();
    let alignment = align(&target_corpus, &source_corpus);

    let mut books: Vec<_> = alignment.units().iter().map(|unit| unit.book()).collect();
    books.sort_by_key(|book| book.as_bytes());
    assert_eq!(books, vec![target.books[1].key(), target.books[0].key()]);
    assert!(alignment.facts().is_empty());
}

#[test]
fn hygiene_findings_publish_through_galley_in_raw_utf16() {
    use sous_core::{CoordinateSpace, CorpusSnapshot, FindingKind, HygieneClass};

    let temp = TempDir::new();
    // Onion masks a lone `\` as a marker; a `\\` pair reaches Sous as
    // content. It sits past a 4-byte char, so the UTF-16 span moves two
    // units less than the raw bytes.
    let path = temp.0.join("mrk.usfm");
    fs::write(&path, "\\id MRK\n\\c 1\n\\p\n\\v 1 An 🧅 \\\\ here.\n").unwrap();
    let target = load_input(&path, false).unwrap();
    let corpus = Corpus::try_new(&target.books).unwrap();
    let (findings, patterns, _) = brigade_findings(&corpus, &[], false, None);
    // The one-verse book rosters every glyph it holds, so the pair rides
    // beside a handful of rarity sites.
    let hygiene: Vec<_> = findings
        .iter()
        .filter(|row| matches!(row.kind(), FindingKind::Hygiene(_)))
        .collect();
    assert_eq!(hygiene.len(), 1);
    let FindingKind::Hygiene(digest) = hygiene[0].kind() else {
        panic!("hygiene kind")
    };
    assert_eq!(digest.class(), HygieneClass::StrandedBackslash);
    assert_eq!(digest.run(), 2);
    assert_eq!((hygiene[0].from(), hygiene[0].to()), (10, 12));

    let buffer = publish(&target.paths, target.sources, &findings, &patterns).unwrap();
    let snapshot = CorpusSnapshot::open(&buffer).unwrap();
    assert_eq!(snapshot.coordinate_space(), CoordinateSpace::Utf16);
    let book = snapshot.book(hygiene[0].book_idx()).unwrap();
    let row = (0..book.len())
        .map(|at| book.at(at).unwrap())
        .find(|row| matches!(row.kind(), FindingKind::Hygiene(_)))
        .expect("the pair is a hygiene row");
    // Raw bytes 29..31; the onion at 24..28 is two UTF-16 units.
    assert_eq!((row.from(), row.to()), (27, 29));
}

#[test]
fn mismatched_single_books_are_reported_as_alignment_facts() {
    let temp = TempDir::new();
    let target_path = temp.0.join("target.usfm");
    let source_path = temp.0.join("source.usfm");
    fs::write(&target_path, MRK).unwrap();
    fs::write(&source_path, GEN).unwrap();
    let target = load_input(&target_path, false).unwrap();
    let source = load_input(&source_path, false).unwrap();
    let target_corpus = Corpus::try_new(&target.books).unwrap();
    let source_corpus = Corpus::try_new(&source.books).unwrap();
    let alignment = align(&target_corpus, &source_corpus);

    assert!(alignment.units().is_empty());
    assert_eq!(alignment.facts().len(), 2);
}

#[test]
fn typos_groups_a_rare_word_under_the_frequent_word_it_might_be_a_slip_of() {
    let temp = TempDir::new();
    let path = temp.0.join("mrk.usfm");
    let body = "says ".repeat(205);
    fs::write(&path, format!("\\id MRK\n\\c 1\n\\p\n\\v 1 {body}saws.\n")).unwrap();
    let target = load_input(&path, false).unwrap();
    let corpus = Corpus::try_new(&target.books).unwrap();

    let words = typo_word_counts(&corpus);
    let config = sous_core::typos::TypoConfig::default();
    let groups = sous_core::typos::typo_candidates(&words, &config);
    let report = format_typo_report(&groups);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].target.0, "says");
    assert_eq!(groups[0].candidates[0].0, "saws");
    assert!(report.contains("says (205)"), "{report}");
    assert!(report.contains("saws (1)   MRK 1:1"), "{report}");
}
