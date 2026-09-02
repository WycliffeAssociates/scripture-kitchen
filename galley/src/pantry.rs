//! The pantry: what a book leaves behind after its text goes home.
//!
//! ```text
//! pantry.update("books/mrk.usfm", Role::Target, &text)?   -> BookKey MRK
//!     retained   chunk products (Warmer), Toc, verse-text Mask,
//!                detached UTF-16 table, published UTF-16 length, Fingerprint
//!     dropped    the string, and every borrow into it
//!
//! pantry.changed_since_update(&id, &edited)   -> [1_284..2_006]   // one chapter
//! pantry.books(Role::Target)                  -> [(id, GEN), (id, MRK)]
//! ```
//!
//! The id-keyed registry beneath lint, CST, and Sous: one whole-book
//! replacement per id, canonical `BookKey` order out, and no splice API.
//! Strings are never retained; products are. Why, and the two hashes' separate
//! jobs: `galley/src/pantry.md`.

use core::fmt;
use core::ops::Range;

use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::BookKey;
use xxhash_rust::xxh3::xxh3_128;

use crate::onion::{self, Filter, Mask, Toc, tables::books::BOOK_CODES};
use crate::utf16::{Utf16Table, utf16_table};
use crate::warmer::Warmer;

/// The caller's opaque book identity, kept consistent across updates — a file
/// path in practice.
///
/// Galley never parses it: two ids may carry the same `\id`, and the same id
/// may carry a different `\id` after an update.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BookId(String);

impl BookId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for BookId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl From<String> for BookId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for BookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The xxh3-128 of a book's RAW bytes — [`crate::CHECKSUM_VERSION`]'s hash,
/// unhexed.
///
/// Distinct from a Sous observation key on purpose: when the raw checksum moves
/// but the observation key does not, the observation is reused and only its
/// coordinates are rebased.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawChecksum([u8; 16]);

impl RawChecksum {
    pub fn of(bytes: &[u8]) -> Self {
        Self(xxh3_128(bytes).to_be_bytes())
    }

    pub fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Debug for RawChecksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Display for RawChecksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// What a registered corpus is FOR, which is what its retention costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Full detached products; publishes findings.
    ///
    /// `Reference` — TOC and per-verse observations only, no mask and no UTF-16
    /// table — is designed and lands with its first consumer, Stage 5
    /// proportionality.
    Target,
}

/// Chunk starts plus per-chunk raw checksums: ~1 KB per book, and no text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    /// Chunk `i` is `starts[i]..starts[i + 1]`, the last running to `len`.
    starts: Vec<u32>,
    /// Index-aligned with `starts`; keys ONLY its chunk's bytes, so a moved
    /// chapter keeps its checksum.
    checksums: Vec<RawChecksum>,
    len: u32,
    whole: RawChecksum,
}

/// One `onion::chunk::pre_scan` plus one xxh3-128 per chunk — [`crate::chunks`]
/// without the hex.
pub fn fingerprint(text: &str) -> Fingerprint {
    let bytes = text.as_bytes();
    let starts = onion::chunk::pre_scan(bytes).starts;
    let checksums = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = starts.get(i + 1).map_or(bytes.len(), |&next| next as usize);
            RawChecksum::of(&bytes[start as usize..end])
        })
        .collect();
    Fingerprint {
        starts,
        checksums,
        len: bytes.len() as u32,
        whole: RawChecksum::of(bytes),
    }
}

impl Fingerprint {
    /// Ranges in `current` whose chunk checksum this fingerprint never saw.
    ///
    /// SET MEMBERSHIP, not position: a chapter that only moved keeps its
    /// checksum, so its products stay valid and it is not reported.
    pub fn changed_chunks(&self, current: &Fingerprint) -> Vec<Range<u32>> {
        let seen: FxHashSet<RawChecksum> = self.checksums.iter().copied().collect();
        current
            .checksums
            .iter()
            .enumerate()
            .filter(|(_, checksum)| !seen.contains(checksum))
            .map(|(i, _)| current.chunk_range(i))
            .collect()
    }

    /// Whether the two byte strings differ at all.
    ///
    /// POSITIONAL, the other of the two questions: a moved chapter is a
    /// different file, so it is dirty even though it needs no rework.
    pub fn differs_from(&self, current: &Fingerprint) -> bool {
        self.whole != current.whole
    }

    /// The whole book's raw checksum.
    pub fn checksum(&self) -> RawChecksum {
        self.whole
    }

    /// Chunks in the text this fingerprint was taken from.
    pub fn chunk_count(&self) -> usize {
        self.starts.len()
    }

    fn chunk_range(&self, i: usize) -> Range<u32> {
        self.starts[i]..self.starts.get(i + 1).copied().unwrap_or(self.len)
    }

    fn resident_bytes(&self) -> usize {
        size_of_val(&self.starts[..]) + size_of_val(&self.checksums[..])
    }
}

/// One book's detached products; the assertion below is the compiler's word for
/// "nothing here borrows the text".
struct Book {
    key: BookKey,
    role: Role,
    checksum: RawChecksum,
    fingerprint: Fingerprint,
    toc: Toc,
    /// The verse-text projection, as source ranges plus their mask starts.
    mask: Mask,
    utf16: Utf16Table,
    /// The raw book's UTF-16 length — a publication's `published_len`.
    len_utf16: u32,
    bytes: usize,
}

const _: () = {
    const fn detached<T: 'static>() {}
    detached::<Book>();
};

/// The Galley-wide, id-keyed registry of detached per-book products.
///
/// Owns a [`Warmer`] for the chunk-level products and adds the book-level ones.
/// THE TEXT RULE: a method taking `&str` needs the current bytes and the caller
/// supplies them; a method that does not works from detached products alone.
pub struct Pantry {
    warmer: Warmer,
    books: FxHashMap<BookId, Book>,
    /// Canonical by [`BookKey`], ties by id — never insertion order.
    targets: Vec<(BookId, BookKey)>,
    derivations: u64,
}

impl Pantry {
    /// `budget_bytes` is the Warmer's LRU ceiling; the Pantry's own per-book
    /// products are retained until their id is removed.
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            warmer: Warmer::new(budget_bytes),
            books: FxHashMap::default(),
            targets: Vec::new(),
            derivations: 0,
        }
    }

    /// Derive and retain this book's products, then drop the text.
    ///
    /// Idempotent: text whose raw checksum matches the retained one derives
    /// nothing. Whole-book replacement is the only mutation, so coordinates
    /// cannot shift inside a book.
    pub fn update(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        text: &str,
    ) -> Result<BookKey, PantryError> {
        let id = id.into();
        let checksum = RawChecksum::of(text.as_bytes());
        if let Some(book) = self.books.get(&id)
            && book.checksum == checksum
            && book.role == role
        {
            return Ok(book.key);
        }

        let parsed = self.warmer.parsed(
            text,
            onion::wire::ParseOptions {
                toc: true,
                ..Default::default()
            },
        );
        let toc = parsed.toc.expect("toc requested");
        if toc.book_token.is_none() {
            return Err(PantryError::MissingBookKey { id });
        }
        let key = BookKey::new(toc.book);
        let mask = onion::mask(
            text.as_bytes(),
            &parsed.tokens,
            &parsed.cst,
            &Filter::verse_text(),
        );
        let utf16 = utf16_table(text.as_bytes());
        let print = fingerprint(text);
        let book = Book {
            key,
            role,
            checksum,
            len_utf16: utf16.len_utf16(),
            bytes: book_bytes(&toc, &mask, &utf16, &print, &id),
            fingerprint: print,
            toc,
            mask,
            utf16,
        };
        self.derivations += 1;
        let reordered = self
            .books
            .insert(id.clone(), book)
            .is_none_or(|old| old.key != key || old.role != role);
        if reordered {
            self.reorder();
        }
        Ok(key)
    }

    /// Drop a book's products. `false` when the id was not registered.
    pub fn remove(&mut self, id: &BookId) -> bool {
        let removed = self.books.remove(id).is_some();
        if removed {
            self.reorder();
        }
        removed
    }

    pub fn checksum_for(&self, id: &BookId) -> Option<RawChecksum> {
        self.books.get(id).map(|book| book.checksum)
    }

    pub fn fingerprint_for(&self, id: &BookId) -> Option<&Fingerprint> {
        self.books.get(id).map(|book| &book.fingerprint)
    }

    /// Ranges of `text` whose chunk checksum this book's last update never saw
    /// — the chunks a caller would have to re-derive.
    pub fn changed_since_update(&self, id: &BookId, text: &str) -> Option<Vec<Range<u32>>> {
        let book = self.books.get(id)?;
        Some(book.fingerprint.changed_chunks(&fingerprint(text)))
    }

    /// Registered books in canonical [`BookKey`] order, ties by id.
    pub fn books(&self, role: Role) -> &[(BookId, BookKey)] {
        match role {
            Role::Target => &self.targets,
        }
    }

    pub fn key_for(&self, id: &BookId) -> Option<BookKey> {
        self.books.get(id).map(|book| book.key)
    }

    pub fn role_for(&self, id: &BookId) -> Option<Role> {
        self.books.get(id).map(|book| book.role)
    }

    /// The retained verse-text projection: source ranges and their mask starts.
    pub fn mask_for(&self, id: &BookId) -> Option<&Mask> {
        self.books.get(id).map(|book| &book.mask)
    }

    pub fn toc_for(&self, id: &BookId) -> Option<&Toc> {
        self.books.get(id).map(|book| &book.toc)
    }

    /// The retained byte → UTF-16 table, valid against the exact text of the
    /// last update.
    pub fn utf16_for(&self, id: &BookId) -> Option<&Utf16Table> {
        self.books.get(id).map(|book| &book.utf16)
    }

    /// The raw book's UTF-16 length — a publication's `published_len`.
    pub fn published_len_for(&self, id: &BookId) -> Option<u32> {
        self.books.get(id).map(|book| book.len_utf16)
    }

    /// The Warmer's resident products plus the Pantry's own detached ones.
    pub fn resident_bytes(&self) -> usize {
        self.warmer.resident_bytes() + self.books.values().map(|book| book.bytes).sum::<usize>()
    }

    /// Books derived rather than served from the retained products, cumulative
    /// — the number that proves an idempotent update did no work.
    pub fn derivations(&self) -> u64 {
        self.derivations
    }

    /// Read-only, for the chunk-level miss and residency counters.
    pub fn warmer(&self) -> &Warmer {
        &self.warmer
    }

    fn reorder(&mut self) {
        self.targets = self
            .books
            .iter()
            .filter(|(_, book)| book.role == Role::Target)
            .map(|(id, book)| (id.clone(), book.key))
            .collect();
        self.targets
            .sort_unstable_by(|(left_id, left), (right_id, right)| {
                canonical_rank(*left)
                    .cmp(&canonical_rank(*right))
                    .then_with(|| left.as_bytes().cmp(&right.as_bytes()))
                    .then_with(|| left_id.cmp(right_id))
            });
    }
}

/// A book's position in the spec's book-identifier order, which Onion owns;
/// codes outside the table sort after every code inside it, by their bytes.
fn canonical_rank(key: BookKey) -> usize {
    let bytes = key.as_bytes();
    BOOK_CODES
        .iter()
        .position(|code| **code == bytes)
        .unwrap_or(BOOK_CODES.len())
}

/// Estimated resident size of one book's detached products.
fn book_bytes(
    toc: &Toc,
    mask: &Mask,
    utf16: &Utf16Table,
    print: &Fingerprint,
    id: &BookId,
) -> usize {
    size_of_val(&toc.chapters[..])
        + size_of_val(&toc.verses[..])
        + size_of_val(&mask.ranges[..])
        + size_of_val(&mask.starts[..])
        + utf16.index_bytes()
        + print.resident_bytes()
        + id.as_str().len()
        + size_of::<Book>()
}

/// Why an update could not be retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PantryError {
    /// The text has no `\id` line to key it.
    MissingBookKey { id: BookId },
}

impl fmt::Display for PantryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBookKey { id } => write!(f, "book {id} has no \\id line to key it"),
        }
    }
}

impl std::error::Error for PantryError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six chapters of a plausible book, one `\c` per chunk.
    fn book(code: &str, chapters: &[&str]) -> String {
        let mut text = format!("\\id {code}\n\\h {code}\n");
        for (number, body) in chapters.iter().enumerate() {
            text.push_str(&format!("\\c {}\n\\p\n\\v 1 {body}\n", number + 1));
        }
        text
    }

    fn mark() -> String {
        book(
            "MRK",
            &[
                "In the beginning of the good news.",
                "He entered Capernaum again.",
                "A man with a withered hand.",
                "The sower went out to sow.",
                "They came to the other side.",
                "Is this not the carpenter?",
            ],
        )
    }

    /// Chunk ranges of `text`, from the same pre-scan the fingerprint uses.
    fn chunk_ranges(text: &str) -> Vec<Range<u32>> {
        let starts = onion::chunk::pre_scan(text.as_bytes()).starts;
        (0..starts.len())
            .map(|i| starts[i]..starts.get(i + 1).copied().unwrap_or(text.len() as u32))
            .collect()
    }

    /// The oracle `changed_chunks` is measured against: chunks of `current`
    /// whose BYTES appear nowhere in `baseline`.
    fn rework_by_bytes(baseline: &str, current: &str) -> Vec<Range<u32>> {
        let seen: Vec<&str> = chunk_ranges(baseline)
            .into_iter()
            .map(|range| &baseline[range.start as usize..range.end as usize])
            .collect();
        chunk_ranges(current)
            .into_iter()
            .filter(|range| !seen.contains(&&current[range.start as usize..range.end as usize]))
            .collect()
    }

    fn changed(baseline: &str, current: &str) -> Vec<Range<u32>> {
        fingerprint(baseline).changed_chunks(&fingerprint(current))
    }

    #[test]
    fn a_fingerprint_carries_the_same_checksums_the_hex_recipe_reports() {
        let text = mark();
        let print = fingerprint(&text);
        let hex = crate::chunks(&text);
        assert_eq!(print.chunk_count(), hex.starts.len());
        for (i, checksum) in hex.checksums.iter().enumerate() {
            assert_eq!(format!("{:?}", print.checksums[i]), *checksum);
        }
        assert_eq!(print.checksum(), RawChecksum::of(text.as_bytes()));
    }

    #[test]
    fn an_edited_chapter_is_the_only_rework_range() {
        let before = mark();
        let after = before.replace("The sower went out to sow.", "The sower went out to sow!");
        let ranges = changed(&before, &after);
        assert_eq!(ranges, rework_by_bytes(&before, &after));
        assert_eq!(ranges.len(), 1);
        let edited = &after[ranges[0].start as usize..ranges[0].end as usize];
        assert!(edited.starts_with("\\c 4\n"), "{edited:?}");
        assert!(fingerprint(&before).differs_from(&fingerprint(&after)));
    }

    #[test]
    fn an_inserted_chapter_is_the_only_rework_range() {
        let before = mark();
        let after = before.replace("\\c 5\n", "\\c 4b\n\\p\n\\v 1 An interpolation.\n\\c 5\n");
        let ranges = changed(&before, &after);
        assert_eq!(ranges, rework_by_bytes(&before, &after));
        assert_eq!(ranges.len(), 1);
        let inserted = &after[ranges[0].start as usize..ranges[0].end as usize];
        assert!(inserted.starts_with("\\c 4b\n"), "{inserted:?}");
    }

    #[test]
    fn a_moved_chapter_is_dirty_but_needs_no_rework() {
        let before = mark();
        let chunks = chunk_ranges(&before);
        let (second, third) = (chunks[2].clone(), chunks[3].clone());
        let mut after = before[..second.start as usize].to_string();
        after.push_str(&before[third.start as usize..third.end as usize]);
        after.push_str(&before[second.start as usize..second.end as usize]);
        after.push_str(&before[third.end as usize..]);

        assert_eq!(changed(&before, &after), Vec::new(), "no rework");
        assert_eq!(changed(&before, &after), rework_by_bytes(&before, &after));
        assert!(
            fingerprint(&before).differs_from(&fingerprint(&after)),
            "a moved chapter is a different file"
        );
    }

    #[test]
    fn identical_text_is_neither_dirty_nor_rework() {
        let text = mark();
        assert_eq!(changed(&text, &text), Vec::new());
        assert!(!fingerprint(&text).differs_from(&fingerprint(&text)));
    }

    fn pantry() -> Pantry {
        Pantry::new(1 << 20)
    }

    #[test]
    fn an_identical_update_derives_nothing() {
        let mut pantry = pantry();
        let text = mark();
        let id = BookId::from("books/mrk.usfm");
        assert_eq!(
            pantry.update(id.clone(), Role::Target, &text).unwrap(),
            BookKey::new(*b"MRK")
        );
        let (derivations, misses) = (pantry.derivations(), pantry.warmer().misses());
        assert_eq!(
            pantry.update(id, Role::Target, &text).unwrap(),
            BookKey::new(*b"MRK")
        );
        assert_eq!(pantry.derivations(), derivations, "no book derivation");
        assert_eq!(pantry.warmer().misses(), misses, "no chunk work");
    }

    #[test]
    fn a_changed_update_replaces_the_products() {
        let mut pantry = pantry();
        let id = BookId::from("books/mrk.usfm");
        let before = mark();
        pantry.update(id.clone(), Role::Target, &before).unwrap();
        let first = pantry.checksum_for(&id).unwrap();
        let published = pantry.published_len_for(&id).unwrap();

        let after = before.replace(
            "A man with a withered hand.",
            "A man with a withered hand 🖐.",
        );
        let rework = pantry.changed_since_update(&id, &after).unwrap();
        assert_eq!(rework, rework_by_bytes(&before, &after));
        assert_eq!(rework.len(), 1);

        pantry.update(id.clone(), Role::Target, &after).unwrap();
        assert_ne!(pantry.checksum_for(&id).unwrap(), first);
        assert_eq!(
            pantry.checksum_for(&id).unwrap(),
            RawChecksum::of(after.as_bytes())
        );
        // The inserted " 🖐" is a space plus a surrogate pair: three units.
        assert_eq!(pantry.published_len_for(&id).unwrap(), published + 3);
        assert_eq!(
            pantry.changed_since_update(&id, &after).unwrap(),
            Vec::new(),
            "the update moved the baseline"
        );
    }

    #[test]
    fn the_retained_products_answer_without_the_text() {
        let mut pantry = pantry();
        let text = mark();
        let id = BookId::from("books/mrk.usfm");
        pantry.update(id.clone(), Role::Target, &text).unwrap();

        let mask = pantry.mask_for(&id).unwrap();
        let projected = mask.text(text.as_bytes());
        assert!(
            !projected.contains('\\'),
            "no markup survives: {projected:?}"
        );
        assert_eq!(
            projected.lines().filter(|line| !line.is_empty()).count(),
            6,
            "six verses"
        );
        let numbers: Vec<u16> = pantry
            .toc_for(&id)
            .unwrap()
            .chapters
            .iter()
            .map(|row| row.number)
            .collect();
        assert_eq!(numbers, vec![0, 1, 2, 3, 4, 5, 6], "front matter plus six");
        let utf16 = pantry.utf16_for(&id).unwrap();
        let index = onion::utf16_index(text.as_bytes());
        for range in &mask.ranges {
            assert_eq!(utf16.to_utf16(range.start), index.to_utf16(range.start));
            assert_eq!(utf16.to_utf16(range.end), index.to_utf16(range.end));
        }
        assert_eq!(pantry.published_len_for(&id).unwrap(), index.len_utf16());
    }

    #[test]
    fn books_are_canonically_ordered_whatever_the_update_order() {
        let mut pantry = pantry();
        let revelation = book("REV", &["A revelation of Jesus Christ."]);
        let genesis = book("GEN", &["In the beginning."]);
        let mark = mark();
        // Two ids carrying the same \id are both present, ordered by id.
        pantry
            .update("z/rev.usfm", Role::Target, &revelation)
            .unwrap();
        pantry.update("b/mrk.usfm", Role::Target, &mark).unwrap();
        pantry.update("a/gen.usfm", Role::Target, &genesis).unwrap();
        pantry
            .update("a/gen-copy.usfm", Role::Target, &genesis)
            .unwrap();

        let listed: Vec<(&str, BookKey)> = pantry
            .books(Role::Target)
            .iter()
            .map(|(id, key)| (id.as_str(), *key))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("a/gen-copy.usfm", BookKey::new(*b"GEN")),
                ("a/gen.usfm", BookKey::new(*b"GEN")),
                ("b/mrk.usfm", BookKey::new(*b"MRK")),
                ("z/rev.usfm", BookKey::new(*b"REV")),
            ]
        );
    }

    #[test]
    fn remove_drops_the_products() {
        let mut pantry = pantry();
        let id = BookId::from("books/mrk.usfm");
        pantry.update(id.clone(), Role::Target, &mark()).unwrap();
        let resident = pantry.resident_bytes();
        assert!(resident > 0);

        assert!(pantry.remove(&id));
        assert!(pantry.resident_bytes() < resident);
        assert_eq!(pantry.checksum_for(&id), None);
        assert_eq!(pantry.fingerprint_for(&id), None);
        assert!(pantry.mask_for(&id).is_none());
        assert!(pantry.toc_for(&id).is_none());
        assert!(pantry.utf16_for(&id).is_none());
        assert_eq!(pantry.changed_since_update(&id, &mark()), None);
        assert!(pantry.books(Role::Target).is_empty());
        assert!(!pantry.remove(&id), "already gone");
    }

    #[test]
    fn a_book_with_no_id_line_is_refused() {
        let mut pantry = pantry();
        assert_eq!(
            pantry.update("scratch.usfm", Role::Target, "\\c 1\n\\p\n\\v 1 keyless\n"),
            Err(PantryError::MissingBookKey {
                id: BookId::from("scratch.usfm")
            })
        );
        assert!(pantry.books(Role::Target).is_empty());
    }
}
