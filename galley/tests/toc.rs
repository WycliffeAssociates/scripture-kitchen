//! The census answers what a parse would have, off the pinned tier alone.
//!
//! VOLUME: the sweep below reads onion's whole test tier, so a row shape that
//! only appears in one translation still meets this.
//!
//! The buffer is decoded here by a hand-written mirror of the generated
//! reader. That duplication is the point: an independent decoder is what makes
//! "the two ends agree" a claim rather than a tautology, the way
//! `find::wire::tests::decode` reads find's buffer.

use std::path::Path;

use usfm_galley::onion;
use usfm_galley::toc::schema;
use usfm_galley::{BookId, Pantry, Retain, Role, SourceLanes};

const TIER: &str = "../testData/exampleCorpora";
const BUDGET: usize = 16 << 20;

// ---------------------------------------------------------------- the mirror

#[derive(Debug, PartialEq, Eq)]
struct Chapter {
    start: u32,
    end: u32,
    number: u16,
    anchors: u16,
    last_verse: u16,
}

#[derive(Debug, PartialEq, Eq)]
struct Verse {
    at: u32,
    chapter: u16,
    first: u16,
    last: u16,
}

#[derive(Debug)]
struct Book {
    code: String,
    id: String,
    chapters: Vec<Chapter>,
    verses: Vec<Verse>,
}

#[derive(Debug)]
struct Census {
    utf16: bool,
    books: Vec<Book>,
}

/// Read the buffer the way `toc-reader.ts` does, through the schema's own
/// constants and nothing hand-counted.
fn decode(bytes: &[u8]) -> Census {
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
    let half = |at: usize| u16::from_le_bytes(bytes[at..at + 2].try_into().expect("two bytes"));

    assert_eq!(word(schema::HEADER_MAGIC_OFFSET), schema::MAGIC, "TOCS");
    assert_eq!(word(schema::HEADER_VERSION_OFFSET), schema::FORMAT_VERSION);
    assert_eq!(
        word(schema::HEADER_CHAPTER_STRIDE_OFFSET) as usize,
        schema::CHAPTER.stride()
    );
    assert_eq!(
        word(schema::HEADER_VERSE_STRIDE_OFFSET) as usize,
        schema::VERSE.stride()
    );
    let flags = word(schema::HEADER_FLAGS_OFFSET);
    let count = word(schema::HEADER_BOOK_COUNT_OFFSET) as usize;
    let directory = word(schema::HEADER_DIRECTORY_AT_OFFSET) as usize;

    let mut books = Vec::with_capacity(count);
    for book in 0..count {
        let entry = directory + book * schema::DIRECTORY_ENTRY_BYTES;
        let code_at = entry + schema::DIRECTORY_CODE_OFFSET;
        assert_eq!(
            bytes[entry + schema::DIRECTORY_CODE_TERMINATOR_OFFSET],
            0,
            "the code's fourth byte is its NUL"
        );
        let code = String::from_utf8(
            bytes[code_at..code_at + 3]
                .iter()
                .copied()
                .take_while(|byte| *byte != 0)
                .collect(),
        )
        .expect("the encoder wrote UTF-8");

        let id_at = word(entry + schema::DIRECTORY_ID_AT_OFFSET) as usize;
        let id_len = word(entry + schema::DIRECTORY_ID_LEN_OFFSET) as usize;
        let id = String::from_utf8(bytes[id_at..id_at + id_len].to_vec()).expect("UTF-8");

        let chapters_at = word(entry + schema::DIRECTORY_CHAPTERS_AT_OFFSET) as usize;
        let chapter_rows = word(entry + schema::DIRECTORY_CHAPTER_ROWS_OFFSET) as usize;
        let chapters = (0..chapter_rows)
            .map(|row| {
                let at = chapters_at + row * schema::CHAPTER.stride();
                Chapter {
                    start: word(at + schema::CHAPTER.offset_of("start")),
                    end: word(at + schema::CHAPTER.offset_of("end")),
                    number: half(at + schema::CHAPTER.offset_of("number")),
                    anchors: half(at + schema::CHAPTER.offset_of("anchors")),
                    last_verse: half(at + schema::CHAPTER.offset_of("lastVerse")),
                }
            })
            .collect();

        let verses_at = word(entry + schema::DIRECTORY_VERSES_AT_OFFSET) as usize;
        let verse_rows = word(entry + schema::DIRECTORY_VERSE_ROWS_OFFSET) as usize;
        let verses = (0..verse_rows)
            .map(|row| {
                let at = verses_at + row * schema::VERSE.stride();
                Verse {
                    at: word(at + schema::VERSE.offset_of("at")),
                    chapter: half(at + schema::VERSE.offset_of("chapter")),
                    first: half(at + schema::VERSE.offset_of("first")),
                    last: half(at + schema::VERSE.offset_of("last")),
                }
            })
            .collect();

        books.push(Book {
            code,
            id,
            chapters,
            verses,
        });
    }
    Census {
        utf16: flags & schema::FLAG_UTF16 != 0,
        books,
    }
}

fn usfm_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("the committed corpus tier is readable") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            out.extend(usfm_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            out.push(path);
        }
    }
    out.sort();
    out
}

// ----------------------------------------------------------------- the claims

/// THE claim: for every book in the tier, the census says exactly what a full
/// parse's TOC says. A consumer that took the expensive route would learn
/// nothing this buffer does not carry.
#[test]
fn the_census_says_what_a_parse_says() {
    let files = usfm_files(Path::new(TIER));
    assert!(
        files.len() > 100,
        "the example tier is 160 books; found {}",
        files.len()
    );

    let mut pantry = Pantry::new(BUDGET);
    let mut ids = Vec::new();
    let mut texts = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("readable");
        let id = path.to_string_lossy().into_owned();
        // A book with no `\id` line cannot be keyed, and is not this test's
        // subject; it is counted below so the sweep cannot go quietly empty.
        if pantry.update(id.as_str(), Role::Target, &text).is_ok() {
            ids.push(BookId::from(id.as_str()));
            texts.push(text);
        }
    }
    assert!(
        ids.len() > 100,
        "only {} of {} books registered",
        ids.len(),
        files.len()
    );

    let census = decode(&usfm_galley::toc::encode(&pantry, &ids, false).expect("bytes"));
    assert_eq!(census.books.len(), ids.len());
    assert!(!census.utf16);

    let options = onion::wire::ParseOptions {
        toc: true,
        ..Default::default()
    };
    for ((book, id), text) in census.books.iter().zip(&ids).zip(&texts) {
        let parsed = onion::wire::parse(text, options);
        let toc = parsed.toc.as_ref().expect("toc requested");
        assert_eq!(&book.id, id.as_str());
        assert_eq!(book.code.as_bytes(), &toc.book[..book.code.len()]);
        assert_eq!(
            book.chapters.len(),
            toc.chapters.len(),
            "{id}: chapter rows"
        );
        assert_eq!(book.verses.len(), toc.verses.len(), "{id}: verse rows");

        for (row, source) in book.chapters.iter().zip(&toc.chapters) {
            assert_eq!(
                (row.start, row.end, row.number),
                (source.start, source.end, source.number),
                "{id}: chapter row {}",
                source.number
            );
        }
        for (row, source) in book.verses.iter().zip(&toc.verses) {
            assert_eq!(
                (row.at, row.first, row.last),
                (source.at, source.first, source.last),
                "{id}: verse anchor at {}",
                source.at
            );
            // The census keys a verse by the chapter row CONTAINING it; onion
            // keys it by the enclosing row it walked. The two must agree, or
            // one of them is reading a different book than it thinks.
            assert_eq!(row.chapter, source.chapter, "{id}: anchor at {}", source.at);
        }
    }
}

/// The three laws the rows carry: chapters tile the book, verses ascend, and
/// each chapter's two counts are the arithmetic its own anchors make.
#[test]
fn the_rows_obey_the_tocs_own_laws() {
    let files = usfm_files(Path::new(TIER));
    let mut pantry = Pantry::new(BUDGET);
    let mut ids = Vec::new();
    let mut lengths = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("readable");
        let id = path.to_string_lossy().into_owned();
        if pantry.update(id.as_str(), Role::Target, &text).is_ok() {
            ids.push(BookId::from(id.as_str()));
            lengths.push(text.len() as u32);
        }
    }
    assert!(!ids.is_empty(), "the tier registered nothing");
    let census = decode(&usfm_galley::toc::encode(&pantry, &ids, false).expect("bytes"));

    for ((book, id), len) in census.books.iter().zip(&ids).zip(&lengths) {
        assert_eq!(book.chapters[0].start, 0, "{id}: row 0 opens the book");
        assert_eq!(
            book.chapters[0].number, 0,
            "{id}: row 0 is the front matter"
        );
        for pair in book.chapters.windows(2) {
            assert_eq!(pair[0].end, pair[1].start, "{id}: the rows leave a hole");
        }
        assert_eq!(
            book.chapters.last().expect("a row").end,
            *len,
            "{id}: the last row runs to the end"
        );

        for pair in book.verses.windows(2) {
            assert!(pair[0].at <= pair[1].at, "{id}: anchors are out of order");
        }

        let mut walked = 0usize;
        for row in &book.chapters {
            let mine: Vec<&Verse> = book.verses[walked..]
                .iter()
                .take_while(|verse| verse.at < row.end)
                .collect();
            assert_eq!(row.anchors as usize, mine.len(), "{id}: anchors in a row");
            assert_eq!(
                row.last_verse,
                mine.iter().map(|verse| verse.last).max().unwrap_or(0),
                "{id}: lastVerse in a row"
            );
            walked += mine.len();
        }
        assert_eq!(walked, book.verses.len(), "{id}: an anchor sits in no row");
    }
}

/// A reference that kept no text kept no table either, so `utf16` is the one
/// thing it cannot answer — and it says so by name rather than handing back
/// bytes labelled as code units.
#[test]
fn utf16_over_a_textless_reference_refuses_by_name() {
    const TARGET: &str = "\\id GEN\n\\c 1\n\\p \\v 1 In the beginning.\n";
    const SOURCE: &str = "\\id GEN\n\\c 1\n\\p \\v 1 Im Anfang.\n";

    let mut pantry = Pantry::new(BUDGET);
    pantry
        .update("books/GEN.usfm", Role::Target, TARGET)
        .expect("a target");
    pantry
        .update_with(
            "ref/GEN.usfm",
            Role::Reference,
            Retain::ProductsOnly,
            SourceLanes::Lengths,
            SOURCE,
        )
        .expect("a reference");
    let ids = [BookId::from("books/GEN.usfm"), BookId::from("ref/GEN.usfm")];

    // Both books are in the census: a reference keeps its Toc whatever else it
    // drops, which is the whole reason this door reaches further than `find`.
    let bytes = usfm_galley::toc::encode(&pantry, &ids, false).expect("bytes");
    let census = decode(&bytes);
    assert_eq!(census.books.len(), 2);
    assert_eq!(census.books[1].chapters.len(), 2);

    let refused = usfm_galley::toc::encode(&pantry, &ids, true).expect_err("no table");
    assert!(
        refused.to_string().contains("ref/GEN.usfm"),
        "the refusal names the book: {refused}"
    );

    // The target alone answers in UTF-16, and its offsets are the table's.
    let target = decode(&usfm_galley::toc::encode(&pantry, &ids[..1], true).expect("bytes"));
    assert!(target.utf16);
    let table = mise::utf16::utf16_table(TARGET.as_bytes());
    assert_eq!(
        target.books[0].chapters.last().expect("a row").end,
        table.len_utf16()
    );
}

/// An id nobody registered contributes nothing and is not listed. The
/// directory names what was FOUND, so a caller comparing its own list against
/// `bookCount` sees the difference; the wasm door refuses such an id by name
/// before it ever reaches the encoder.
#[test]
fn an_unregistered_id_is_not_in_the_directory() {
    let mut pantry = Pantry::new(BUDGET);
    pantry
        .update("books/GEN.usfm", Role::Target, "\\id GEN\n\\c 1\n\\v 1 a\n")
        .expect("a target");
    let ids = [
        BookId::from("books/NOPE.usfm"),
        BookId::from("books/GEN.usfm"),
    ];
    let census = decode(&usfm_galley::toc::encode(&pantry, &ids, false).expect("bytes"));
    assert_eq!(census.books.len(), 1);
    assert_eq!(census.books[0].id, "books/GEN.usfm");

    let empty = decode(&usfm_galley::toc::encode(&pantry, &[], false).expect("bytes"));
    assert!(empty.books.is_empty(), "an empty call is an empty census");
}
