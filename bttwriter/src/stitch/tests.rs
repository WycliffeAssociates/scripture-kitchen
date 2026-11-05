//! Sefer has no tests for `writerBook.ts`, so these pin each behaviour its
//! doc comments promise. `tests/golden.rs` pins the bytes on real projects.

use super::*;

fn key(code: &str) -> BookKey {
    BookKey::new(code.as_bytes().try_into().unwrap())
}

fn tree(files: &[(&str, &str)]) -> WriterTree {
    tree_from_files(files.iter().copied())
}

#[test]
fn text_paths_are_the_title_and_the_numbered_chapter_files() {
    for path in [
        "front/title.txt",
        "01/title.txt",
        "01/01.txt",
        "150/150.txt",
        "01/TITLE.TXT",
        "01/Title.Txt",
        "00/title.txt",
    ] {
        assert!(is_text_path(path), "{path}");
    }
    for path in [
        "manifest.json",
        "LICENSE.md",
        ".git/HEAD",
        "Front/title.txt",
        "front/TITLE.txt",
        "front/intro.txt",
        "1/01.txt",
        "0001/01.txt",
        "01/1.txt",
        "01/0001.txt",
        "01/01.usfm",
        "01/01.txt.bak",
        "01/sub/01.txt",
        "/01/01.txt",
        "01/reference.txt",
        "a1/01.txt",
    ] {
        assert!(!is_text_path(path), "{path}");
    }
}

#[test]
fn title_is_each_non_empty_line_trimmed() {
    let t = tree(&[("front/title.txt", "  First Line \r\n\r\n\tSecond\n   \n")]);
    assert_eq!(t.title, ["First Line", "Second"]);
    assert!(t.chapters.is_empty());
}

#[test]
fn chapters_and_chunks_are_in_numeric_order() {
    let t = tree(&[
        ("10/01.txt", "\\c 10 \\v 1 ten"),
        ("02/05.txt", "\\v 5 two-five"),
        ("02/01.txt", "\\c 2 \\v 1 two-one"),
        ("02/title.txt", "2"),
        ("01/01.txt", "\\c 1 \\v 1 one"),
    ]);
    let numbers: Vec<u32> = t.chapters.iter().map(|c| c.number).collect();
    assert_eq!(numbers, [1, 2, 10]);
    assert_eq!(
        t.chapters[1].chunks,
        ["\\c 2 \\v 1 two-one", "\\v 5 two-five"]
    );
}

#[test]
fn front_matter_and_unnumbered_files_are_not_chapters() {
    let t = tree(&[
        ("00/title.txt", "Front"),
        ("00/01.txt", "\\v 1 front matter"),
        ("01/00.txt", "\\v 0 not a chunk"),
        ("01/01.txt", "\\v 1 one"),
        ("manifest.json", "{}"),
        ("01/notes.txt", "x"),
    ]);
    assert_eq!(t.chapters.len(), 1);
    assert_eq!(t.chapters[0].chunks, ["\\v 1 one"]);
}

#[test]
fn a_chapter_with_only_a_title_is_dropped() {
    let t = tree(&[("02/title.txt", "Chapter Two"), ("01/01.txt", "\\v 1 a")]);
    assert_eq!(t.chapters.len(), 1);
    assert_eq!(t.chapters[0].number, 1);
}

#[test]
fn chapter_labels_are_dropped_when_empty_or_the_number() {
    let t = tree(&[
        ("01/title.txt", "  \n"),
        ("01/01.txt", "a"),
        ("02/title.txt", " 2 \n"),
        ("02/01.txt", "b"),
        ("03/title.txt", "03"),
        ("03/01.txt", "c"),
        ("04/title.txt", "  Sura 4 \r\n"),
        ("04/01.txt", "d"),
        ("05/01.txt", "e"),
    ]);
    let labels: Vec<Option<&str>> = t.chapters.iter().map(|c| c.label.as_deref()).collect();
    // "03" is not the string "3", so it stays, as in Sefer.
    assert_eq!(labels, [None, None, Some("03"), Some("Sura 4"), None]);
}

#[test]
fn three_digit_and_two_digit_names_meet_on_one_number() {
    let t = tree(&[
        ("119/176.txt", "\\v 176 last"),
        ("119/01.txt", "\\c 119 \\v 1 first"),
        ("01/001.txt", "\\v 1 three digits"),
        ("001/01.txt", "\\v 1 other folder"),
    ]);
    let numbers: Vec<u32> = t.chapters.iter().map(|c| c.number).collect();
    assert_eq!(numbers, [1, 119]);
    // Equal chunk numbers keep the order they were given in.
    assert_eq!(
        t.chapters[0].chunks,
        ["\\v 1 three digits", "\\v 1 other folder"]
    );
    assert_eq!(
        t.chapters[1].chunks,
        ["\\c 119 \\v 1 first", "\\v 176 last"]
    );
}

#[test]
fn a_path_given_twice_reads_as_a_map_would() {
    let t = tree(&[
        ("01/01.txt", "first"),
        ("01/02.txt", "second"),
        ("01/01.txt", "replaced"),
    ]);
    assert_eq!(t.chapters[0].chunks, ["replaced", "second"]);
}

#[test]
fn the_template_writes_the_header_then_the_chapters() {
    let t = tree(&[
        ("front/title.txt", "Yohanes Tiga\nSurat\n"),
        ("01/title.txt", "Pasal Satu"),
        ("01/01.txt", "\\c 1 \\v 1 Dari penatua\n"),
        ("01/05.txt", "\n\\v 5 Saudaraku  \n"),
        ("02/01.txt", "\\c 2\n\\p\n\\v 1 Dua"),
    ]);
    assert_eq!(
        book_to_usfm(&t, key("3JN"), "3 John"),
        "\\id 3JN\n\
         \\ide UTF-8\n\
         \\h Yohanes Tiga\n\
         \\toc1 Yohanes Tiga\n\
         \\toc2 Yohanes Tiga\n\
         \\toc3 3jn\n\
         \\mt1 Yohanes Tiga\n\
         \\mt2 Surat\n\
         \\c 1\n\
         \\cl Pasal Satu\n\
         \\v 1 Dari penatua\n\
         \\v 5 Saudaraku\n\
         \\c 2\n\
         \\p\n\\v 1 Dua\n"
    );
}

#[test]
fn the_fallback_title_stands_in_for_a_missing_one() {
    let t = tree(&[("front/title.txt", " \n \n"), ("01/01.txt", "\\v 1 a")]);
    assert_eq!(
        book_to_usfm(&t, key("JUD"), "Jude"),
        "\\id JUD\n\\ide UTF-8\n\\h Jude\n\\toc1 Jude\n\\toc2 Jude\n\\toc3 jud\n\\mt1 Jude\n\\c 1\n\\v 1 a\n"
    );
}

#[test]
fn an_empty_book_is_its_header() {
    let t = WriterTree::default();
    assert_eq!(
        book_to_usfm(&t, key("REV"), "Revelation"),
        "\\id REV\n\\ide UTF-8\n\\h Revelation\n\\toc1 Revelation\n\\toc2 Revelation\n\\toc3 rev\n\\mt1 Revelation\n"
    );
}

#[test]
fn every_leading_chapter_marker_of_the_first_chunk_goes() {
    // Moffat_chola's bwc REV: `\c 3 \c 3`.
    let t = tree(&[
        ("03/01.txt", "  \\c 3 \\c 3\n\\v 1 text"),
        ("03/02.txt", "\\c 3 \\v 2 kept, not the first chunk"),
    ]);
    let usfm = book_to_usfm(&t, key("REV"), "Revelation");
    assert!(
        usfm.ends_with("\\c 3\n\\v 1 text\n\\c 3 \\v 2 kept, not the first chunk\n"),
        "{usfm}"
    );
}

#[test]
fn the_chapter_marker_strip_is_the_regex() {
    assert_eq!(without_chapter_marker("\\c 1 \\v 1 a"), "\\v 1 a");
    assert_eq!(without_chapter_marker("\\c 1\\c 1 \\v 1 a"), "\\v 1 a");
    assert_eq!(without_chapter_marker("\n\\c  12\n\n\\p"), "\\p");
    // Whatever number it says: it is Writer's framing either way.
    assert_eq!(without_chapter_marker("\\c 4 \\v 1 a"), "\\v 1 a");
    assert_eq!(without_chapter_marker("\\c 12abc"), "abc");
    // Not a chapter marker: no space, no number, another marker.
    assert_eq!(without_chapter_marker("\\c1 \\v 1"), "\\c1 \\v 1");
    assert_eq!(without_chapter_marker("\\c \\v 1"), "\\c \\v 1");
    assert_eq!(without_chapter_marker("\\cl Title"), "\\cl Title");
    assert_eq!(without_chapter_marker("\\v 1 \\c 2"), "\\v 1 \\c 2");
    // A marker the regex cannot take whole stays as it is, leading space too.
    assert_eq!(without_chapter_marker(" \\c 1 \\c x"), "\\c x");
    assert_eq!(without_chapter_marker(" \\cx"), " \\cx");
    assert_eq!(without_chapter_marker(""), "");
}

#[test]
fn a_chunk_that_is_only_its_marker_or_space_is_left_out() {
    let t = tree(&[
        ("01/01.txt", "\\c 1"),
        ("01/02.txt", "  \n"),
        ("01/03.txt", "\\v 3 c"),
    ]);
    assert!(book_to_usfm(&t, key("PHM"), "Philemon").ends_with("\\c 1\n\\v 3 c\n"));
}

#[test]
fn trimming_is_javascripts() {
    // U+FEFF is space to JavaScript; U+0085 is not.
    assert_eq!(js_trim("\u{FEFF} a \u{3000}"), "a");
    assert_eq!(js_trim("\u{0085}a"), "\u{0085}a");
    let t = tree(&[
        ("front/title.txt", "\u{FEFF}Title"),
        ("01/title.txt", "\u{0085}"),
    ]);
    assert_eq!(t.title, ["Title"]);
}

#[test]
fn decoding_is_sefers_decode() {
    assert_eq!(decode_text(b"a\r\nb\rc\nd").as_deref(), Some("a\nb\nc\nd"));
    assert_eq!(decode_text(b"\r\r\n").as_deref(), Some("\n\n"));
    assert_eq!(decode_text(b"\xEF\xBB\xBFtext").as_deref(), Some("text"));
    // Two marks: Sefer drops one, its TextDecoder the other.
    assert_eq!(
        decode_text(b"\xEF\xBB\xBF\xEF\xBB\xBFtext").as_deref(),
        Some("text")
    );
    assert_eq!(
        decode_text(b"\xEF\xBB\xBF\xEF\xBB\xBF\xEF\xBB\xBFtext").as_deref(),
        Some("\u{FEFF}text")
    );
    assert_eq!(decode_text(b"\xFF\xFEa"), None);
    assert_eq!(decode_text(b"\xED\xA0\x80"), None); // a lone surrogate
    assert_eq!(decode_text(b"").as_deref(), Some(""));
}

#[test]
fn stitch_reads_only_the_books_decodable_text() {
    let files: [(&str, &[u8]); 4] = [
        ("manifest.json", b"{\"project\": {}}"),
        ("01/01.txt", b"\xEF\xBB\xBF\\c 1 \\v 1 one\r\n"),
        ("01/02.txt", b"\xFF not utf-8"),
        ("front/title.txt", b"Title\r\n"),
    ];
    assert_eq!(
        stitch(files, key("PHM"), "Philemon"),
        "\\id PHM\n\\ide UTF-8\n\\h Title\n\\toc1 Title\n\\toc2 Title\n\\toc3 phm\n\\mt1 Title\n\\c 1\n\\v 1 one\n"
    );
}
