//! The pantry: what a book leaves behind, and the text it keeps beside it.
//!
//! ```text
//! pantry.update("books/mrk.usfm", Role::Target, &text)?   -> Entry for MRK
//!     .lint()          the Warmer's report, fed from the retained text
//!     .mask()          the verse-text projection, no text needed
//!     .text()          Ok(&str) — or Err(NoText) under Retain::ProductsOnly
//!
//! pantry.book(&id)                            -> Option<Entry<'_>>
//! pantry.changed_since_update(&id, &edited)   -> [1_284..2_006]   // one chapter
//! pantry.books(Role::Target)                  -> [(id, GEN), (id, MRK)]
//! ```
//!
//! The id-keyed registry beneath lint, CST, and Sous: one whole-book
//! replacement per id, canonical `BookKey` order out, and no splice API. A
//! target keeps its text as last updated unless the host opts out. Why, and
//! the two hashes' separate jobs: `galley/src/pantry.md`.

use core::fmt;
use core::ops::Range;

use mise::books::{BookKey, canonical_rank};
use mise::utf16::{Utf16Table, utf16_table};
use rustc_hash::{FxHashMap, FxHashSet};
use xxhash_rust::xxh3::xxh3_128;

use crate::onion::{self, Filter, Mask, Toc, lint::LintReport};
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
    /// Full detached products, the text beside them, and it publishes findings.
    ///
    /// `Reference` — TOC and per-verse observations only, no mask and no UTF-16
    /// table — is designed and lands with its first consumer, Stage 5
    /// proportionality.
    Target,
}

/// Whether a target keeps its text beside its products.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Retain {
    /// The text as last updated, so nothing is resent per call.
    #[default]
    Text,
    /// No text: the host holds it, and text-needing methods refuse rather than
    /// quietly answering from nothing.
    ProductsOnly,
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

    /// Heap bytes this fingerprint occupies — capacity, not length.
    pub fn resident_bytes(&self) -> usize {
        self.starts.capacity() * size_of::<u32>()
            + self.checksums.capacity() * size_of::<RawChecksum>()
    }
}

/// One book's detached products, plus the text they came from under
/// [`Retain::Text`]; the assertion below is the compiler's word for "nothing
/// here borrows".
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
    /// The text as last updated; `None` under [`Retain::ProductsOnly`].
    text: Option<String>,
    bytes: usize,
}

const _: () = {
    const fn detached<T: 'static>() {}
    detached::<Book>();
};

impl Book {
    /// What the last update kept, read back off the text rather than stored
    /// twice.
    fn retain(&self) -> Retain {
        if self.text.is_some() {
            Retain::Text
        } else {
            Retain::ProductsOnly
        }
    }
}

/// The Galley-wide, id-keyed registry of detached per-book products.
///
/// Owns a [`Warmer`] for the chunk-level products and adds the book-level ones.
/// THE TEXT RULE: a target keeps its text, so [`update`](Self::update) is the
/// only method that has to be GIVEN it — the one other `&str` argument,
/// [`changed_since_update`](Self::changed_since_update), takes a candidate
/// because a diff names both its sides.
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

    /// Derive and retain this book's products and its text.
    ///
    /// [`update_with`](Self::update_with) under [`Retain::Text`].
    pub fn update(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        text: &str,
    ) -> Result<Entry<'_>, PantryError> {
        self.update_with(id, role, Retain::Text, text)
    }

    /// Derive and retain this book's products, keeping or dropping its text.
    ///
    /// Idempotent: text whose raw checksum, role and retain mode all match the
    /// retained ones derives nothing and copies nothing. Whole-book replacement
    /// is the only mutation, so coordinates cannot shift inside a book.
    pub fn update_with(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        retain: Retain,
        text: &str,
    ) -> Result<Entry<'_>, PantryError> {
        let id = id.into();
        let checksum = RawChecksum::of(text.as_bytes());
        let served = self.books.get(&id).is_some_and(|book| {
            book.checksum == checksum && book.role == role && book.retain() == retain
        });
        if served {
            return Ok(Entry { pantry: self, id });
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
        let kept = match retain {
            Retain::Text => Some(text.to_string()),
            Retain::ProductsOnly => None,
        };
        let book = Book {
            key,
            role,
            checksum,
            len_utf16: utf16.len_utf16(),
            bytes: book_bytes(&toc, &mask, &utf16, &print, &id, kept.as_deref()),
            fingerprint: print,
            toc,
            mask,
            utf16,
            text: kept,
        };
        self.derivations += 1;
        let reordered = self
            .books
            .insert(id.clone(), book)
            .is_none_or(|old| old.key != key || old.role != role);
        if reordered {
            self.reorder();
        }
        Ok(Entry { pantry: self, id })
    }

    /// A handle on one registered book, or `None` when the id is unknown.
    ///
    /// `&mut` because [`Entry::lint`] and [`Entry::parse`] run the Warmer.
    pub fn book(&mut self, id: &BookId) -> Option<Entry<'_>> {
        if !self.books.contains_key(id) {
            return None;
        }
        Some(Entry {
            pantry: self,
            id: id.clone(),
        })
    }

    /// Drop a book's products and its text. `false` when the id was not
    /// registered.
    pub fn remove(&mut self, id: &BookId) -> bool {
        let removed = self.books.remove(id).is_some();
        if removed {
            self.reorder();
        }
        removed
    }

    /// Ranges of `text` whose chunk checksum this book's last update never saw
    /// — the chunks a caller would have to re-derive.
    ///
    /// Takes text because it names BOTH sides of the diff: the retained
    /// baseline, and a candidate the host has not sent yet.
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

    /// The Warmer's resident products plus the Pantry's own — detached
    /// products and retained text alike.
    pub fn resident_bytes(&self) -> usize {
        self.warmer.resident_bytes() + self.books.values().map(|book| book.bytes).sum::<usize>()
    }

    /// Bytes of text retained across every registered book — zero for a book
    /// under [`Retain::ProductsOnly`]. The other component of
    /// [`resident_bytes`](Self::resident_bytes) not already reachable
    /// through [`warmer`](Self::warmer): what's left is `resident_bytes()
    /// - warmer().resident_bytes() - text_bytes()`, the Pantry's own
    /// per-book products (`Toc`, `Mask`, `Utf16Table`, `Fingerprint`).
    pub fn text_bytes(&self) -> usize {
        self.books
            .values()
            .map(|book| book.text.as_deref().map_or(0, str::len))
            .sum()
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

    /// One book's retained products, borrowed together.
    ///
    /// Crate-private, because publication reads every book at once where an
    /// [`Entry`] borrows the whole Pantry mutably for one.
    pub(crate) fn products(&self, id: &BookId) -> Option<Products<'_>> {
        self.books.get(id).map(|book| Products {
            checksum: book.checksum,
            toc: &book.toc,
            mask: &book.mask,
            utf16: &book.utf16,
            published_len: book.len_utf16,
            text: book.text.as_deref(),
        })
    }

    /// The Warmer's lint over one book's retained text. Split-borrowed, which
    /// is why this is not `Entry::text` plus a call.
    fn lint_book(&mut self, id: &BookId) -> Result<LintReport, PantryError> {
        let text = retained(&self.books, id)?;
        Ok(self.warmer.lint(text))
    }

    /// The Warmer's parse over one book's retained text. See
    /// [`lint_book`](Self::lint_book).
    fn parse_book(
        &mut self,
        id: &BookId,
        opts: onion::wire::ParseOptions,
    ) -> Result<Vec<u8>, PantryError> {
        let text = retained(&self.books, id)?;
        Ok(self.warmer.parse(text, opts))
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

/// One book's retained products, borrowed for one read.
pub(crate) struct Products<'a> {
    pub(crate) checksum: RawChecksum,
    pub(crate) toc: &'a Toc,
    pub(crate) mask: &'a Mask,
    pub(crate) utf16: &'a Utf16Table,
    /// The raw book's UTF-16 length — a publication's `published_len`.
    pub(crate) published_len: u32,
    /// `None` under [`Retain::ProductsOnly`].
    pub(crate) text: Option<&'a str>,
}

/// One book, one handle: `pantry.update(id, role, &text)?.lint()`.
///
/// Borrows the Pantry MUTABLY, because the Warmer pass-throughs run the cache;
/// the read-only accessors ride along rather than pay for a second handle type.
pub struct Entry<'p> {
    pantry: &'p mut Pantry,
    id: BookId,
}

impl Entry<'_> {
    pub fn key(&self) -> BookKey {
        self.book().key
    }

    pub fn role(&self) -> Role {
        self.book().role
    }

    /// The xxh3-128 of the raw bytes of the last update.
    pub fn checksum(&self) -> RawChecksum {
        self.book().checksum
    }

    /// Chunk starts plus per-chunk checksums — the baseline for the next
    /// update.
    pub fn fingerprint(&self) -> &Fingerprint {
        &self.book().fingerprint
    }

    /// The text as last updated, or the refusal a `ProductsOnly` book answers
    /// with.
    pub fn text(&self) -> Result<&str, PantryError> {
        retained(&self.pantry.books, &self.id)
    }

    /// The retained verse-text projection: source ranges and their mask starts.
    pub fn mask(&self) -> &Mask {
        &self.book().mask
    }

    pub fn toc(&self) -> &Toc {
        &self.book().toc
    }

    /// The retained byte → UTF-16 table, valid against the exact text of the
    /// last update.
    pub fn utf16(&self) -> &Utf16Table {
        &self.book().utf16
    }

    /// The raw book's UTF-16 length — a publication's `published_len`.
    pub fn published_len(&self) -> u32 {
        self.book().len_utf16
    }

    /// [`Warmer::lint`] over the retained text.
    pub fn lint(&mut self) -> Result<LintReport, PantryError> {
        self.pantry.lint_book(&self.id)
    }

    /// [`Warmer::parse`] over the retained text.
    pub fn parse(&mut self, opts: onion::wire::ParseOptions) -> Result<Vec<u8>, PantryError> {
        self.pantry.parse_book(&self.id, opts)
    }

    /// An entry is only ever built for a registered id, and it holds the
    /// Pantry exclusively, so nothing can remove the book underneath it.
    fn book(&self) -> &Book {
        self.pantry.books.get(&self.id).expect("registered id")
    }
}

/// One book's text, keyed off a borrow of the map alone so a caller can hold
/// `&mut self.warmer` at the same time.
fn retained<'b>(books: &'b FxHashMap<BookId, Book>, id: &BookId) -> Result<&'b str, PantryError> {
    books
        .get(id)
        .and_then(|book| book.text.as_deref())
        .ok_or_else(|| PantryError::NoText { id: id.clone() })
}

/// Resident size of one book's detached products and retained text —
/// capacity, not length, since capacity is what a `Vec` actually holds on
/// the heap.
fn book_bytes(
    toc: &Toc,
    mask: &Mask,
    utf16: &Utf16Table,
    print: &Fingerprint,
    id: &BookId,
    text: Option<&str>,
) -> usize {
    toc.chapters.capacity() * size_of::<onion::ChapterRow>()
        + toc.verses.capacity() * size_of::<onion::VerseAnchor>()
        + mask.ranges.capacity() * size_of::<Range<u32>>()
        + mask.starts.capacity() * size_of::<u32>()
        + utf16.index_bytes()
        + print.resident_bytes()
        + id.as_str().len()
        + text.map_or(0, str::len)
        + size_of::<Book>()
}

/// Why an update could not be retained, or a retained book could not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PantryError {
    /// The text has no `\id` line to key it.
    MissingBookKey { id: BookId },
    /// The book was registered [`Retain::ProductsOnly`], so the host holds its
    /// text and this operation needs it.
    NoText { id: BookId },
}

impl fmt::Display for PantryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBookKey { id } => write!(f, "book {id} has no \\id line to key it"),
            Self::NoText { id } => write!(f, "book {id} retains no text"),
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

    fn mrk() -> BookId {
        BookId::from("books/mrk.usfm")
    }

    #[test]
    fn an_identical_update_derives_nothing_and_recopies_no_text() {
        let mut pantry = pantry();
        let text = mark();
        let id = mrk();
        let first = pantry.update(id.clone(), Role::Target, &text).unwrap();
        assert_eq!(first.key(), BookKey::new(*b"MRK"));
        let retained = first.text().unwrap().as_ptr();
        let (derivations, misses) = (pantry.derivations(), pantry.warmer().misses());

        let again = pantry.update(id, Role::Target, &text).unwrap();
        assert_eq!(again.key(), BookKey::new(*b"MRK"));
        assert_eq!(
            again.text().unwrap().as_ptr(),
            retained,
            "text not recopied"
        );
        assert_eq!(pantry.derivations(), derivations, "no book derivation");
        assert_eq!(pantry.warmer().misses(), misses, "no chunk work");
    }

    #[test]
    fn a_changed_update_replaces_the_products() {
        let mut pantry = pantry();
        let id = mrk();
        let before = mark();
        let entry = pantry.update(id.clone(), Role::Target, &before).unwrap();
        let first = entry.checksum();
        let published = entry.published_len();

        let after = before.replace(
            "A man with a withered hand.",
            "A man with a withered hand 🖐.",
        );
        let rework = pantry.changed_since_update(&id, &after).unwrap();
        assert_eq!(rework, rework_by_bytes(&before, &after));
        assert_eq!(rework.len(), 1);

        let entry = pantry.update(id.clone(), Role::Target, &after).unwrap();
        assert_ne!(entry.checksum(), first);
        assert_eq!(entry.checksum(), RawChecksum::of(after.as_bytes()));
        assert_eq!(
            entry.text().unwrap(),
            after,
            "the copy moved with the update"
        );
        // The inserted " 🖐" is a space plus a surrogate pair: three units.
        assert_eq!(entry.published_len(), published + 3);
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
        let entry = pantry.update(mrk(), Role::Target, &text).unwrap();

        let projected = entry.mask().text(text.as_bytes());
        assert!(
            !projected.contains('\\'),
            "no markup survives: {projected:?}"
        );
        assert_eq!(
            projected.lines().filter(|line| !line.is_empty()).count(),
            6,
            "six verses"
        );
        let numbers: Vec<u16> = entry.toc().chapters.iter().map(|row| row.number).collect();
        assert_eq!(numbers, vec![0, 1, 2, 3, 4, 5, 6], "front matter plus six");
        let index = onion::utf16_index(text.as_bytes());
        for range in &entry.mask().ranges {
            assert_eq!(
                entry.utf16().to_utf16(range.start),
                index.to_utf16(range.start)
            );
            assert_eq!(entry.utf16().to_utf16(range.end), index.to_utf16(range.end));
        }
        assert_eq!(entry.published_len(), index.len_utf16());
    }

    #[test]
    fn a_target_retains_its_text_by_default() {
        let mut pantry = pantry();
        let text = mark();
        let mut entry = pantry.update(mrk(), Role::Target, &text).unwrap();
        assert_eq!(entry.text().unwrap(), text);
        assert_eq!(entry.role(), Role::Target);
        assert_eq!(
            entry.lint().unwrap().observations,
            Warmer::new(1 << 20).lint(&text).observations
        );
    }

    #[test]
    fn products_only_refuses_the_text_and_everything_that_needs_it() {
        let mut pantry = pantry();
        let id = mrk();
        let mut entry = pantry
            .update_with(id.clone(), Role::Target, Retain::ProductsOnly, &mark())
            .unwrap();
        let refused = Err(PantryError::NoText { id: id.clone() });
        assert_eq!(entry.text(), refused);
        assert_eq!(entry.lint().err(), refused.clone().err());
        assert_eq!(
            entry.parse(onion::wire::ParseOptions::default()).err(),
            refused.err()
        );
        // The detached products never needed the string in the first place.
        assert!(!entry.mask().ranges.is_empty());
        assert_eq!(entry.toc().chapters.len(), 7);
        assert!(entry.utf16().len_utf16() > 0);
        assert_eq!(entry.key(), BookKey::new(*b"MRK"));
    }

    #[test]
    fn a_retained_lint_equals_the_warmers_own() {
        let mut pantry = pantry();
        let text = mark();
        let through_entry = pantry
            .update(mrk(), Role::Target, &text)
            .unwrap()
            .lint()
            .unwrap();
        let direct = Warmer::new(1 << 20).lint(&text);
        assert_eq!(through_entry.observations, direct.observations);

        let opts = onion::wire::ParseOptions {
            toc: true,
            ..Default::default()
        };
        let plated = pantry.book(&mrk()).unwrap().parse(opts).unwrap();
        assert_eq!(plated, Warmer::new(1 << 20).parse(&text, opts));
    }

    #[test]
    fn retention_is_what_resident_bytes_grows_by() {
        let text = mark();
        let mut kept = pantry();
        kept.update(mrk(), Role::Target, &text).unwrap();
        let mut dropped = pantry();
        dropped
            .update_with(mrk(), Role::Target, Retain::ProductsOnly, &text)
            .unwrap();

        let difference = kept.resident_bytes() - dropped.resident_bytes();
        assert_eq!(difference, text.len(), "the text and nothing else");
    }

    #[test]
    fn switching_retention_mode_rederives() {
        let mut pantry = pantry();
        let text = mark();
        pantry.update(mrk(), Role::Target, &text).unwrap();
        let derivations = pantry.derivations();

        pantry
            .update_with(mrk(), Role::Target, Retain::ProductsOnly, &text)
            .unwrap();
        assert_eq!(
            pantry.derivations(),
            derivations + 1,
            "a different retention"
        );
        assert!(pantry.book(&mrk()).unwrap().text().is_err());
    }

    #[test]
    fn book_answers_for_a_registered_id_only() {
        let mut pantry = pantry();
        let id = mrk();
        let key = pantry
            .update(id.clone(), Role::Target, &mark())
            .unwrap()
            .key();
        assert_eq!(pantry.book(&id).unwrap().key(), key);
        assert!(pantry.book(&BookId::from("books/luk.usfm")).is_none());
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
        let id = mrk();
        pantry.update(id.clone(), Role::Target, &mark()).unwrap();
        let resident = pantry.resident_bytes();
        assert!(resident > 0);

        assert!(pantry.remove(&id));
        assert!(pantry.resident_bytes() < resident);
        assert!(pantry.book(&id).is_none());
        assert_eq!(pantry.changed_since_update(&id, &mark()), None);
        assert!(pantry.books(Role::Target).is_empty());
        assert!(!pantry.remove(&id), "already gone");
    }

    #[test]
    fn a_book_with_no_id_line_is_refused() {
        let mut pantry = pantry();
        assert_eq!(
            pantry
                .update("scratch.usfm", Role::Target, "\\c 1\n\\p\n\\v 1 keyless\n")
                .err(),
            Some(PantryError::MissingBookKey {
                id: BookId::from("scratch.usfm")
            })
        );
        assert!(pantry.books(Role::Target).is_empty());
    }
}
