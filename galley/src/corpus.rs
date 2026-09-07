//! A whole project in one buffer: the cold door's envelope.
//!
//! ```text
//! galley::corpus::plate(&[("GEN", gen_dish), ("EXO", exo_dish)])
//!
//! bytes[0..4]    "ONCP"            magic
//! bytes[4..8]    1                 format version
//! bytes[8..12]   2                 book count
//! bytes[12..16]  0                 reserved
//! bytes[16..]    [keyAt, keyLen, dishAt, dishLen] x 2, then the bodies
//! ```
//!
//! Every dish inside is exactly what [`onion::wire::plate`] writes for one
//! book, unchanged — the envelope frames, it does not re-encode. So a consumer
//! reads a project with the same reader it reads a single parse with, and the
//! cold door and the editing door decode through one implementation.
//!
//! # The key is yours
//!
//! Onion never parses a key. It is whatever the caller identifies a book by —
//! a slug, a path, a row id — carried back verbatim so the answer can be
//! matched to the question. A key that means something to onion would be a key
//! onion could disagree with the caller about.
//!
//! # TODO: `load_corpus` should seed the warmer, and carry checksums
//!
//! Nothing here is wired to [`crate::Warmer`] yet, and it should be. A cold
//! load parses every book in the project — which is exactly the work the
//! warmer would otherwise do lazily, one book at a time, as an editor opens
//! them. Throwing it away means paying for it twice.
//!
//! A `load_corpus` that owned the read-and-parse would:
//!
//! - **checksum while it is there.** Galley already computes a per-chunk
//!   checksum for the warmer (xxh3-128); the cold load has every chunk in hand
//!   and is the natural place to do it.
//! - **seed the cache from what it just built,** so the first book an editor
//!   opens is already warm rather than a full miss.
//! - **carry a per-book checksum in the envelope,** so a consumer holding a
//!   previous load can tell which books actually changed without re-reading
//!   them, and a warm start can validate rather than assume.
//!
//! That last one is a FORMAT change, not just an addition: an entry is 16
//! bytes today and an xxh3-128 is another 16, so it bumps
//! [`FORMAT_VERSION`]. The header's `reserved` word is not big enough to hold
//! it, and should not be spent pretending otherwise.
//!
//! Deliberately not built yet — the envelope is useful without it, and what a
//! consumer wants back from a cold load (counts, or every finding) is still
//! open. Deciding that first avoids designing the checksum into the wrong
//! shape.
//!
//! # Parallelism is the caller's
//!
//! The expensive half is the parse, and it is embarrassingly parallel per book
//! (measured: 28.80 ms across 10 threads against 158.38 ms serial for the whole
//! English Bible). This takes dishes already plated, so a caller reads and
//! parses however it likes — `rayon`, a thread pool, a work queue — and galley
//! adds no dependency to frame the result.

use crate::onion;

/// `"ONCP"`, little-endian — a corpus, not a single parse.
pub const MAGIC: u32 = u32::from_le_bytes(*b"ONCP");

/// Bumped when this envelope's shape changes. Independent of the per-book
/// wire's version, which each dish carries for itself.
pub const FORMAT_VERSION: u32 = 1;

const HEADER_BYTES: usize = 16;
const ENTRY_BYTES: usize = 16;

/// One book's key and its plated parse.
pub struct Book<'a> {
    pub key: &'a str,
    pub dish: Vec<u8>,
}

/// Frame plated books into one buffer.
pub fn plate(books: &[Book<'_>]) -> Vec<u8> {
    let directory = HEADER_BYTES + books.len() * ENTRY_BYTES;
    let body: usize = books
        .iter()
        .map(|b| align4(b.key.len()) + align4(b.dish.len()))
        .sum();
    let mut out = Vec::with_capacity(directory + body);

    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(books.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    // Keys and dishes both 4-aligned, so a reader may take a typed-array view
    // over any dish's sections without copying it out first.
    let mut at = directory;
    for book in books {
        out.extend_from_slice(&(at as u32).to_le_bytes());
        out.extend_from_slice(&(book.key.len() as u32).to_le_bytes());
        at += align4(book.key.len());
        out.extend_from_slice(&(at as u32).to_le_bytes());
        out.extend_from_slice(&(book.dish.len() as u32).to_le_bytes());
        at += align4(book.dish.len());
    }

    for book in books {
        out.extend_from_slice(book.key.as_bytes());
        out.resize(align4(out.len()), 0);
        out.extend_from_slice(&book.dish);
        out.resize(align4(out.len()), 0);
    }
    out
}

/// Parse each book's text and frame the results — the one-call form for a
/// caller with no reason to hold the parses.
///
/// SERIAL. A project of any size wants the parse parallelised, which is the
/// caller's to do: plate each book with [`onion::wire::plate`] on whatever
/// threads it likes and hand the dishes to [`plate`].
pub fn plate_texts(books: &[(&str, &str)], opts: onion::wire::ParseOptions) -> Vec<u8> {
    let plated: Vec<Book<'_>> = books
        .iter()
        .map(|(key, text)| Book {
            key,
            dish: onion::wire::plate(&onion::wire::parse(text, opts)),
        })
        .collect();
    plate(&plated)
}

const fn align4(n: usize) -> usize {
    (n + 3) & !3
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEN: &str = "\\id GEN\n\\c 1\n\\p \\v 1 In the beginning.\n";
    const EXO: &str = "\\id EXO\n\\c 1\n\\p \\v 1 These are the names.\n";

    fn word(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn the_envelope_frames_without_re_encoding() {
        let opts = onion::wire::ParseOptions {
            toc: true,
            ..Default::default()
        };
        let genesis = onion::wire::plate(&onion::wire::parse(GEN, opts));
        let exo = onion::wire::plate(&onion::wire::parse(EXO, opts));
        let corpus = plate_texts(&[("GEN", GEN), ("EXO", EXO)], opts);

        assert_eq!(word(&corpus, 0), MAGIC);
        assert_eq!(word(&corpus, 4), FORMAT_VERSION);
        assert_eq!(word(&corpus, 8), 2);

        // Each dish inside is byte-identical to the one plated alone.
        for (n, (key, want)) in [("GEN", &genesis), ("EXO", &exo)].iter().enumerate() {
            let entry = HEADER_BYTES + n * ENTRY_BYTES;
            let key_at = word(&corpus, entry) as usize;
            let key_len = word(&corpus, entry + 4) as usize;
            let dish_at = word(&corpus, entry + 8) as usize;
            let dish_len = word(&corpus, entry + 12) as usize;
            assert_eq!(&corpus[key_at..key_at + key_len], key.as_bytes());
            assert_eq!(&corpus[dish_at..dish_at + dish_len], want.as_slice());
            assert_eq!(dish_at % 4, 0, "a dish must be 4-aligned to view");
        }
    }

    #[test]
    fn an_empty_corpus_is_a_header() {
        let bytes = plate(&[]);
        assert_eq!(bytes.len(), HEADER_BYTES);
        assert_eq!(word(&bytes, 8), 0);
    }

    /// Two books under keys a reader has to carry verbatim — a bare code and a
    /// path with a spaced em dash — plated with every optional section on.
    ///
    /// The same bytes are dropped in `target/` for hand-checking a reader
    /// against what THIS writer produces; the assertions are the test.
    #[test]
    fn a_two_book_corpus_plates_both_keys_and_both_dishes_aligned() {
        let opts = onion::wire::ParseOptions {
            toc: true,
            diagnostics: true,
            ..Default::default()
        };
        let keys = ["GEN", "books/02 — Exodus.usfm"];
        let bytes = plate_texts(&[(keys[0], GEN), (keys[1], EXO)], opts);
        assert_eq!(word(&bytes, 8), 2, "two entries");
        for (n, key) in keys.iter().enumerate() {
            let entry = HEADER_BYTES + n * ENTRY_BYTES;
            let key_at = word(&bytes, entry) as usize;
            let key_len = word(&bytes, entry + 4) as usize;
            let dish_at = word(&bytes, entry + 8) as usize;
            let dish_len = word(&bytes, entry + 12) as usize;
            assert_eq!(&bytes[key_at..key_at + key_len], key.as_bytes());
            assert!(dish_len > 0, "{key} plated a dish");
            assert_eq!(dish_at % 4, 0, "a dish must be 4-aligned to view");
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target");
        if dir.is_dir() {
            let _ = std::fs::write(dir.join("corpus-fixture.bin"), &bytes);
        }
    }

    /// The key is carried, never interpreted — a path, a slug, whatever the
    /// caller identified the book by comes back as it went in.
    #[test]
    fn the_key_is_opaque() {
        let opts = onion::wire::ParseOptions::default();
        let weird = "books/01 — Genesis (draft).usfm";
        let corpus = plate_texts(&[(weird, GEN)], opts);
        let at = word(&corpus, HEADER_BYTES) as usize;
        let len = word(&corpus, HEADER_BYTES + 4) as usize;
        assert_eq!(
            std::str::from_utf8(&corpus[at..at + len]).unwrap(),
            weird,
            "the key survived verbatim"
        );
    }
}
