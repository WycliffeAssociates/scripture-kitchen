//! The pantry: what a book leaves behind, and the text it keeps beside it.
//!
//! ```text
//! pantry.update("books/mrk.usfm", Role::Target, &text)?   -> Entry for MRK
//!     .lint()          the folded lint report, from the retained text
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

use sous_core::{InputError, SourceVerse, SourceWords, source_lengths};

use crate::onion::{self, Filter, Mask, Toc, lint::LintReport};
use crate::sous::OnionBook;

pub mod budget;
mod chunks;
pub(crate) mod derived;

pub use budget::{Budget, Tally, Tier};
pub use chunks::ChunkStats;

use chunks::ChunkStore;

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
    Target,
    /// A declared source: `Toc`, one projected grapheme length per verse, and
    /// that verse's word hashes — nothing else. No mask, no UTF-16 table, no
    /// text.
    ///
    /// A reference publishes no findings and is never a target, so it needs no
    /// coordinate of its own. [`Retain::Text`] buys it one anyway: the text
    /// AND the projection a target gets, so a search can read it.
    Reference,
}

impl Role {
    /// What this role keeps by default: a target its text, a reference
    /// nothing but the products above.
    const fn retention(self) -> Retain {
        match self {
            Self::Target => Retain::Text,
            Self::Reference => Retain::ProductsOnly,
        }
    }
}

/// Which verse lanes a [`Role::Reference`] derives and keeps.
///
/// The word lane is the expensive half — 2.77 MB against 0.37 MB of lengths
/// over a whole Bible — so it is built only for a host that will judge with
/// it. A reference registered under [`Lengths`](Self::Lengths) and later
/// judged with the source-copy lane on publishes nothing for that book until
/// the host re-sends its text; the publication reports how many books that
/// was rather than going quietly silent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SourceLanes {
    /// One projected grapheme count per verse, and nothing else.
    #[default]
    Lengths,
    /// The grapheme counts plus each verse's sorted deduplicated word hashes.
    LengthsAndWords,
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
    /// What publication needs to place a finding, and what a search reads.
    /// A [`Role::Reference`] has one only under [`Retain::Text`].
    projection: Option<Projection>,
    /// [`Role::Reference`] only: one row per keyed verse, in TOC order.
    lengths: Option<Box<[SourceVerse]>>,
    /// [`Role::Reference`] only: the same verses' word sets, index-aligned
    /// with `lengths`.
    words: Option<Box<SourceWords>>,
    /// The text as last updated; `None` under [`Retain::ProductsOnly`].
    text: Option<String>,
    bytes: usize,
}

/// A book's coordinate products: the verse-text projection and the table
/// that turns its bytes into the units a host publishes.
struct Projection {
    /// Source ranges plus their mask starts.
    mask: Mask,
    utf16: Utf16Table,
    /// The raw book's UTF-16 length — a publication's `published_len`.
    len_utf16: u32,
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

    /// Read back off the lanes themselves, so a re-registration under a
    /// different setting is not served from the cheaper one.
    fn lanes(&self) -> SourceLanes {
        if self.words.is_some() {
            SourceLanes::LengthsAndWords
        } else {
            SourceLanes::Lengths
        }
    }
}

/// The Galley-wide, id-keyed registry of detached per-book products, and the
/// one caching layer beneath them.
///
/// Owns the chunk cache and adds the book-level products.
/// THE TEXT RULE: a target keeps its text, so [`update`](Self::update) is the
/// only method that has to be GIVEN it — the one other `&str` argument,
/// [`changed_since_update`](Self::changed_since_update), takes a candidate
/// because a diff names both its sides.
pub struct Pantry {
    chunks: ChunkStore,
    books: FxHashMap<BookId, Book>,
    /// Canonical by [`BookKey`], ties by id — never insertion order.
    targets: Vec<(BookId, BookKey)>,
    /// The same order over the declared sources.
    references: Vec<(BookId, BookKey)>,
    derivations: u64,
    /// The marker-registry generation every product here was derived under.
    registry: u64,
}

impl Pantry {
    /// `budget_bytes` is the ceiling on the rebuildable tier; the Pantry's
    /// own per-book products are pinned until their id is removed.
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            chunks: ChunkStore::new(budget_bytes),
            books: FxHashMap::default(),
            targets: Vec::new(),
            references: Vec::new(),
            derivations: 0,
            registry: onion::extensions::generation(),
        }
    }

    /// Flushes what a changed marker registry made stale.
    ///
    /// Both caches key on CONTENT — the chunk store on a chunk's bytes, a Book
    /// on its raw checksum — which is right only while the same bytes parse
    /// the same way. `set_extensions` makes them a different document, so
    /// every derived product is stale at once. Runs at the head of every door
    /// that reads or derives one; a comparison against a `u64`, so the
    /// overwhelmingly common no-change case costs a load.
    ///
    /// A book keeps its TEXT, which is the host's and was never derived: it is
    /// re-derived from it here, once. A book retaining no text cannot be
    /// rebuilt and keeps the products it has — dropping it would make `books`
    /// lie about what the host registered, where stale source counts only
    /// misreport a reference until the host re-registers it.
    fn check_registry(&mut self) {
        let now = onion::extensions::generation();
        if now == self.registry {
            return;
        }
        // Before re-deriving: the nested `update_with` calls below must see a
        // Pantry already at the new generation, or each would recurse.
        self.registry = now;
        self.chunks.clear();
        let stale: Vec<(BookId, Role, Retain, SourceLanes, String)> = self
            .books
            .iter()
            .filter_map(|(id, book)| {
                book.text.as_ref().map(|text| {
                    (
                        id.clone(),
                        book.role,
                        book.retain(),
                        book.lanes(),
                        text.clone(),
                    )
                })
            })
            .collect();
        for (id, ..) in &stale {
            self.books.remove(id);
        }
        for (id, role, retain, lanes, text) in stale {
            // A book that derived once derives again; a book whose bytes have
            // become invalid under the new rows is dropped, which is the same
            // answer `update` would give the host for them now.
            let _ = self.update_with(id, role, retain, lanes, &text);
        }
        self.reorder();
    }

    /// Derive and retain this book's products under the role's own retention:
    /// a target keeps its text, a reference keeps none.
    ///
    /// [`update_with`](Self::update_with) is how a host overrides that.
    pub fn update(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        text: &str,
    ) -> Result<Entry<'_>, PantryError> {
        self.update_with(id, role, role.retention(), SourceLanes::Lengths, text)
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
        lanes: SourceLanes,
        text: &str,
    ) -> Result<Entry<'_>, PantryError> {
        self.check_registry();
        let id = id.into();
        if role == Role::Target && retain == Retain::ProductsOnly {
            return Err(PantryError::TargetNeedsText { id });
        }
        let checksum = RawChecksum::of(text.as_bytes());
        let served = self.books.get(&id).is_some_and(|book| {
            book.checksum == checksum
                && book.role == role
                && book.retain() == retain
                && book.lanes() == lanes
        });
        if served {
            return Ok(Entry { pantry: self, id });
        }

        let parsed = self.chunks.parsed(
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
        // A reference counts its verses once, here, and then owns neither the
        // mask that projected them nor the text they came from.
        let (lengths, words) =
            match role {
                Role::Target => (None, None),
                Role::Reference => {
                    let projected = OnionBook::from_parts(text, mask.clone(), toc.clone())
                        .map_err(|error| PantryError::InvalidBook {
                            id: id.clone(),
                            error,
                        })?;
                    (
                        Some(source_lengths(&projected).into_boxed_slice()),
                        (lanes == SourceLanes::LengthsAndWords)
                            .then(|| Box::new(SourceWords::of(&projected))),
                    )
                }
            };
        // A target always projects; a reference does when it keeps the text
        // the projection indexes, so Find has both halves or neither.
        let projection = (retain == Retain::Text).then(|| {
            let utf16 = utf16_table(text.as_bytes());
            Projection {
                len_utf16: utf16.len_utf16(),
                mask,
                utf16,
            }
        });
        let print = fingerprint(text);
        let kept = match retain {
            Retain::Text => Some(text.to_string()),
            Retain::ProductsOnly => None,
        };
        let book = Book {
            key,
            role,
            checksum,
            bytes: book_bytes(
                &toc,
                projection.as_ref(),
                lengths.as_deref(),
                words.as_deref(),
                &print,
                &id,
                kept.as_deref(),
            ),
            fingerprint: print,
            toc,
            projection,
            lengths,
            words,
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
    /// `&mut` because [`Entry::lint`] and [`Entry::parse`] run the cache.
    pub fn book(&mut self, id: &BookId) -> Option<Entry<'_>> {
        self.check_registry();
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

    /// Registered books of one role in canonical [`BookKey`] order, ties by id.
    pub fn books(&self, role: Role) -> &[(BookId, BookKey)] {
        match role {
            Role::Target => &self.targets,
            Role::Reference => &self.references,
        }
    }

    /// Registered books of one role that retain BOTH text and a projection —
    /// what a search can actually read — in the same canonical order.
    ///
    /// Every target qualifies; a reference does only when the host registered
    /// it under [`Retain::Text`].
    pub fn books_with_text(&self, role: Role) -> Vec<BookId> {
        self.books(role)
            .iter()
            .filter(|(id, _)| self.searchable(id))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Whether this book retains what a search reads: its text, and the
    /// projection the hits are placed in. `false` for an unknown id.
    pub fn searchable(&self, id: &BookId) -> bool {
        self.books
            .get(id)
            .is_some_and(|book| book.text.is_some() && book.projection.is_some())
    }

    /// The chunk cache's resident products plus the Pantry's own — detached
    /// products and retained text alike.
    pub fn resident_bytes(&self) -> usize {
        self.tally().total()
    }

    /// The same bytes, attributed: a book's text and products are
    /// [`Tier::Pinned`], and so are the canonical order's own rows, which name
    /// every registered id a second time. The chunk products are
    /// [`Tier::Rebuildable`]. Nothing here is hot; the hot tier is the
    /// Expediter's chapter rows.
    pub fn tally(&self) -> Tally {
        let ordered: usize = self
            .targets
            .iter()
            .chain(&self.references)
            .map(|(id, _)| size_of::<(BookId, BookKey)>() + id.as_str().len())
            .sum();
        let mut tally = Tally::default();
        tally.add(
            Tier::Pinned,
            self.books.values().map(|book| book.bytes).sum::<usize>() + ordered,
        );
        tally.add(Tier::Rebuildable, self.chunks.resident_bytes());
        tally
    }

    /// The declared ceiling. Enforced on the rebuildable tier's chunk
    /// products and nowhere else — `galley/src/pantry.md`.
    pub fn budget(&self) -> Budget {
        self.chunks.budget()
    }

    /// The chunk cache's counters and size.
    pub fn chunk_stats(&self) -> ChunkStats {
        self.chunks.stats()
    }

    /// The whole-book lint report over loose text the host holds, equal to a
    /// fresh `lint(lex(text), build(…))` with every unchanged chunk served
    /// from the cache.
    pub fn lint(&mut self, text: &str) -> LintReport {
        self.check_registry();
        self.chunks.lint(text)
    }

    /// The book, plated — byte-identical to a cold `onion::wire::parse`, with
    /// the lex, the tree, and the lint walk reused per unchanged chunk.
    pub fn parse(&mut self, text: &str, opts: onion::wire::ParseOptions) -> Vec<u8> {
        self.check_registry();
        self.chunks.parse(text, opts)
    }

    /// The same ingredients as engine types, for a caller that is not
    /// crossing a boundary.
    pub fn parsed<'a>(
        &mut self,
        text: &'a str,
        opts: onion::wire::ParseOptions,
    ) -> onion::wire::Parsed<'a> {
        self.check_registry();
        self.chunks.parsed(text, opts)
    }

    /// One recipe's mask over the same assembled ingredients — what a
    /// downstream consumer of verse text reads.
    pub fn masked(&mut self, text: &str, filter: &onion::mask::Filter) -> Mask {
        self.check_registry();
        self.chunks.masked(text, filter)
    }

    /// Bytes of text retained across every registered book — zero for a book
    /// under [`Retain::ProductsOnly`]. The other component of
    /// [`resident_bytes`](Self::resident_bytes) not already reachable through
    /// [`chunk_stats`](Self::chunk_stats): what's left is `resident_bytes() -
    /// chunk_stats().resident_bytes - text_bytes()`, the Pantry's own per-book
    /// products (`Toc`, `Mask`, `Utf16Table`, `Fingerprint`) and the canonical
    /// order's id rows.
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

    /// One book's retained products, borrowed together.
    ///
    /// Crate-private, because publication reads every book at once where an
    /// [`Entry`] borrows the whole Pantry mutably for one.
    pub(crate) fn products(&self, id: &BookId) -> Option<Products<'_>> {
        let book = self.books.get(id)?;
        let projection = book.projection.as_ref()?;
        Some(Products {
            checksum: book.checksum,
            toc: &book.toc,
            mask: &projection.mask,
            utf16: &projection.utf16,
            published_len: projection.len_utf16,
            text: book.text.as_deref(),
        })
    }

    /// One registered book's retained `Toc`, and the table that rebases its
    /// offsets when the book kept one — the pinned tier, read through `&self`
    /// where an [`Entry`] borrows the whole Pantry mutably. `None` for an
    /// unknown id; `None` in the second slot is a book that kept no text.
    pub(crate) fn census(&self, id: &BookId) -> Option<(&Toc, Option<&Utf16Table>)> {
        let book = self.books.get(id)?;
        Some((
            &book.toc,
            book.projection.as_ref().map(|projection| &projection.utf16),
        ))
    }

    /// One registered book's raw checksum, whatever its role.
    pub(crate) fn checksum(&self, id: &BookId) -> Option<RawChecksum> {
        self.books.get(id).map(|book| book.checksum)
    }

    /// What this id is registered AS right now, or `None` when it is not.
    pub fn role(&self, id: &BookId) -> Option<Role> {
        self.books.get(id).map(|book| book.role)
    }

    /// One reference book's retained per-verse lengths, or `None` for an
    /// unknown id or a book of another role.
    pub(crate) fn reference_lengths(&self, id: &BookId) -> Option<&[SourceVerse]> {
        self.books.get(id)?.lengths.as_deref()
    }

    /// One reference book's retained word sets, index-aligned with
    /// [`reference_lengths`](Self::reference_lengths).
    pub(crate) fn reference_words(&self, id: &BookId) -> Option<&SourceWords> {
        self.books.get(id)?.words.as_deref()
    }

    /// The chunk cache's lint over one book's retained text. Split-borrowed,
    /// which is why this is not `Entry::text` plus a call.
    fn lint_book(&mut self, id: &BookId) -> Result<LintReport, PantryError> {
        let text = retained(&self.books, id)?;
        Ok(self.chunks.lint(text))
    }

    /// One book's tokens, tree and lint report off its retained text — the
    /// ingredients a skeleton walk reads, from the same warm chunks.
    ///
    /// Owned rather than borrowed, so the caller can reopen the book for its
    /// mask and TOC. Split-borrowed for the same reason as
    /// [`lint_book`](Self::lint_book).
    pub(crate) fn skeleton_input(
        &mut self,
        id: &BookId,
    ) -> Result<(Vec<onion::Token>, onion::cst::Cst, LintReport), PantryError> {
        let text = retained(&self.books, id)?;
        let parsed = self.chunks.parsed(
            text,
            onion::wire::ParseOptions {
                diagnostics: true,
                ..Default::default()
            },
        );
        Ok((
            parsed.tokens,
            parsed.cst,
            parsed.lint.expect("diagnostics were asked for"),
        ))
    }

    /// The same over one book's retained text. See
    /// [`lint_book`](Self::lint_book).
    fn parse_book(
        &mut self,
        id: &BookId,
        opts: onion::wire::ParseOptions,
    ) -> Result<Vec<u8>, PantryError> {
        let text = retained(&self.books, id)?;
        Ok(self.chunks.parse(text, opts))
    }

    fn reorder(&mut self) {
        self.targets = self.ordered(Role::Target);
        self.references = self.ordered(Role::Reference);
    }

    fn ordered(&self, role: Role) -> Vec<(BookId, BookKey)> {
        let mut books: Vec<(BookId, BookKey)> = self
            .books
            .iter()
            .filter(|(_, book)| book.role == role)
            .map(|(id, book)| (id.clone(), book.key))
            .collect();
        books.sort_unstable_by(|(left_id, left), (right_id, right)| {
            canonical_rank(*left)
                .cmp(&canonical_rank(*right))
                .then_with(|| left.as_bytes().cmp(&right.as_bytes()))
                .then_with(|| left_id.cmp(right_id))
        });
        books
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
/// Borrows the Pantry MUTABLY, because the pass-throughs run the cache; the
/// read-only accessors ride along rather than pay for a second handle type.
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

    /// The retained verse-text projection: source ranges and their mask
    /// starts, or the refusal a book keeping no text answers with.
    pub fn mask(&self) -> Result<&Mask, PantryError> {
        self.projection().map(|projection| &projection.mask)
    }

    pub fn toc(&self) -> &Toc {
        &self.book().toc
    }

    /// The retained byte → UTF-16 table, valid against the exact text of the
    /// last update. A book keeping no text keeps none.
    pub fn utf16(&self) -> Result<&Utf16Table, PantryError> {
        self.projection().map(|projection| &projection.utf16)
    }

    /// The raw book's UTF-16 length — a publication's `published_len`.
    pub fn published_len(&self) -> Result<u32, PantryError> {
        self.projection().map(|projection| projection.len_utf16)
    }

    /// One projected grapheme length per keyed verse, in TOC order — what a
    /// reference retains instead of a projection. A target keeps none.
    pub fn verse_lengths(&self) -> Result<&[SourceVerse], PantryError> {
        self.book()
            .lengths
            .as_deref()
            .ok_or_else(|| PantryError::NoLengths {
                id: self.id.clone(),
            })
    }

    /// The same verses' word sets, one sorted deduplicated `u32` per distinct
    /// word — what the source-copy lane reads instead of the source's text.
    pub fn verse_words(&self) -> Result<&SourceWords, PantryError> {
        self.book()
            .words
            .as_deref()
            .ok_or_else(|| PantryError::NoLengths {
                id: self.id.clone(),
            })
    }

    /// [`Pantry::lint`] over the retained text.
    pub fn lint(&mut self) -> Result<LintReport, PantryError> {
        self.pantry.lint_book(&self.id)
    }

    /// [`Pantry::parse`] over the retained text.
    pub fn parse(&mut self, opts: onion::wire::ParseOptions) -> Result<Vec<u8>, PantryError> {
        self.pantry.parse_book(&self.id, opts)
    }

    /// An entry is only ever built for a registered id, and it holds the
    /// Pantry exclusively, so nothing can remove the book underneath it.
    fn book(&self) -> &Book {
        self.pantry.books.get(&self.id).expect("registered id")
    }

    fn projection(&self) -> Result<&Projection, PantryError> {
        self.book()
            .projection
            .as_ref()
            .ok_or_else(|| PantryError::NoProjection {
                id: self.id.clone(),
            })
    }
}

/// One book's text, keyed off a borrow of the map alone so a caller can hold
/// `&mut self.chunks` at the same time.
fn retained<'b>(books: &'b FxHashMap<BookId, Book>, id: &BookId) -> Result<&'b str, PantryError> {
    books
        .get(id)
        .and_then(|book| book.text.as_deref())
        .ok_or_else(|| PantryError::NoText { id: id.clone() })
}

/// Resident size of one book's detached products and retained text.
///
/// A `Vec` is weighed by capacity, which is what it actually holds on the
/// heap; a `String` the Pantry built by copying the update's text has no slack
/// to weigh, so the text is its length.
fn book_bytes(
    toc: &Toc,
    projection: Option<&Projection>,
    lengths: Option<&[SourceVerse]>,
    words: Option<&SourceWords>,
    print: &Fingerprint,
    id: &BookId,
    text: Option<&str>,
) -> usize {
    let projected = projection.map_or(0, |projection| {
        projection.mask.ranges.capacity() * size_of::<Range<u32>>()
            + projection.mask.starts.capacity() * size_of::<u32>()
            + projection.utf16.index_bytes()
            + size_of::<Projection>()
    });
    toc.chapters.capacity() * size_of::<onion::ChapterRow>()
        + toc.verses.capacity() * size_of::<onion::VerseAnchor>()
        + toc.members.capacity() * size_of::<onion::VerseMember>()
        + projected
        + lengths.map_or(0, size_of_val)
        + words.map_or(0, SourceWords::resident_bytes)
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
    /// A target publishes findings, and placing them rescans its text, so it
    /// keeps it. [`Retain::ProductsOnly`] is for a reference.
    TargetNeedsText { id: BookId },
    /// The book retains no verse-text projection and no UTF-16 table — a
    /// reference registered without its text — and this operation needs one.
    NoProjection { id: BookId },
    /// The book is a [`Role::Target`], which retains a projection rather than
    /// per-verse lengths.
    NoLengths { id: BookId },
    /// The book's projection is not an analyzable `sous-core` input.
    InvalidBook { id: BookId, error: InputError },
}

impl fmt::Display for PantryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBookKey { id } => write!(f, "book {id} has no \\id line to key it"),
            Self::NoText { id } => write!(f, "book {id} retains no text"),
            Self::TargetNeedsText { id } => {
                write!(f, "target {id} must retain its text to be sited")
            }
            Self::NoProjection { id } => {
                write!(f, "book {id} retains no verse-text projection")
            }
            Self::NoLengths { id } => write!(f, "target {id} retains no verse lengths"),
            Self::InvalidBook { id, error } => {
                write!(f, "book {id} is not analyzable: {error}")
            }
        }
    }
}

impl std::error::Error for PantryError {}

#[cfg(test)]
mod tests;
