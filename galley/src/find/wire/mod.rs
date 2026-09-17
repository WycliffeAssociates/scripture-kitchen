//! The hits of one search as one flat buffer — what a host outside Rust reads.
//!
//! Find hands back Rust ranges in the mask's and the source's BYTE space. A
//! JavaScript host counts in UTF-16 and cannot see a `Vec<Hit>` at all, so the
//! crossing needs one encoding — and it lives here rather than in `wasm.rs`
//! because BOTH of Sefer's doors encode it: the browser through
//! [`crate::wasm::Galley::find`], the desktop through the same
//! [`Expediter::find`](crate::Expediter::find) linked natively. One format and
//! one encoder, so the two doors cannot drift.
//!
//! ```text
//! u32   magic              0x444E4946 — "FIND", little-endian
//! u32   version            1
//! u32   hitCount
//! u32   bookCount
//! hit   × hitCount    bookIndex, projectedFrom, projectedTo, pieceCount,
//!                     pieceCount × (sourceFrom, sourceTo)
//! u32   × bookCount   idByteLen
//! u32   × hitCount    previewByteLen
//! bytes               every id's UTF-8 in order, then every preview's
//! ```
//!
//! The two leading words are what the onion and sous buffers both lead with,
//! and for the same reason: a reader that is a version behind fails on the
//! header rather than on a field it misread.
//!
//! Little-endian `u32` throughout, and every offset is UTF-16 — the unit the
//! editor's coordinates are already in (`onion::wire`'s `utf16: true`). The
//! two length arrays come BEFORE the byte blob so that every `u32` in the
//! buffer is four-byte aligned and a reader can take a `Uint32Array` view over
//! the head of it.
//!
//! `pieceCount` is the reason this is not two numbers per hit: a hit that
//! crosses a masked gap is one range per contiguous source piece, in order
//! (`find.md`, "why `Split` exists"). A consumer that replaces text has to
//! decide about the markup between the pieces; the buffer's job is to have
//! told it the gap is there.
//!
//! The preview is the PROJECTED text around the hit — the reading a result
//! card shows. It is encoded here because the projection is materialized for
//! the search and dropped with it: a host that wanted the same string later
//! would have to mask the whole book again.
//!
//! The ROWS are declared in [`schema`] and written by the generated writers
//! beside it, which is also where `galley/find-reader.ts` comes from — so no
//! consumer reads this layout, and the two ends cannot disagree about it. The
//! ENVELOPE is hand-written below: four header words, two length arrays and
//! two blobs are what this format IS, and declaring them would be a grammar
//! written to save fifty lines.
//!
//! `galley/src/find.md` is the search's contract; this is its wire.

pub mod emit;
pub mod generated;
pub mod schema;

use mise::utf16::utf16_table;

use super::Find;
use crate::pantry::{BookId, Pantry};

pub use schema::{MAGIC, VERSION};

/// Bytes of projected context a preview aims for, either side of the hit
/// together. Bytes, not characters: it is a display string that gets
/// trimmed to a char boundary, not a coordinate anything reads back.
const PREVIEW_WIDTH: usize = 90;

/// Every hit of `find` over these books' retained verse-text projections,
/// encoded as the module's buffer.
///
/// `ids` gives both what is searched and the `bookIndex` space: index `i`
/// of a hit record is `ids[i]`, whether or not the other books produced
/// anything. Books are searched in the order given — canonical order, when
/// the caller took them from [`Pantry::books`].
///
/// `limit` bounds hits ACROSS books, not per book; `0` means no bound. An
/// id that is not registered, or a book that retains neither text nor
/// projection, contributes no hits rather than an error: the caller asked
/// which of these books match, and the answer for that one is "it
/// cannot".
pub fn encode(pantry: &mut Pantry, ids: &[BookId], find: &Find<'_>, limit: u32) -> Vec<u8> {
    let ceiling = if limit == 0 {
        usize::MAX
    } else {
        limit as usize
    };
    let mut hits: Vec<WireHit> = Vec::new();
    let mut previews: Vec<String> = Vec::new();

    for (index, id) in ids.iter().enumerate() {
        if previews.len() >= ceiling {
            break;
        }
        let Some(entry) = pantry.book(id) else {
            continue;
        };
        let (Ok(text), Ok(mask), Ok(source16)) = (entry.text(), entry.mask(), entry.utf16()) else {
            continue;
        };
        let mut found = find.in_projection(mask, text.as_bytes());
        let taken: Vec<super::Hit> = found.by_ref().take(ceiling - previews.len()).collect();
        if taken.is_empty() {
            continue;
        }
        // One table per book that HAS a hit, off the projection the search
        // already materialized: a book nobody matched pays nothing.
        let projected = found.projection();
        let projected16 = utf16_table(projected.as_bytes());
        for hit in &taken {
            hits.push(WireHit {
                book: index as u32,
                from: projected16.to_utf16(hit.projected.start),
                to: projected16.to_utf16(hit.projected.end),
                pieces: hit
                    .source
                    .pieces()
                    .map(|piece| (source16.to_utf16(piece.start), source16.to_utf16(piece.end)))
                    .collect(),
            });
            previews.push(snippet(
                projected,
                hit.projected.start as usize,
                hit.projected.end as usize,
            ));
        }
    }

    let strings: usize = ids.iter().map(|id| id.as_str().len()).sum::<usize>()
        + previews.iter().map(String::len).sum::<usize>();
    let rows: usize = hits.iter().map(|hit| 16 + hit.pieces.len() * 8).sum();
    let mut out = Vec::with_capacity(
        schema::HEADER_BYTES + rows + 4 * (ids.len() + previews.len()) + strings,
    );
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(previews.len() as u32).to_le_bytes());
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    debug_assert_eq!(out.len(), schema::HEADER_BYTES);

    // The offsets this fills are dropped: every value above is ALREADY UTF-16,
    // converted per book against its own two tables, so there is no later
    // sweep for them to feed. See the note in `generated.rs`.
    let mut dropped = generated::Offsets::new();
    generated::write_hits(&hits, &mut out, &mut dropped);
    generated::write_id_lens(ids, &mut out, &mut dropped);
    generated::write_preview_lens(&previews, &mut out, &mut dropped);

    for id in ids {
        out.extend_from_slice(id.as_str().as_bytes());
    }
    for preview in &previews {
        out.extend_from_slice(preview.as_bytes());
    }
    out
}

/// One hit as the wire takes it: every offset already in UTF-16, because find
/// converts per book against two different tables — the projection's and the
/// source's — and a row cannot carry the question of which.
pub struct WireHit {
    pub book: u32,
    pub from: u32,
    pub to: u32,
    pub pieces: Vec<(u32, u32)>,
}

/// The projected line around `from..to`, narrowed to about
/// [`PREVIEW_WIDTH`] bytes and marked with an ellipsis where it was cut.
///
/// Display text only, and deliberately not a coordinate: it is trimmed,
/// elided, and clamped to char boundaries, so nothing may read an offset
/// back out of it.
fn snippet(text: &str, from: usize, to: usize) -> String {
    let line_start = text[..from].rfind('\n').map_or(0, |at| at + 1);
    let line_end = text[to..].find('\n').map_or(text.len(), |at| to + at);
    let mut start = line_start;
    let mut end = line_end;
    if line_end - line_start > PREVIEW_WIDTH {
        let slack = PREVIEW_WIDTH.saturating_sub(to.min(line_end) - from);
        start = line_start.max(from.saturating_sub(slack / 2));
        end = line_end.min(start + PREVIEW_WIDTH);
        start = line_start.max(end.saturating_sub(PREVIEW_WIDTH));
    }
    while start < end && !text.is_char_boundary(start) {
        start += 1;
    }
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = if start > line_start { "…" } else { "" };
    let tail = if end < line_end { "…" } else { "" };
    format!("{head}{}{tail}", text[start..end].trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pantry::Role;

    /// One decoded hit: book index, projected range, source pieces.
    type Row = (u32, u32, u32, Vec<(u32, u32)>);

    /// The buffer, read back the way a host reads it.
    struct Read {
        hits: Vec<Row>,
        ids: Vec<String>,
        previews: Vec<String>,
    }

    fn decode(bytes: &[u8]) -> Read {
        let word = |at: usize| {
            u32::from_le_bytes(bytes[at * 4..at * 4 + 4].try_into().expect("four bytes"))
        };
        assert_eq!(word(0), MAGIC, "the buffer leads with FIND");
        assert_eq!(word(1), VERSION);
        let hit_count = word(2) as usize;
        let book_count = word(3) as usize;
        let mut at = 4;
        let mut hits = Vec::new();
        for _ in 0..hit_count {
            let (book, from, to) = (word(at), word(at + 1), word(at + 2));
            let pieces = word(at + 3) as usize;
            at += 4;
            let mut spans = Vec::new();
            for piece in 0..pieces {
                spans.push((word(at + piece * 2), word(at + piece * 2 + 1)));
            }
            at += pieces * 2;
            hits.push((book, from, to, spans));
        }
        let id_lens: Vec<usize> = (0..book_count).map(|i| word(at + i) as usize).collect();
        at += book_count;
        let preview_lens: Vec<usize> = (0..hit_count).map(|i| word(at + i) as usize).collect();
        at += hit_count;
        let mut cursor = at * 4;
        let mut take = |len: usize| {
            let text = String::from_utf8(bytes[cursor..cursor + len].to_vec())
                .expect("the encoder wrote UTF-8");
            cursor += len;
            text
        };
        let ids = id_lens.into_iter().map(&mut take).collect();
        let previews = preview_lens.into_iter().map(&mut take).collect();
        Read {
            hits,
            ids,
            previews,
        }
    }

    /// A footnote between two halves of the needle, so the hit that
    /// crosses it must come back as two source pieces — and an astral
    /// scalar before it, so a byte offset and a UTF-16 one differ.
    const BOOK: &str =
        "\\id MRK\n\\c 1\n\\v 1 \u{1F600} Jesus wept.\\f + \\ft why\\f* Then he rose.\n";

    #[test]
    fn one_book_round_trips_through_the_buffer() {
        let mut pantry = Pantry::new(1 << 20);
        pantry
            .update("books/MRK.usfm", Role::Target, BOOK)
            .expect("a well-formed book");
        let ids = vec![BookId::from("books/MRK.usfm")];

        let buffer = encode(&mut pantry, &ids, &Find::literal("wept. Then"), 0);
        let read = decode(&buffer);
        assert_eq!(read.ids, vec!["books/MRK.usfm".to_string()]);
        assert_eq!(read.hits.len(), 1);
        let (book, from, to, pieces) = &read.hits[0];
        assert_eq!(*book, 0);
        // Two pieces: the projection dropped the footnote between them,
        // and nothing here hands back one range that would swallow it.
        assert_eq!(pieces.len(), 2);
        assert_eq!(*to - *from, "wept. Then".encode_utf16().count() as u32);
        // UTF-16, not bytes: the emoji ahead of the hit is one code point,
        // four bytes, two units.
        let source16: Vec<u16> = BOOK.encode_utf16().collect();
        let units: Vec<u16> = pieces
            .iter()
            .flat_map(|(a, b)| source16[*a as usize..*b as usize].iter().copied())
            .collect();
        // The pieces concatenate to the needle, which is what says the
        // offsets landed on the right side of the dropped markup.
        assert_eq!(
            String::from_utf16(&units).expect("valid UTF-16"),
            "wept. Then"
        );
        assert!(read.previews[0].contains("Jesus wept. Then he rose."));
    }

    #[test]
    fn the_limit_bounds_hits_across_books() {
        let mut pantry = Pantry::new(1 << 20);
        pantry
            .update("a", Role::Target, BOOK)
            .expect("a well-formed book");
        pantry
            .update("b", Role::Target, BOOK)
            .expect("a well-formed book");
        let ids = vec![BookId::from("a"), BookId::from("b")];

        let all = decode(&encode(&mut pantry, &ids, &Find::literal("e"), 0));
        let two = decode(&encode(&mut pantry, &ids, &Find::literal("e"), 2));
        assert!(all.hits.len() > 2);
        assert_eq!(two.hits.len(), 2);
        // Both books are named whether or not they contributed a hit: the
        // id table is the bookIndex space, not the list of books that hit.
        assert_eq!(two.ids.len(), 2);
        assert!(two.hits.iter().all(|(book, ..)| *book == 0));
        assert_eq!(two.previews.len(), 2);
    }

    /// The buffer the HAND-WRITTEN v1 encoder wrote, kept for exactly as long
    /// as it takes to prove the generated writer writes the same bytes.
    ///
    /// Delete this and its test when find's wire next changes on purpose: it
    /// is a migration's evidence, not a second implementation to maintain.
    fn encode_v1_by_hand(
        pantry: &mut Pantry,
        ids: &[BookId],
        find: &Find<'_>,
        limit: u32,
    ) -> Vec<u8> {
        let ceiling = if limit == 0 {
            usize::MAX
        } else {
            limit as usize
        };
        let mut records: Vec<u32> = Vec::new();
        let mut previews: Vec<String> = Vec::new();

        for (index, id) in ids.iter().enumerate() {
            if previews.len() >= ceiling {
                break;
            }
            let Some(entry) = pantry.book(id) else {
                continue;
            };
            let (Ok(text), Ok(mask), Ok(source16)) = (entry.text(), entry.mask(), entry.utf16())
            else {
                continue;
            };
            let mut hits = find.in_projection(mask, text.as_bytes());
            let found: Vec<super::super::Hit> =
                hits.by_ref().take(ceiling - previews.len()).collect();
            if found.is_empty() {
                continue;
            }
            let projected = hits.projection();
            let projected16 = utf16_table(projected.as_bytes());
            for hit in &found {
                records.push(index as u32);
                records.push(projected16.to_utf16(hit.projected.start));
                records.push(projected16.to_utf16(hit.projected.end));
                records.push(hit.source.pieces().count() as u32);
                for piece in hit.source.pieces() {
                    records.push(source16.to_utf16(piece.start));
                    records.push(source16.to_utf16(piece.end));
                }
                previews.push(snippet(
                    projected,
                    hit.projected.start as usize,
                    hit.projected.end as usize,
                ));
            }
        }

        let strings: usize = ids.iter().map(|id| id.as_str().len()).sum::<usize>()
            + previews.iter().map(String::len).sum::<usize>();
        let mut out =
            Vec::with_capacity(16 + 4 * records.len() + 4 * (ids.len() + previews.len()) + strings);
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(previews.len() as u32).to_le_bytes());
        out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
        for word in &records {
            out.extend_from_slice(&word.to_le_bytes());
        }
        for id in ids {
            out.extend_from_slice(&(id.as_str().len() as u32).to_le_bytes());
        }
        for preview in &previews {
            out.extend_from_slice(&(preview.len() as u32).to_le_bytes());
        }
        for id in ids {
            out.extend_from_slice(id.as_str().as_bytes());
        }
        for preview in &previews {
            out.extend_from_slice(preview.as_bytes());
        }
        out
    }

    /// THE migration's gate: the generated writer writes what the hand-written
    /// one wrote, byte for byte, so `VERSION` stays 1 and no consumer — Sefer's
    /// vendored decoder included — needed a release.
    #[test]
    fn the_generated_writer_reproduces_v1_byte_for_byte() {
        let mut pantry = Pantry::new(1 << 20);
        pantry.update("a", Role::Target, BOOK).expect("a book");
        pantry.update("b", Role::Target, BOOK).expect("a book");
        // Registered, but the needle below is not in it: a book with no hits
        // still owns an id-table slot, and the two encoders must agree on that.
        pantry
            .update("c", Role::Target, "\\id LUK\n\\c 1\n\\v 1 Nothing here.\n")
            .expect("a book");

        let a = BookId::from("a");
        let b = BookId::from("b");
        let c = BookId::from("c");
        let gone = BookId::from("never/registered.usfm");
        let cases: &[(&str, Vec<BookId>, &str, u32)] = &[
            ("one book, split hit", vec![a.clone()], "wept. Then", 0),
            ("two books, limited", vec![a.clone(), b.clone()], "e", 2),
            (
                "an unregistered id and a book with no hits",
                vec![gone, c, a.clone()],
                "Jesus",
                0,
            ),
            ("no books at all", vec![], "Jesus", 0),
        ];
        for (what, ids, needle, limit) in cases {
            let find = Find::literal(needle);
            let generated = encode(&mut pantry, ids, &find, *limit);
            let by_hand = encode_v1_by_hand(&mut pantry, ids, &find, *limit);
            assert_eq!(generated, by_hand, "{what}: the bytes moved");
        }
    }

    #[test]
    fn an_unregistered_id_contributes_nothing() {
        let mut pantry = Pantry::new(1 << 20);
        let ids = vec![BookId::from("never/registered.usfm")];
        let read = decode(&encode(&mut pantry, &ids, &Find::literal("Jesus"), 0));
        assert!(read.hits.is_empty());
        assert_eq!(read.ids, vec!["never/registered.usfm".to_string()]);
    }
}
