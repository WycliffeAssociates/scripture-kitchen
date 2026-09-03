//! What an incremental publication owes a cold one, proved step by seeded step.
//!
//! ```text
//! thread 'churn_over_a_synthetic_corpus' panicked at galley/tests/equivalence.rs:
//!   published bytes differ from cold: synthetic: seed=0x5eed0001 step=37
//!   edit=DeleteRun
//! ```
//!
//! Every step edits one registered book — or adds or removes one — republishes
//! through the resident [`Expediter`], and asserts two things: the bytes equal
//! `cold_publish` over exactly those texts, and no more chapters were mapped
//! than the step's own analysis inputs changed. The bound is recomputed here
//! from `for_each_chapter`, not read back off the cache it bounds.
//!
//! A failure prints its seed and step. Every draw descends from the seed the
//! failing test names, so rerunning that test reproduces it exactly; narrow it
//! by cutting that test's step count down to the printed step.

use rustc_hash::{FxHashMap, FxHashSet};
use sous_core::{
    Brigade, ChapterPass, Corpus, CorpusSnapshot, SnapshotId, analyze, for_each_chapter,
    hygiene::HygieneBytes, substrate::Substrate,
};
use usfm_galley::onion::{Filter, cst, lex, mask};
use usfm_galley::sous::{Expediter, OnionBook, OnionInputBook, publish_onion_findings};
use usfm_galley::{BookId, Role};

/// One book as the harness holds it: the host id, and its whole raw text.
type Book = (String, String);

// ---------------------------------------------------------------- the oracle

/// `analyze` over freshly parsed books, published through the string-taking
/// publisher: the bytes an incremental publication has to equal.
///
/// The snapshot identity is an input, not a result, so both paths publish
/// under the one the caller names.
fn cold_publish<P: ChapterPass + Sync>(pass: &P, books: &[Book], snapshot: SnapshotId) -> Vec<u8> {
    let parsed: Vec<OnionBook> = books
        .iter()
        .map(|(id, text)| {
            OnionBook::parse(text).unwrap_or_else(|error| panic!("{id} is not analyzable: {error}"))
        })
        .collect();
    let corpus = Corpus::try_new(&parsed).expect("the harness registers distinct book keys");
    let findings = analyze(&corpus, pass).into_rows();
    let inputs = books
        .iter()
        .map(|(id, text)| OnionInputBook::new(id.as_str(), text.clone()))
        .collect();
    publish_onion_findings(inputs, &findings, snapshot).expect("cold publication")
}

/// The Expediter's own books, in the canonical order it publishes them.
fn ordered<P: ChapterPass + Sync>(
    sous: &Expediter<P>,
    texts: &FxHashMap<String, String>,
) -> Vec<Book> {
    sous.pantry()
        .books(Role::Target)
        .iter()
        .map(|(id, _)| {
            let id = id.as_str().to_string();
            let text = texts[&id].clone();
            (id, text)
        })
        .collect()
}

/// Publish both ways and assert the bytes, under the incremental snapshot id.
fn assert_publications_agree<P: ChapterPass + Sync + Copy>(
    sous: &mut Expediter<P>,
    books: &[Book],
    context: &str,
) -> u64 {
    let texts: FxHashMap<String, String> = books.iter().cloned().collect();
    let buffer = sous
        .publish()
        .unwrap_or_else(|error| panic!("{context}: {error}"));
    let snapshot = CorpusSnapshot::open(&buffer).unwrap().snapshot_id();
    let pass = *sous.pass();
    let cold = cold_publish(&pass, &ordered(sous, &texts), snapshot);
    assert_eq!(buffer, cold, "published bytes differ from cold: {context}");
    sous.last_mapped()
}

// ------------------------------------------------------------- the work bound

/// One chapter's analysis input, spelled out: its projected text, then every
/// rebased verse row.
///
/// Written here rather than read from `ObservationKey` so the bound is
/// independent of the cache it bounds.
fn chapter_digests(text: &str) -> Vec<String> {
    let book = OnionBook::parse(text).expect("the caller checked this text parses");
    let mut digests = Vec::new();
    for_each_chapter(&book, |_, chapter| {
        let mut digest = String::from(chapter.text);
        for verse in chapter.verses {
            let key = verse.key();
            digest.push_str(&format!(
                "\u{1}{}:{}-{}@{}..{}",
                key.chapter(),
                key.first(),
                key.last(),
                verse.text().from(),
                verse.text().to(),
            ));
        }
        digests.push(digest);
    });
    digests
}

/// Distinct chapter inputs the corpus gained in this step — the most chapters
/// a publication may map.
///
/// An upper bound, not an equality: an input the cache kept from an earlier
/// generation is a hit the corpus-wide difference cannot see.
fn work_bound(
    before: &FxHashMap<String, Vec<String>>,
    after: &FxHashMap<String, Vec<String>>,
) -> u64 {
    let held: FxHashSet<&String> = before.values().flatten().collect();
    let gained: FxHashSet<&String> = after
        .values()
        .flatten()
        .filter(|digest| !held.contains(*digest))
        .collect();
    gained.len() as u64
}

// ------------------------------------------------------------------ the seeds

/// Five lines of xorshift64: reproducible from its seed, and no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// `0..n`; `n` is never zero at a call site.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn between(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }
}

/// What a random run is drawn from: letters and space for ordinary text, then
/// one of each thing hygiene and the classifier have to see — a NUL, a bare
/// CR, the backslash Onion's mask hands through as content, a combining mark,
/// an astral char, and a Devanagari conjunct.
const ALPHABET: &[&str] = &[
    "a",
    "e",
    "s",
    "t",
    "A",
    "Q",
    " ",
    " ",
    "\0",
    "\r",
    "\\\\",
    "\u{301}",
    "\u{1F9C5}",
    "क्ष",
];

/// A run of at least `low` and at most about `high` bytes, drawn a piece at a
/// time so the tail may overshoot by one piece.
fn run_of(rng: &mut Rng, low: usize, high: usize) -> String {
    let bytes = rng.between(low, high);
    let mut out = String::new();
    while out.len() < bytes {
        out.push_str(ALPHABET[rng.below(ALPHABET.len())]);
    }
    out
}

// ------------------------------------------------------------- reading a book

/// Raw offsets of every projected char boundary: the positions an edit lands
/// in verse text rather than in markup.
fn content_positions(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let tokens = lex(text);
    let tree = cst::build(&tokens);
    let mask = mask(bytes, &tokens, &tree, &Filter::verse_text());
    let projected = mask.text(bytes);
    (0..projected.len())
        .filter(|at| projected.is_char_boundary(*at))
        .map(|at| mask.to_source(at as u32) as usize)
        .collect()
}

/// Every line-initial `\c N` block as `(start, end, number)`, in source order.
fn chapter_blocks(text: &str) -> Vec<(usize, usize, u32)> {
    let mut blocks: Vec<(usize, usize, u32)> = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("\\c ")
            && let Ok(number) = rest.trim().parse::<u32>()
        {
            if let Some(last) = blocks.last_mut() {
                last.1 = at;
            }
            blocks.push((at, text.len(), number));
        }
        at += line.len();
    }
    blocks
}

/// A block's text past its own `\c` line — what a move or a copy carries.
fn body_of(text: &str, block: (usize, usize, u32)) -> (usize, usize) {
    let head = text[block.0..block.1]
        .find('\n')
        .map_or(block.1, |at| block.0 + at + 1);
    (head, block.1)
}

// -------------------------------------------------------------- the edit menu

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    InsertRun,
    DeleteRun,
    ReplaceRun,
    /// A footnote the verse-text mask removes: every projected input stands
    /// still while every offset past it shifts.
    InsertFootnote,
    AppendChapter,
    DropChapter,
    /// Two chapters' bodies swapped under their own `\c` numbers.
    ///
    /// A move is never zero-rework: `validate` wants ascending chapter
    /// numbers, so content can only move under a different number, and the
    /// number is in the verse rows an `ObservationKey` hashes.
    MoveChapter,
    /// One chapter's body copied onto another's: one observation, two rows.
    CopyChapter,
    /// A NUL run either side of a chapter seam, which the fold must publish
    /// as two findings rather than one.
    ChapterSeam,
    AddBook,
    RemoveBook,
}

const MENU: &[Edit] = &[
    Edit::InsertRun,
    Edit::InsertRun,
    Edit::DeleteRun,
    Edit::DeleteRun,
    Edit::ReplaceRun,
    Edit::ReplaceRun,
    Edit::InsertFootnote,
    Edit::AppendChapter,
    Edit::DropChapter,
    Edit::MoveChapter,
    Edit::CopyChapter,
    Edit::ChapterSeam,
    Edit::AddBook,
    Edit::RemoveBook,
];

/// Book codes no committed corpus here uses, so an added book never collides
/// with a registered `BookKey`.
const SPARE_CODES: &[&str] = &[
    "TOB", "JDT", "WIS", "SIR", "BAR", "1MA", "2MA", "1ES", "MAN", "ODA",
];

fn small_book(code: &str, rng: &mut Rng) -> String {
    let mut text = format!("\\id {code}\n\\h {code}\n");
    for chapter in 1..=rng.between(1, 3) {
        text.push_str(&format!(
            "\\c {chapter}\n\\p\n\\v 1 {}\n\\v 2 {}\n",
            run_of(rng, 24, 24),
            run_of(rng, 24, 24)
        ));
    }
    text
}

/// Draws one edit and applies it to `books`, returning what it drew.
///
/// `None` when the draw had nothing to work with — an empty chapter, a corpus
/// of one book — and the caller redraws.
fn apply_edit(
    rng: &mut Rng,
    books: &mut Vec<Book>,
    spares: &mut Vec<&'static str>,
) -> Option<Edit> {
    let edit = MENU[rng.below(MENU.len())];
    match edit {
        Edit::AddBook => {
            if spares.is_empty() {
                return None;
            }
            let code = spares.remove(rng.below(spares.len()));
            books.push((format!("spare/{code}.usfm"), small_book(code, rng)));
        }
        Edit::RemoveBook => {
            if books.len() < 2 {
                return None;
            }
            let (id, text) = books.remove(rng.below(books.len()));
            if let Some(code) = id
                .strip_prefix("spare/")
                .and_then(|rest| rest.split('.').next())
                && let Some(spare) = SPARE_CODES.iter().find(|spare| **spare == code)
            {
                spares.push(spare);
            }
            let _ = text;
        }
        Edit::CopyChapter => {
            if books.len() < 2 {
                return None;
            }
            let (from, onto) = two_of(rng, books.len());
            copy_chapter(rng, books, from, onto)?;
        }
        _ => {
            let book = rng.below(books.len());
            let text = &mut books[book].1;
            edit_one(rng, edit, text)?;
        }
    }
    Some(edit)
}

/// Copies one chapter's body onto the chapter of the SAME number in another
/// book: identical projected text under identical verse keys, so the two
/// positions share one observation.
fn copy_chapter(rng: &mut Rng, books: &mut [Book], from: usize, onto: usize) -> Option<()> {
    let source = chapter_blocks(&books[from].1);
    let target = chapter_blocks(&books[onto].1);
    let shared: Vec<u32> = source
        .iter()
        .filter(|block| target.iter().any(|other| other.2 == block.2))
        .map(|block| block.2)
        .collect();
    if shared.is_empty() {
        return None;
    }
    let number = shared[rng.below(shared.len())];
    let block = source.iter().find(|block| block.2 == number)?;
    let (head, tail) = body_of(&books[from].1, *block);
    let body = books[from].1[head..tail].to_string();
    let block = target.iter().find(|block| block.2 == number)?;
    let (head, tail) = body_of(&books[onto].1, *block);
    books[onto].1.replace_range(head..tail, &body);
    Some(())
}

fn edit_one(rng: &mut Rng, edit: Edit, text: &mut String) -> Option<()> {
    let positions = content_positions(text);
    if positions.len() < 4 {
        return None;
    }
    match edit {
        Edit::InsertRun => {
            let at = positions[rng.below(positions.len())];
            text.insert_str(at, &run_of(rng, 1, 40));
        }
        Edit::DeleteRun | Edit::ReplaceRun => {
            let from = rng.below(positions.len() - 1);
            let to = (from + rng.between(1, 40)).min(positions.len() - 1);
            let (from, to) = (positions[from], positions[to]);
            text.replace_range(
                from..to,
                &match edit {
                    Edit::ReplaceRun => run_of(rng, 1, 40),
                    _ => String::new(),
                },
            );
        }
        Edit::InsertFootnote => {
            // A raw `\\` escape is two bytes and one projected char, so its
            // second byte is a content position an insertion would split —
            // and splitting it is not a markup-only edit.
            let at = *positions
                .iter()
                .cycle()
                .skip(rng.below(positions.len()))
                .take(positions.len())
                .find(|at| text.as_bytes()[at.saturating_sub(1)] != b'\\')?;
            text.insert_str(at, "\\f + \\ft note\\f*");
        }
        Edit::AppendChapter => {
            let next = chapter_blocks(text).last().map_or(1, |block| block.2 + 1);
            text.push_str(&format!("\\c {next}\n\\p\n\\v 1 {}\n", run_of(rng, 4, 40)));
        }
        Edit::DropChapter => {
            let blocks = chapter_blocks(text);
            if blocks.len() < 2 {
                return None;
            }
            let block = blocks[rng.below(blocks.len())];
            text.replace_range(block.0..block.1, "");
        }
        Edit::MoveChapter => {
            let blocks = chapter_blocks(text);
            if blocks.len() < 2 {
                return None;
            }
            let (left, right) = two_of(rng, blocks.len());
            let (left, right) = (
                body_of(text, blocks[left.min(right)]),
                body_of(text, blocks[left.max(right)]),
            );
            let (first, second) = (
                text[left.0..left.1].to_string(),
                text[right.0..right.1].to_string(),
            );
            // Right first: replacing it cannot move the left block's offsets.
            text.replace_range(right.0..right.1, &first);
            text.replace_range(left.0..left.1, &second);
        }
        Edit::ChapterSeam => {
            let blocks = chapter_blocks(text);
            if blocks.len() < 2 {
                return None;
            }
            let seam = blocks[rng.between(1, blocks.len() - 1)].0;
            let after = *positions.iter().find(|at| **at >= seam)?;
            // The later insertion first, so the earlier offset still stands.
            text.insert_str(after, "\0\0");
            text.insert_str(seam, "\0\0");
        }
        Edit::AddBook | Edit::RemoveBook | Edit::CopyChapter => {
            unreachable!("handled by the caller")
        }
    }
    Some(())
}

fn two_of(rng: &mut Rng, len: usize) -> (usize, usize) {
    let left = rng.below(len);
    let right = (left + 1 + rng.below(len - 1)) % len;
    (left, right)
}

// -------------------------------------------------------------- the churn run

/// The Warmer LRU ceiling every harness Expediter declares.
const BUDGET: usize = 64 << 20;

/// Residency past that budgeted LRU: the chapter cache and the per-book
/// products, which are what an edit churn could grow without bound.
fn cache_bytes<P: ChapterPass + Sync>(sous: &Expediter<P>) -> usize {
    sous.resident_bytes() - sous.pantry().warmer().resident_bytes()
}

/// Registers `books`, then runs `steps` seeded edits, asserting equality and
/// the work bound after each.
fn churn<P: ChapterPass + Sync + Copy>(
    pass: P,
    name: &str,
    seed: u64,
    steps: usize,
    books: Vec<Book>,
) {
    let mut rng = Rng(seed);
    let mut spares: Vec<&'static str> = SPARE_CODES.to_vec();
    let mut books = books;
    let mut sous = Expediter::new(pass, BUDGET);
    for (id, text) in &books {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    let mut digests: FxHashMap<String, Vec<String>> = books
        .iter()
        .map(|(id, text)| (id.clone(), chapter_digests(text)))
        .collect();
    assert_publications_agree(
        &mut sous,
        &books,
        &format!("{name}: seed={seed:#x} step=cold"),
    );

    let mut skipped = 0;
    for step in 0..steps {
        let mut next = books.clone();
        let mut spare_draw = spares.clone();
        let mut edit = None;
        for _attempt in 0..8 {
            next = books.clone();
            spare_draw = spares.clone();
            let drawn = apply_edit(&mut rng, &mut next, &mut spare_draw);
            if drawn.is_some() && next.iter().all(|(_, text)| OnionBook::parse(text).is_ok()) {
                edit = drawn;
                break;
            }
        }
        let Some(edit) = edit else {
            skipped += 1;
            continue;
        };
        spares = spare_draw;
        let context = format!("{name}: seed={seed:#x} step={step} edit={edit:?}");

        let live: FxHashSet<&String> = next.iter().map(|(id, _)| id).collect();
        let mut after: FxHashMap<String, Vec<String>> = FxHashMap::default();
        for (id, text) in &next {
            let unchanged = books
                .iter()
                .any(|(old_id, old_text)| old_id == id && old_text == text);
            let rows = if unchanged {
                digests[id].clone()
            } else {
                sous.update(id.as_str(), Role::Target, text).unwrap();
                chapter_digests(text)
            };
            after.insert(id.clone(), rows);
        }
        for (id, _) in &books {
            if !live.contains(id) {
                assert!(sous.remove(&BookId::from(id.as_str())), "{context}");
            }
        }
        let bound = work_bound(&digests, &after);

        let mapped = assert_publications_agree(&mut sous, &next, &context);
        assert!(
            mapped <= bound,
            "{context}: mapped {mapped} chapters for a bound of {bound}"
        );
        match edit {
            Edit::InsertFootnote => {
                assert_eq!(bound, 0, "{context}: a masked footnote moves no input");
                assert_eq!(mapped, 0, "{context}: and so maps nothing");
            }
            Edit::CopyChapter => {
                assert_eq!(bound, 0, "{context}: the copy's input was already here");
                assert_eq!(mapped, 0, "{context}: and so maps nothing");
            }
            _ => {}
        }
        books = next;
        digests = after;
    }

    assert!(
        skipped * 4 < steps,
        "{name}: {skipped} of {steps} draws found nothing to edit"
    );

    // Residency is bounded by what the corpus IS, not by how many edits it
    // took: the ring keeps `kept + 1` tables per book and the observations
    // those tables name, and the sweep frees the rest.
    let mut fresh = Expediter::new(pass, BUDGET);
    for (id, text) in &books {
        fresh.update(id.as_str(), Role::Target, text).unwrap();
    }
    fresh.publish().unwrap();
    assert!(
        sous.pantry().warmer().resident_bytes() <= BUDGET,
        "{name}: the Warmer is over its declared budget"
    );
    assert!(
        cache_bytes(&sous) < 3 * cache_bytes(&fresh),
        "{name}: {} cached bytes after the churn, against {} fresh",
        cache_bytes(&sous),
        cache_bytes(&fresh)
    );
}

// ------------------------------------------------------------- the two tiers

/// Three books, one of them long enough to move chapters around in.
fn synthetic() -> Vec<Book> {
    let mut rng = Rng(0x5EED_0000_0000_0001);
    let books = [("GEN", 8), ("EXO", 5), ("MRK", 3)];
    books
        .iter()
        .map(|(code, chapters)| {
            let mut text = format!("\\id {code}\n\\h {code}\n");
            for chapter in 1..=*chapters {
                text.push_str(&format!("\\c {chapter}\n\\p\n"));
                for verse in 1..=4 {
                    text.push_str(&format!("\\v {verse} {}\n", run_of(&mut rng, 48, 48)));
                }
            }
            (format!("synthetic/{code}.usfm"), text)
        })
        .collect()
}

/// The committed 66-book corpus the benches use.
fn en_ulb() -> Vec<Book> {
    let dir = format!(
        "{}/../testData/exampleCorpora/en_ulb",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "usfm")
        })
        .collect();
    paths.sort();
    let books: Vec<Book> = paths
        .iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    assert_eq!(books.len(), 66, "{dir} is a whole-Bible corpus");
    books
}

#[test]
fn churn_over_a_synthetic_corpus() {
    churn(HygieneBytes, "synthetic", 0x5EED_0001, 200, synthetic());
}

#[test]
fn churn_over_a_synthetic_corpus_from_a_second_seed() {
    churn(
        HygieneBytes,
        "synthetic-b",
        0xD00D_1234_5678_9ABD,
        200,
        synthetic(),
    );
}

/// The product pass through the same churn: the finish sort, the
/// substrate's hygiene lane, and the Expediter's aggregate cache all at once.
#[test]
fn churn_over_a_synthetic_corpus_with_brigade() {
    churn(Brigade::default(), "brigade", 0x5EED_0004, 200, synthetic());
}

/// The Level 1b substrate through the same churn. It publishes no findings
/// yet, so what this pins is the cache: which chapters are mapped, what the
/// ring keeps, and that a fold cannot tell a reused row from a fresh one.
#[test]
fn churn_over_a_synthetic_corpus_with_substrate() {
    churn(Substrate, "substrate", 0x5EED_0003, 200, synthetic());
}

#[test]
#[ignore = "corpus-scale oracle: 50 cold whole-Bible publications; run --include-ignored at pass end"]
fn churn_over_en_ulb() {
    churn(HygieneBytes, "en_ulb", 0x5EED_0002, 50, en_ulb());
}

#[test]
#[ignore = "corpus-scale oracle: 50 cold whole-Bible publications; run --include-ignored at pass end"]
fn churn_over_en_ulb_from_a_second_seed() {
    churn(
        HygieneBytes,
        "en_ulb-b",
        0xBEEF_0F0F_0F0F_0F0F,
        50,
        en_ulb(),
    );
}

// --------------------------------------------------- the named gate bullets

/// The claim `galley/src/sous/expediter.md` rests on, at its smallest: one
/// resident publication, byte for byte the cold one.
#[test]
fn publish_byte_equals_cold_analyze_through_the_string_taking_publisher() {
    let books = synthetic();
    let mut sous = Expediter::new(HygieneBytes, BUDGET);
    // Registered backwards: the publication is canonical order either way,
    // which is what `cold_publish` is handed.
    for (id, text) in books.iter().rev() {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    let mapped = assert_publications_agree(&mut sous, &books, "cold equality");
    assert_eq!(mapped, 16, "every chapter of all three books");
}

/// The Stage 2 bullet: cold analysis equals the chapter-at-a-time rebuild.
fn replace_every_chapter(name: &str, mut books: Vec<Book>, edited: usize) {
    let mut sous = Expediter::new(HygieneBytes, BUDGET);
    for (id, text) in &books {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    assert_publications_agree(&mut sous, &books, name);

    let mut rng = Rng(0x00C0_FFEE);
    let chapters = chapter_blocks(&books[edited].1).len();
    for chapter in 0..chapters {
        let text = &mut books[edited].1;
        let block = chapter_blocks(text)[chapter];
        let (from, to) = body_of(text, block);
        text.replace_range(
            from..to,
            &format!(
                "\\p\n\\v 1 replaced {chapter} {}\n",
                run_of(&mut rng, 32, 32)
            ),
        );
        let text = text.clone();
        sous.update(books[edited].0.as_str(), Role::Target, &text)
            .unwrap();
        let mapped =
            assert_publications_agree(&mut sous, &books, &format!("{name}: chapter {chapter}"));
        assert_eq!(mapped, 1, "{name}: chapter {chapter} and no other");
    }
}

#[test]
fn every_chapter_replaced_in_sequence_equals_cold() {
    replace_every_chapter("synthetic", synthetic(), 0);
}

#[test]
#[ignore = "corpus-scale oracle: one cold whole-Bible publication per chapter; run --include-ignored at pass end"]
fn every_chapter_of_a_whole_bible_book_replaced_in_sequence_equals_cold() {
    let books = en_ulb();
    let edited = books
        .iter()
        .position(|(id, _)| id.contains("MRK"))
        .expect("the corpus carries Mark");
    replace_every_chapter("en_ulb", books, edited);
}

/// The Stage 2 bullet: identical chapter content is mapped once and counted at
/// both positions.
#[test]
fn a_chapter_copied_onto_another_maps_nothing_and_publishes_both() {
    let mut books = synthetic();
    // No ring: the replaced chapter's observation is swept at the very next
    // publication, so the count is the corpus's own.
    let mut sous = Expediter::new(HygieneBytes, BUDGET).with_generations(0);
    for (id, text) in &books {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    assert_publications_agree(&mut sous, &books, "copy: cold");
    let before = sous.resident_observations();

    copy_chapter(&mut Rng(1), &mut books, 0, 1).expect("both books carry chapter 1");
    sous.update(books[1].0.as_str(), Role::Target, &books[1].1.clone())
        .unwrap();

    let mapped = assert_publications_agree(&mut sous, &books, "copy: after");
    assert_eq!(mapped, 0, "the copy's input was already mapped");
    assert_eq!(
        sous.resident_observations(),
        before - 1,
        "two positions, one observation"
    );
}

/// The Stage 2 bullet: a markup-only edit reuses every observation while the
/// new source map rebinds every position.
#[test]
fn a_markup_only_edit_reuses_every_observation_and_still_rebinds() {
    let mut books = synthetic();
    let mut sous = Expediter::new(HygieneBytes, BUDGET);
    for (id, text) in &books {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    let before = assert_publications_agree(&mut sous, &books, "markup: cold");
    assert!(before > 0);

    let text = &mut books[1].1;
    let at = content_positions(text)[3];
    text.insert_str(at, "\\f + \\ft note\\f*");
    let text = text.clone();
    sous.update(books[1].0.as_str(), Role::Target, &text)
        .unwrap();
    let mapped = assert_publications_agree(&mut sous, &books, "markup: after");
    assert_eq!(mapped, 0, "no projected input moved");
}

/// Resident bytes fall when a book leaves, and the sweep is what frees the
/// chapter tables behind it.
#[test]
fn removing_a_book_lowers_resident_bytes() {
    let books = synthetic();
    let mut sous = Expediter::new(HygieneBytes, BUDGET);
    for (id, text) in &books {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    sous.publish().unwrap();
    let before = sous.resident_bytes();

    assert!(sous.remove(&BookId::from(books[0].0.as_str())));
    sous.publish().unwrap();
    assert!(
        sous.resident_bytes() < before,
        "{} is not below {before}",
        sous.resident_bytes()
    );
}

/// Under `--features parallel` the map runs on rayon; the publication is the
/// serial cold oracle's bytes either way.
#[cfg(feature = "parallel")]
#[test]
#[ignore = "corpus-scale oracle: a cold whole-Bible publication both ways; run --include-ignored at pass end"]
fn parallel_publish_byte_equals_the_serial_cold_oracle_over_en_ulb() {
    let books = en_ulb();
    let mut sous = Expediter::new(HygieneBytes, BUDGET);
    for (id, text) in &books {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    let mapped = assert_publications_agree(&mut sous, &books, "parallel: en_ulb");
    assert!(mapped > 1000, "a whole Bible maps every chapter cold");
}
