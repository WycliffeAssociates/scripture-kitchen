//! Literal search over a book's projection, with the hits placed back in the
//! source — markup gaps and all.
//!
//! ```text
//! source   \v 1 Jesus wept.\f + \ft why\f* Then…
//! mask     ·····Jesus wept.················ Then…      · = dropped
//!
//! Find::literal("wept. Then").in_projection(&mask, source)
//!   → Hit { projected: 6..16, source: Split([11..16, 31..36]) }
//!
//! Find::literal("why").in_projection(&mask, source)
//!   → nothing: the footnote is not in this view
//! ```
//!
//! Replacement stays with the caller, because a hit is already source ranges
//! and source ranges are what `onion::Edit` takes:
//!
//! ```text
//! SourceSpan::Contiguous(at) => Edit { from: at.start, to: at.end, insert }
//! SourceSpan::Split(pieces)  => the caller's call, and only the caller's:
//!                              keep the markup between the pieces, or not
//! ```
//!
//! `find.md` beside this file is the contract: what `Split` means, why the
//! whole-word rule is a restatement rather than a call, and what the fold
//! costs.

use std::ops::Range;

use memchr::memmem;
use mise::unicode::{Class, class_of};

use crate::onion::Mask;
use crate::pantry::{BookId, Entry, Pantry, Role};

// ------------------------------------------------------------------ the hit

/// Where one hit sits in the source, once the mask has had its say.
///
/// A hit that never leaves one kept range is `Contiguous`. A hit that crosses
/// a masked gap is `Split`, one range per contiguous source piece: the offset
/// map knows the markup is there, so nothing here hands back a single range
/// that would swallow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSpan {
    Contiguous(Range<u32>),
    Split(Box<[Range<u32>]>),
}

impl SourceSpan {
    /// The pieces in source order, one for `Contiguous`.
    pub fn pieces(&self) -> impl Iterator<Item = &Range<u32>> {
        match self {
            Self::Contiguous(range) => std::slice::from_ref(range).iter(),
            Self::Split(pieces) => pieces.iter(),
        }
    }

    /// The first kept byte.
    pub fn start(&self) -> u32 {
        match self {
            Self::Contiguous(range) => range.start,
            Self::Split(pieces) => pieces[0].start,
        }
    }

    /// One past the last kept byte. NOT `start() + needle.len()`: the dropped
    /// bytes between the pieces sit inside this interval.
    pub fn end(&self) -> u32 {
        match self {
            Self::Contiguous(range) => range.end,
            Self::Split(pieces) => pieces[pieces.len() - 1].end,
        }
    }

    /// Whether the hit crosses at least one masked gap.
    pub fn is_split(&self) -> bool {
        matches!(self, Self::Split(_))
    }
}

/// One match: where it sits in the projection, and where its bytes are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Half-open, in the mask's byte space — what a second search or a
    /// projected-coordinate consumer wants.
    pub projected: Range<u32>,
    pub source: SourceSpan,
}

// ----------------------------------------------------------------- the query

/// A literal needle plus its two options, prepared once and run over many
/// books.
///
/// Literal only: the `regex` crate is a galley dependency the day a consumer
/// asks for one, and not before.
pub struct Find<'n> {
    needle: &'n str,
    /// Over `needle`, or over its simple fold under `case_insensitive`.
    /// `'static` because a folded needle is owned here.
    finder: memmem::Finder<'static>,
    /// Byte length of whatever `finder` searches for.
    finder_len: usize,
    case_insensitive: bool,
    whole_word: bool,
}

impl<'n> Find<'n> {
    /// A case-sensitive, any-position search for `needle`.
    ///
    /// An empty needle matches nothing rather than matching everywhere.
    pub fn literal(needle: &'n str) -> Self {
        Self {
            needle,
            finder: memmem::Finder::new(needle.as_bytes()).into_owned(),
            finder_len: needle.len(),
            case_insensitive: false,
            whole_word: false,
        }
    }

    /// Fold both sides with the simple lowercase fold before comparing.
    ///
    /// The fold is one scalar per scalar — the first of `char::to_lowercase`
    /// — so `ß` stays itself and `İ` loses its dot, exactly as
    /// `sous_core::words` folds. It is a convention check, not a collator.
    pub fn case_insensitive(mut self, on: bool) -> Self {
        if on == self.case_insensitive {
            return self;
        }
        self.case_insensitive = on;
        let folded = on.then(|| fold(self.needle));
        let searched = folded.as_deref().unwrap_or(self.needle);
        self.finder_len = searched.len();
        self.finder = memmem::Finder::new(searched.as_bytes()).into_owned();
        self
    }

    /// Require a word boundary on both sides, under the words rule restated in
    /// this module — letters plus glue, extended through one medial nonletter
    /// with a letter either side. `find.md` says why it is a restatement.
    pub fn whole_word(mut self, on: bool) -> Self {
        self.whole_word = on;
        self
    }

    /// Every hit in one projection, leftmost-first.
    ///
    /// `source` must be the bytes `mask` was built from. The projection is
    /// materialized here and dropped with the iterator; the Pantry retains the
    /// mask, not the projected text.
    pub fn in_projection<'a>(&'a self, mask: &'a Mask, source: &[u8]) -> Hits<'a> {
        let projected = mask.text(source);
        let folded = self.case_insensitive.then(|| Folded::of(&projected));
        Hits {
            finder: &self.finder,
            finder_len: self.finder_len,
            whole_word: self.whole_word,
            empty: self.needle.is_empty(),
            mask,
            projected,
            folded,
            at: 0,
        }
    }

    /// Every hit in one registered book's verse-text projection.
    ///
    /// A book that retains no text and no projection — a reference registered
    /// without its text — has nothing to search and yields nothing.
    pub fn in_book<'a>(&'a self, book: &'a Entry<'_>) -> Hits<'a> {
        match (book.text(), book.mask()) {
            (Ok(text), Ok(mask)) => self.in_projection(mask, text.as_bytes()),
            _ => self.in_projection(&EMPTY_MASK, b""),
        }
    }

    /// Every hit in every book registered under `role`, in the Pantry's
    /// canonical book order. Books with no hit are left out.
    pub fn in_pantry(&self, pantry: &mut Pantry, role: Role) -> Vec<(BookId, Vec<Hit>)> {
        let ids: Vec<BookId> = pantry
            .books(role)
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        let mut out = Vec::new();
        for id in ids {
            let Some(entry) = pantry.book(&id) else {
                continue;
            };
            let hits: Vec<Hit> = self.in_book(&entry).collect();
            if !hits.is_empty() {
                out.push((id, hits));
            }
        }
        out
    }
}

/// The projection a text-less book searches: no ranges, so no hits.
static EMPTY_MASK: Mask = Mask {
    ranges: Vec::new(),
    starts: Vec::new(),
};

// ---------------------------------------------------------------- the search

/// The hits of one [`Find`] over one projection.
///
/// Owns the projected text, so the mask and the source it was built from need
/// not outlive the search.
pub struct Hits<'a> {
    finder: &'a memmem::Finder<'static>,
    finder_len: usize,
    whole_word: bool,
    empty: bool,
    mask: &'a Mask,
    projected: String,
    folded: Option<Folded>,
    /// Cursor into the haystack the finder reads.
    at: usize,
}

impl Hits<'_> {
    /// The projection these hits are placed in, borrowed rather than copied
    /// again.
    ///
    /// `Hit.projected` indexes THIS string, so a consumer that has to convert
    /// those offsets — to UTF-16 for an editor, or to a preview for a result
    /// card — reads them off here instead of masking the source a second time.
    /// The projection is materialized once per search and dropped with the
    /// iterator, so borrow it after collecting the hits and before letting the
    /// iterator go.
    pub fn projection(&self) -> &str {
        &self.projected
    }
}

impl Iterator for Hits<'_> {
    type Item = Hit;

    fn next(&mut self) -> Option<Hit> {
        if self.empty {
            return None;
        }
        let hay = self
            .folded
            .as_ref()
            .map_or(self.projected.as_bytes(), |f| f.text.as_bytes());
        loop {
            let found = self.at + self.finder.find(hay.get(self.at..)?)?;
            let end = found + self.finder_len;
            let (from, to) = match &self.folded {
                None => (found as u32, end as u32),
                Some(f) => (f.to_projected(found as u32), f.to_projected(end as u32)),
            };
            if self.whole_word && !is_whole_word(&self.projected, from as usize, to as usize) {
                // One byte, not one match: a rejected hit must not hide a
                // legal one that overlaps it.
                self.at = found + 1;
                continue;
            }
            self.at = end.max(found + 1);
            return Some(Hit {
                projected: from..to,
                source: locate(self.mask, from, to),
            });
        }
    }
}

/// The source pieces a projected interval covers, one per kept range it
/// touches.
fn locate(mask: &Mask, from: u32, to: u32) -> SourceSpan {
    let row = mask
        .starts
        .partition_point(|start| *start <= from)
        .saturating_sub(1);
    let range = &mask.ranges[row];
    let row_start = mask.starts[row];
    let row_end = row_start + (range.end - range.start);
    let first = range.start + (from - row_start)..range.start + (to.min(row_end) - row_start);
    // The overwhelmingly common shape, and the one that must not allocate:
    // a hit that never leaves the kept range it started in.
    if to <= row_end {
        return SourceSpan::Contiguous(first);
    }
    let mut pieces = vec![first];
    let mut cursor = row_end;
    let mut row = row + 1;
    while cursor < to {
        let range = &mask.ranges[row];
        let row_start = mask.starts[row];
        let row_end = row_start + (range.end - range.start);
        let stop = to.min(row_end);
        pieces.push(range.start..range.start + (stop - row_start));
        cursor = stop;
        row += 1;
    }
    SourceSpan::Split(pieces.into_boxed_slice())
}

// ------------------------------------------------------------------ the fold

/// A simple-folded copy of a projection, plus the offset map back.
///
/// The fold changes byte offsets — `İ` is two bytes and folds to one — but it
/// changes them at very few scalars, so the map records only the points where
/// the running difference moves. Pure-ASCII text records nothing and the
/// lookup is a length check.
struct Folded {
    text: String,
    /// `(folded offset, projected offset)` at every scalar where the running
    /// difference changes, ascending, plus a terminal entry when the text ends
    /// on a difference the last change did not already state.
    marks: Vec<(u32, u32)>,
}

impl Folded {
    fn of(projected: &str) -> Self {
        let mut text = String::with_capacity(projected.len());
        let mut marks = Vec::new();
        let mut delta = 0i64;
        for (at, scalar) in projected.char_indices() {
            let here = at as i64 - text.len() as i64;
            if here != delta {
                marks.push((text.len() as u32, at as u32));
                delta = here;
            }
            text.push(simple_fold(scalar));
        }
        if projected.len() as i64 - text.len() as i64 != delta {
            marks.push((text.len() as u32, projected.len() as u32));
        }
        Self { text, marks }
    }

    /// The projected offset of a folded SCALAR BOUNDARY. UTF-8 is
    /// self-synchronizing, so a match of valid UTF-8 is always on one.
    fn to_projected(&self, folded: u32) -> u32 {
        match self.marks.partition_point(|(at, _)| *at <= folded) {
            0 => folded,
            row => {
                let (at, projected) = self.marks[row - 1];
                projected + (folded - at)
            }
        }
    }
}

/// The simple lowercase fold of one scalar: the first of `char::to_lowercase`.
///
/// The ASCII arm is not an optimization of a different rule — `A`..`Z` lower to
/// `a`..`z` and nothing else moves — and it is what keeps the fold near memcpy
/// speed on Latin scripture, where the general path is a table lookup per
/// scalar (`find.md`, the fold row).
#[inline]
fn simple_fold(scalar: char) -> char {
    if scalar.is_ascii() {
        scalar.to_ascii_lowercase()
    } else {
        scalar.to_lowercase().next().unwrap_or(scalar)
    }
}

/// The simple lowercase fold, one scalar per scalar.
fn fold(text: &str) -> String {
    text.chars().map(simple_fold).collect()
}

// ------------------------------------------------------------- the word rule

/// A word's own scalars: letters, and the glue that rides them.
const fn is_letterish(class: Class) -> bool {
    class.is_alphabetic() || class.is_glue()
}

/// What a word is built from before the joiner rule extends it — a digit
/// beside a letter joins the word (`3rd`, `1Ki`).
const fn is_core(class: Class) -> bool {
    is_letterish(class) || class.is_decimal_digit()
}

/// A nonletter that can act as the one medial joiner: not a word scalar, not
/// whitespace, and not a digit (a digit confirms nothing).
const fn is_run_atom(class: Class) -> bool {
    !is_core(class) && !class.is_whitespace()
}

/// Whether `hay[from..to]` is a whole word under the words rule.
///
/// The rule is a maximal run of letters and glue, extended through ONE
/// nonletter that has a letter immediately on both sides — so the only
/// question at each edge is whether the scalar outside it continues the run,
/// and whether an outside nonletter is a confirmed joiner rather than a break.
fn is_whole_word(hay: &str, from: usize, to: usize) -> bool {
    let first = hay[from..to].chars().next();
    let last = hay[from..to].chars().next_back();

    let mut back = hay[..from].chars().rev();
    let left = match back.next() {
        None => true,
        Some(prev) => {
            let class = class_of(prev);
            if is_core(class) {
                false
            } else if !is_run_atom(class) {
                true
            } else {
                // A joiner needs a letter on BOTH sides; the inner side is the
                // hit's own first scalar.
                let outer = back.next().is_some_and(|c| is_letterish(class_of(c)));
                !(outer && first.is_some_and(|c| is_letterish(class_of(c))))
            }
        }
    };
    if !left {
        return false;
    }

    let mut ahead = hay[to..].chars();
    match ahead.next() {
        None => true,
        Some(next) => {
            let class = class_of(next);
            if is_core(class) {
                false
            } else if !is_run_atom(class) {
                true
            } else {
                let outer = ahead.next().is_some_and(|c| is_letterish(class_of(c)));
                !(outer && last.is_some_and(|c| is_letterish(class_of(c))))
            }
        }
    }
}

// ------------------------------------------------------------------ the wire

/// The hits of one search as one flat buffer — what a host outside Rust reads.
///
/// Find hands back Rust ranges in the mask's and the source's BYTE space. A
/// JavaScript host counts in UTF-16 and cannot see a `Vec<Hit>` at all, so the
/// crossing needs one encoding — and it lives here rather than in `wasm.rs`
/// because BOTH of Sefer's doors encode it: the browser through
/// [`crate::wasm::Galley::find`], the desktop through the same
/// [`Expediter::find`](crate::Expediter::find) linked natively. One format and
/// one encoder, so the two doors cannot drift.
///
/// ```text
/// u32   magic              0x444E4946 — "FIND", little-endian
/// u32   version            1
/// u32   hitCount
/// u32   bookCount
/// hit   × hitCount    bookIndex, projectedFrom, projectedTo, pieceCount,
///                     pieceCount × (sourceFrom, sourceTo)
/// u32   × bookCount   idByteLen
/// u32   × hitCount    previewByteLen
/// bytes               every id's UTF-8 in order, then every preview's
/// ```
///
/// The two leading words are what the onion and sous buffers both lead with,
/// and for the same reason: a reader that is a version behind fails on the
/// header rather than on a field it misread.
///
/// Little-endian `u32` throughout, and every offset is UTF-16 — the unit the
/// editor's coordinates are already in (`onion::wire`'s `utf16: true`). The
/// two length arrays come BEFORE the byte blob so that every `u32` in the
/// buffer is four-byte aligned and a reader can take a `Uint32Array` view over
/// the head of it.
///
/// `pieceCount` is the reason this is not two numbers per hit: a hit that
/// crosses a masked gap is one range per contiguous source piece, in order
/// (`find.md`, "why `Split` exists"). A consumer that replaces text has to
/// decide about the markup between the pieces; the buffer's job is to have
/// told it the gap is there.
///
/// The preview is the PROJECTED text around the hit — the reading a result
/// card shows. It is encoded here because the projection is materialized for
/// the search and dropped with it: a host that wanted the same string later
/// would have to mask the whole book again.
///
/// `galley/src/wasm.md` restates the layout for the reader on the other side.
pub mod wire {
    use mise::utf16::utf16_table;

    use super::Find;
    use crate::pantry::{BookId, Pantry};

    /// `FIND` in ASCII, read out of the buffer's first four bytes in order.
    pub const MAGIC: u32 = 0x444E_4946;

    /// The layout above. A reader that does not know this number stops.
    pub const VERSION: u32 = 1;

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
            let found: Vec<super::Hit> = hits.by_ref().take(ceiling - previews.len()).collect();
            if found.is_empty() {
                continue;
            }
            // One table per book that HAS a hit, off the projection the search
            // already materialized: a book nobody matched pays nothing.
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

        #[test]
        fn an_unregistered_id_contributes_nothing() {
            let mut pantry = Pantry::new(1 << 20);
            let ids = vec![BookId::from("never/registered.usfm")];
            let read = decode(&encode(&mut pantry, &ids, &Find::literal("Jesus"), 0));
            assert!(read.hits.is_empty());
            assert_eq!(read.ids, vec!["never/registered.usfm".to_string()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A whole-text identity projection: every byte kept, offsets unchanged.
    fn identity(text: &str) -> Mask {
        Mask {
            ranges: std::iter::once(0..text.len() as u32).collect(),
            starts: vec![0],
        }
    }

    fn hits(find: &Find<'_>, text: &str) -> Vec<Hit> {
        find.in_projection(&identity(text), text.as_bytes())
            .collect()
    }

    #[test]
    fn a_hit_inside_one_kept_range_is_contiguous() {
        let mask = Mask {
            ranges: vec![5..16, 31..39],
            starts: vec![0, 11],
        };
        let source = b"\\v 1 Jesus wept.\\f + \\ft why\\f* Then\xE2\x80\xA6";
        let found: Vec<Hit> = Find::literal("wept").in_projection(&mask, source).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].projected, 6..10);
        assert_eq!(found[0].source, SourceSpan::Contiguous(11..15));
    }

    #[test]
    fn a_hit_across_a_gap_is_split_per_piece() {
        let mask = Mask {
            ranges: vec![5..16, 31..39],
            starts: vec![0, 11],
        };
        let source = b"\\v 1 Jesus wept.\\f + \\ft why\\f* Then\xE2\x80\xA6";
        let found: Vec<Hit> = Find::literal("wept. Then")
            .in_projection(&mask, source)
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].projected, 6..16);
        assert_eq!(
            found[0].source,
            SourceSpan::Split(Box::new([11..16, 31..36]))
        );
        assert_eq!(found[0].source.start(), 11);
        assert_eq!(found[0].source.end(), 36);
    }

    #[test]
    fn an_empty_needle_matches_nothing() {
        assert!(hits(&Find::literal(""), "anything").is_empty());
    }

    #[test]
    fn whole_word_the_is_not_then() {
        let text = "the then other the";
        // `then` and the `the` inside `other` both match, and neither is a word.
        let all = hits(&Find::literal("the"), text);
        assert_eq!(all.len(), 4);
        let whole = hits(&Find::literal("the").whole_word(true), text);
        assert_eq!(
            whole
                .iter()
                .map(|h| h.projected.clone())
                .collect::<Vec<_>>(),
            vec![0..3, 15..18]
        );
    }

    #[test]
    fn one_medial_nonletter_joins_and_two_do_not() {
        // `mother-in-law` is one word, so its `in` is not whole; `a--b` is two
        // words, so the `b` is.
        let joined = hits(&Find::literal("in").whole_word(true), "mother-in-law");
        assert!(joined.is_empty());
        let split = hits(&Find::literal("b").whole_word(true), "a--b");
        assert_eq!(split.len(), 1);
        // A digit confirms nothing: `the-3` ends the word at the dash.
        let digit = hits(&Find::literal("the").whole_word(true), "the-3");
        assert_eq!(digit.len(), 1);
        // …but a digit beside a letter joins: `3rd` is one word.
        let riding = hits(&Find::literal("rd").whole_word(true), "3rd");
        assert!(riding.is_empty());
    }

    #[test]
    fn the_fold_maps_a_shortened_scalar_back() {
        // `İ` is two bytes and folds to one, so every offset after it moves.
        let text = "İ Melchizedek";
        let found = hits(&Find::literal("melchizedek").case_insensitive(true), text);
        assert_eq!(found.len(), 1);
        assert_eq!(&text[found[0].projected.start as usize..], "Melchizedek");
    }

    #[test]
    fn a_rejected_hit_does_not_hide_an_overlapping_one() {
        // `aa` at 0 is not a whole word; the one at 1 is.
        let found = hits(&Find::literal("aa").whole_word(true), "aaa aa");
        assert_eq!(
            found
                .iter()
                .map(|h| h.projected.clone())
                .collect::<Vec<_>>(),
            vec![4..6]
        );
    }
}
