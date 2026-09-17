//! What the mask map promises about the projection it describes.
//!
//! Two instruments in one file, and the module says which is which per test:
//!
//! - SHAPES — the sous fixtures through a real `Pantry`, for the claims a
//!   corpus cannot state cleanly: the map a find hit's pieces must agree with,
//!   in both units, and a loose cut of a registered book's own text answering
//!   the resident door's buffer byte for byte.
//! - VOLUME — the whole test tier, `testData/exampleCorpora` (160 books,
//!   12.8 MB): every book's ranges obey the five laws and concatenate to its
//!   verse text, in bytes and in UTF-16.
//!
//! Absent corpus bytes are a loud failure, never a silent skip.

use std::path::PathBuf;

use mise::utf16::{Utf16Table, utf16_len, utf16_table};
use usfm_galley::mask::{Recipe, encode, pieces, schema};
use usfm_galley::onion::mask::Mask;
use usfm_galley::onion::{cst, lex, mask as cut};
use usfm_galley::{BookId, Find, Pantry, Role};

// ------------------------------------------------------------------ fixtures

/// One book's mask, built through the real pipeline.
fn project(source: &str, recipe: Recipe) -> Mask {
    let tokens = lex(source);
    let tree = cst::build(&tokens);
    cut(source.as_bytes(), &tokens, &tree, &recipe.filter())
}

/// Every `.usfm` in the test tier, path and text.
fn tier() -> Vec<(PathBuf, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testData/exampleCorpora");
    let mut books = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "usfm") {
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                books.push((path, text));
            }
        }
    }
    assert!(!books.is_empty(), "no *.usfm under {}", root.display());
    books
}

/// One sous fixture's text.
fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sous")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

// ------------------------------------------------------------- the decoder

/// The buffer read back the way a host reads it — the independent oracle for
/// the writer, and the algorithm `mask-reader.ts` runs, restated here so a
/// native test pins it before a consumer does.
struct Map {
    flags: u32,
    recipe: u32,
    source_len: u32,
    projected_len: u32,
    ranges: Vec<(u32, u32)>,
    starts: Vec<u32>,
}

impl Map {
    fn open(bytes: &[u8]) -> Self {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four"));
        assert_eq!(word(schema::HEADER_MAGIC_OFFSET), schema::MAGIC);
        assert_eq!(word(schema::HEADER_VERSION_OFFSET), schema::FORMAT_VERSION);
        let count = word(schema::HEADER_RANGE_COUNT_OFFSET) as usize;
        assert_eq!(
            bytes.len(),
            schema::HEADER_BYTES + count * schema::RANGE.stride()
        );
        let mut ranges = Vec::with_capacity(count);
        let mut starts = Vec::with_capacity(count);
        let mut at = 0u32;
        for n in 0..count {
            let row = schema::HEADER_BYTES + n * schema::RANGE.stride();
            let (from, to) = (word(row), word(row + 4));
            starts.push(at);
            at += to - from;
            ranges.push((from, to));
        }
        Self {
            flags: word(schema::HEADER_FLAGS_OFFSET),
            recipe: word(schema::HEADER_RECIPE_OFFSET),
            source_len: word(schema::HEADER_SOURCE_LEN_OFFSET),
            projected_len: word(schema::HEADER_PROJECTED_LEN_OFFSET),
            ranges,
            starts,
        }
    }

    /// Laws 1, 2, 3 and 5. Law 4 is per-unit and stated by its caller.
    fn laws(&self, what: &str) {
        let mut at = 0u32;
        for (n, (from, to)) in self.ranges.iter().enumerate() {
            assert!(from < to, "{what}: range {n} is empty");
            assert!(
                *to <= self.source_len,
                "{what}: range {n} runs past the source"
            );
            assert_eq!(
                self.starts[n], at,
                "{what}: range {n}'s start is not the prefix sum"
            );
            if n > 0 {
                let previous = self.ranges[n - 1].1;
                assert!(
                    previous < *from,
                    "{what}: range {n} is not strictly after {}",
                    n - 1
                );
            }
            at += to - from;
        }
        assert_eq!(
            at, self.projected_len,
            "{what}: the prefix sum is the projection"
        );
    }

    /// The reader's `pieces`, restated: binary search on `starts`, then walk.
    fn pieces(&self, from: u32, to: u32) -> Vec<(u32, u32)> {
        if to <= from || from >= self.projected_len {
            return Vec::new();
        }
        let mut row = self
            .starts
            .partition_point(|start| *start <= from)
            .saturating_sub(1);
        let mut cursor = from;
        let mut out = Vec::new();
        while cursor < to && row < self.ranges.len() {
            let (source_from, source_to) = self.ranges[row];
            let start = self.starts[row];
            let end = start + (source_to - source_from);
            let stop = to.min(end);
            out.push((source_from + (cursor - start), source_from + (stop - start)));
            cursor = stop;
            row += 1;
        }
        out
    }
}

/// The find buffer's hits, as `(projectedFrom, projectedTo, pieces)`.
type WireHit = (u32, u32, Vec<(u32, u32)>);

fn find_hits(bytes: &[u8]) -> Vec<WireHit> {
    let word = |at: usize| u32::from_le_bytes(bytes[at * 4..at * 4 + 4].try_into().expect("four"));
    let count = word(2) as usize;
    let mut at = 4;
    let mut hits = Vec::with_capacity(count);
    for _ in 0..count {
        let (from, to) = (word(at + 1), word(at + 2));
        let runs = word(at + 3) as usize;
        at += 4;
        let spans = (0..runs)
            .map(|n| (word(at + n * 2), word(at + n * 2 + 1)))
            .collect();
        at += runs * 2;
        hits.push((from, to, spans));
    }
    hits
}

// ---------------------------------------------------------------- the laws

/// VOLUME. Every book in the tier, both recipes, both units: the ranges obey
/// the laws and their source slices ARE the projection — nothing inserted.
#[test]
fn every_book_s_ranges_concatenate_to_its_projection() {
    for (path, text) in tier() {
        let what = path.display().to_string();
        let table = utf16_table(text.as_bytes());
        let units: Vec<u16> = text.encode_utf16().collect();
        for recipe in [Recipe::VerseText, Recipe::Structure] {
            let mask = project(&text, recipe);
            let projected = mask.text(text.as_bytes());

            let bytes = Map::open(&encode(&mask, recipe, None, text.len() as u32));
            bytes.laws(&what);
            assert_eq!(bytes.flags, 0);
            assert_eq!(bytes.recipe, recipe.word());
            assert_eq!(bytes.source_len, text.len() as u32);
            let joined: String = bytes
                .ranges
                .iter()
                .map(|(from, to)| &text[*from as usize..*to as usize])
                .collect();
            assert_eq!(
                joined, projected,
                "{what}: the byte slices are the projection"
            );
            assert_eq!(bytes.projected_len as usize, projected.len());

            let wide = Map::open(&encode(
                &mask,
                recipe,
                Some(&table),
                utf16_len(text.as_bytes()),
            ));
            wide.laws(&what);
            assert_eq!(wide.flags, schema::FLAG_UTF16);
            assert_eq!(wide.source_len, utf16_len(text.as_bytes()));
            let joined: Vec<u16> = wide
                .ranges
                .iter()
                .flat_map(|(from, to)| units[*from as usize..*to as usize].iter().copied())
                .collect();
            assert_eq!(
                String::from_utf16(&joined).expect("ranges fall on character boundaries"),
                projected,
                "{what}: the UTF-16 slices are the same projection"
            );
            assert_eq!(wide.projected_len as usize, joined.len());
            assert_eq!(
                wide.ranges.len(),
                bytes.ranges.len(),
                "{what}: one map, two units"
            );
        }
    }
}

/// SHAPES. Every hit find publishes names the source pieces the map does, in
/// the projected span the same buffer carries — the cross-check that pins the
/// reader's algorithm against the one find already ships.
#[test]
fn every_find_hit_s_pieces_are_the_map_s() {
    let mut pantry = Pantry::new(1 << 24);
    let books = ["GEN.usfm", "RUT.usfm", "JON.usfm"];
    let ids: Vec<BookId> = books.iter().map(|name| BookId::from(*name)).collect();
    for name in books {
        pantry
            .update(name, Role::Target, &fixture(name))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }

    let find = Find::literal("the").case_insensitive(false);
    let mut checked = 0;
    for (id, name) in ids.iter().zip(books) {
        let text = fixture(name);
        let mask = project(&text, Recipe::VerseText);
        let table = utf16_table(text.as_bytes());

        // Bytes: the engine's own hits, against the byte map.
        for hit in find.in_projection(&mask, text.as_bytes()) {
            let mine = pieces(&mask, hit.projected.start, hit.projected.end);
            let theirs: Vec<_> = hit.source.pieces().cloned().collect();
            assert_eq!(mine, theirs, "{name}: byte pieces");
            checked += 1;
        }

        // UTF-16: the published buffer's hits, against the UTF-16 map.
        let wire = usfm_galley::find::wire::encode(&mut pantry, std::slice::from_ref(id), &find, 0);
        let map = Map::open(&encode(
            &mask,
            Recipe::VerseText,
            Some(&table),
            utf16_len(text.as_bytes()),
        ));
        let hits = find_hits(&wire);
        assert!(!hits.is_empty(), "{name}: the fixture holds the needle");
        for (from, to, spans) in hits {
            assert_eq!(map.pieces(from, to), spans, "{name}: UTF-16 pieces");
        }
    }
    assert!(checked > 0, "the fixtures hold the needle");
}

/// SHAPES. A loose cut of a registered book's own text is the resident door's
/// buffer, byte for byte — both recipes, both units. The two doors differ in
/// where the mask and the table come from, never in what they write.
#[test]
fn a_loose_cut_answers_what_the_resident_one_does() {
    let mut pantry = Pantry::new(1 << 24);
    for name in ["GEN.usfm", "RUT.usfm", "JON.usfm"] {
        let text = fixture(name);
        pantry
            .update(name, Role::Target, &text)
            .unwrap_or_else(|error| panic!("{name}: {error}"));

        let id = BookId::from(name);
        let resident_mask = pantry
            .book(&id)
            .expect("registered")
            .mask()
            .expect("a target")
            .clone();
        let resident_table: Utf16Table = pantry
            .book(&id)
            .expect("registered")
            .utf16()
            .expect("a target")
            .clone();
        let published = pantry
            .book(&id)
            .expect("registered")
            .published_len()
            .expect("a target");

        for recipe in [Recipe::VerseText, Recipe::Structure] {
            // The resident door serves verseText off the retained projection
            // and cuts anything else; the loose door always cuts.
            let resident = match recipe {
                Recipe::VerseText => resident_mask.clone(),
                Recipe::Structure => project(&text, recipe),
            };
            let loose = project(&text, recipe);
            assert_eq!(
                encode(&resident, recipe, None, text.len() as u32),
                encode(&loose, recipe, None, text.len() as u32),
                "{name}: {recipe:?} in bytes"
            );
            assert_eq!(
                encode(&resident, recipe, Some(&resident_table), published),
                encode(
                    &loose,
                    recipe,
                    Some(&utf16_table(text.as_bytes())),
                    utf16_len(text.as_bytes())
                ),
                "{name}: {recipe:?} in UTF-16"
            );
        }
    }
}
