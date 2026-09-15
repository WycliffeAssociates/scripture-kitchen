//! The Expediter's own claims: what a publication recomputes, what it reuses,
//! and what it sweeps.
//!
//! Instrument: SHAPES — synthetic books built here, chosen so one keystroke
//! moves exactly one chapter.

use core::sync::atomic::{AtomicU64, Ordering};

use super::*;
use sous_core::ChapterInput;
use sous_core::hygiene::HygieneBytes;
use sous_core::{
    BandStep, BookIndex, Brigade, Channel, CorpusSnapshot, FindingKind, JudgingConfig, SchemaStamp,
    Staircase, Substrate, Words,
};

use crate::pantry::{PantryError, Retain};

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

fn sous() -> Expediter<Brigade> {
    Expediter::new(Brigade::default(), 1 << 20)
}

/// A chapter-grain pass: `Brigade` carries `Words`, which is retained at
/// book grain, so what one chapter's cache key does and does not cover is
/// only visible through a pass that keeps its chapters.
fn grain() -> Expediter<HygieneBytes> {
    Expediter::new(HygieneBytes, 1 << 20)
}

/// One member's own map and remap calls, which `last_mapped` cannot
/// separate: it counts chapters, not the walks a chapter ran.
#[derive(Debug, Default)]
struct Counting<P> {
    inner: P,
    maps: AtomicU64,
    remaps: AtomicU64,
}

impl<P> Counting<P> {
    fn maps(&self) -> u64 {
        self.maps.load(Ordering::Relaxed)
    }

    fn remaps(&self) -> u64 {
        self.remaps.load(Ordering::Relaxed)
    }
}

impl<P: ChapterPass> ChapterPass for Counting<P> {
    type Observation = P::Observation;
    type Aggregate = P::Aggregate;
    type Config = P::Config;
    const SCHEMA: SchemaStamp = P::SCHEMA;
    const RETAIN_CHAPTERS: bool = P::RETAIN_CHAPTERS;
    const CHAPTER_SITES: bool = P::CHAPTER_SITES;

    fn map(&self, chapter: ChapterInput<'_>) -> Self::Observation {
        self.maps.fetch_add(1, Ordering::Relaxed);
        self.inner.map(chapter)
    }

    fn fold(&self, book: &[ChapterObs<&Self::Observation>]) -> Self::Aggregate {
        self.inner.fold(book)
    }

    fn release(&self, obs: &mut Self::Observation) {
        self.inner.release(obs);
    }

    fn is_released(&self, observation: &Self::Observation) -> bool {
        self.inner.is_released(observation)
    }

    fn remap(&self, chapter: ChapterInput<'_>, observation: &mut Self::Observation) {
        self.remaps.fetch_add(1, Ordering::Relaxed);
        self.inner.remap(chapter, observation);
    }

    fn judge(&self, corpus: &[&Self::Aggregate], config: &Self::Config, out: &mut Findings) {
        self.inner.judge(corpus, config, out);
    }

    fn aggregate_bytes(&self, aggregate: &Self::Aggregate) -> usize {
        self.inner.aggregate_bytes(aggregate)
    }

    fn tally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        self.inner.tally(totals, books);
    }

    fn untally(&self, totals: &mut CorpusTotals, books: &[&Self::Aggregate]) {
        self.inner.untally(totals, books);
    }

    fn judge_resident(
        &self,
        corpus: &[&Self::Aggregate],
        totals: &CorpusTotals,
        config: &Self::Config,
        out: &mut Findings,
    ) {
        self.inner.judge_resident(corpus, totals, config, out);
    }

    fn locate(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[sous_core::Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        self.inner
            .locate(book, text, chapters, verses, aggregate, out);
    }

    fn locate_book(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[sous_core::Verse],
        aggregate: &Self::Aggregate,
        out: &mut Findings,
    ) {
        self.inner
            .locate_book(book, text, chapters, verses, aggregate, out);
    }

    fn locate_chapters(
        &self,
        book: BookIndex,
        text: &str,
        chapters: &[Chapter],
        verses: &[sous_core::Verse],
        range: core::ops::Range<usize>,
        aggregate: &Self::Aggregate,
        counts: &mut Vec<u32>,
        out: &mut Findings,
    ) {
        self.inner
            .locate_chapters(book, text, chapters, verses, range, aggregate, counts, out);
    }

    fn firing(
        &self,
        aggregate: &Self::Aggregate,
        patterns: &[Pattern],
        out: &mut Vec<PatternIndex>,
    ) {
        self.inner.firing(aggregate, patterns, out);
    }
}

/// Every HYGIENE row of a published buffer as (book index, id, from, to).
///
/// The substrate sites its conventions into the same sections; those rows
/// are counted by [`site_rows`].
fn rows(buffer: &[u8]) -> Vec<(u16, String, u32, u32)> {
    let snapshot = CorpusSnapshot::open(buffer).unwrap();
    (0..snapshot.len())
        .flat_map(|index| {
            let book = snapshot
                .book(BookIndex::new(index).unwrap())
                .expect("directory position");
            (0..book.len()).filter_map(move |row| {
                let finding = book.at(row).unwrap();
                matches!(finding.kind(), FindingKind::Hygiene(_)).then(|| {
                    (
                        finding.book_idx().get(),
                        book.id().to_string(),
                        finding.from(),
                        finding.to(),
                    )
                })
            })
        })
        .collect()
}

/// Every convention row as (book index, pattern index, reason bits).
fn site_rows(buffer: &[u8]) -> Vec<(u16, u16, u16)> {
    let snapshot = CorpusSnapshot::open(buffer).unwrap();
    (0..snapshot.len())
        .flat_map(|index| {
            let book = snapshot
                .book(BookIndex::new(index).unwrap())
                .expect("directory position");
            (0..book.len()).filter_map(move |row| {
                let finding = book.at(row).unwrap();
                let FindingKind::Convention(digest) = finding.kind() else {
                    return None;
                };
                Some((
                    finding.book_idx().get(),
                    digest.pattern().get(),
                    digest.reasons().bits(),
                ))
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

/// Byte equality with a cold `analyze` is pinned where its oracle lives,
/// in `galley/tests/equivalence.rs`; these tests pin what is reused.
#[test]
fn the_publication_is_in_canonical_order_and_maps_every_chapter_once() {
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    let buffer = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 4, "one GEN chapter, three MRK");
    // Canonical order, not update order: GEN before MRK.
    assert_eq!(ids(&buffer), vec!["a/gen.usfm", "b/mrk.usfm"]);
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
    let mut sous = grain();
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
    let mut sous = grain();
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
    let mut sous = grain();
    sous.update("b/mrk.usfm", Role::Target, &before).unwrap();
    sous.publish().unwrap();

    sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
    let buffer = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 1);
    assert_eq!(rows(&buffer).len(), 3, "still one finding per chapter");
}

/// `Words` retains no chapter rows, so an edit anywhere in a COLD book
/// re-maps that whole book — and no other. `with_hot_books(0)` is what
/// makes every book cold; the hot set's own claim is pinned below.
#[test]
fn a_book_grain_pass_remaps_the_edited_book_and_nothing_else() {
    let mut sous = sous().with_hot_books(0);
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    sous.publish().unwrap();

    let after = mark().replace("A withered", "A shrivelled");
    sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
    let buffer = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 3, "MRK's three chapters, GEN's none");
    assert_eq!(sous.last_folded(), 1, "GEN judged its cached aggregate");
    assert_eq!(rows(&buffer).len(), 4, "still one finding per chapter");
}

/// What a book-grain member leaves resident is its aggregate, not its
/// rows: the fold reads them and the publication drops them.
#[test]
fn a_book_grain_pass_sheds_its_chapter_rows_after_the_fold() {
    let mut sous = sous().with_hot_books(0);
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.resident_observations(), 3);
    assert!(
        sous.observations
            .values()
            .all(|(_, _, words)| words.words().is_empty()),
        "every word row is back to its empty default"
    );
}

/// A book-grain member sheds its rows, so its book is walked whole again —
/// but only IT is: the chapter-grain member beside it keeps every slot the
/// edit did not move, and maps the one chapter it did.
#[test]
fn a_keystroke_in_one_chapter_rewalks_words_for_the_book_but_glyphs_only_for_the_chapter() {
    let mut sous: Expediter<(Counting<Substrate>, Words)> =
        Expediter::new((Counting::default(), Words), 1 << 20).with_hot_books(0);
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    let before = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 3, "cold: every chapter");
    assert_eq!(sous.last_remapped(), 0, "nothing was held to re-walk");
    assert_eq!(sous.pass().0.maps(), 3, "the glyph walk ran three times");

    let after = mark().replace("A withered", "A shrivelled");
    sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
    let published = sous.publish().unwrap();
    assert_eq!(
        sous.last_mapped(),
        3,
        "the words are still a book-wide walk"
    );
    assert_eq!(
        sous.last_remapped(),
        2,
        "two chapters kept their glyph rows"
    );
    assert_eq!(
        sous.pass().0.maps(),
        4,
        "one more glyph walk, for the edited chapter alone"
    );
    assert_eq!(
        sous.pass().0.remaps(),
        0,
        "a member that shed nothing is not asked to walk again"
    );
    assert_ne!(published, before, "the edit is published");
}

/// Four books in canonical order, so a cold publication leaves the last
/// two hot and the first two cold.
fn four_books(sous: &mut Expediter<Brigade>) {
    for (id, code) in [
        ("a/gen.usfm", "GEN"),
        ("b/mrk.usfm", "MRK"),
        ("c/luk.usfm", "LUK"),
        ("d/rev.usfm", "REV"),
    ] {
        let text = match code {
            "GEN" => genesis(),
            "MRK" => mark(),
            other => book(other, &["A first \\\\ chapter.", "A second \\\\ one."]),
        };
        sous.update(id, Role::Target, &text).unwrap();
    }
    sous.publish().unwrap();
}

/// Every word row the observation cache is holding right now.
fn word_row_bytes(sous: &Expediter<Brigade>) -> usize {
    sous.observations
        .values()
        .map(|(_, _, words)| words.resident_bytes())
        .sum()
}

/// One keystroke into MRK and the publication after it.
impl Expediter<Brigade> {
    fn publish_after(&mut self, text: &str) -> Vec<u8> {
        self.update("b/mrk.usfm", Role::Target, text).unwrap();
        self.publish().unwrap()
    }
}

/// The hot set is what turns the second keystroke in one book into one
/// chapter's walk: the first one's rows were shed at the last fold, and
/// the edit that shed them is what made the book hot.
#[test]
fn a_second_keystroke_in_a_hot_book_remaps_only_the_chapter_it_moved() {
    let mut sous = sous();
    four_books(&mut sous);
    assert_eq!(
        sous.hot_books(),
        [BookId::from("d/rev.usfm"), BookId::from("c/luk.usfm")],
        "the last two indexed"
    );

    let once = mark().replace("A withered", "A shrivelled");
    sous.update("b/mrk.usfm", Role::Target, &once).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 3, "cold: MRK's three chapters");
    assert_eq!(sous.last_remapped(), 2, "two of them kept a glyph row");
    assert_eq!(sous.hot_books()[0], BookId::from("b/mrk.usfm"));

    let twice = once.replace("He entered", "He walked into");
    sous.update("b/mrk.usfm", Role::Target, &twice).unwrap();
    let published = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 1, "hot: the moved chapter alone");
    assert_eq!(sous.last_remapped(), 0, "nothing was shed to walk again");
    assert_eq!(rows(&published).len(), 8, "one finding per chapter");
}

/// The site cache follows the same grain: a keystroke walks the chapter it
/// landed in for word rows and replays the rest of the book rebased.
#[test]
fn a_keystroke_re_sites_one_chapter() {
    let mut sous = sous().with_hot_books(1);
    four_books(&mut sous);
    let once = mark().replace("A withered", "A shrivelled");
    sous.update("b/mrk.usfm", Role::Target, &once).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_located(), 1, "MRK alone rescanned");
    assert_eq!(sous.last_sited_chapters(), 3, "none of them cached yet");

    let twice = once.replace("He entered", "He walked into");
    let published = sous.publish_after(&twice);
    assert_eq!(sous.last_located(), 1, "MRK alone again");
    assert_eq!(sous.last_sited_chapters(), 1, "the chapter it moved");
    assert_eq!(rows(&published).len(), 8, "one finding per chapter");
}

/// A moved judging knob renumbers and re-decides the whole table, so every
/// chapter's key moves with it and nothing replays.
#[test]
fn a_pattern_change_re_sites_every_chapter() {
    let mut sous = sous().with_hot_books(1);
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_sited_chapters(), 3, "cold: none of them cached");
    sous.publish_after(&mark().replace("A withered", "A shrivelled"));
    assert_eq!(sous.last_sited_chapters(), 1, "warm before the knob");

    let config = JudgingConfig {
        rarity_floor: 1,
        ..JudgingConfig::default()
    };
    sous.set_config(((), config, config));
    sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 0, "a knob maps nothing");
    assert_eq!(
        sous.last_sited_chapters(),
        3,
        "a new firing set is a new key for every chapter"
    );
}

// ------------------------------------------------- kept word verdicts

/// One book of ordinary prose: shared function words, and a noun only this
/// book uses, so a keystroke here moves some of the tally's keys and not
/// the rest.
fn prose(code: &str, own: &str) -> String {
    let first = format!("The {own} spoke and the people heard the {own} gladly");
    let second = format!("A {own} came to the city and a {own} left the city");
    book(code, &[&first, &second])
}

/// Four such books, in canonical order.
fn prose_corpus() -> Vec<(&'static str, String)> {
    vec![
        ("a/gen.usfm", prose("GEN", "shepherd")),
        ("b/mrk.usfm", prose("MRK", "fisher")),
        ("c/luk.usfm", prose("LUK", "tanner")),
        ("d/rev.usfm", prose("REV", "rider")),
    ]
}

/// Doubled-channel rows in a publication's pattern table: what a corpus-wide
/// recusal turns on and off.
fn doubled_rows(buffer: &[u8]) -> usize {
    CorpusSnapshot::open(buffer)
        .unwrap()
        .patterns()
        .unwrap()
        .iter()
        .filter(|pattern| pattern.channel == Channel::Doubled)
        .count()
}

fn registered(books: &[(&str, String)]) -> Expediter<Brigade> {
    let mut sous = sous();
    for (id, text) in books {
        sous.update(*id, Role::Target, text).unwrap();
    }
    sous
}

/// A cold publication of exactly these texts: the bytes a resident one owes
/// and the key count "everything" means.
fn cold(books: &[(&str, String)]) -> (Vec<u8>, usize) {
    let mut fresh = registered(books);
    let buffer = fresh.publish().unwrap();
    (buffer, fresh.last_words_judged())
}

/// The delta is the whole claim: a keystroke re-judges the words the edited
/// book holds and keeps every other verdict, and still publishes the bytes
/// a cold run does.
#[test]
fn a_keystroke_re_judges_only_the_moved_words() {
    let mut books = prose_corpus();
    let mut sous = registered(&books);
    sous.publish().unwrap();
    let whole = sous.last_words_judged();
    assert!(whole > 0, "the corpus has words to judge");

    books[1].1 = prose("MRK", "fisher").replace("gladly", "sadly");
    sous.update(books[1].0, Role::Target, &books[1].1).unwrap();
    let published = sous.publish().unwrap();
    let moved = sous.last_words_judged();
    assert!(moved > 0, "the edited book's own keys moved");
    assert!(
        moved < whole,
        "a keystroke judged {moved} of {whole} keys, which is all of them"
    );
    assert_eq!(published, cold(&books).0, "the merged list is the cold one");
}

/// A glyph crossing `terminal_upper_share_bp` re-decides which stored
/// positions are free, so every casing verdict is re-judged, moved or not.
///
/// The control is the same edit in the same book under a follower that
/// leaves `;` where it was: that one keeps the verdicts the delta did not
/// name, which is what makes this a claim about the table.
fn after_handoffs(follower: &str) -> (usize, usize) {
    let handoffs = "and; a fig and; a fig and; a fig and; a fig and; a fig and; a fig";
    let mut books = prose_corpus();
    books[0].1 = book("GEN", &["In the beginning", handoffs]);
    let mut sous = registered(&books);
    sous.publish().unwrap();

    let moved = handoffs.replace("; a", &format!("; {follower}"));
    books[0].1 = book("GEN", &["In the beginning", &moved]);
    sous.update(books[0].0, Role::Target, &books[0].1).unwrap();
    let published = sous.publish().unwrap();
    let (bytes, whole) = cold(&books);
    assert_eq!(published, bytes, "the publication is the cold one");
    (sous.last_words_judged(), whole)
}

#[test]
fn a_table_change_re_judges_everything() {
    let (kept, whole) = after_handoffs("e");
    assert!(
        kept < whole,
        "a follower that moves no table keeps {} verdicts",
        whole - kept
    );
    let (judged, whole) = after_handoffs("A");
    assert_eq!(
        judged, whole,
        "a moved terminal table re-judges the whole tally"
    );
}

/// A judging knob moves verdicts the delta never names, so it re-judges the
/// whole tally even though not one book's counts moved.
#[test]
fn a_config_flip_re_judges_everything() {
    let books = prose_corpus();
    let mut sous = registered(&books);
    sous.publish().unwrap();

    let config = JudgingConfig {
        word_support_floor: 1,
        ..JudgingConfig::default()
    };
    sous.set_config(((), config, config));
    let published = sous.publish().unwrap();
    let (_, whole) = cold(&books);
    assert_eq!(sous.last_mapped(), 0, "a knob maps nothing");
    assert_eq!(
        sous.last_words_judged(),
        whole,
        "a moved knob re-judges the whole tally"
    );
    let mut fresh = registered(&books);
    fresh.set_config(((), config, config));
    assert_eq!(
        published,
        fresh.publish().unwrap(),
        "and publishes the cold bytes under that knob"
    );
}

/// The doubled channel recuses itself corpus-wide, so vocabulary arriving
/// in one book decides whether a doubling in another is judged at all.
#[test]
fn a_recusal_flip_re_judges_everything() {
    let mut books = vec![
        ("a/gen.usfm", book("GEN", &["the the sky the the"])),
        ("b/mrk.usfm", book("MRK", &["one two three four five six"])),
    ];
    // A vocabulary of eight words, one of which doubles, is 1,250 bp; ten
    // more words put it under the bar and the channel stops recusing.
    let config = JudgingConfig {
        doubles_productive_bp: 1_000,
        word_support_floor: 1,
        word_bands: Staircase::new(Staircase::WORD_STEPS.map(|step| BandStep {
            share_bp: 9_000,
            ..step
        }))
        .expect("the word bounds still ascend"),
        ..JudgingConfig::default()
    };
    let mut sous = registered(&books);
    sous.set_config(((), config, config));
    let before = doubled_rows(&sous.publish().unwrap());

    books[1].1 = book("MRK", &["one two three four five six seven eight nine ten"]);
    sous.update(books[1].0, Role::Target, &books[1].1).unwrap();
    let published = sous.publish().unwrap();
    assert_ne!(
        before,
        doubled_rows(&published),
        "the recusal crossed the bar"
    );

    let mut fresh = registered(&books);
    fresh.set_config(((), config, config));
    let bytes = fresh.publish().unwrap();
    assert_eq!(
        sous.last_words_judged(),
        fresh.last_words_judged(),
        "a crossed recusal re-judges the whole tally"
    );
    assert_eq!(published, bytes, "and publishes the cold bytes");
}

/// A firing set is a function of a book's counts and the table's CLAIMS, so
/// a publication whose table says the same thing walks nobody's again.
#[test]
fn firing_is_replayed_for_untouched_books() {
    let mut books = prose_corpus();
    let mut sous = registered(&books);
    sous.publish().unwrap();
    assert_eq!(sous.last_firing_walks(), 4, "cold: every book");

    sous.publish().unwrap();
    assert_eq!(sous.last_firing_walks(), 0, "an unchanged republication");

    // A masked footnote moves the book's checksum and not one count, so
    // the table says exactly what it said and only this book is walked.
    books[1].1 = books[1]
        .1
        .replace("The fisher", "The\\f + \\ft note\\f* fisher");
    sous.update(books[1].0, Role::Target, &books[1].1).unwrap();
    let published = sous.publish().unwrap();
    assert_eq!(sous.last_firing_walks(), 1, "the book whose text moved");
    assert_eq!(published, cold(&books).0, "and the bytes are the cold ones");
}

/// And the rows go when the book does: the hot set is what keeps them, so
/// a book pushed out of it leaves nothing behind.
#[test]
fn a_book_leaving_the_hot_set_drops_its_chapter_rows() {
    let mut sous = sous().with_hot_books(1);
    four_books(&mut sous);
    sous.publish_after(&mark().replace("A withered", "A shrivelled"));
    assert_eq!(sous.chapter_sites.len(), 3, "MRK's three chapters");

    let text = book("LUK", &["A first \\\\ chapter.", "A later \\\\ one."]);
    sous.update("c/luk.usfm", Role::Target, &text).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.hot_books(), [BookId::from("c/luk.usfm")]);
    assert_eq!(sous.chapter_sites.len(), 2, "LUK's two, and MRK's are gone");
}

/// And it is bounded: the third book edited after it pushes the first out,
/// and the rows it was keeping go back at that publication.
#[test]
fn a_book_that_falls_out_of_the_hot_set_walks_whole_again() {
    let mut sous = sous();
    four_books(&mut sous);
    let edited = mark().replace("A withered", "A shrivelled");
    sous.update("b/mrk.usfm", Role::Target, &edited).unwrap();
    sous.publish().unwrap();
    let warm = word_row_bytes(&sous);

    for (id, code) in [("c/luk.usfm", "LUK"), ("d/rev.usfm", "REV")] {
        let text = book(code, &["A first \\\\ chapter.", "A later \\\\ one."]);
        sous.update(id, Role::Target, &text).unwrap();
        sous.publish().unwrap();
    }
    assert!(
        !sous.hot_books().contains(&BookId::from("b/mrk.usfm")),
        "two other books were edited after it"
    );
    assert!(
        word_row_bytes(&sous) < warm,
        "MRK's word rows went back: {} B against {warm} B",
        word_row_bytes(&sous)
    );

    let again = edited.replace("He entered", "He walked into");
    sous.update("b/mrk.usfm", Role::Target, &again).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 3, "cold again: the whole book");
    assert_eq!(sous.last_remapped(), 2);
}

/// What the hot set costs is the rows it keeps, and `resident_bytes`
/// says so: the same corpus with the set switched off is smaller by
/// exactly one book's word rows.
#[test]
fn resident_bytes_counts_the_rows_a_hot_book_keeps() {
    let mut cold = sous().with_hot_books(0);
    let mut hot = sous();
    for sous in [&mut cold, &mut hot] {
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        sous.publish().unwrap();
    }
    let kept: usize = hot
        .observations
        .values()
        .map(|obs| hot.pass().observation_bytes(obs))
        .sum::<usize>()
        - cold
            .observations
            .values()
            .map(|obs| cold.pass().observation_bytes(obs))
            .sum::<usize>();
    assert!(kept > 0, "MRK's three chapters hold cased words");
    let sited: usize = hot
        .chapter_sites
        .values()
        .map(|rows| size_of::<ChapterSiteKey>() + size_of_val(&**rows))
        .sum();
    assert!(sited > 0, "and their own site rows");
    assert!(cold.chapter_sites.is_empty(), "a cold book keeps none");
    // The set names its books, and that list is pinned like the rings are.
    let named: usize = hot
        .hot_books()
        .iter()
        .map(|id| size_of::<BookId>() + id.as_str().len())
        .sum();
    assert_eq!(
        hot.resident_bytes() - cold.resident_bytes(),
        kept + sited + named,
        "the difference is the rows a hot book keeps, and the set that names it"
    );
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

/// A target's findings are placed by rescanning its own text, so the
/// registry refuses one that would keep none.
#[test]
fn a_target_cannot_be_products_only() {
    let mut sous = sous();
    let id = BookId::from("b/mrk.usfm");
    assert_eq!(
        sous.update_with(id.clone(), Role::Target, Retain::ProductsOnly, &mark())
            .err(),
        Some(PublishError::Pantry(PantryError::TargetNeedsText {
            id: id.clone()
        }))
    );
    assert!(sous.pantry().books(Role::Target).is_empty());
}

/// A book whose text and firing set both stand replays its rows.
#[test]
fn an_unchanged_book_replays_its_sites() {
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    let first = sous.publish().unwrap();
    assert_eq!(sous.last_located(), 2, "both books were rescanned once");
    assert!(!site_rows(&first).is_empty(), "the corpus sites something");

    let second = sous.publish().unwrap();
    assert_eq!(sous.last_located(), 0, "no text was read again");
    assert_eq!(second, first, "a replay publishes the same bytes");
}

/// A glyph added to one book moves the corpus denominators, so the other
/// book's own firing set may stand while its neighbour's moves.
#[test]
fn a_denominator_flip_relocates_only_books_whose_firing_set_moved() {
    let mut sous = sous();
    // The dagger is GEN's alone, so MRK's firing set cannot move when
    // GEN's counts of it do.
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update(
        "a/gen.usfm",
        Role::Target,
        &book("GEN", &["In the \\\\ beginning\u{2020}."]),
    )
    .unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_located(), 2);

    sous.update(
        "a/gen.usfm",
        Role::Target,
        &book(
            "GEN",
            &["In the \\\\ beginning\u{2020}\u{2020}\u{2020}\u{2020}\u{2020}."],
        ),
    )
    .unwrap();
    sous.publish().unwrap();
    assert_eq!(
        sous.last_located(),
        1,
        "only the edited book; MRK's own firing set never moved"
    );
}

/// Judging config is in no chapter or book key, so a re-judge places its
/// new patterns without mapping or folding anything.
#[test]
fn set_config_relocates_from_cached_aggregates_without_mapping() {
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    let before = sous.publish().unwrap();

    let config = JudgingConfig {
        rarity_floor: 1,
        ..JudgingConfig::default()
    };
    sous.set_config(((), config, config));
    let after = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 0, "no chapter was mapped");
    assert_eq!(sous.last_folded(), 0, "no book was folded");
    assert_eq!(
        sous.last_located(),
        1,
        "the firing set moved, so it rescanned"
    );
    assert_ne!(site_rows(&after), site_rows(&before));
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
///
/// A chapter-grain pass, deliberately: `Brigade` carries `Words`, whose
/// aggregate is pruned to the current checksum only (see the aggregate
/// tests below), so an in-ring undo of a book-grain pass re-maps anyway.
#[test]
fn an_undo_inside_the_ring_republishes_without_mapping() {
    let mut sous = grain().with_generations(2);
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
    let mut sous = grain().with_generations(1);
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

/// A book-grain pass keeps its aggregate for the CURRENT checksum only:
/// older generations are dropped at sweep even while the ring and the
/// chapter tables behind them survive, so an in-ring undo still re-maps
/// and re-folds the book it lands on — and publishes the identical bytes.
#[test]
fn a_book_grain_pass_keeps_one_aggregate_per_book_through_edits_and_an_undo() {
    let mut sous = sous().with_generations(4).with_hot_books(0);
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    let versions: Vec<String> = ["one", "two", "three", "four"]
        .iter()
        .map(|word| mark().replace("A withered", word))
        .collect();
    let mut published = Vec::new();
    for text in &versions {
        sous.update("b/mrk.usfm", Role::Target, text).unwrap();
        published.push(sous.publish().unwrap());
    }
    assert_eq!(
        sous.resident_aggregates(),
        2,
        "one per book, not one per retained generation"
    );

    sous.update("b/mrk.usfm", Role::Target, &versions[0])
        .unwrap();
    let undone = sous.publish().unwrap();
    assert_eq!(undone, published[0], "byte for byte");
    assert!(
        sous.last_mapped() > 0,
        "the pruned aggregate forced a re-map"
    );
    assert_eq!(
        sous.resident_aggregates(),
        2,
        "still one per book after the undo"
    );
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

/// The other half of the parallel gate: `galley/tests/equivalence.rs`
/// holds the whole-Bible run against the cold oracle.
#[cfg(feature = "parallel")]
#[test]
fn the_parallel_map_publishes_the_serial_bytes() {
    // The twin is the same chapters under a second id, so the cross-book
    // dedup the parallel queue does for itself is exercised too.
    let books = [
        ("a/gen.usfm", genesis()),
        ("b/mrk.usfm", mark()),
        ("c/mrk-copy.usfm", mark()),
    ];
    let mut serial = sous().serial();
    let mut parallel = sous();
    for (id, text) in &books {
        serial.update(*id, Role::Target, text).unwrap();
        parallel.update(*id, Role::Target, text).unwrap();
    }
    assert_eq!(serial.publish().unwrap(), parallel.publish().unwrap());
    assert_eq!(serial.last_mapped(), 4, "GEN's chapter and MRK's three");
    assert_eq!(parallel.last_mapped(), serial.last_mapped());
    assert_eq!(
        parallel.resident_observations(),
        serial.resident_observations()
    );
}

#[test]
fn publish_unchanged_folds_nothing_and_rejudges_to_the_same_bytes() {
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    let first = sous.publish().unwrap();
    assert_eq!(sous.last_folded(), 2, "both books cold");

    let second = sous.publish().unwrap();
    assert_eq!(sous.last_folded(), 0, "no book's projection moved");
    assert_eq!(first, second, "the cached aggregates judge the same");
}

#[test]
fn only_an_edited_book_is_folded_again() {
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    sous.publish().unwrap();

    let after = mark().replace("A withered", "A shrivelled");
    sous.update("b/mrk.usfm", Role::Target, &after).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_folded(), 1, "GEN judged its cached aggregate");
}

/// A config change is a re-judge and never a re-fold or a re-map.
#[test]
fn setting_the_config_folds_nothing_and_maps_nothing() {
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    let first = sous.publish().unwrap();

    sous.set_config(<Brigade as ChapterPass>::Config::default());
    let second = sous.publish().unwrap();
    assert_eq!(sous.last_folded(), 0);
    assert_eq!(sous.last_mapped(), 0);
    assert_eq!(first, second, "the same config judges the same bytes");
}

/// The same aggregates, a different config, a different pattern table:
/// judging is the only step a config reaches.
#[test]
fn changing_the_config_re_judges_from_cached_aggregates() {
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.update("a/gen.usfm", Role::Target, &genesis()).unwrap();
    let before = CorpusSnapshot::open(&sous.publish().unwrap())
        .unwrap()
        .pattern_count();

    let config = JudgingConfig {
        rarity_floor: 10_000,
        ..JudgingConfig::default()
    };
    sous.set_config(((), config, config));
    let buffer = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 0);
    assert_eq!(sous.last_folded(), 0);
    assert_ne!(
        CorpusSnapshot::open(&buffer).unwrap().pattern_count(),
        before
    );
}

// ---------------------------------------------------- the source pairing

/// A book of `count` one-verse chapters is too slow to build; this is one
/// chapter of `count` verses, verse `i` as long as `length(i)` says.
fn sized(code: &str, count: usize, length: impl Fn(usize) -> usize) -> String {
    let mut text = format!("\\id {code}\n\\h {code}\n\\c 1\n\\p\n");
    for verse in 0..count {
        text.push_str(&format!(
            "\\v {} {}\n",
            verse + 1,
            "a".repeat(length(verse))
        ));
    }
    text
}

/// Sixty verses whose lengths cycle 40..=46, which is the varied sample a
/// median and a MAD need; verse 61 is half length and verse 62 double.
fn paired_target() -> String {
    sized("MRK", 62, |verse| match verse {
        60 => 20,
        61 => 80,
        other => 40 + other % 7,
    })
}

/// The same keys at a constant 40 characters.
fn paired_source() -> String {
    sized("MRK", 62, |_| 40)
}

/// Every length row of a published buffer, as (book index, from, to).
fn length_rows(buffer: &[u8]) -> Vec<(u16, u32, u32)> {
    let snapshot = CorpusSnapshot::open(buffer).unwrap();
    (0..snapshot.len())
        .flat_map(|index| {
            let book = snapshot
                .book(BookIndex::new(index).unwrap())
                .expect("directory position");
            (0..book.len()).filter_map(move |row| {
                let finding = book.at(row).unwrap();
                matches!(finding.kind(), FindingKind::LengthProportionality(_))
                    .then(|| (finding.book_idx().get(), finding.from(), finding.to()))
            })
        })
        .collect()
}

/// A reference is a whole optional corpus: with none registered the target
/// publishes exactly what it published alone, and with one it gains the
/// two verses whose length the source disagrees with.
#[test]
fn a_reference_of_the_same_key_adds_length_rows_and_nothing_else() {
    let mut sous = sous();
    sous.update("t/mrk.usfm", Role::Target, &paired_target())
        .unwrap();
    let alone = sous.publish().unwrap();
    assert!(length_rows(&alone).is_empty(), "no source, no ratios");

    sous.update("s/mrk.usfm", Role::Reference, &paired_source())
        .unwrap();
    let paired = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 0, "a reference maps no target chapter");
    assert_eq!(sous.last_folded(), 0, "and folds no target book");
    assert_eq!(length_rows(&paired).len(), 2, "the short one and the long");
    assert_eq!(
        ids(&paired),
        vec!["t/mrk.usfm"],
        "a reference publishes no section of its own"
    );
    assert_eq!(
        rows(&paired),
        rows(&alone),
        "every target-only row survives the source"
    );
    assert_eq!(site_rows(&paired), site_rows(&alone));
}

/// The gate the charter names: the source choice legitimately changes the
/// results without invalidating the target-only observations.
#[test]
fn replacing_the_reference_moves_only_the_length_rows() {
    let mut sous = sous();
    sous.update("t/mrk.usfm", Role::Target, &paired_target())
        .unwrap();
    sous.update("s/mrk.usfm", Role::Reference, &paired_source())
        .unwrap();
    let before = sous.publish().unwrap();
    assert_eq!(length_rows(&before).len(), 2);

    // A source that matches the target verse for verse: every ratio is
    // one, and the sample is degenerate, so nothing is an outlier.
    sous.update("s/mrk.usfm", Role::Reference, &paired_target())
        .unwrap();
    let after = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 0, "no target chapter was re-mapped");
    assert_eq!(sous.last_folded(), 0, "no target book was re-folded");
    assert_eq!(sous.last_located(), 0, "no target text was read again");
    assert!(length_rows(&after).is_empty(), "against itself, nothing");
    assert_eq!(rows(&after), rows(&before), "target-only rows stand");
    assert_eq!(site_rows(&after), site_rows(&before));
    assert_ne!(
        CorpusSnapshot::open(&before).unwrap().snapshot_id(),
        CorpusSnapshot::open(&after).unwrap().snapshot_id(),
        "a different source is a different publication"
    );
}

/// Removing the reference removes the ratios and leaves the rest.
#[test]
fn removing_the_reference_removes_the_length_rows() {
    let mut sous = sous();
    sous.update("t/mrk.usfm", Role::Target, &paired_target())
        .unwrap();
    let alone = sous.publish().unwrap();
    sous.update("s/mrk.usfm", Role::Reference, &paired_source())
        .unwrap();
    assert_eq!(length_rows(&sous.publish().unwrap()).len(), 2);

    assert!(sous.remove(&BookId::from("s/mrk.usfm")));
    let after = sous.publish().unwrap();
    assert!(length_rows(&after).is_empty());
    assert_eq!(rows(&after), rows(&alone));
}

/// The length settings live on the same judging config as every other knob,
/// so moving them maps nothing and folds nothing.
#[test]
fn a_length_knob_re_judges_without_mapping_or_folding() {
    let mut sous = sous();
    sous.update("t/mrk.usfm", Role::Target, &paired_target())
        .unwrap();
    sous.update("s/mrk.usfm", Role::Reference, &paired_source())
        .unwrap();
    let before = sous.publish().unwrap();
    assert_eq!(length_rows(&before).len(), 2);

    let config = JudgingConfig {
        lengths: sous_core::LengthConfig {
            z_long: 1_000.0,
            ..sous_core::LengthConfig::default()
        },
        ..JudgingConfig::default()
    };
    sous.set_config(((), config, config));
    let after = sous.publish().unwrap();
    assert_eq!(sous.last_mapped(), 0);
    assert_eq!(sous.last_folded(), 0);
    assert_eq!(
        length_rows(&after).len(),
        1,
        "the long side is out of reach"
    );

    let off = JudgingConfig {
        lengths: sous_core::LengthConfig {
            enabled: false,
            ..sous_core::LengthConfig::default()
        },
        ..JudgingConfig::default()
    };
    sous.set_config(((), off, off));
    assert!(length_rows(&sous.publish().unwrap()).is_empty());
}

/// The varied target and the flat source of [`paired_target`] under any
/// book code, so a paired corpus can have more than one book in it.
fn varied(code: &str) -> String {
    sized(code, 62, |verse| match verse {
        60 => 20,
        61 => 80,
        other => 40 + other % 7,
    })
}

fn flat(code: &str) -> String {
    sized(code, 62, |_| 40)
}

/// Two targets and their two sources, published once: the pairing cache is
/// full and the counter says so.
fn two_paired_books() -> Expediter<Brigade> {
    let mut sous = sous();
    for code in ["GEN", "MRK"] {
        sous.update(format!("t/{code}"), Role::Target, &varied(code))
            .unwrap();
        sous.update(format!("s/{code}"), Role::Reference, &flat(code))
            .unwrap();
    }
    sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 2, "the first publication pairs both");
    sous
}

/// The one lane of this step that reads text reads it inside the pairing,
/// so an unchanged republication walks no words: the runs come back from
/// the cache keyed by the two checksums.
#[test]
fn an_unchanged_publish_walks_no_text_for_source_copy() {
    let mut sous = with_source_copy();
    let shared = worded("MRK", "the beginning of the good news");
    sous.update("t/mrk.usfm", Role::Target, &shared).unwrap();
    sous.update("s/mrk.usfm", Role::Reference, &shared).unwrap();
    let before = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 1);
    assert!(
        !copy_rows(&before).is_empty(),
        "a source identical to the target shares every run with it"
    );

    let after = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 0, "neither side moved");
    assert_eq!(sous.last_mapped(), 0);
    assert_eq!(sous.last_folded(), 0);
    assert_eq!(sous.last_located(), 0);
    assert_eq!(after, before, "and the rows came back verbatim");
}

/// A reference registered while the lane was off keeps no word lane, so
/// turning the lane on publishes nothing for it — and says so, instead of
/// reading as "no run was found". Re-sending the text is the fix.
#[test]
fn a_reference_registered_before_the_lane_keeps_no_words_and_says_so() {
    let mut sous = sous();
    let shared = worded("MRK", "the beginning of the good news");
    sous.update("t/mrk.usfm", Role::Target, &shared).unwrap();
    sous.update("s/mrk.usfm", Role::Reference, &shared).unwrap();
    sous.publish().unwrap();
    assert_eq!(sous.last_wordless_references(), 0, "the lane is off");

    let config = source_copy_config();
    sous.set_config(((), config, config));
    let silent = sous.publish().unwrap();
    assert!(copy_rows(&silent).is_empty());
    assert_eq!(
        sous.last_wordless_references(),
        1,
        "the reference has no word lane to walk"
    );

    sous.update("s/mrk.usfm", Role::Reference, &shared).unwrap();
    let heard = sous.publish().unwrap();
    assert_eq!(sous.last_wordless_references(), 0);
    assert!(
        !copy_rows(&heard).is_empty(),
        "the re-sent reference carries the lane"
    );
}

/// The switch is in the pair cache's identity: turning it off re-pairs
/// without the walk, and turning it on re-pairs with it.
#[test]
fn the_source_copy_switch_re_pairs_and_the_floor_does_not() {
    let mut sous = with_source_copy();
    let shared = worded("MRK", "the beginning of the good news");
    sous.update("t/mrk.usfm", Role::Target, &shared).unwrap();
    sous.update("s/mrk.usfm", Role::Reference, &shared).unwrap();
    sous.publish().unwrap();

    let mut config = source_copy_config();
    config.lengths.source_copy_min_run = 4;
    sous.set_config(((), config, config));
    let raised = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 0, "a floor is not in the key");
    assert!(!copy_rows(&raised).is_empty());

    config.lengths.source_copy = false;
    sous.set_config(((), config, config));
    let off = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 1, "the switch is");
    assert!(copy_rows(&off).is_empty());
}

/// The lane ships off, so every case that judges it turns it on.
fn source_copy_config() -> JudgingConfig {
    let mut config = JudgingConfig::default();
    config.lengths.source_copy = true;
    config
}

fn with_source_copy() -> Expediter<Brigade> {
    let mut sous = sous();
    let config = source_copy_config();
    sous.set_config(((), config, config));
    sous
}

/// One verse per line, all of them the same words.
fn worded(code: &str, body: &str) -> String {
    let mut text = format!("\\id {code}\n\\h {code}\n\\c 1\n\\p\n");
    for verse in 1..=62 {
        text.push_str(&format!("\\v {verse} {body}\n"));
    }
    text
}

/// Every source-copy row of a published buffer, as (book index, from, to).
fn copy_rows(buffer: &[u8]) -> Vec<(u16, u32, u32)> {
    let snapshot = CorpusSnapshot::open(buffer).unwrap();
    (0..snapshot.len())
        .flat_map(|index| {
            let book = snapshot
                .book(BookIndex::new(index).unwrap())
                .expect("directory position");
            (0..book.len()).filter_map(move |row| {
                let finding = book.at(row).unwrap();
                matches!(finding.kind(), FindingKind::SourceCopy(_))
                    .then(|| (finding.book_idx().get(), finding.from(), finding.to()))
            })
        })
        .collect()
}

/// The ratios are a pure function of both sides' rows, so a republication
/// that moved neither re-pairs nothing and republishes the same bytes.
#[test]
fn an_unchanged_source_and_target_re_pair_nothing() {
    let mut sous = sous();
    sous.update("t/mrk.usfm", Role::Target, &paired_target())
        .unwrap();
    sous.update("s/mrk.usfm", Role::Reference, &paired_source())
        .unwrap();
    let before = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 1);
    assert_eq!(length_rows(&before).len(), 2);

    let after = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 0, "neither side moved");
    assert_eq!(after, before, "and the publication is the same bytes");
}

/// A keystroke in one target re-pairs that book and leaves the other's
/// ratios where they are.
#[test]
fn a_target_edit_re_pairs_only_that_book() {
    let mut sous = two_paired_books();
    sous.update(
        "t/MRK",
        Role::Target,
        &format!("{}\\v 63 {}\n", varied("MRK"), "a".repeat(41)),
    )
    .unwrap();
    let after = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 1, "only the edited book");
    assert_eq!(length_rows(&after).len(), 4, "two rows a book, still");
}

/// Replacing one declared source re-pairs the target of that key alone —
/// the other target's checksum and its source's both stand.
#[test]
fn a_source_replacement_re_pairs_only_books_whose_source_moved() {
    let mut sous = two_paired_books();
    // A source that matches its target verse for verse: every ratio is
    // one, so that book falls silent and the other's rows stand.
    sous.update("s/MRK", Role::Reference, &varied("MRK"))
        .unwrap();
    let after = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 1, "only the book whose source moved");
    assert_eq!(sous.last_mapped(), 0, "a source moves no target chapter");
    assert_eq!(sous.last_folded(), 0);
    assert_eq!(
        length_rows(&after),
        length_rows(&sous.publish().unwrap()),
        "and the next publication, which re-pairs nothing, says it again"
    );
    let rows = length_rows(&after);
    assert_eq!(rows.len(), 2, "GEN keeps its two rows");
    assert!(
        rows.iter().all(|(book, _, _)| *book == 0),
        "and MRK, against itself, has none: {rows:?}"
    );
}

/// A judging knob is not in the cache key, because neither pairing nor a
/// book's order statistics read one: the rows move and nothing re-pairs.
#[test]
fn set_config_re_judges_lengths_without_re_pairing() {
    let mut sous = two_paired_books();
    assert_eq!(length_rows(&sous.publish().unwrap()).len(), 4);
    assert_eq!(sous.last_paired(), 0);

    let config = JudgingConfig {
        lengths: sous_core::LengthConfig {
            z_long: 1_000.0,
            ..sous_core::LengthConfig::default()
        },
        ..JudgingConfig::default()
    };
    sous.set_config(((), config, config));
    let after = sous.publish().unwrap();
    assert_eq!(sous.last_paired(), 0, "a knob is not in the key");
    assert_eq!(sous.last_mapped(), 0);
    assert_eq!(sous.last_folded(), 0);
    assert_eq!(
        length_rows(&after).len(),
        2,
        "the long side is out of reach in both books"
    );
}

/// A reference of another key pairs with nothing, and says nothing.
#[test]
fn a_reference_of_another_book_is_silence_not_an_error() {
    let mut sous = sous();
    sous.update("t/mrk.usfm", Role::Target, &paired_target())
        .unwrap();
    sous.update("s/gen.usfm", Role::Reference, &sized("GEN", 62, |_| 40))
        .unwrap();
    let buffer = sous.publish().unwrap();
    assert!(length_rows(&buffer).is_empty());
    assert_eq!(ids(&buffer), vec!["t/mrk.usfm"]);
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
    let book = snapshot.book_by_id("b/mrk.usfm").unwrap();
    let finding = (0..book.len())
        .map(|row| book.at(row).unwrap())
        .find(|row| matches!(row.kind(), FindingKind::Hygiene(_)))
        .expect("the pair is a hygiene row");

    let FindingKind::Hygiene(digest) = finding.kind() else {
        unreachable!("just filtered")
    };
    assert_eq!(digest.run(), 2);
    assert_eq!((finding.from(), finding.to()), (27, 29));
}

/// The tiers hold what their names claim: a book's text is pinned, the chapter
/// rows a hot book keeps are hot, and every content-addressed derived value is
/// rebuildable.
///
/// That the total is the truth is not assertable from in here —
/// `resident_bytes` IS `tally().total()` — so a counting allocator measures it
/// instead, in `galley/tests/aggregate_accounting.rs`.
#[test]
fn the_tally_accounts_for_every_resident_byte() {
    let mut sous = sous();
    four_books(&mut sous);
    let cold = sous.tally();
    assert!(
        cold.pinned >= sous.pantry().text_bytes(),
        "the text is pinned"
    );
    assert!(cold.rebuildable > 0, "the derived values are rebuildable");

    // A keystroke makes one book hot, and the chapter rows it keeps are the
    // only bytes in the hot tier.
    let edited = mark().replace("A withered", "A withering");
    sous.update("b/mrk.usfm", Role::Target, &edited).unwrap();
    sous.publish().unwrap();
    let warm = sous.tally();
    assert!(warm.hot > 0, "the hot book kept its chapter rows");
    assert!(
        warm.pinned > cold.pinned,
        "the second book's text and ring are pinned too"
    );

    // The ceiling is the rebuildable tier's, and only the chunk products are
    // held under it.
    assert_eq!(sous.budget().ceiling(), 1 << 20);
    assert!(sous.pantry().chunk_stats().resident_bytes <= sous.budget().ceiling());
}

/// A Target re-sent as a Reference stops being cached as a target: its ring,
/// its rows, its aggregate and its share of the corpus totals all go, and it
/// gives back the hot slot it was holding.
#[test]
fn a_target_re_sent_as_a_reference_frees_its_rows() {
    let text = mark();
    let mut flipped = sous();
    flipped.update("b/mrk.usfm", Role::Target, &text).unwrap();
    flipped
        .update("a/gen.usfm", Role::Target, &genesis())
        .unwrap();
    flipped.publish().unwrap();
    assert!(flipped.hot_books().contains(&BookId::from("b/mrk.usfm")));

    flipped
        .update_with("b/mrk.usfm", Role::Reference, Retain::ProductsOnly, &text)
        .unwrap();
    let after = flipped.publish().unwrap();

    // The same corpus, declared that way from the start.
    let mut fresh = sous();
    fresh
        .update_with("b/mrk.usfm", Role::Reference, Retain::ProductsOnly, &text)
        .unwrap();
    fresh
        .update("a/gen.usfm", Role::Target, &genesis())
        .unwrap();
    let published = fresh.publish().unwrap();

    assert_eq!(after, published, "the publication is the fresh one's");
    assert!(
        !flipped.hot_books().contains(&BookId::from("b/mrk.usfm")),
        "the hot slot is free"
    );
    assert_eq!(flipped.hot_books(), fresh.hot_books());
    assert_eq!(
        (
            flipped.resident_observations(),
            flipped.resident_tables(),
            flipped.resident_aggregates()
        ),
        (
            fresh.resident_observations(),
            fresh.resident_tables(),
            fresh.resident_aggregates()
        ),
        "no row of the former target survives"
    );
    assert_eq!(flipped.resident_bytes(), fresh.resident_bytes());
}

/// A `FindingHandle` is a snapshot id plus a row, so two publications that
/// differ only by a knob may not share one.
#[test]
fn two_configs_publish_two_snapshot_ids() {
    let snapshot_of = |config: JudgingConfig| {
        let mut sous = sous();
        sous.set_config(((), config, config));
        sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
        let buffer = sous.publish().unwrap();
        CorpusSnapshot::open(&buffer).unwrap().snapshot_id()
    };
    let mut moved = JudgingConfig::default();
    moved.channels.casing = false;
    assert_ne!(snapshot_of(JudgingConfig::default()), snapshot_of(moved));

    // And the same settings are the same publication, however they were reached.
    let mut sous = sous();
    sous.update("b/mrk.usfm", Role::Target, &mark()).unwrap();
    sous.publish().unwrap();
    sous.set_config(((), moved, moved));
    sous.publish().unwrap();
    sous.set_config(((), JudgingConfig::default(), JudgingConfig::default()));
    let back = sous.publish().unwrap();
    assert_eq!(
        CorpusSnapshot::open(&back).unwrap().snapshot_id(),
        snapshot_of(JudgingConfig::default()),
    );
}
