//! The resident Sous coordinator: one pass, one Pantry, one snapshot out.
//!
//! ```text
//! let mut sous = Expediter::new(Hygiene, 1 << 20);
//! sous.update("books/mrk.usfm", Role::Target, &text)?;
//! sous.publish()?          -> SOUS corpus buffer, raw-book UTF-16
//! sous.last_mapped()       -> 6      // six chapters mapped
//! sous.publish()?          -> the same bytes
//! sous.last_mapped()       -> 0      // nothing re-read, nothing re-mapped
//! ```
//!
//! Same projected chapter, same schema, same observation — so a hit skips the
//! map. A book that retains its text is keyed lazily at
//! [`Expediter::publish`]; one that does not is keyed at
//! [`update`](Expediter::update), while its text is still in hand. Every
//! publication sweeps what no retained generation names. Why the two hashes
//! divide the work this way: `expediter.md`.

use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::{
    BookIndex, ChapterInput, ChapterObs, ChapterPass, CoordinateSpace, CorpusWireError, Findings,
    PackedFinding, PublicationBook, SnapshotId, encode_to_corpus_buffer, for_each_chapter,
};
use xxhash_rust::xxh3::Xxh3Default;

use mise::books::BookKey;

use super::{OnionBook, PublishError, rebase_span};
use crate::pantry::{BookId, Pantry, RawChecksum, Retain, Role};

/// One chapter's cache identity: xxh3-128 over its projected text, its rebased
/// verse rows, and the pass schema.
///
/// A markup-only edit moves the [`RawChecksum`] and not this, so the
/// observation is reused and only its coordinates are rebased. The chapter's
/// own address is absent, which is what lets identical chapters share one
/// entry — see `expediter.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservationKey([u8; 16]);

impl ObservationKey {
    fn of<P: ChapterPass>(chapter: &ChapterInput<'_>) -> Self {
        let mut hasher = Xxh3Default::new();
        hasher.update(&(chapter.text.len() as u64).to_le_bytes());
        hasher.update(chapter.text.as_bytes());
        hasher.update(&(chapter.verses.len() as u64).to_le_bytes());
        for verse in chapter.verses {
            let key = verse.key();
            hasher.update(&key.chapter().to_le_bytes());
            hasher.update(&key.first().to_le_bytes());
            hasher.update(&key.last().to_le_bytes());
            hasher.update(&verse.text().from().to_le_bytes());
            hasher.update(&verse.text().to().to_le_bytes());
        }
        hasher.update(&P::SCHEMA.get().to_le_bytes());
        Self(hasher.digest128().to_be_bytes())
    }

    pub fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// What publication needs of one chapter without its text: whose observation,
/// and where to rebase it to.
///
/// No `ChapterKey`: reduce rebases by `start` and never reads an address.
#[derive(Debug, Clone, Copy)]
struct ChapterRow {
    observation: ObservationKey,
    /// Projected-book offset the reduce rebases chapter coordinates by.
    start: u32,
}

/// Previous checksums a book keeps beside its current one, so an undo of that
/// many edits still lands on a retained chapter table.
const DEFAULT_GENERATIONS: usize = 4;

/// The Sous coordinator over one [`ChapterPass`].
///
/// Holds the [`Pantry`] the host mutates it through, the observation cache
/// keyed by content, and one chapter table per retained book text.
pub struct Expediter<P: ChapterPass> {
    pantry: Pantry,
    pass: P,
    /// Content-addressed across books: two identical chapters are one entry,
    /// and reduce still counts both positions.
    observations: FxHashMap<ObservationKey, P::Observation>,
    /// Keyed by the book's raw checksum, so an unchanged book publishes
    /// without touching its text.
    chapter_tables: FxHashMap<RawChecksum, Vec<ChapterRow>>,
    /// Per book, its current checksum ahead of the previous ones still kept —
    /// exactly the tables the sweep spares.
    generations: FxHashMap<BookId, Vec<RawChecksum>>,
    /// Ring depth behind the current checksum.
    kept: usize,
    /// Set when a table or a ring changed, so something may now be
    /// unreachable; a publication that finds it clear skips the sweep.
    dirty: bool,
    /// Chapters mapped since the last publication, eager and lazy alike.
    pending: u64,
    misses: u64,
}

impl<P: ChapterPass> Expediter<P> {
    /// `budget_bytes` is the Pantry's Warmer LRU ceiling.
    pub fn new(pass: P, budget_bytes: usize) -> Self {
        Self {
            pantry: Pantry::new(budget_bytes),
            pass,
            observations: FxHashMap::default(),
            chapter_tables: FxHashMap::default(),
            generations: FxHashMap::default(),
            kept: DEFAULT_GENERATIONS,
            dirty: false,
            pending: 0,
            misses: 0,
        }
    }

    /// Previous chapter tables kept per book, the current one aside; the
    /// default is [`DEFAULT_GENERATIONS`].
    pub fn with_generations(mut self, kept: usize) -> Self {
        self.kept = kept;
        self
    }

    /// Derive and retain this book's products and its text; its chapters are
    /// keyed at the next [`publish`](Self::publish).
    pub fn update(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        text: &str,
    ) -> Result<BookKey, PublishError> {
        self.update_with(id, role, Retain::Text, text)
    }

    /// Derive and retain this book's products, keeping or dropping its text.
    ///
    /// A [`Retain::ProductsOnly`] book is keyed and mapped here: after this
    /// call nothing holds its text.
    pub fn update_with(
        &mut self,
        id: impl Into<BookId>,
        role: Role,
        retain: Retain,
        text: &str,
    ) -> Result<BookKey, PublishError> {
        let id = id.into();
        let key = self
            .pantry
            .update_with(id.clone(), role, retain, text)
            .map_err(PublishError::Pantry)?
            .key();
        if retain == Retain::ProductsOnly {
            self.index_book(&id, Some(text))?;
        }
        Ok(key)
    }

    /// Drop a book's products, its text, and its retained generations; the
    /// tables and observations go at the next [`publish`](Self::publish).
    ///
    /// `false` when the id was not registered.
    pub fn remove(&mut self, id: &BookId) -> bool {
        self.dirty |= self.generations.remove(id).is_some();
        self.pantry.remove(id)
    }

    /// The registry, read-only: books are registered through
    /// [`update`](Self::update) so their chapters cannot go unkeyed.
    pub fn pantry(&self) -> &Pantry {
        &self.pantry
    }

    /// Chapters mapped for the last [`publish`](Self::publish), the eager maps
    /// of the updates before it included.
    pub fn last_mapped(&self) -> u64 {
        self.misses
    }

    /// Observations the cache holds after the last sweep.
    pub fn resident_observations(&self) -> usize {
        self.observations.len()
    }

    /// Chapter tables the cache holds — one per retained generation of every
    /// registered book.
    pub fn resident_tables(&self) -> usize {
        self.chapter_tables.len()
    }

    /// Keys this book's chapters under the Pantry's current checksum and maps
    /// the ones the cache is missing; a checksum already keyed only ages.
    ///
    /// `supplied` is the update's own text, the only source for a book whose
    /// products retain none.
    fn index_book(&mut self, id: &BookId, supplied: Option<&str>) -> Result<(), PublishError> {
        let Self {
            pantry,
            pass,
            observations,
            chapter_tables,
            generations,
            kept,
            dirty,
            pending,
            ..
        } = self;
        let products = pantry.products(id).expect("the caller named a live book");
        let checksum = products.checksum;
        let ring = generations.entry(id.clone()).or_default();
        if ring.first() != Some(&checksum) {
            ring.retain(|seen| *seen != checksum);
            ring.insert(0, checksum);
            ring.truncate(*kept + 1);
            *dirty = true;
        }
        if chapter_tables.contains_key(&checksum) {
            return Ok(());
        }

        let text = supplied
            .or(products.text)
            .ok_or_else(|| PublishError::NoText { id: id.clone() })?;
        // The one place projected text exists, and only for a book whose
        // chapters are not already keyed.
        let book = OnionBook::from_parts(text, products.mask.clone(), products.toc.clone())
            .map_err(|error| PublishError::InvalidBook {
                id: id.clone(),
                error,
            })?;
        let mut table = Vec::new();
        for_each_chapter(&book, |start, chapter| {
            let observation = ObservationKey::of::<P>(&chapter);
            table.push(ChapterRow { observation, start });
            observations.entry(observation).or_insert_with(|| {
                *pending += 1;
                pass.map(chapter)
            });
        });
        chapter_tables.insert(checksum, table);
        *dirty = true;
        Ok(())
    }

    /// Drops every chapter table outside a book's retained generations, and
    /// every observation no surviving table names.
    ///
    /// A publication that added no table and aged no ring has nothing to free,
    /// and skips the walk.
    fn sweep(&mut self) {
        if !core::mem::take(&mut self.dirty) {
            return;
        }
        let live: FxHashSet<RawChecksum> = self.generations.values().flatten().copied().collect();
        self.chapter_tables
            .retain(|checksum, _| live.contains(checksum));
        let named: FxHashSet<ObservationKey> = self
            .chapter_tables
            .values()
            .flatten()
            .map(|row| row.observation)
            .collect();
        self.observations.retain(|key, _| named.contains(key));
    }

    /// One complete corpus publication over every `Target` book, in canonical
    /// order, in raw-book UTF-16.
    ///
    /// Maps only chapters whose [`ObservationKey`] is absent, projects only a
    /// book whose chapter table is missing, and sweeps once reduce is done.
    pub fn publish(&mut self) -> Result<Vec<u8>, PublishError> {
        let books: Vec<(BookId, BookKey)> = self.pantry.books(Role::Target).to_vec();
        if books.len() > usize::from(u16::MAX) + 1 {
            return Err(PublishError::Wire(CorpusWireError::BookCountOverflow {
                count: books.len(),
            }));
        }

        for (id, _) in &books {
            self.index_book(id, None)?;
        }
        self.misses = core::mem::take(&mut self.pending);

        let mut projected_lens = Vec::with_capacity(books.len());
        let mut published_lens = Vec::with_capacity(books.len());
        for (id, _) in &books {
            let products = self.pantry.products(id).expect("the pantry listed this id");
            projected_lens.push(products.mask.len());
            published_lens.push(products.published_len);
        }

        // Scoped so the borrowed observations are released before the sweep.
        let projected = {
            let mut findings = Findings::new(projected_lens);
            let mut chapters: Vec<ChapterObs<&P::Observation>> = Vec::new();
            for (index, (id, _)) in books.iter().enumerate() {
                let checksum = self
                    .pantry
                    .products(id)
                    .expect("the pantry listed this id")
                    .checksum;
                chapters.clear();
                chapters.extend(self.chapter_tables[&checksum].iter().map(|row| ChapterObs {
                    start: row.start,
                    obs: &self.observations[&row.observation],
                }));
                findings.open_book(BookIndex::new(index).expect("the book count was checked"));
                self.pass
                    .reduce(&chapters, &mut P::Carry::default(), &mut findings);
            }
            findings.into_rows()
        };
        self.sweep();

        let mut per_book: Vec<Vec<PackedFinding>> = (0..books.len()).map(|_| Vec::new()).collect();
        for (row, finding) in projected.iter().enumerate() {
            let index = usize::from(finding.book_idx().get());
            let products = self
                .pantry
                .products(&books[index].0)
                .expect("reduce named an open book");
            let (from, to) = rebase_span(
                products.mask,
                |byte| products.utf16.to_utf16(byte),
                finding.from()..finding.to(),
            );
            per_book[index].push(
                PackedFinding::new(
                    from,
                    to,
                    finding.book_idx(),
                    finding.kind(),
                    &published_lens,
                )
                .map_err(|error| PublishError::Rebase { row, error })?,
            );
        }

        let sections: Vec<PublicationBook<'_>> = books
            .iter()
            .zip(&per_book)
            .zip(&published_lens)
            .map(|(((id, key), findings), &published_len)| {
                PublicationBook::new(*key, id.as_str(), published_len, findings)
            })
            .collect();
        let snapshot = snapshot_id::<P>(&self.pantry, &books);
        encode_to_corpus_buffer(snapshot, CoordinateSpace::Utf16, &sections)
            .map_err(PublishError::Wire)
    }
}

/// xxh3-128 over the canonical (`BookKey`, id, `RawChecksum`) table plus the
/// pass schema: what the publication is OF, not what it says.
fn snapshot_id<P: ChapterPass>(pantry: &Pantry, books: &[(BookId, BookKey)]) -> SnapshotId {
    let mut hasher = Xxh3Default::new();
    for (id, key) in books {
        hasher.update(&key.as_bytes());
        hasher.update(&(id.as_str().len() as u32).to_le_bytes());
        hasher.update(id.as_str().as_bytes());
        hasher.update(
            &pantry
                .products(id)
                .expect("the pantry listed this id")
                .checksum
                .as_bytes(),
        );
    }
    hasher.update(&P::SCHEMA.get().to_le_bytes());
    SnapshotId::new(hasher.digest128().to_be_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sous_core::{Corpus, CorpusSnapshot, FindingKind, analyze, hygiene::Hygiene};

    use crate::pantry::Retain;
    use crate::sous::{OnionInputBook, publish_onion_findings};

    /// A `\\` pair is the one backslash Onion's mask hands to Sous as content,
    /// so every chapter body below carries exactly one hygiene finding.
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
                "Jesus wept \\\\ here.",
                "He entered \\\\ Capernaum.",
                "A withered \\\\ hand.",
            ],
        )
    }

    fn genesis() -> String {
        book("GEN", &["In the \\\\ beginning."])
    }

    fn sous() -> Expediter<Hygiene> {
        Expediter::new(Hygiene, 1 << 20)
    }

    /// Every row of a published buffer as (book index, id, from, to).
    fn rows(buffer: &[u8]) -> Vec<(u16, String, u32, u32)> {
        let snapshot = CorpusSnapshot::open(buffer).unwrap();
        (0..snapshot.len())
            .flat_map(|index| {
                let book = snapshot
                    .book(BookIndex::new(index).unwrap())
                    .expect("directory position");
                (0..book.len()).map(move |row| {
                    let finding = book.at(row).unwrap();
                    (
                        finding.book_idx().get(),
                        book.id().to_string(),
                        finding.from(),
                        finding.to(),
                    )
                })
            })
            .collect()
    }

    fn ids(buffer: &[u8]) -> Vec<String> {
        let snapshot = CorpusSnapshot::open(buffer).unwrap();
        (0..snapshot.len())
            .map(|index| {
                snapshot
                    .book(BookIndex::new(index).unwrap())
                    .unwrap()
                    .id()
                    .to_string()
            })
            .collect()
    }

    /// `analyze` + the string-taking publisher over the same books, under the
    /// snapshot identity the Expediter chose.
    fn cold(books: &[(&str, String)], snapshot: SnapshotId) -> Vec<u8> {
        let parsed: Vec<OnionBook> = books
            .iter()
            .map(|(_, source)| OnionBook::parse(source).unwrap())
            .collect();
        let corpus = Corpus::try_new(&parsed).unwrap();
        let findings = analyze(&corpus, &Hygiene).into_rows();
        let inputs = books
            .iter()
            .map(|(id, source)| OnionInputBook::new(*id, source.clone()))
            .collect();
        publish_onion_findings(inputs, &findings, snapshot).unwrap()
    }

    #[test]
    fn publish_byte_equals_cold_analyze_through_the_string_taking_publisher() {
        let (mark, genesis) = (mark(), genesis());
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis).unwrap();
        let buffer = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 4, "one GEN chapter, three MRK");

        // Canonical order, not update order: GEN before MRK.
        assert_eq!(ids(&buffer), vec!["a/gen.usfm", "b/mrk.usfm"]);
        let snapshot = CorpusSnapshot::open(&buffer).unwrap().snapshot_id();
        assert_eq!(
            buffer,
            cold(&[("a/gen.usfm", genesis), ("b/mrk.usfm", mark)], snapshot)
        );
    }

    #[test]
    fn a_second_publish_maps_nothing_and_republishes_the_same_bytes() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        let first = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 3);

        let second = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0);
        assert_eq!(first, second, "identity and every row");
    }

    /// A footnote whose content the verse-text mask removes: the projected
    /// chapters are byte-identical, the raw book is 16 UTF-16 units longer.
    #[test]
    fn a_markup_only_edit_maps_nothing_and_shifts_the_published_offsets() {
        let before = mark();
        let after = before.replace("Jesus", "Jesus\\f + \\ft note\\f*");
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &before).unwrap();
        let first = rows(&sous.publish().unwrap());

        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        let second = rows(&sous.publish().unwrap());
        assert_eq!(sous.last_mapped(), 0, "no projected chapter moved");

        let shifted: Vec<_> = first
            .iter()
            .map(|(book, id, from, to)| (*book, id.clone(), from + 16, to + 16))
            .collect();
        assert_eq!(second, shifted, "every span past the insertion moves by it");
    }

    /// `\v 1` to `\v 1-2` masks out identically but rekeys the verse row.
    #[test]
    fn a_verse_marker_edit_with_unchanged_content_remaps_only_its_chapter() {
        let before = mark();
        let after = before.replace("\\v 1 He entered", "\\v 1-2 He entered");
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &before).unwrap();
        sous.publish().unwrap();

        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 1, "the rekeyed chapter and no other");
    }

    #[test]
    fn a_one_chapter_content_edit_maps_exactly_one_chapter() {
        let before = mark();
        let after = before.replace("A withered", "A shrivelled");
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &before).unwrap();
        sous.publish().unwrap();

        sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
        let buffer = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 1);
        assert_eq!(rows(&buffer).len(), 3, "still one finding per chapter");
    }

    /// The same chapter 1 in two books: one cache entry, two published rows.
    #[test]
    fn two_identical_chapters_share_one_observation_and_report_both() {
        let text = book("GEN", &["The same \\\\ words."]);
        let twin = text
            .replace("\\id GEN", "\\id MRK")
            .replace("\\h GEN", "\\h MRK");
        let mut sous = sous();
        sous.update("a/gen.usfm", Role::Target, &text).unwrap();
        sous.update("b/mrk.usfm", Role::Target, &twin).unwrap();
        let buffer = sous.publish().unwrap();

        assert_eq!(sous.last_mapped(), 1, "the second chapter hit");
        assert_eq!(sous.observations.len(), 1);
        let published = rows(&buffer);
        assert_eq!(published.len(), 2, "counted at both positions");
        assert_eq!(published[0].0, 0);
        assert_eq!(published[1].0, 1);
        assert_eq!(
            (published[0].2, published[0].3),
            (published[1].2, published[1].3),
            "identical books rebase identically"
        );
    }

    #[test]
    fn a_removed_book_and_its_id_leave_the_next_snapshot() {
        let mut sous = sous();
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        let before = sous.publish().unwrap();
        assert_eq!(ids(&before), vec!["a/gen.usfm", "b/mrk.usfm"]);

        assert!(sous.remove(&BookId::from("a/gen.usfm")));
        let after = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 0, "the survivor was already mapped");
        assert_eq!(ids(&after), vec!["b/mrk.usfm"]);
        assert_ne!(
            CorpusSnapshot::open(&before).unwrap().snapshot_id(),
            CorpusSnapshot::open(&after).unwrap().snapshot_id(),
            "a different corpus is a different snapshot"
        );
    }

    #[test]
    fn a_products_only_book_publishes_from_its_eager_map_or_from_cache() {
        let text = mark();
        let id = BookId::from("b/mrk.usfm");
        let mut cold = sous();
        cold.update_with(id.clone(), Role::Target, Retain::ProductsOnly, &text)
            .unwrap();
        let eager = cold.publish().unwrap();
        assert_eq!(
            cold.last_mapped(),
            3,
            "the update mapped all three while it held the text"
        );

        let mut warm = sous();
        warm.update(id.clone(), Role::Target, &text).unwrap();
        let first = warm.publish().unwrap();
        warm.update_with(id, Role::Target, Retain::ProductsOnly, &text)
            .unwrap();
        assert_eq!(warm.publish().unwrap(), first, "the chapter table sufficed");
        assert_eq!(warm.last_mapped(), 0);
        assert_eq!(eager, first, "either retention publishes the same bytes");
    }

    /// The case a lazy map could not serve: nothing holds the text of the
    /// version being edited away from, so each update must key its own.
    #[test]
    fn a_products_only_book_is_edited_and_republished_through_the_expediter() {
        let id = BookId::from("b/mrk.usfm");
        let before = mark();
        let after = before.replace("A withered", "A shrivelled");
        let mut sous = sous();
        sous.update_with(id.clone(), Role::Target, Retain::ProductsOnly, &before)
            .unwrap();
        let first = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 3);

        sous.update_with(id, Role::Target, Retain::ProductsOnly, &after)
            .unwrap();
        let second = sous.publish().unwrap();
        assert_eq!(sous.last_mapped(), 1, "only the edited chapter");
        assert_eq!(rows(&second).len(), 3, "still one finding per chapter");
        assert_ne!(first, second, "a different corpus is a different snapshot");
    }

    /// One book edited past its ring: the current table plus `kept` behind it,
    /// and no observation only a dropped table named.
    #[test]
    fn edits_past_the_ring_leave_one_table_per_generation_and_no_orphans() {
        let mut sous = sous().with_generations(1);
        for word in ["one", "two", "three", "four"] {
            let text = mark().replace("A withered", word);
            sous.update("b/mrk.usfm", Role::Target, &text).unwrap();
            sous.publish().unwrap();
        }

        assert_eq!(sous.resident_tables(), 2, "the current text and one before");
        assert_eq!(
            sous.resident_observations(),
            4,
            "chapters 1 and 2 are shared; chapter 3 survives twice"
        );
    }

    /// An undo restores byte-identical text, so it restores the checksum: two
    /// edits back is a table hit while the ring is that deep.
    #[test]
    fn an_undo_inside_the_ring_republishes_without_mapping() {
        let mut sous = sous().with_generations(2);
        let versions: Vec<String> = ["one", "two", "three"]
            .iter()
            .map(|word| mark().replace("A withered", word))
            .collect();
        let mut published = Vec::new();
        for text in &versions {
            sous.update("b/mrk.usfm", Role::Target, text).unwrap();
            published.push(sous.publish().unwrap());
        }

        sous.update("b/mrk.usfm", Role::Target, &versions[0])
            .unwrap();
        assert_eq!(sous.publish().unwrap(), published[0], "byte for byte");
        assert_eq!(sous.last_mapped(), 0, "the table was still resident");
    }

    #[test]
    fn an_undo_beyond_the_ring_remaps_only_its_chapter() {
        let mut sous = sous().with_generations(1);
        let versions: Vec<String> = ["one", "two", "three"]
            .iter()
            .map(|word| mark().replace("A withered", word))
            .collect();
        let mut published = Vec::new();
        for text in &versions {
            sous.update("b/mrk.usfm", Role::Target, text).unwrap();
            published.push(sous.publish().unwrap());
        }

        sous.update("b/mrk.usfm", Role::Target, &versions[0])
            .unwrap();
        assert_eq!(sous.publish().unwrap(), published[0]);
        assert_eq!(sous.last_mapped(), 1, "chapters 1 and 2 still hit");
    }

    #[test]
    fn removing_a_book_drops_its_ring_and_every_table_in_it() {
        let mut sous = sous().with_generations(4);
        for word in ["one", "two"] {
            let text = mark().replace("A withered", word);
            sous.update("b/mrk.usfm", Role::Target, &text).unwrap();
            sous.publish().unwrap();
        }
        sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
        sous.publish().unwrap();
        assert_eq!(sous.resident_tables(), 3, "two MRK generations and GEN");

        assert!(sous.remove(&BookId::from("b/mrk.usfm")));
        sous.publish().unwrap();
        assert_eq!(sous.resident_tables(), 1);
        assert_eq!(sous.resident_observations(), 1, "GEN's one chapter");
    }

    #[test]
    fn one_id_line_under_two_ids_publishes_both_in_id_order() {
        let text = genesis();
        let mut sous = sous();
        sous.update("a/gen.usfm", Role::Target, &text).unwrap();
        sous.update("a/gen-copy.usfm", Role::Target, &text).unwrap();
        let buffer = sous.publish().unwrap();

        assert_eq!(sous.last_mapped(), 1, "one text, one chapter table");
        assert_eq!(ids(&buffer), vec!["a/gen-copy.usfm", "a/gen.usfm"]);
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot.book_by_id("a/gen.usfm").unwrap().index().get(),
            1,
            "the second row is reachable only by id"
        );
    }

    #[test]
    fn published_rows_carry_the_pass_findings_in_raw_utf16() {
        let mut sous = sous();
        // The onion is 2 UTF-16 units for 4 raw bytes, so the pair after it
        // publishes two units short of its raw offset.
        sous.update(
            "b/mrk.usfm",
            Role::Target,
            "\\id MRK\n\\c 1\n\\p\n\\v 1 An 🧅 \\\\ here.\n",
        )
        .unwrap();
        let buffer = sous.publish().unwrap();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        let finding = snapshot.book_by_id("b/mrk.usfm").unwrap().at(0).unwrap();

        let FindingKind::Hygiene(digest) = finding.kind() else {
            panic!("hygiene kind")
        };
        assert_eq!(digest.run(), 2);
        assert_eq!((finding.from(), finding.to()), (27, 29));
    }
}
