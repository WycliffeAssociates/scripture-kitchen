//! The census: what a project CONTAINS, off the pinned tier alone.
//!
//! ```text
//! galley::toc::encode(&pantry, &pantry.books(Role::Target).ids(), false)
//!
//!   GEN   51 chapter rows (50 `\c`)   1,533 anchors
//!   EXO   41 chapter rows (40 `\c`)   1,213 anchors
//!   …
//! ```
//!
//! A registered book already retains its [`Toc`](crate::onion::toc::Toc) — it
//! is built once by `Pantry::update` and pinned until the id is removed — so
//! this door reads resident state and derives nothing: no chunk resolve, no
//! text read, no wire plate. It is the answer to "how many chapters and verses
//! has each book" that does NOT cost one parse per book.
//!
//! `galley/src/toc.md` is the contract: what the rows carry, what they refuse
//! to carry, and why a verse COUNT is the host's arithmetic rather than the
//! engine's claim. [`schema`] is the one declaration both ends are generated
//! from.

pub mod emit;
pub mod generated;
pub mod schema;

use mise::utf16::Utf16Table;

use crate::onion::toc::Toc;
use crate::pantry::{BookId, Pantry, PantryError};

/// One chapter as it crosses: the retained row, plus what its verses count to.
///
/// The two counts are both here because neither is THE count — see
/// [`Self::last_verse`].
pub struct Chapter {
    pub start: u32,
    pub end: u32,
    pub number: u16,
    /// `\v` markers inside this chapter's span.
    pub anchors: u16,
    /// The highest verse number any of those anchors names. A bridge
    /// `\v 5-7` is ONE anchor reaching 7, so a host keying by verse number
    /// reads this and a host counting markers reads [`Self::anchors`].
    pub last_verse: u16,
}

/// One verse anchor as it crosses.
pub struct Verse {
    pub at: u32,
    pub chapter: u16,
    pub first: u16,
    pub last: u16,
}

/// The two row sets for one retained `Toc`.
///
/// A verse belongs to the chapter row whose span CONTAINS it — position, never
/// the designator — so a book with two `\c 3`s gives each row its own anchors.
pub fn rows_of(toc: &Toc) -> (Vec<Chapter>, Vec<Verse>) {
    let mut chapters = Vec::with_capacity(toc.chapters.len());
    let mut verses = Vec::with_capacity(toc.verses.len());
    let mut next = 0usize;
    for row in &toc.chapters {
        let mut anchors: u16 = 0;
        let mut last_verse: u16 = 0;
        while let Some(anchor) = toc.verses.get(next).filter(|a| a.at < row.end) {
            anchors = anchors.saturating_add(1);
            last_verse = last_verse.max(anchor.last);
            verses.push(Verse {
                at: anchor.at,
                chapter: row.number,
                first: anchor.first,
                last: anchor.last,
            });
            next += 1;
        }
        chapters.push(Chapter {
            start: row.start,
            end: row.end,
            number: row.number,
            anchors,
            last_verse,
        });
    }
    (chapters, verses)
}

/// Every named book's census, as the module's buffer.
///
/// `&Pantry`, not `&mut`: nothing here runs the chunk cache, which is what an
/// [`Entry`](crate::pantry::Entry) borrows mutably for.
///
/// An id that is not registered contributes nothing and is not listed — the
/// directory names what was found, and a caller comparing its own list against
/// `bookCount` sees the difference. The wasm door refuses an unknown id by
/// name before it gets here, as `parse(id)` does.
///
/// Under `utf16`, every offset is rebased through that book's own retained
/// table. A book that kept no text kept no table either, so it answers
/// [`PantryError::NoProjection`] rather than silently returning bytes.
pub fn encode(pantry: &Pantry, ids: &[BookId], utf16: bool) -> Result<Vec<u8>, PantryError> {
    struct Block<'a> {
        id: &'a BookId,
        code: [u8; 3],
        chapters: Vec<u8>,
        verses: Vec<u8>,
        chapter_rows: u32,
        verse_rows: u32,
    }

    let mut blocks: Vec<Block<'_>> = Vec::with_capacity(ids.len());
    for id in ids {
        let Some((toc, table)) = pantry.census(id) else {
            continue;
        };
        if utf16 && table.is_none() {
            return Err(PantryError::NoProjection { id: id.clone() });
        }
        let (chapter_rows, verse_rows) = rows_of(toc);
        let mut chapters = Vec::new();
        let mut verses = Vec::new();
        let mut chapter_offsets = generated::Offsets::new();
        let mut verse_offsets = generated::Offsets::new();
        generated::write_chapters(&chapter_rows, &mut chapters, &mut chapter_offsets);
        generated::write_verses(&verse_rows, &mut verses, &mut verse_offsets);
        if let Some(table) = table.filter(|_| utf16) {
            rebase(&mut chapters, &chapter_offsets, table);
            rebase(&mut verses, &verse_offsets, table);
        }
        blocks.push(Block {
            id,
            code: toc.book,
            chapters,
            verses,
            chapter_rows: chapter_rows.len() as u32,
            verse_rows: verse_rows.len() as u32,
        });
    }

    // Lay the buffer out before writing it, so every directory entry names a
    // position that is already decided: header, directory, each book's two
    // blocks, then every id's bytes.
    let directory_at = schema::HEADER_BYTES;
    let mut at = directory_at + blocks.len() * schema::DIRECTORY_ENTRY_BYTES;
    let mut places: Vec<[u32; 3]> = Vec::with_capacity(blocks.len());
    for block in &blocks {
        at = aligned(at);
        let chapters_at = at;
        at = aligned(at + block.chapters.len());
        let verses_at = at;
        at += block.verses.len();
        places.push([chapters_at as u32, verses_at as u32, 0]);
    }
    for (block, place) in blocks.iter().zip(&mut places) {
        place[2] = at as u32;
        at += block.id.as_str().len();
    }

    let mut out = Vec::with_capacity(at);
    out.extend_from_slice(&schema::MAGIC.to_le_bytes());
    out.extend_from_slice(&schema::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(if utf16 { schema::FLAG_UTF16 } else { 0 }).to_le_bytes());
    out.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    out.extend_from_slice(&(schema::CHAPTER.stride() as u32).to_le_bytes());
    out.extend_from_slice(&(schema::VERSE.stride() as u32).to_le_bytes());
    out.extend_from_slice(&(directory_at as u32).to_le_bytes());
    debug_assert_eq!(out.len(), schema::HEADER_BYTES);

    for (block, [chapters_at, verses_at, id_at]) in blocks.iter().zip(&places) {
        out.extend_from_slice(&block.code);
        out.push(0);
        out.extend_from_slice(&chapters_at.to_le_bytes());
        out.extend_from_slice(&block.chapter_rows.to_le_bytes());
        out.extend_from_slice(&verses_at.to_le_bytes());
        out.extend_from_slice(&block.verse_rows.to_le_bytes());
        out.extend_from_slice(&id_at.to_le_bytes());
        out.extend_from_slice(&(block.id.as_str().len() as u32).to_le_bytes());
    }

    for block in &blocks {
        out.resize(aligned(out.len()), 0);
        out.extend_from_slice(&block.chapters);
        out.resize(aligned(out.len()), 0);
        out.extend_from_slice(&block.verses);
    }
    for block in &blocks {
        out.extend_from_slice(block.id.as_str().as_bytes());
    }
    Ok(out)
}

/// Convert every collected offset in place, through one book's own table.
fn rebase(bytes: &mut [u8], offsets: &generated::Offsets, table: &Utf16Table) {
    for &at in offsets {
        let raw = u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
        bytes[at..at + 4].copy_from_slice(&table.to_utf16(raw).to_le_bytes());
    }
}

const fn aligned(n: usize) -> usize {
    n.next_multiple_of(schema::SECTION_ALIGNMENT)
}
