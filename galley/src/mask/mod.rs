//! The mask map: WHICH source spans the projection is made of, as a buffer.
//!
//! ```text
//! source  \v 1 Jesus wept.\f + \ft why\f* Then he rose.
//!
//! galley::mask::encode(&mask, Recipe::VerseText, None, source.len() as u32)
//!
//!   MASK v1  flags 0  recipe 0  ranges 2  sourceLen 50  projectedLen 27
//!   range 0   5..16      "Jesus wept."
//!   range 1   36..52     " Then he rose."
//! ```
//!
//! The projection is a pure CONCATENATION of those slices — nothing is
//! inserted between two ranges — so a host holding the source rebuilds the
//! reading from the map alone, and maps a projected offset back to the byte
//! an edit has to land on. `galley/src/mask.md` is the contract; [`schema`] is
//! the one declaration both ends are generated from.

pub mod emit;
pub mod generated;
pub mod schema;

use core::ops::Range;

use mise::utf16::Utf16Table;

use crate::onion::mask::{Filter, Mask};

/// Which cut a map describes. The buffer carries it, so a consumer caching
/// both cannot confuse them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Recipe {
    #[default]
    VerseText,
    Structure,
}

impl Recipe {
    /// The header word.
    pub const fn word(self) -> u32 {
        match self {
            Self::VerseText => schema::RECIPE_VERSE_TEXT,
            Self::Structure => schema::RECIPE_STRUCTURE,
        }
    }

    /// The filter that cuts it — the one place the two doors agree about what
    /// a recipe name means.
    pub fn filter(self) -> Filter {
        match self {
            Self::VerseText => Filter::verse_text(),
            Self::Structure => Filter::structure(),
        }
    }
}

/// One mask as the module's buffer.
///
/// `source_len` is the length of the text the mask was cut from, already in
/// the unit `table` implies: bytes when it is `None`, UTF-16 units when it is
/// not. With a table every offset is rebased through it in one sweep, and the
/// projected length is summed from the ranges AFTER that sweep — a surrogate
/// pair inside a kept span is two units where it was four bytes.
pub fn encode(mask: &Mask, recipe: Recipe, table: Option<&Utf16Table>, source_len: u32) -> Vec<u8> {
    let mut rows = Vec::with_capacity(mask.ranges.len() * schema::RANGE.stride());
    let mut offsets = generated::Offsets::new();
    generated::write_ranges(&mask.ranges, &mut rows, &mut offsets);
    if let Some(table) = table {
        for &at in &offsets {
            let raw = u32::from_le_bytes(rows[at..at + 4].try_into().expect("four bytes"));
            rows[at..at + 4].copy_from_slice(&table.to_utf16(raw).to_le_bytes());
        }
    }

    let projected_len: u32 = rows
        .chunks_exact(schema::RANGE.stride())
        .map(|row| word(row, 4) - word(row, 0))
        .sum();
    debug_assert!(table.is_some() || projected_len == mask.len());

    let mut out = Vec::with_capacity(schema::HEADER_BYTES + rows.len());
    out.extend_from_slice(&schema::MAGIC.to_le_bytes());
    out.extend_from_slice(&schema::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(
        &(if table.is_some() {
            schema::FLAG_UTF16
        } else {
            0
        })
        .to_le_bytes(),
    );
    out.extend_from_slice(&recipe.word().to_le_bytes());
    out.extend_from_slice(&(mask.ranges.len() as u32).to_le_bytes());
    out.extend_from_slice(&source_len.to_le_bytes());
    out.extend_from_slice(&projected_len.to_le_bytes());
    debug_assert_eq!(out.len(), schema::HEADER_BYTES);
    out.extend_from_slice(&rows);
    out
}

/// The source spans a projected interval covers, one per kept range it
/// touches.
///
/// The Rust twin of `mask-reader.ts`'s `pieces`, and of the `locate` a find
/// hit's pieces come from: it is here so the TypeScript algorithm is pinned
/// by a native test before a consumer runs it.
pub fn pieces(mask: &Mask, from: u32, to: u32) -> Vec<Range<u32>> {
    // Past the projection there is nothing to name; `to` past it clamps.
    if to <= from || from >= mask.len() {
        return Vec::new();
    }
    let mut row = mask
        .starts
        .partition_point(|start| *start <= from)
        .saturating_sub(1);
    let mut cursor = from;
    let mut out = Vec::new();
    while cursor < to && row < mask.ranges.len() {
        let range = &mask.ranges[row];
        let start = mask.starts[row];
        let end = start + (range.end - range.start);
        let stop = to.min(end);
        out.push(range.start + (cursor - start)..range.start + (stop - start));
        cursor = stop;
        row += 1;
    }
    out
}

/// One little-endian `u32` out of a row.
fn word(row: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(row[at..at + 4].try_into().expect("four bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onion::{cst, lex, mask as cut};
    use mise::utf16::{utf16_len, utf16_table};

    /// A footnote between two halves of a sentence, so the projection drops a
    /// span in the middle of it, and an astral scalar so a byte offset and a
    /// UTF-16 one differ.
    const BOOK: &str =
        "\\id MRK\n\\c 1\n\\p\n\\v 1 \u{1F600} Jesus wept.\\f + \\ft why\\f* Then he rose.\n";

    fn project(source: &str, recipe: Recipe) -> Mask {
        let tokens = lex(source);
        let tree = cst::build(&tokens);
        cut(source.as_bytes(), &tokens, &tree, &recipe.filter())
    }

    /// The buffer read back the way a host reads it: header words, then pairs.
    struct Read {
        flags: u32,
        recipe: u32,
        source_len: u32,
        projected_len: u32,
        ranges: Vec<(u32, u32)>,
    }

    fn decode(bytes: &[u8]) -> Read {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four"));
        assert_eq!(word(schema::HEADER_MAGIC_OFFSET), schema::MAGIC);
        assert_eq!(word(schema::HEADER_VERSION_OFFSET), schema::FORMAT_VERSION);
        let count = word(schema::HEADER_RANGE_COUNT_OFFSET) as usize;
        assert_eq!(
            bytes.len(),
            schema::HEADER_BYTES + count * schema::RANGE.stride(),
            "the rows are the rest of the buffer"
        );
        let ranges = (0..count)
            .map(|n| {
                let at = schema::HEADER_BYTES + n * schema::RANGE.stride();
                (word(at), word(at + 4))
            })
            .collect();
        Read {
            flags: word(schema::HEADER_FLAGS_OFFSET),
            recipe: word(schema::HEADER_RECIPE_OFFSET),
            source_len: word(schema::HEADER_SOURCE_LEN_OFFSET),
            projected_len: word(schema::HEADER_PROJECTED_LEN_OFFSET),
            ranges,
        }
    }

    /// Laws 1, 2, 3 and 5 over one decoded buffer, whatever its unit.
    fn laws(read: &Read) {
        let mut at = 0u32;
        for (n, (from, to)) in read.ranges.iter().enumerate() {
            assert!(from < to, "range {n} is empty");
            assert!(*to <= read.source_len, "range {n} runs past the source");
            if n > 0 {
                let previous = read.ranges[n - 1].1;
                assert!(
                    previous < *from,
                    "range {n} is not strictly after {}",
                    n - 1
                );
            }
            at += to - from;
        }
        assert_eq!(at, read.projected_len, "the prefix sum is the projection");
    }

    #[test]
    fn the_bytes_map_the_projection() {
        let mask = project(BOOK, Recipe::VerseText);
        let read = decode(&encode(&mask, Recipe::VerseText, None, BOOK.len() as u32));
        laws(&read);
        assert_eq!(read.flags, 0);
        assert_eq!(read.recipe, schema::RECIPE_VERSE_TEXT);
        assert_eq!(read.source_len, BOOK.len() as u32);
        // Law 4: the slices concatenate to the projection, with nothing
        // between them — the whole claim a host rebuilds a reading on.
        let joined: String = read
            .ranges
            .iter()
            .map(|(from, to)| &BOOK[*from as usize..*to as usize])
            .collect();
        assert_eq!(joined, mask.text(BOOK.as_bytes()));
        assert_eq!(read.projected_len as usize, joined.len());
        // The footnote is the gap: more than one range, and the dropped span
        // is not in any of them.
        assert!(read.ranges.len() > 1);
        assert!(!joined.contains("why"));
    }

    #[test]
    fn utf16_rebases_every_offset_and_the_length_with_them() {
        let mask = project(BOOK, Recipe::VerseText);
        let table = utf16_table(BOOK.as_bytes());
        let read = decode(&encode(
            &mask,
            Recipe::VerseText,
            Some(&table),
            utf16_len(BOOK.as_bytes()),
        ));
        laws(&read);
        assert_eq!(read.flags, schema::FLAG_UTF16);
        assert_eq!(read.source_len, utf16_len(BOOK.as_bytes()));
        let source: Vec<u16> = BOOK.encode_utf16().collect();
        let joined: Vec<u16> = read
            .ranges
            .iter()
            .flat_map(|(from, to)| source[*from as usize..*to as usize].iter().copied())
            .collect();
        assert_eq!(
            String::from_utf16(&joined).expect("range bounds are character boundaries"),
            mask.text(BOOK.as_bytes())
        );
        assert_eq!(read.projected_len as usize, joined.len());
        // The emoji is four bytes and two units, so the two encodings differ.
        let bytes = decode(&encode(&mask, Recipe::VerseText, None, BOOK.len() as u32));
        assert_ne!(bytes.ranges, read.ranges);
        assert!(read.projected_len < bytes.projected_len);
    }

    #[test]
    fn the_structure_recipe_is_its_own_buffer() {
        let structure = project(BOOK, Recipe::Structure);
        let read = decode(&encode(
            &structure,
            Recipe::Structure,
            None,
            BOOK.len() as u32,
        ));
        laws(&read);
        assert_eq!(read.recipe, schema::RECIPE_STRUCTURE);
        let joined: String = read
            .ranges
            .iter()
            .map(|(from, to)| &BOOK[*from as usize..*to as usize])
            .collect();
        assert_eq!(joined, structure.text(BOOK.as_bytes()));
        // A skeleton keeps the markers and drops the prose; verse text does
        // the opposite, so the two maps cannot be the same map.
        assert!(joined.contains("\\c 1"));
        assert!(!joined.contains("Jesus wept."));
    }

    #[test]
    fn an_empty_mask_is_a_header_and_nothing_else() {
        let empty = Mask::default();
        let buffer = encode(&empty, Recipe::VerseText, None, 12);
        assert_eq!(buffer.len(), schema::HEADER_BYTES);
        let read = decode(&buffer);
        assert_eq!(read.projected_len, 0);
        assert!(read.ranges.is_empty());
        assert!(pieces(&empty, 0, 5).is_empty());
    }

    #[test]
    fn pieces_answers_what_a_find_hit_carries() {
        let mask = project(BOOK, Recipe::VerseText);
        let find = crate::find::Find::literal("wept. Then");
        let mut hits = 0;
        for hit in find.in_projection(&mask, BOOK.as_bytes()) {
            let mine = pieces(&mask, hit.projected.start, hit.projected.end);
            let theirs: Vec<Range<u32>> = hit.source.pieces().cloned().collect();
            assert_eq!(mine, theirs);
            hits += 1;
        }
        assert_eq!(hits, 1, "the needle crosses the footnote once");
        // One unit at a time, `pieces` starts where `to_source` says it does
        // — the two halves of the same map, and the reader ships both.
        for at in 0..mask.len() {
            assert_eq!(pieces(&mask, at, at + 1)[0].start, mask.to_source(at));
        }
        // One unit never leaves the range it starts in; the whole projection
        // crosses every gap there is.
        assert!((0..mask.len()).all(|at| pieces(&mask, at, at + 1).len() == 1));
        assert_eq!(pieces(&mask, 0, mask.len()).len(), mask.ranges.len());
        // An interval starting at or past the end names nothing; one running
        // past the end clamps to the last unit rather than inventing a span.
        let len = mask.len();
        assert!(pieces(&mask, len, len + 3).is_empty());
        assert!(pieces(&mask, len + 7, len + 9).is_empty());
        let last = pieces(&mask, len - 1, len + 5);
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].end, mask.ranges.last().expect("ranges").end);
    }
}
