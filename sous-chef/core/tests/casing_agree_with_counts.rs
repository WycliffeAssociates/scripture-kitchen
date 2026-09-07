//! Instrument: VOLUME. The word rescan against the counts it materializes.
//!
//! For every book and every firing word pattern, the sites `Words::locate`
//! places equal that book's own count for the pattern's key — free positions
//! only on a casing row, every occurrence on a length row, every pair on a
//! doubled row. The rescan runs the same walk over the same chapters and reads
//! the same terminal table, so this pins that the two agree about word
//! boundaries, case folding, which positions the corpus forced, and what
//! counts as a gap between two occurrences of one word.
//!
//! The pass under test is `(Substrate, Words)`: the terminal table is the
//! substrate's follow lane, and `Words` alone abstains. One channel at a time,
//! because a span two channels name is one row carrying both reasons.
//!
//! The default sweep is synthetic and fast. The ignored one runs the same
//! equality over the committed corpus tier, where real glue, apostrophes,
//! quote conventions, and chapter seams are.

use sous_core::judge::{BandStep, Channels, DoublesPolicy, PatternIndex, Staircase};
use sous_core::substrate::Substrate;
use sous_core::words::{WordAggregate, Words, fold_book, free_in};
use sous_core::{
    BookKey, Chapter, ChapterObs, ChapterPass, Corpus, FindingKind, JudgingConfig, ProjectedBook,
    TextRange, Verse, VerseKey, WordRow, analyze_with, for_each_chapter,
};

const TIER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpora/");
const FILES: [&str; 8] = [
    "WA-en-ulb.txt",
    "amh.txt",
    "francl.txt",
    "grcsr.txt",
    "hin2017.txt",
    "nya.txt",
    "spaRV1909.txt",
    "swhulb.txt",
];

// ── The harness ─────────────────────────────────────────────────────────

struct Book {
    key: BookKey,
    text: String,
    chapters: Vec<Chapter>,
    verses: Vec<Verse>,
}

impl ProjectedBook for Book {
    fn key(&self) -> BookKey {
        self.key
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn chapters(&self) -> impl Iterator<Item = Chapter> {
        self.chapters.iter().copied()
    }

    fn verses(&self) -> impl Iterator<Item = Verse> {
        self.verses.iter().copied()
    }
}

/// One book from its chapter texts, joined in order with no gap; each chapter
/// is one verse, so a verse start is also a chapter start here.
fn book(key: BookKey, texts: &[String]) -> Book {
    let mut text = String::new();
    let mut chapters = Vec::new();
    let mut verses = Vec::new();
    for (index, chapter) in texts.iter().enumerate() {
        let number = index as u16 + 1;
        let from = text.len() as u32;
        text.push_str(chapter);
        let span = TextRange::new(from, text.len() as u32).expect("a chapter grows forward");
        chapters.push(Chapter::new(number, span).expect("chapters are numbered from one"));
        verses.push(Verse::new(
            VerseKey::new(number, 1, 1).expect("a verse key is well formed"),
            span,
        ));
    }
    Book {
        key,
        text,
        chapters,
        verses,
    }
}

/// The fold product of one book, mapped chapter by chapter as a host would.
fn aggregate(book: &Book) -> WordAggregate {
    let mut rows: Vec<(u32, WordRow)> = Vec::new();
    for_each_chapter(book, |start, input| rows.push((start, Words.map(input))));
    let view: Vec<ChapterObs<&WordRow>> = rows
        .iter()
        .map(|(start, obs)| ChapterObs { start: *start, obs })
        .collect();
    fold_book(&view)
}

/// Judges the corpus, then compares each book's sites against its own counts.
/// Returns `(books, patterns compared, occurrences compared)`.
fn agree(books: &[Book], config: &JudgingConfig) -> (usize, usize, u64) {
    let corpus = Corpus::try_new(books).expect("a synthetic corpus is valid");
    let findings = analyze_with(&corpus, &(Substrate, Words), &(*config, *config));
    let patterns = findings.patterns().to_vec();
    let table = findings
        .terminals()
        .expect("the substrate publishes a terminal table")
        .clone();
    let word: Vec<usize> = patterns
        .iter()
        .enumerate()
        .filter(|(_, row)| row.channel.judged_by_words())
        .map(|(at, _)| at)
        .collect();

    let mut compared = 0;
    let mut occurrences = 0;
    for (index, _) in corpus.iter() {
        let counts = aggregate(&books[index.get() as usize]);
        let mut sited = vec![0u64; patterns.len()];
        for row in findings.rows() {
            if row.book_idx() != index {
                continue;
            }
            // The substrate publishes its scalar hygiene lane beside the
            // convention rows; only the latter name a pattern.
            if let FindingKind::Convention(digest) = row.kind() {
                sited[usize::from(digest.pattern().get())] += 1;
            }
        }

        let mut set = Vec::new();
        Words.firing(&counts, &patterns, &mut set);
        for &at in &word {
            let pattern = &patterns[at];
            let walked = free_in(&counts, pattern, &table);
            assert_eq!(
                sited[at],
                walked,
                "book {} pattern[{at}] {:?}",
                books[index.get() as usize].key,
                pattern.key,
            );
            // The firing set is position-blind, so it may name a row this book
            // holds only where the corpus forced the capital; it may never
            // miss one `locate` places.
            assert!(
                set.contains(&PatternIndex::new(at as u16)) || walked == 0,
                "the firing set names what locate finds"
            );
            compared += 1;
            occurrences += walked;
        }
    }
    (books.len(), compared, occurrences)
}

// ── The synthetic sweep ─────────────────────────────────────────────────

/// Words drawn to exercise every form, the joiner rule, digits, glue, an
/// uncased script riding beside cased ones, and a letter this vocabulary
/// repeats twice often and three times rarely.
const VOCABULARY: [&str; 19] = [
    "david",
    "David",
    "DAVID",
    "McDonald",
    "don't",
    "Don't",
    "mother-in-law",
    "ng'ombe",
    "1Ki",
    "3rd",
    "he\u{301}llo",
    "HE\u{301}LLO",
    "\u{5d0}\u{5d1}\u{5d2}",
    "the",
    "The",
    "and",
    "feel",
    "keen",
    "feeel",
];

/// The gaps between words: spaces, terminals, separators, and quotes, so the
/// forced rule is exercised in both directions.
const GAPS: [&str; 8] = [
    " ",
    ". ",
    ", ",
    "; ",
    "! ",
    " \u{201C}",
    "\u{201D} ",
    " \u{2014} ",
];

/// A sixteen-word vocabulary over eight gaps doubles often enough on its own
/// that the doubled channel is exercised without a fixture seeded for it.
fn generated(seed: u64, books: usize, chapters: usize, words: usize) -> Vec<Book> {
    let mut state = seed | 1;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (state >> 33) as usize
    };
    let mut out = Vec::new();
    for index in 0..books {
        let mut texts = Vec::new();
        for _ in 0..chapters {
            let mut text = String::new();
            for _ in 0..words {
                text.push_str(VOCABULARY[next() % VOCABULARY.len()]);
                text.push_str(GAPS[next() % GAPS.len()]);
            }
            texts.push(text);
        }
        let key = BookKey::new([
            b'A' + (index / 26) as u8 % 26,
            b'A' + index as u8 % 26,
            b'A',
        ]);
        out.push(book(key, &texts));
    }
    out
}

/// Every rung at half, so any form that is not the majority fires and the
/// sweep exercises the rescan instead of abstaining through most of it. One
/// channel at a time, so a span two channels name is never one row here.
fn permissive() -> JudgingConfig {
    let steps = Staircase::DEFAULT_STEPS.map(|step| BandStep {
        share_bp: 5_000,
        ..step
    });
    JudgingConfig {
        word_support_floor: 1,
        word_bands: Staircase::new(steps).expect("the default bounds ascend"),
        channels: Channels {
            doubled: false,
            letter_runs: false,
            ..Channels::default()
        },
        ..JudgingConfig::default()
    }
}

/// The doubled channel alone, recusal off, on bands any sweep reaches.
fn doubling() -> JudgingConfig {
    JudgingConfig {
        doubles: DoublesPolicy::Always,
        channels: Channels {
            casing: false,
            doubled: true,
            letter_runs: false,
            ..Channels::default()
        },
        ..permissive()
    }
}

/// The letter-run channel alone. A word this channel names may also be a
/// casing row, and then it is ONE row carrying both bits — so, like the
/// others, it is measured on its own or the merged row would rob the row it
/// merged into.
fn sticky() -> JudgingConfig {
    JudgingConfig {
        channels: Channels {
            casing: false,
            doubled: false,
            letter_runs: true,
            ..Channels::default()
        },
        ..permissive()
    }
}

/// The length channel alone, on a sigma any sweep reaches.
fn lengthy() -> JudgingConfig {
    JudgingConfig {
        word_length_sigma: 2,
        channels: Channels {
            casing: false,
            word_length: true,
            doubled: false,
            letter_runs: false,
            ..Channels::default()
        },
        ..permissive()
    }
}

#[test]
fn the_rescan_agrees_with_the_counts_over_a_synthetic_sweep() {
    let mut fired = 0;
    let mut occurrences = 0;
    let (mut pairs, mut paired) = (0, 0);
    let (mut sticky_rows, mut sticky_sites) = (0, 0);
    for seed in 1..=12u64 {
        let books = generated(seed, 3, 4, 120);
        let (_, compared, found) = agree(&books, &permissive());
        fired += compared;
        occurrences += found;
        let (_, long, sited) = agree(&books, &lengthy());
        fired += long;
        occurrences += sited;
        let (_, doubles, sites) = agree(&books, &doubling());
        pairs += doubles;
        paired += sites;
        let (_, runs, run_sites) = agree(&books, &sticky());
        sticky_rows += runs;
        sticky_sites += run_sites;
    }
    assert!(
        fired > 100 && occurrences > 100,
        "the sweep must judge and site something: {fired} rows, {occurrences} sites"
    );
    assert!(
        pairs > 10 && paired > 10,
        "the doubled channel must be exercised too: {pairs} rows, {paired} sites"
    );
    assert!(
        sticky_rows > 0 && sticky_sites > 0,
        "the letter-run channel must be exercised too:          {sticky_rows} rows, {sticky_sites} sites"
    );
}

/// The shipped defaults, on the claim `rules/word-conventions.md` states:
/// `David` many times against `david` twice flags the two. The shipped word
/// ladder is a tenth of the glyph one, so "many" is twenty thousand.
#[test]
fn the_default_config_sites_the_minority_form() {
    let mut text = "and David went. ".repeat(20_000);
    text.push_str("and david went and david went.");
    let books = vec![book(BookKey::new(*b"MRK"), &[text])];
    let (_, compared, occurrences) = agree(&books, &JudgingConfig::default());
    assert_eq!((compared, occurrences), (1, 2));
}

/// A corpus with no cased letter judges nothing and sites nothing, without
/// hashing a word.
#[test]
fn an_uncased_corpus_is_silent() {
    let hebrew = "\u{5d0}\u{5d1}\u{5d2} \u{5d3}\u{5d4}\u{5d5}. ".repeat(60);
    let books = vec![book(BookKey::new(*b"MRK"), &[hebrew])];
    let (_, compared, occurrences) = agree(&books, &JudgingConfig::default());
    assert_eq!((compared, occurrences), (0, 0));
}

// ── The tier ────────────────────────────────────────────────────────────

/// One corpus's vref lines as books of chapter texts.
fn group(raw: &str) -> Vec<(BookKey, Vec<String>)> {
    let mut books: Vec<(BookKey, Vec<String>)> = Vec::new();
    let mut seen: Option<(String, u32)> = None;
    for line in raw.lines() {
        let Some((address, text)) = line.split_once('\t') else {
            continue;
        };
        let mut parts = address.split_whitespace();
        let (Some(code), Some(cv)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(chapter) = cv.split_once(':').and_then(|(c, _)| c.parse::<u32>().ok()) else {
            continue;
        };
        let bytes: [u8; 3] = code.as_bytes()[..3].try_into().expect("a three-byte code");
        if seen.as_ref().map(|(held, _)| held.as_str()) != Some(code) {
            books.push((BookKey::new(bytes), Vec::new()));
        }
        let chapters = &mut books.last_mut().expect("just pushed").1;
        if seen.as_ref() != Some(&(code.to_string(), chapter)) {
            chapters.push(String::new());
        }
        let held = chapters.last_mut().expect("just pushed");
        if !held.is_empty() {
            held.push(' ');
        }
        held.push_str(text);
        seen = Some((code.to_string(), chapter));
    }
    books
}

#[test]
#[ignore = "only proof that the word rescan and the word walk agree on free positions and forms on every chapter of the 8-corpus tier"]
fn the_rescan_agrees_with_the_counts_over_the_tier() {
    for name in FILES {
        let path = format!("{TIER}{name}");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("test-tier corpus {path} must be present: {error}"));
        let books: Vec<Book> = group(&raw)
            .iter()
            .filter(|(_, chapters)| !chapters.is_empty())
            .map(|(key, chapters)| book(*key, chapters))
            .collect();
        assert!(!books.is_empty(), "{name} holds books");
        // The shipped defaults minus the letter-run channel, which is
        // measured on its own below: a word both channels name is one row.
        let cased = JudgingConfig {
            channels: Channels {
                letter_runs: false,
                ..Channels::default()
            },
            ..JudgingConfig::default()
        };
        let (count, compared, occurrences) = agree(&books, &cased);
        let (_, long, long_sites) = agree(&books, &lengthy());
        let (_, pairs, paired) = agree(&books, &doubling());
        let (_, runs, run_sites) = agree(&books, &sticky());
        println!(
            "{name}: {count} books, {compared} casing rows / {occurrences} sites, \
             {long} length rows / {long_sites} sites, \
             {pairs} doubled rows / {paired} sites, \
             {runs} letter-run rows / {run_sites} sites"
        );
    }
}
