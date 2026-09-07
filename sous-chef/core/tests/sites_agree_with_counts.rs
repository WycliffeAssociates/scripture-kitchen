//! Instrument: VOLUME. The site rescan against the counts it materializes.
//!
//! For every book and every firing pattern, the number of matching occurrences
//! `sites::locate` finds equals that book's own count for the pattern's key —
//! the number the substrate walk put there. This is the roadmap's Stage 3 gate
//! "current-text site rescan agrees with the pattern counts it materializes",
//! and it is not a second walk: the rescan reports its own occurrences and the
//! aggregate answers from the lanes.
//!
//! The default sweep is synthetic and fast. The ignored one runs the same
//! equality over the committed corpus tier, where real glue, combining
//! sequences, and chapter seams are.

use sous_core::judge::{Channel, Pattern, PatternIndex, PatternKey, Side};
use sous_core::sites;
use sous_core::substrate::{BookAggregate, Edge, OuterClass, RUN_BUCKETS, ScalarKey, fold_book};
use sous_core::unicode::pool_of;
use sous_core::{
    BookKey, Chapter, ChapterObs, ChapterPass, Corpus, Findings, JudgingConfig, ProjectedBook,
    Substrate, TextRange, Verse, VerseKey, analyze_with, for_each_chapter,
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

/// One book from its chapter texts, joined in order with no gap.
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
fn aggregate(book: &Book) -> BookAggregate {
    let mut rows = Vec::new();
    for_each_chapter(book, |start, input| {
        rows.push((start, Substrate.map(input)))
    });
    let view: Vec<ChapterObs<_>> = rows
        .iter()
        .map(|(start, obs)| ChapterObs { start: *start, obs })
        .collect();
    fold_book(&view, &mut Edge::default())
}

/// What the walk counted for one pattern's key in one book — the same unit
/// `sites::locate_counted` reports.
fn counted(book: &BookAggregate, pattern: &Pattern) -> u64 {
    let glyph = pattern.glyph;
    match pattern.key {
        PatternKey::Placement { side, class } => book
            .pairs()
            .iter()
            .filter(|(key, _)| key.scalar() == glyph)
            .filter(|(key, _)| {
                class
                    == match side {
                        Side::Prev => key.prev(),
                        Side::Next => key.next(),
                    }
            })
            .map(|(_, count)| u64::from(*count))
            .sum(),
        PatternKey::RunShape { pure, bucket } => book
            .runs()
            .filter(|(atoms, _)| atoms.contains(&glyph))
            .filter(|(atoms, _)| {
                (
                    atoms.iter().all(|atom| *atom == glyph),
                    atoms.len().min(RUN_BUCKETS) as u8,
                ) == (pure, bucket)
            })
            .map(|(_, count)| u64::from(count))
            .sum(),
        PatternKey::ExactNeighbor(neighbor) => book
            .runs()
            .map(|(atoms, count)| {
                let pairs = atoms
                    .windows(2)
                    .filter(|pair| pair[0] == glyph && pair[1] == neighbor)
                    .count() as u64;
                pairs * u64::from(count)
            })
            .sum(),
        PatternKey::PooledNeighbor(pool) => book
            .runs()
            .map(|(atoms, count)| {
                let pairs = atoms
                    .windows(2)
                    .filter(|pair| {
                        pair[0] == glyph
                            && pair[1].scalar().is_some_and(|next| pool_of(next) == pool)
                    })
                    .count() as u64;
                pairs * u64::from(count)
            })
            .sum(),
        PatternKey::Rarity => book
            .scalars()
            .iter()
            .find(|(key, _)| *key == glyph)
            .map_or(0, |(_, count)| u64::from(*count)),
        // A word hash is not a glyph: `tests/casing_agree_with_counts.rs`.
        PatternKey::Casing { .. } | PatternKey::WordLength { .. } | PatternKey::Doubled { .. } => 0,
    }
}

/// Judges the corpus, then compares every book's rescan against its own
/// counts. Returns `(books, patterns compared, occurrences compared)`.
fn agree(books: &[Book], config: &JudgingConfig) -> (usize, usize, u64) {
    let corpus = Corpus::try_new(books).expect("a synthetic corpus is valid");
    let findings = analyze_with(&corpus, &Substrate, config);
    let patterns = findings.patterns().to_vec();
    let mut compared = 0;
    let mut occurrences = 0;

    for (index, projected) in corpus.iter() {
        let counts = aggregate(&books[index.get() as usize]);
        let mut set = Vec::new();
        sites::firing(&counts, &patterns, &mut set);
        let table: Vec<(PatternIndex, Pattern)> = set
            .iter()
            .map(|&at| (at, patterns[usize::from(at.get())]))
            .collect();
        let chapters: Vec<Chapter> = projected.chapters().collect();
        let (mut found, mut tally) = (Vec::new(), Vec::new());
        sites::locate_counted(projected.text(), &chapters, &table, &mut found, &mut tally);

        for ((at, pattern), rescanned) in table.iter().zip(&tally) {
            let walked = counted(&counts, pattern);
            assert_eq!(
                *rescanned,
                walked,
                "book {} pattern[{}] {:?} {:?} on {:?}",
                books[index.get() as usize].key,
                at.get(),
                pattern.channel,
                pattern.key,
                pattern.glyph.scalar(),
            );
            compared += 1;
            occurrences += walked;
        }
        // A pattern the book's counts do not hold has nothing to find.
        for pattern in &patterns {
            if !table.iter().any(|(_, held)| held == pattern) {
                assert_eq!(counted(&counts, pattern), 0, "an absent glyph counts zero");
            }
        }
    }
    (books.len(), compared, occurrences)
}

// ── The synthetic sweep ─────────────────────────────────────────────────

/// The alphabets a generated chapter mixes: letters, spaces, digits in two
/// scripts, punctuation, and glue that rides a base.
const POOL: [&str; 6] = [
    "abcdefghijklmnopqrstuvwxyzABCDE",
    "     \n\t",
    "0123456789\u{966}\u{967}\u{968}",
    ",.;:!?\"'()[]\u{2014}\u{201c}\u{201d}\u{ab}\u{bb}\u{964}",
    "\u{301}\u{94d}\u{200d}",
    "zq\u{1e9e}\u{60}~\u{2020}",
];

/// A generated corpus: `books` books of `chapters` chapters each, every
/// chapter a mix drawn from `POOL`.
fn generated(seed: u64, books: usize, chapters: usize, chapter_len: usize) -> Vec<Book> {
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
            while text.chars().count() < chapter_len {
                // Weighted so letters and spaces dominate, as real text does.
                let lane = match next() % 16 {
                    0..=7 => 0,
                    8..=11 => 1,
                    12 => 2,
                    13 => 3,
                    14 => 4,
                    _ => 5,
                };
                let alphabet: Vec<char> = POOL[lane].chars().collect();
                text.push(alphabet[next() % alphabet.len()]);
            }
            texts.push(text);
        }
        let key = BookKey::new([
            b'A' + (index / 26) as u8 % 26,
            b'A' + index as u8 % 26,
            b'A',
        ]);
        if index == 0 {
            // One rare nonletter and one rare letter, absent from POOL, so the
            // roster has something to say and its sites have somewhere to be.
            texts[0].push_str(" wo\u{2e2e}rd \u{225}nd ");
        }
        out.push(book(key, &texts));
    }
    out
}

/// A low support floor so a channel is entitled on a small input and the sweep
/// exercises every rung rather than abstaining through most of them.
fn permissive() -> JudgingConfig {
    JudgingConfig {
        support_floor: 1,
        rarity_floor: 5,
        letters: sous_core::LetterRoster::Always,
        channels: sous_core::Channels {
            pooled_neighbor: true,
            ..sous_core::Channels::default()
        },
        ..JudgingConfig::default()
    }
}

#[test]
fn the_rescan_agrees_with_the_counts_over_a_synthetic_sweep() {
    let mut patterns = 0;
    for seed in 1..=12u64 {
        let books = generated(seed, 3, 4, 400);
        let (_, compared, _) = agree(&books, &permissive());
        patterns += compared;
    }
    assert!(
        patterns > 1_000,
        "the sweep must actually judge something: {patterns} comparisons"
    );
}

/// Every channel has to be reached over the same seeds, or the sweep above
/// proves nothing about the ones it missed.
#[test]
fn the_synthetic_sweep_reaches_every_channel_and_both_placement_sides() {
    let mut seen: Vec<(Channel, Option<Side>)> = Vec::new();
    for seed in 1..=12u64 {
        let books = generated(seed, 3, 4, 400);
        let corpus = Corpus::try_new(&books).unwrap();
        let findings: Findings = analyze_with(&corpus, &Substrate, &permissive());
        for row in findings.patterns() {
            let side = match row.key {
                PatternKey::Placement { side, .. } => Some(side),
                _ => None,
            };
            if !seen.contains(&(row.channel, side)) {
                seen.push((row.channel, side));
            }
            // A book edge is never a convention.
            assert!(
                !matches!(
                    row.key,
                    PatternKey::Placement {
                        class: OuterClass::Edge,
                        ..
                    }
                ),
                "an edge fired a placement pattern"
            );
        }
    }
    for channel in [
        Channel::ExactNeighbor,
        Channel::PooledNeighbor,
        Channel::RunShape,
        Channel::Rarity,
    ] {
        assert!(
            seen.contains(&(channel, None)),
            "no {channel:?} pattern fired"
        );
    }
    for side in [Side::Prev, Side::Next] {
        assert!(
            seen.contains(&(Channel::Placement, Some(side))),
            "no placement on {side:?}"
        );
    }
}

/// The pooled digit lane is judged and sited too. A digit breaks a run and
/// joins none, so placement is the one channel that can still name it.
#[test]
fn the_pooled_digit_lane_is_judged_and_sited() {
    let books = vec![book(
        BookKey::new(*b"MRK"),
        &[format!("{}b1,c", "a1 ".repeat(200))],
    )];
    let corpus = Corpus::try_new(&books).expect("a synthetic corpus is valid");
    let findings: Findings = analyze_with(&corpus, &Substrate, &JudgingConfig::default());
    let pooled: Vec<_> = findings
        .patterns()
        .iter()
        .filter(|row| row.glyph == ScalarKey::DIGITS)
        .map(|row| (row.channel, row.key, row.numerator, row.denominator))
        .collect();
    assert_eq!(
        pooled,
        vec![(
            Channel::Placement,
            PatternKey::Placement {
                side: Side::Next,
                class: OuterClass::Nonletter,
            },
            1,
            201
        )],
        "the one digit a comma follows, against two hundred that a space does"
    );
    agree(&books, &JudgingConfig::default());
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
#[ignore = "only proof that the cursor's prev/next/run semantics equal the walk's on real scripts' glue and combining sequences, every chapter of the 8-corpus tier"]
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
        let (count, compared, occurrences) = agree(&books, &JudgingConfig::default());
        println!("{name}: {count} books, {compared} patterns, {occurrences} occurrences");
    }
}
