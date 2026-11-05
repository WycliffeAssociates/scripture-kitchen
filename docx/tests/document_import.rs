//! The best-effort path against `@sefer/document-import`, whose rules it
//! ports: its own three fixtures, and the USFM it writes for them.
//!
//! ```text
//! fixtures/master-titus.docx   a real draft: superscript numbers split across runs,
//!                              blank paragraphs, "Chapter N" lines, a title line
//! fixtures/titus.docx          the same text, verse numbers on a raised baseline
//! fixtures/titus.odt           the same text from Writer
//! fixtures/titus.document-import.usfm   what document-import writes for all three
//! ```
//!
//! The golden is document-import's output, not ours: a difference is either a
//! bug in the port or a rule changed on purpose, and the second needs saying.

use docx::{Settings, VerseMode, VersePoint};

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn titus() -> Settings {
    Settings { book: Some("TIT".to_owned()), ..Settings::default() }
}

#[test]
fn word_word_raised_and_writer_all_write_document_imports_usfm() {
    let golden = String::from_utf8(fixture("titus.document-import.usfm")).unwrap();
    for name in ["master-titus.docx", "titus.docx", "titus.odt"] {
        let out = docx::import(&fixture(name), &titus()).unwrap();
        assert_eq!(out.books.len(), 1, "{name}");
        assert_eq!(out.books[0].usfm, golden, "{name}");
        assert_eq!(out.report.detected, VerseMode::Superscript, "{name}");
        let chapters: Vec<(u32, usize, usize)> =
            out.report.chapters.iter().map(|c| (c.chapter, c.verses, c.missing.len())).collect();
        assert_eq!(chapters, [(1, 16, 0), (2, 15, 0), (3, 15, 0)], "{name}");
    }
}

#[test]
fn the_documents_own_paragraphs_are_kept_and_the_title_line_too() {
    let usfm = docx::import(&fixture("master-titus.docx"), &titus()).unwrap().books.remove(0).usfm;
    assert!(usfm.starts_with("\\id TIT\n\\rem MASTER:\n\\c 1\n"), "{usfm}");
    let chapter_one = &usfm[..usfm.find("\\c 2").unwrap()];
    let starts: Vec<&str> = chapter_one
        .split("\\p\n\\v ")
        .skip(1)
        .map(|rest| rest.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(starts, ["1", "4", "5", "10"]);
}

#[test]
fn a_comment_lands_on_the_same_words_from_word_and_from_writer() {
    for name in ["titus.docx", "titus.odt"] {
        let out = docx::import(&fixture(name), &Settings::default()).unwrap();
        let threads = &out.books[0].comments;
        assert_eq!(threads.len(), 1, "{name}");
        let t = &threads[0];
        assert_eq!(t.r#ref.as_deref(), Some("1:1"), "{name}");
        assert_eq!(t.start, Some(VersePoint { r#ref: "1:1".to_owned(), offset: 23 }), "{name}");
        assert_eq!(t.end, Some(VersePoint { r#ref: "1:1".to_owned(), offset: 40 }), "{name}");
        assert_eq!(t.anchor_text, "and an apostle of", "{name}");
    }
}
