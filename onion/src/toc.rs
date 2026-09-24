//! `Toc`: WHERE things are — the book code, the chapter table, the verse
//! anchors, and `locate()` over them.
//!
//! ```text
//! \id GEN            book     = "GEN"
//! \mt1 Genesis       chapters = [ 0: 0..24,  ← front matter is chapter 0
//! \c 1                            1: 24..46, label "1" ]
//! \p                 verses   = [ at 33, chapter 1, 1..1, label "1", members [1] ]
//! \v 1 In the…       locate(40) = "GEN 1:1"   locate(10) = "GEN"
//!
//! \v 1,3,5 …         verse 1..5, label "1,3,5", members [1] [3] [5]   ← holes at 2 and 4
//! \v 12a …           verse 12..12, label "12a", members [12a]          ← a place inside 12
//! ```
//!
//! Every row carries its designator's LABEL as a span into the source — the
//! spelling as written, `12b` or `1,3,5`, which the numbers cannot carry — and
//! every verse row its MEMBERS ([`designator::members`]) as a run in
//! [`Toc::members`]. Both are positions, never copied text: whoever holds the
//! Toc holds, or can reach, the text it indexes.
//!
//! A pure pass over TOKENS, no CST: `\c` and `\v` plus their designators are
//! flat facts, so an editor can hold a Toc without ever building a tree. It
//! owns no position and emits no tokens; it only INDEXES rows the scanner
//! already emitted, and re-reads the source only for designator interiors and
//! the book code.
//!
//! Chapter rows TILE `0..source.len()`, which is what makes [`Toc::locate`]
//! total: every byte lands in some chapter, and the bytes before the first `\c`
//! land in chapter 0 (a book-only sid, never a fabricated verse).

use crate::designator::{self, Designator};
use crate::scanner::payload_label;
use crate::tables::generated;
use crate::tables::schema::MarkerKind;
use crate::token::{Token, TokenKind};

/// One chapter's number and the source bytes it covers.
///
/// Byte layout — `#[repr(C)]`, 24 bytes, stride 24:
///
/// ```text
/// 0  u32  start        first byte of the `\c` marker (0 for chapter 0)
/// 4  u32  end          one past the last byte, exclusive
/// 8  u32  token        that marker's token index; u32::MAX for the
///                      synthetic front-matter row
/// 12 u32  label_start  the designator as written, minus its folded delimiter
/// 16 u32  label_end
/// 20 u16  number       the designator's number; 0 = absent or malformed
/// 22      —            2 bytes of trailing padding (22 bytes of content, align 4)
/// ```
///
/// The u32s come first so the only padding is at the END: an interior hole
/// would be a field a JS reader could silently mis-offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ChapterRow {
    pub start: u32,
    pub end: u32,
    /// The `\c` marker's token index — `u32::MAX` for the front-matter row,
    /// which no marker opens.
    pub token: u32,
    /// The designator's LABEL, `label_start..label_end` in the source: the
    /// spelling (`12b`) the `number` cannot carry. Empty at the marker's end
    /// when the `\c` owns no designator, and `0..0` on the front-matter row.
    pub label_start: u32,
    pub label_end: u32,
    /// The number [`designator::chapter`] read, saturated into u16. `0` for a
    /// `\c` with no designator or a malformed one (`\c 12b`) — degrade, never
    /// repair. Chapter 0 is also the synthetic front-matter row, told apart by
    /// being row index 0.
    pub number: u16,
}

/// Where one `\v` sits, and which verses it names.
///
/// Byte layout — `#[repr(C)]`, 28 bytes, stride 28:
///
/// ```text
/// 0  u32  at            first byte of the `\v` marker — where the verse starts
/// 4  u32  token         that marker's token index
/// 8  u32  label_start   the designator as written, minus its folded delimiter
/// 12 u32  label_end
/// 16 u32  members_from  this verse's run in `Toc::members`
/// 20 u16  chapter       the enclosing chapter row's `number`
/// 22 u16  first         lowest verse the designator names
/// 24 u16  last          highest — `first == last` unless this is a bridge
/// 26 u16  members_len   how many members the run holds; 0 when malformed
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct VerseAnchor {
    pub at: u32,
    pub token: u32,
    /// The designator's LABEL in the source, as [`ChapterRow::label_start`]:
    /// `6a`, `1,3,5`, or the malformed `7"`. Empty at the marker's end when
    /// the `\v` owns no designator.
    pub label_start: u32,
    pub label_end: u32,
    /// `members_from..members_from + members_len` in [`Toc::members`]: what
    /// the designator COVERS, where `first..=last` is only its hull.
    pub members_from: u32,
    pub chapter: u16,
    /// `first`/`last` are 0 when the designator is absent or malformed, and the
    /// row still exists: dropping it would let the PREVIOUS verse's extent
    /// silently swallow this verse's text.
    pub first: u16,
    pub last: u16,
    pub members_len: u16,
}

/// One thing a `\v` designator covers ([`designator::Member`]), with its
/// segments placed in the source.
///
/// Byte layout — `#[repr(C)]`, 20 bytes, stride 20:
///
/// ```text
/// 0  u32  from_segment_start   the segment after `from` (`a` in `12a`);
/// 4  u32  from_segment_end     equal to the start when none is written
/// 8  u32  to_segment_start
/// 12 u32  to_segment_end
/// 16 u16  from                 the member's first number
/// 18 u16  to                   its last; equal to `from` for a single place
/// ```
///
/// A backwards member (`3-1`) is kept as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct VerseMember {
    pub from_segment_start: u32,
    pub from_segment_end: u32,
    pub to_segment_start: u32,
    pub to_segment_end: u32,
    pub from: u16,
    pub to: u16,
}

// The layout is internal hygiene, not a versioned wire format — but it is the
// shape a wasm consumer would read, so the widths are pinned here rather than
// rediscovered there.
const _: () = assert!(size_of::<ChapterRow>() == 24);
const _: () = assert!(size_of::<VerseAnchor>() == 28);
const _: () = assert!(size_of::<VerseMember>() == 20);

impl ChapterRow {
    pub fn span(&self) -> core::ops::Range<u32> {
        self.start..self.end
    }

    /// The designator's span, for the raw label (`12b`) `number` cannot carry.
    /// `None` for the front-matter row or a `\c` with no designator. `source`
    /// and `tokens` must be what the Toc was built from.
    pub fn designator_span(&self, source: &[u8], tokens: &[Token]) -> Option<(u32, u16)> {
        if self.token == u32::MAX {
            return None;
        }
        label_span(source, tokens, self.token as usize)
    }
}

impl VerseAnchor {
    /// The designator's span, for the raw spelling (`6a`, `7"`) the numbers
    /// above cannot carry. `None` when the `\v` owns no designator.
    ///
    /// `source` and `tokens` must be what the Toc was built from.
    pub fn designator_span(&self, source: &[u8], tokens: &[Token]) -> Option<(u32, u16)> {
        label_span(source, tokens, self.token as usize)
    }
}

/// A located reference: book, chapter, and the verse range at that byte.
///
/// ```text
/// GEN 1:1     first == last
/// MRK 6:1-3   a bridge reports its WHOLE range; a renderer that wants
///             ebible's first-verse keying reads `first` itself
/// GEN 1       inside a chapter, ahead of its first verse (or on a verse
///             whose designator was malformed)
/// GEN         chapter 0 — front matter names no chapter and no verse
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sid {
    /// The book code's first three bytes, as-is — see [`Toc::book`].
    pub book: [u8; 3],
    pub chapter: u16,
    pub first: u16,
    pub last: u16,
}

/// Rendered when the book code is absent: a sid still has to name SOMETHING,
/// and three question marks read as "unknown book" where three NUL bytes read
/// as a corrupted string.
const UNKNOWN_BOOK: &str = "###";

impl core::fmt::Display for Sid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let code = book_str(&self.book);
        let code = if code.is_empty() { UNKNOWN_BOOK } else { &code };
        f.write_str(code)?;
        if self.chapter == 0 {
            return Ok(());
        }
        write!(f, " {}", self.chapter)?;
        match (self.first, self.last) {
            (0, _) => Ok(()),
            (first, last) if first == last => write!(f, ":{first}"),
            (first, last) => write!(f, ":{first}-{last}"),
        }
    }
}

/// What a book's token stream says about where its contents are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toc {
    /// The FIRST `\id`'s code, first three bytes, verbatim and zero-padded:
    /// `\id gen` keeps its case, `\id GENESIS` keeps `GEN`, `\id 1` becomes
    /// `1\0\0`, and no `\id` at all is all zeros. Validation is lint's — the
    /// Toc never judges, it just renders what it was given.
    pub book: [u8; 3],
    /// The `BookCode` token index, or `None` when the book has no `\id` line
    /// with a code (real in the wild: BSB Ecclesiastes). The full spelling and
    /// its length live there; `book` above is only the sid's three bytes.
    pub book_token: Option<u32>,
    /// One row per chapter, in source order, TILING `0..source.len()`. Row 0 is
    /// always present and always chapter 0 — the front matter, empty when the
    /// file opens on `\c`.
    pub chapters: Vec<ChapterRow>,
    /// One row per `\v`, in source order, therefore sorted by `at`.
    pub verses: Vec<VerseAnchor>,
    /// Every verse's members, run after run in `verses` order; a row's
    /// `members_from`/`members_len` name its run.
    pub members: Vec<VerseMember>,
}

/// Indexes an already-lexed book: the book code, the chapter table, the verse
/// anchors.
///
/// `source` must be the bytes the tokens were lexed from — it is read only for
/// the `\id` code and for designator interiors.
pub fn toc(source: &[u8], tokens: &[Token]) -> Toc {
    let end = source.len() as u32;
    let mut out = Toc {
        book: [0; 3],
        book_token: None,
        // The front-matter row, claiming the whole file until a `\c` takes the
        // rest of it: the last row pushed is always already correct, so the
        // pass needs no epilogue.
        chapters: vec![ChapterRow {
            start: 0,
            end,
            token: u32::MAX,
            label_start: 0,
            label_end: 0,
            number: 0,
        }],
        verses: Vec::new(),
        members: Vec::new(),
    };

    for (row, token) in tokens.iter().enumerate() {
        match token.kind() {
            // No marker lookup needed: `BookCode` belongs to exactly one row,
            // so the first one in the stream IS the first `\id`'s code.
            TokenKind::BookCode if out.book_token.is_none() => {
                out.book_token = Some(row as u32);
                // The label, not the span: a short or degraded code must not
                // copy the delimiter the scanner folded onto it.
                let code = payload_label(&source[token.start as usize..token.end() as usize]);
                let take = code.len().min(3);
                out.book[..take].copy_from_slice(&code[..take]);
            }
            // The ROW is the authority on what opens a chapter or a verse, not
            // the payload — `\ca`/`\cp`/`\va`/`\vp` carve a `Designator` too,
            // and `\+c` is not a chapter marker at all.
            TokenKind::Marker { nested: false } => match generated::kind(token.marker_idx) {
                MarkerKind::Chapter => {
                    let number = number_after(source, tokens, row, designator::chapter)
                        .map_or(0, |(first, _)| first);
                    let (label_start, label_end) = label_after(source, tokens, row);
                    if let Some(open) = out.chapters.last_mut() {
                        open.end = token.start;
                    }
                    out.chapters.push(ChapterRow {
                        start: token.start,
                        end,
                        token: row as u32,
                        label_start,
                        label_end,
                        number,
                    });
                }
                MarkerKind::Verse => {
                    let (first, last) =
                        number_after(source, tokens, row, designator::verse).unwrap_or((0, 0));
                    let (label_start, label_end) = label_after(source, tokens, row);
                    let members_from = out.members.len() as u32;
                    let label = &source[label_start as usize..label_end as usize];
                    let place = |at: u32| label_start + at;
                    out.members
                        .extend(designator::members(label).map(|m| VerseMember {
                            from_segment_start: place(m.from.segment_start),
                            from_segment_end: place(m.from.segment_end),
                            to_segment_start: place(m.to.segment_start),
                            to_segment_end: place(m.to.segment_end),
                            from: narrow(m.from.number),
                            to: narrow(m.to.number),
                        }));
                    out.verses.push(VerseAnchor {
                        at: token.start,
                        token: row as u32,
                        label_start,
                        label_end,
                        members_from,
                        chapter: out.chapters.last().map_or(0, |c| c.number),
                        first,
                        last,
                        members_len: (out.members.len() as u32 - members_from)
                            .min(u32::from(u16::MAX)) as u16,
                    });
                }
                _ => {}
            },
            _ => {}
        }
    }
    out
}

impl Toc {
    /// The reference at a source byte. TOTAL: a byte past the end of the
    /// source reports the last chapter rather than nothing, because every
    /// caller of this is labelling a position it already has.
    pub fn locate(&self, src_byte: u32) -> Sid {
        let chapter = self.chapters[self.chapter_row(src_byte)].number;
        let (first, last) = self
            .verse_at(src_byte)
            .map_or((0, 0), |v| (v.first, v.last));
        Sid {
            book: self.book,
            chapter,
            first,
            last,
        }
    }

    /// The bytes chapter `n` covers — the editor's chapter window and its
    /// clamp. `None` when the book has no such chapter; the FIRST run of a
    /// number that repeats (a reopened chapter is real data, and this API
    /// answers about a number, so it answers about the first).
    pub fn chapter_span(&self, n: u16) -> Option<core::ops::Range<u32>> {
        self.chapters
            .iter()
            .find(|row| row.number == n)
            .map(ChapterRow::span)
    }

    /// The verse whose extent covers `src_byte`, or `None` for a byte in front
    /// matter or ahead of its chapter's first verse.
    ///
    /// A verse runs from its own `\v` to the next `\v` or `\c` — so the answer
    /// is the last anchor at or before the byte, unless that anchor belongs to
    /// an earlier chapter.
    pub fn verse_at(&self, src_byte: u32) -> Option<&VerseAnchor> {
        let candidate = self.verses.partition_point(|v| v.at <= src_byte);
        let anchor = self.verses.get(candidate.checked_sub(1)?)?;
        let chapter = &self.chapters[self.chapter_row(src_byte)];
        (anchor.at >= chapter.start).then_some(anchor)
    }

    /// What a verse row's designator covers, in written order.
    pub fn members_of(&self, verse: &VerseAnchor) -> &[VerseMember] {
        let from = verse.members_from as usize;
        &self.members[from..from + usize::from(verse.members_len)]
    }

    /// Index into `chapters` of the row covering `src_byte`. Row 0 starts at
    /// byte 0, so the search always lands on a row.
    fn chapter_row(&self, src_byte: u32) -> usize {
        self.chapters
            .partition_point(|row| row.start <= src_byte)
            .saturating_sub(1)
    }
}

/// The `Designator` row belonging to the `\c`/`\v` at `marker_row`.
///
/// A front-position attribute list belongs to the marker and so sits between it
/// and its payload (`\v |x-a="b"| 1`, the U25001 form). Exactly one kind can
/// intervene, so this is a step, not a search.
pub(crate) fn designator_row(tokens: &[Token], marker_row: usize) -> Option<usize> {
    let mut next = marker_row + 1;
    // Delimiter surplus is invisible to attachment, exactly as it is to the
    // scanner's own carve: `\v   1` still owns its designator.
    while matches!(tokens.get(next).map(Token::kind), Some(TokenKind::Pad)) {
        next += 1;
    }
    if matches!(tokens.get(next).map(Token::kind), Some(TokenKind::AttrList)) {
        next += 1;
        while matches!(tokens.get(next).map(Token::kind), Some(TokenKind::Pad)) {
            next += 1;
        }
    }
    match tokens.get(next) {
        Some(t) if t.kind() == TokenKind::Designator => Some(next),
        _ => None,
    }
}

/// The designator after `marker_row` MINUS the delimiter it folded in — the raw
/// label a client renders ([`designator::label`]).
fn label_span(source: &[u8], tokens: &[Token], marker_row: usize) -> Option<(u32, u16)> {
    let token = tokens[designator_row(tokens, marker_row)?];
    let span = &source[token.start as usize..token.end() as usize];
    Some((token.start, designator::label(span).len() as u16))
}

/// [`label_span`] as the row stores it: `start..end`, or an empty span at the
/// marker's end when the marker owns no designator.
fn label_after(source: &[u8], tokens: &[Token], marker_row: usize) -> (u32, u32) {
    match label_span(source, tokens, marker_row) {
        Some((start, len)) => (start, start + u32::from(len)),
        None => {
            let end = tokens[marker_row].end();
            (end, end)
        }
    }
}

/// Numbers SATURATE into u16, as the rows' own do.
fn narrow(n: u32) -> u16 {
    n.min(u32::from(u16::MAX)) as u16
}

/// `read`'s verdict on the designator after `marker_row`, as u16s. `None` for
/// no designator or a malformed one — the caller degrades both to 0.
///
/// Numbers SATURATE into u16 (real maxima are ~150 chapters and ~176 verses, so
/// anything near the ceiling is junk already, and it must stay ORDERED junk).
fn number_after(
    source: &[u8],
    tokens: &[Token],
    marker_row: usize,
    read: fn(&[u8]) -> Designator,
) -> Option<(u16, u16)> {
    let token = tokens[designator_row(tokens, marker_row)?];
    let span = &source[token.start as usize..token.end() as usize];
    let (first, last) = read(span).range()?;
    Some((narrow(first), narrow(last)))
}

/// A book code as text: trailing padding dropped, invalid UTF-8 replaced.
///
/// Lossy because the code is the first three BYTES of the `\id` payload, which
/// a non-ASCII code (`\id ΓΕΝ`) can cut mid-scalar.
fn book_str(book: &[u8; 3]) -> std::borrow::Cow<'_, str> {
    let end = book.iter().position(|b| *b == 0).unwrap_or(book.len());
    String::from_utf8_lossy(&book[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    fn built(source: &str) -> Toc {
        toc(source.as_bytes(), &lex(source))
    }

    /// Every chapter row as `(number, span)` — the table a human would draw.
    fn table(source: &str) -> Vec<(u16, core::ops::Range<u32>)> {
        built(source)
            .chapters
            .iter()
            .map(|row| (row.number, row.span()))
            .collect()
    }

    fn sid(source: &str, at: u32) -> String {
        built(source).locate(at).to_string()
    }

    #[test]
    fn chapter_rows_tile_the_whole_source() {
        let source = "\\id GEN\n\\c 1\n\\p a\n\\c 2\n\\p b\n";
        let toc = built(source);
        assert_eq!(toc.chapters.first().unwrap().start, 0);
        assert_eq!(
            toc.chapters.last().unwrap().end,
            source.len() as u32,
            "the last row must reach the end of the source"
        );
        for pair in toc.chapters.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
    }

    #[test]
    fn front_matter_is_chapter_zero() {
        assert_eq!(
            table("\\id GEN\n\\c 1\n\\p a\n"),
            vec![(0, 0..8), (1, 8..18)]
        );
        // A file that opens on `\c` still gets row 0 — empty, so consumers
        // iterating chapters never have to special-case its absence.
        assert_eq!(table("\\c 1\n"), vec![(0, 0..0), (1, 0..5)]);
        // No chapters at all: the whole book is front matter.
        assert_eq!(table("\\id FRT\n\\periph Title Page\n"), vec![(0, 0..27)]);
        assert_eq!(table(""), vec![(0, 0..0)]);
    }

    #[test]
    fn a_chapter_zero_byte_locates_to_the_book_alone() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a\n";
        assert_eq!(sid(source, 0), "GEN");
        assert_eq!(sid(source, 7), "GEN");
        assert_eq!(sid(source, 8), "GEN 1");
        assert_eq!(sid(source, 20), "GEN 1:1");
    }

    #[test]
    fn a_bridge_reports_its_whole_range() {
        let source = "\\id MRK\n\\c 6\n\\p \\v 1-3 a \\v 4 b\n";
        let toc = built(source);
        assert_eq!(toc.verses[0].first, 1);
        assert_eq!(toc.verses[0].last, 3);
        assert_eq!(sid(source, 22), "MRK 6:1-3");
        assert_eq!(sid(source, 30), "MRK 6:4");
    }

    #[test]
    fn a_malformed_designator_still_gets_a_row() {
        // The en_ulb ZEC 12:7 shape: `\v 2"` glues the quote to the number, so
        // the carved span is `2"`. The row exists with first/last 0, keeping
        // verse 1's extent from swallowing this verse's text.
        let source = "\\c 1\n\\p \\v 1 a \\v 2\" b \\v 3 c\n";
        let toc = built(source);
        assert_eq!(
            toc.verses
                .iter()
                .map(|v| (v.first, v.last))
                .collect::<Vec<_>>(),
            vec![(1, 1), (0, 0), (3, 3)]
        );
        assert_eq!(sid(source, 20), "### 1");
        // The raw spelling stays reachable through the token.
        let (start, len) = toc.verses[1]
            .designator_span(source.as_bytes(), &lex(source))
            .unwrap();
        assert_eq!(
            &source[start as usize..(start + len as u32) as usize],
            "2\""
        );

        // A `\c` with no designator: number 0, and the row still tiles.
        assert_eq!(table("\\c\n\\p a\n"), vec![(0, 0..0), (0, 0..8)]);
        // `\c 12b` fails the CHAPTER pattern (digits only) the same way.
        assert_eq!(built("\\c 12b\n").chapters[1].number, 0);
    }

    /// The designator gate's unification: prose after `\v ` carves no
    /// designator, so `\v Then` is the row `\v \p` and `\v ⏎` already were —
    /// anchored at the MARKER, numbers 0, and no designator span to reach.
    #[test]
    fn a_verse_without_a_designator_is_one_row_shape() {
        for source in [
            "\\c 1\n\\p \\v Then He declared\n",
            "\\c 1\n\\p \\v \\p x\n",
        ] {
            let toc = built(source);
            let tokens = lex(source);
            let marker = toc.verses[0].token as usize;
            assert_eq!(toc.verses.len(), 1);
            assert_eq!((toc.verses[0].first, toc.verses[0].last), (0, 0));
            assert_eq!(toc.verses[0].at, tokens[marker].start);
            assert_eq!(&source[tokens[marker].start as usize..][..2], "\\v");
            assert_eq!(
                toc.verses[0].designator_span(source.as_bytes(), &tokens),
                None
            );
            // The row still splits the chapter: the byte after `\v` is inside it.
            assert_eq!(sid(source, toc.verses[0].at + 3), "### 1");
        }
    }

    #[test]
    fn a_book_with_no_id_renders_a_raw_sid() {
        let toc = built("\\c 1\n\\p \\v 1 a\n");
        assert_eq!(toc.book, [0, 0, 0]);
        assert_eq!(toc.book_token, None);
        assert_eq!(toc.locate(12).to_string(), "### 1:1");
        // The file opens on `\c`, so chapter 0 is empty and byte 0 is already
        // chapter 1.
        assert_eq!(toc.locate(0).to_string(), "### 1");
    }

    #[test]
    fn the_book_code_is_the_first_ids_first_three_bytes() {
        assert_eq!(built("\\id GEN\n").book, *b"GEN");
        // A BOM lexes as Text, so `\id` is not token 0.
        assert_eq!(built("\u{FEFF}\\id GEN\n").book, *b"GEN");
        assert_eq!(built("\\id GEN Some description\n").book, *b"GEN");
        // Kept verbatim: case, over-length and under-length alike.
        assert_eq!(built("\\id gen\n").book, *b"gen");
        assert_eq!(built("\\id GENESIS\n").book, *b"GEN");
        assert_eq!(built("\\id 1\n").book, [b'1', 0, 0]);
        assert_eq!(built("\\id 1\n").locate(0).to_string(), "1");
        // Later `\id`s are ordinary tokens.
        assert_eq!(built("\\id GEN\n\\id EXO\n").book, *b"GEN");
    }

    #[test]
    fn verse_extents_run_to_the_next_verse_or_chapter() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b\n\\c 2\n\\s head\n\\p \\v 1 c\n";
        let toc = built(source);
        // Chapter 2's heading is inside chapter 2 but ahead of its first verse.
        let heading = source.find("\\s head").unwrap() as u32;
        assert!(toc.verse_at(heading).is_none());
        assert_eq!(toc.locate(heading).to_string(), "GEN 2");
        assert_eq!(toc.locate(source.len() as u32 - 2).to_string(), "GEN 2:1");
        // Every anchor round-trips through the byte it names.
        for anchor in &toc.verses {
            let sid = toc.locate(anchor.at);
            assert_eq!(
                (sid.chapter, sid.first, sid.last),
                (anchor.chapter, anchor.first, anchor.last)
            );
        }
    }

    #[test]
    fn alternate_numbers_and_nested_spellings_open_nothing() {
        // `\ca`/`\cp`/`\va`/`\vp` carve designators but are not chapters or
        // verses; `\+c` is the nesting SPELLING, not a chapter marker.
        let source = "\\c 1\n\\ca 2\\ca*\n\\cp A\n\\p \\v 1 a \\va 2\\va*\n\\+c 3\n";
        let toc = built(source);
        assert_eq!(toc.chapters.len(), 2);
        assert_eq!(toc.verses.len(), 1);
    }

    #[test]
    fn a_front_position_attribute_list_does_not_hide_the_designator() {
        assert_eq!(built("\\c |x-a=\"b\"| 1\n").chapters[1].number, 1);
        let toc = built("\\c 1\n\\p \\v |x-a=\"b\"| 7 a\n");
        assert_eq!((toc.verses[0].first, toc.verses[0].last), (7, 7));
    }

    #[test]
    fn chapter_span_answers_by_number_and_prefers_the_first_run() {
        let source = "\\c 1\n\\p a\n\\c 2\n\\p b\n\\c 1\n\\p c\n";
        let toc = built(source);
        assert_eq!(toc.chapter_span(1), Some(0..10));
        assert_eq!(toc.chapter_span(2), Some(10..20));
        assert_eq!(toc.chapter_span(0), Some(0..0));
        assert_eq!(toc.chapter_span(9), None);
    }

    #[test]
    fn locate_is_total_past_the_end_of_the_source() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a\n";
        assert_eq!(sid(source, source.len() as u32), "GEN 1:1");
        assert_eq!(sid(source, u32::MAX), "GEN 1:1");
        assert_eq!(built("").locate(u32::MAX).to_string(), "###");
    }

    #[test]
    fn huge_numbers_saturate_into_the_row() {
        let source = format!("\\c {}\n\\p \\v {} a\n", "9".repeat(12), "9".repeat(12));
        let toc = built(&source);
        assert_eq!(toc.chapters[1].number, u16::MAX);
        assert_eq!(toc.verses[0].first, u16::MAX);
    }

    #[test]
    fn rows_are_fixed_width() {
        assert_eq!(size_of::<ChapterRow>(), 24);
        assert_eq!(size_of::<VerseAnchor>(), 28);
        assert_eq!(size_of::<VerseMember>(), 20);
    }

    fn text(source: &str, start: u32, end: u32) -> &str {
        &source[start as usize..end as usize]
    }

    /// Each verse as `(label, members)`, a member drawn `from-to` with its
    /// segments, the way a reader would render the row.
    fn verses_as_written(source: &str) -> Vec<(String, Vec<String>)> {
        let toc = built(source);
        toc.verses
            .iter()
            .map(|v| {
                let members = toc
                    .members_of(v)
                    .iter()
                    .map(|m| {
                        let from = format!(
                            "{}{}",
                            m.from,
                            text(source, m.from_segment_start, m.from_segment_end)
                        );
                        let to = format!(
                            "{}{}",
                            m.to,
                            text(source, m.to_segment_start, m.to_segment_end)
                        );
                        if from == to {
                            from
                        } else {
                            format!("{from}-{to}")
                        }
                    })
                    .collect();
                (
                    text(source, v.label_start, v.label_end).to_string(),
                    members,
                )
            })
            .collect()
    }

    #[test]
    fn a_verse_row_carries_its_label_and_what_it_covers() {
        let source = "\\c 1\n\\p \\v 1,3,5 a \\v 2 b \\v 12a-14b c \\v 7\" d \\v e\n";
        let owned = |label: &str, members: &[&str]| {
            (
                label.to_string(),
                members.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            verses_as_written(source),
            vec![
                owned("1,3,5", &["1", "3", "5"]),
                owned("2", &["2"]),
                owned("12a-14b", &["12a-14b"]),
                // Malformed: the spelling survives, it covers nothing.
                owned("7\"", &[]),
                // No designator: an empty label, no members.
                owned("", &[]),
            ]
        );
        // The hull is unchanged: a list still spans its holes.
        let toc = built(source);
        assert_eq!((toc.verses[0].first, toc.verses[0].last), (1, 5));
    }

    #[test]
    fn a_chapter_row_carries_its_label_even_when_the_number_is_malformed() {
        let source = "\\c 12b\n\\p a\n\\c 3\n";
        let toc = built(source);
        let labels: Vec<_> = toc
            .chapters
            .iter()
            .map(|c| (c.number, text(source, c.label_start, c.label_end)))
            .collect();
        assert_eq!(labels, vec![(0, ""), (0, "12b"), (3, "3")]);
    }

    #[test]
    fn lists_and_segments_leave_the_chapter_rows_tiling() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1,3 a \\v 2a b \\v 2b c\n\\c 2\n\\p \\v 1-2,4 d\n";
        let toc = built(source);
        assert_eq!(toc.chapters.first().unwrap().start, 0);
        assert_eq!(toc.chapters.last().unwrap().end, source.len() as u32);
        for pair in toc.chapters.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        // Every member run is inside the arena and in verse order.
        let mut next = 0;
        for verse in &toc.verses {
            assert_eq!(verse.members_from, next);
            next += u32::from(verse.members_len);
        }
        assert_eq!(next as usize, toc.members.len());
    }
}
