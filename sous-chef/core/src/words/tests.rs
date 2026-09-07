//! Coverage for the word walk, the fold, and the casing channel over them.

use super::*;
use crate::judge::{Channel, Channels, DoublesPolicy, LetterRoster, Staircase, TerminalTable};
use crate::pass::analyze_with;
use crate::substrate::{FollowCounts, ScalarKey, Substrate};
use crate::{BookKey, Chapter, Corpus, ProjectedBook, VerseKey};

/// A terminal table built by hand: `(glyph, upper, lower)` handoffs, judged
/// under the default share.
fn table(rows: &[(char, u32, u32)]) -> TerminalTable {
    let follows: Vec<(ScalarKey, FollowCounts)> = rows
        .iter()
        .map(|&(glyph, upper, lower)| (ScalarKey::of(glyph), FollowCounts::new([upper, lower, 0])))
        .collect();
    TerminalTable::learn(&follows, &JudgingConfig::default())
}

// ── The walk ────────────────────────────────────────────────────────────

fn words(text: &str) -> Vec<(&str, Form, Before)> {
    let mut out = Vec::new();
    for_each_word(text, &[], |word| {
        out.push((
            &text[word.from as usize..word.to as usize],
            word.form,
            word.before,
        ))
    });
    out
}

const fn glyph(scalar: char) -> Before {
    Before::Glyph(ScalarKey::of(scalar))
}

fn spans(text: &str) -> Vec<&str> {
    words(text).into_iter().map(|word| word.0).collect()
}

fn hash_of(text: &str) -> u64 {
    let mut found = None;
    for_each_word(text, &[], |word| found = Some(word.hash));
    found.expect("one word")
}

#[test]
fn module_doc_example_is_exact() {
    assert_eq!(
        words("He said. \u{201C}Go,\u{201D} said david. David went."),
        vec![
            ("He", Form::Title, Before::Start),
            ("said", Form::Lower, Before::None),
            ("Go", Form::Title, glyph('.')),
            ("said", Form::Lower, glyph(',')),
            ("david", Form::Lower, Before::None),
            ("David", Form::Title, glyph('.')),
            ("went", Form::Lower, Before::None),
        ]
    );
}

#[test]
fn one_nonletter_with_a_letter_on_both_sides_joins_the_word() {
    assert_eq!(
        spans("ng'ombe don't mother-in-law"),
        vec!["ng'ombe", "don't", "mother-in-law"]
    );
    assert_eq!(spans("a--b"), vec!["a", "b"], "two nonletters join nothing");
    assert_eq!(spans("a- b"), vec!["a", "b"], "a space is not a letter");
    assert_eq!(spans("-ab-"), vec!["ab"], "an edge joiner is not one");
}

#[test]
fn a_digit_beside_a_letter_joins_and_a_digit_run_alone_is_no_word() {
    assert_eq!(spans("3rd 1Ki x2"), vec!["3rd", "1Ki", "x2"]);
    assert_eq!(spans("in 12,345 and 7"), vec!["in", "and"]);
    // A digit does not confirm a joiner: the dash ends the word.
    assert_eq!(spans("a-3 4-b"), vec!["a", "b"]);
}

#[test]
fn the_form_is_the_capitals_the_word_carries() {
    let forms = |text: &str| -> Vec<Form> { words(text).into_iter().map(|w| w.1).collect() };
    assert_eq!(
        forms("david David DAVID McDonald iPhone A"),
        vec![
            Form::Lower,
            Form::Title,
            Form::Upper,
            Form::Mixed,
            Form::Mixed,
            // One cased letter, so `Upper` needs a second one and `Title` takes it.
            Form::Title,
        ]
    );
    // 1Ki: the first LETTER is the capital, whatever the digit before it.
    assert_eq!(forms("1Ki"), vec![Form::Title]);
    // An uncased script has no casing convention to break.
    assert_eq!(
        forms("\u{5d0}\u{5d1}\u{5d2} \u{915}\u{94b}\u{908}"),
        vec![Form::Uncased, Form::Uncased]
    );
}

/// The walk records the glyph; the table, not the walk, decides.
#[test]
fn the_walk_records_what_stood_before_each_word() {
    let before = |text: &str| -> Vec<Before> { words(text).into_iter().map(|w| w.2).collect() };
    assert_eq!(
        before("one. two, three four. five"),
        vec![
            Before::Start,
            glyph('.'),
            glyph(','),
            Before::None,
            glyph('.')
        ]
    );
    // A dash is a glyph like any other: the corpus decides what it does. The
    // closing quote is transparent too, so `c` reads the word behind it.
    assert_eq!(
        before("a. \u{201C}b\u{201D} c. \u{2014}d"),
        vec![Before::Start, glyph('.'), Before::None, glyph('\u{2014}')]
    );
}

/// A quote hides the terminal in front of it, and it is the terminal the
/// capital answers to.
#[test]
fn an_opening_quote_takes_the_glyph_behind_it() {
    let before = |text: &str| -> Vec<Before> { words(text).into_iter().map(|w| w.2).collect() };
    assert_eq!(before("a. \u{201C}b"), vec![Before::Start, glyph('.')]);
    assert_eq!(before("a. (b"), vec![Before::Start, glyph('.')]);
    // And behind a comma it is the comma, which is the whole `he said, "Stop`
    // question: the corpus's own commas answer it.
    assert_eq!(before("a, \u{201C}b"), vec![Before::Start, glyph(',')]);
    // A quote with a word behind it hides nothing.
    assert_eq!(before("a \u{201C}b"), vec![Before::Start, Before::None]);
}

/// 95% of the letters after `.` are capitals, so the corpus capitalizes there
/// and a word in that position is evidence of nothing.
#[test]
fn a_glyph_that_precedes_capitals_forces() {
    let learned = table(&[('.', 95, 5)]);
    assert!(learned.forces(ScalarKey::of('.')));
    assert!(!glyph('.').is_free(&learned));
    assert!(!Before::Start.is_free(&learned), "a start always forces");
    assert!(Before::None.is_free(&learned));
}

/// 5% after a comma, so the comma decides nothing and the word does.
#[test]
fn a_glyph_that_rarely_precedes_capitals_is_free() {
    let learned = table(&[('.', 95, 5), (',', 5, 95)]);
    assert!(!learned.forces(ScalarKey::of(',')));
    assert!(glyph(',').is_free(&learned));
    // And under the support floor a glyph decides nothing either way.
    assert!(!table(&[('!', 4, 0)]).forces(ScalarKey::of('!')));
    assert!(table(&[('!', 5, 0)]).forces(ScalarKey::of('!')));
}

/// The same glyph, two corpora: a corpus that reports speech after a comma
/// forces there, and one that does not leaves the position free.
#[test]
fn one_glyph_decides_differently_in_two_corpora() {
    let reported = table(&[(',', 90, 10)]);
    let plain = table(&[(',', 10, 90)]);
    assert!(!glyph(',').is_free(&reported));
    assert!(glyph(',').is_free(&plain));
}

#[test]
fn a_verse_start_is_forced() {
    let text = "one two three four";
    let verse = |chapter, number, from, to| {
        Verse::new(
            VerseKey::new(chapter, number, number).unwrap(),
            TextRange::new(from, to).unwrap(),
        )
    };
    let verses = [verse(1, 1, 0, 7), verse(1, 2, 8, 18)];
    let mut before = Vec::new();
    for_each_word(text, &verses, |word| before.push(word.before));
    assert_eq!(
        before,
        vec![Before::Start, Before::None, Before::Start, Before::None]
    );
    // Whatever the corpus does with punctuation, a start is never evidence.
    let empty = TerminalTable::default();
    assert!(before.iter().filter(|at| !at.is_free(&empty)).count() == 2);
}

// ── The row ─────────────────────────────────────────────────────────────

fn row(text: &str) -> WordRow {
    walk::walk(text, &[])
}

#[test]
fn a_word_row_counts_forms_by_hash_and_by_what_stood_before() {
    let observed = row("Then david went and David wept and DAVID sang. david ran.");
    let david: Vec<_> = observed
        .words()
        .iter()
        .filter(|word| word.hash == hash_of("david"))
        .collect();
    assert_eq!(david.len(), 2, "one row per Before the word was seen under");
    let open = david
        .iter()
        .find(|row| row.before() == Before::None)
        .expect("three davids stand after a word");
    assert_eq!(open.count_of(Form::Lower), 1);
    assert_eq!(open.count_of(Form::Title), 1);
    assert_eq!(open.count_of(Form::Upper), 1);
    assert_eq!(open.len, 5);
    let stopped = david
        .iter()
        .find(|row| row.before() == glyph('.'))
        .expect("one david stands after a terminal");
    assert_eq!(stopped.count_of(Form::Lower), 1);
    assert!(
        observed
            .words()
            .windows(2)
            .all(|pair| (pair[0].hash, pair[0].before()) < (pair[1].hash, pair[1].before())),
        "the lane is sorted and distinct"
    );
}

/// An uncased chapter holds no casing row — a word with no cased letter can
/// carry no casing convention — but it does hold a doubles row per word, which
/// is where the doubled channel's denominator comes from there.
#[test]
fn an_uncased_chapter_stores_no_casing_row_and_one_doubles_row_per_word() {
    let hebrew = row("\u{5d0}\u{5d1}\u{5d2} \u{5d3}\u{5d4}. \u{5d0}\u{5d1}\u{5d2}");
    assert!(hebrew.words().is_empty());
    assert!(!hebrew.cased());
    assert_eq!(hebrew.doubles().len(), 2, "two distinct words");
    assert_eq!(hebrew.doubles()[0].bare + hebrew.doubles()[0].separated, 0);
    assert_eq!(
        hebrew.doubles().iter().map(|row| row.uncased).sum::<u16>(),
        3
    );
    assert_eq!(
        hebrew.resident_bytes(),
        size_of::<WordRow>() + 2 * size_of::<DoubleCount>()
    );
    assert!(row("David").cased());
}

// ── The doubles lane ────────────────────────────────────────────────────

/// One word's doubles row, or the zero row when the chapter holds none.
fn doubles_of(text: &str, word: &str) -> DoubleCount {
    let hash = hash_of(word);
    row(text)
        .doubles()
        .iter()
        .find(|row| row.hash == hash)
        .copied()
        .unwrap_or(DoubleCount::new(hash))
}

#[test]
fn the_double_count_is_sixteen_bytes() {
    assert_eq!(size_of::<DoubleCount>(), 16);
}

/// Two claims, kept apart, and compared by the case fold: `The the` is a
/// double.
#[test]
fn adjacent_and_separated_doubles_are_different_counters() {
    assert_eq!(doubles_of("go go on", "go").bare, 1);
    assert_eq!(doubles_of("go go on", "go").separated, 0);
    assert_eq!(doubles_of("na, na now", "na").separated, 1);
    assert_eq!(doubles_of("na, na now", "na").bare, 0);
    assert_eq!(doubles_of("The the end", "the").bare, 1);
    // A newline is whitespace, so a line break is still bare.
    assert_eq!(doubles_of("go\ngo on", "go").bare, 1);
    // Three in a row are two pairs.
    assert_eq!(doubles_of("go go go on", "go").bare, 2);
    // The chapter is the walk's whole world, so its edges bound a pair too.
    assert_eq!(doubles_of("go go", "go").bare, 1);
}

/// Only a letter, glue, or digit between disqualifies a pair. Each of those
/// means the walk dropped a token there — a digit run holds no letter, so it
/// is no word at all — and a double must not claim across one.
#[test]
fn a_word_between_two_occurrences_is_not_a_double() {
    assert_eq!(doubles_of("go on go", "go").bare, 0);
    assert_eq!(doubles_of("na 3 na", "na").separated, 0);
    assert_eq!(doubles_of("na 3 na", "na").bare, 0);
    // `a--b` is two words, so `go--go` is a separated double.
    assert_eq!(doubles_of("go--go on", "go").separated, 1);
}

/// The lane is keyed by hash alone: a double is a double whatever stood
/// before it, so one row carries every position.
#[test]
fn the_doubles_lane_is_keyed_by_hash_alone() {
    let observed = row("Go go on. Go go on, go go.");
    let go = observed
        .doubles()
        .iter()
        .filter(|row| row.hash == hash_of("go"))
        .count();
    assert_eq!(go, 1);
    assert_eq!(doubles_of("Go go on. Go go on, go go.", "go").bare, 3);
    assert!(
        observed
            .doubles()
            .windows(2)
            .all(|pair| pair[0].hash < pair[1].hash),
        "the lane is sorted and distinct"
    );
}

#[test]
fn the_word_count_is_twenty_four_bytes() {
    assert_eq!(size_of::<WordCount>(), 24);
    let observed = row("one two three");
    assert_eq!(
        observed.resident_bytes(),
        size_of::<WordRow>() + 3 * size_of::<WordCount>()
    );
}

#[test]
fn a_saturating_lane_stops_at_its_width() {
    let observed = row(&"word ".repeat(70_000));
    let word = observed.words()[0];
    assert_eq!(word.count_of(Form::Lower), u16::MAX);
}

// ── The fold ────────────────────────────────────────────────────────────

fn fold(texts: &[&str]) -> WordAggregate {
    let rows: Vec<WordRow> = texts.iter().map(|text| row(text)).collect();
    let view: Vec<ChapterObs<&WordRow>> = rows
        .iter()
        .enumerate()
        .map(|(at, obs)| ChapterObs {
            start: at as u32 * 100,
            obs,
        })
        .collect();
    fold_book(&view)
}

#[test]
fn the_fold_merges_by_hash_and_carries_no_seam() {
    let joined = fold(&["David went", "David wept"]);
    let david = joined.rows_for(hash_of("david"));
    // Both chapters' first words stand at their own chapter's start.
    assert_eq!(david.len(), 1);
    assert_eq!(david[0].before(), Before::Start);
    assert_eq!(david[0].count_of(Form::Title), 2);
    assert!(joined.cased());

    // A word is never split across a masked `\c`, so order cannot matter.
    let reversed = fold(&["David wept", "David went"]);
    assert_eq!(joined.words(), reversed.words());
    assert!(!fold(&["\u{5d0}\u{5d1}", "\u{5d2}"]).cased());
}

/// The doubles lane merges by hash, and the seam carries nothing: a pair the
/// chapter edge split was never counted, so there is nothing to fold.
#[test]
fn the_fold_merges_the_doubles_lane_and_the_seam_ends_a_pair() {
    let joined = fold(&["go go on", "go, go on"]);
    let go = joined.doubles_for(hash_of("go")).expect("one row");
    assert_eq!((go.bare, go.separated), (1, 1));

    // A cased word that never doubles is in no doubles row at all, which is
    // what keeps the lane cheap in a cased script.
    let split = fold(&["and go", "go and"]);
    assert!(split.doubles().is_empty());
}

// ── The channel ─────────────────────────────────────────────────────────

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

fn book(key: &[u8; 3], text: impl Into<String>) -> Book {
    let text = text.into();
    let whole = TextRange::new(0, text.len() as u32).unwrap();
    Book {
        key: BookKey::new(*key),
        text,
        chapters: vec![Chapter::new(1, whole).unwrap()],
        verses: vec![Verse::new(VerseKey::new(1, 1, 1).unwrap(), whole)],
    }
}

/// The parts joined by one space, each part its own span; the joining spaces
/// belong to no part, which is what makes a chapter seam a real edge of text.
fn parts(texts: &[&str]) -> (String, Vec<TextRange>) {
    let mut text = String::new();
    let mut spans = Vec::new();
    for part in texts {
        if !text.is_empty() {
            text.push(' ');
        }
        let from = text.len() as u32;
        text.push_str(part);
        spans.push(TextRange::new(from, text.len() as u32).unwrap());
    }
    (text, spans)
}

/// One chapter, one verse per part: verse state crosses the seam, which is
/// exactly what a doubled pair must ride through.
fn versed(key: &[u8; 3], texts: &[&str]) -> Book {
    let (text, spans) = parts(texts);
    let whole = TextRange::new(0, text.len() as u32).unwrap();
    Book {
        key: BookKey::new(*key),
        text,
        chapters: vec![Chapter::new(1, whole).unwrap()],
        verses: spans
            .iter()
            .enumerate()
            .map(|(at, span)| {
                Verse::new(
                    VerseKey::new(1, at as u16 + 1, at as u16 + 1).unwrap(),
                    *span,
                )
            })
            .collect(),
    }
}

/// One chapter per part, each holding one verse: the seam is an edge of text.
fn chaptered(key: &[u8; 3], texts: &[&str]) -> Book {
    let (text, spans) = parts(texts);
    Book {
        key: BookKey::new(*key),
        text,
        chapters: spans
            .iter()
            .enumerate()
            .map(|(at, span)| Chapter::new(at as u16 + 1, *span).unwrap())
            .collect(),
        verses: spans
            .iter()
            .enumerate()
            .map(|(at, span)| Verse::new(VerseKey::new(at as u16 + 1, 1, 1).unwrap(), *span))
            .collect(),
    }
}

/// The word channels judge beside the substrate, because the terminal table
/// is the substrate's follow lane. `(Substrate, Words)` is that seam; the
/// word rows are the ones with a hash.
fn analyzed(books: &[Book], config: &JudgingConfig) -> Findings {
    let corpus = Corpus::try_new(books).expect("a synthetic corpus is valid");
    analyze_with(&corpus, &(Substrate, Words), &(*config, *config))
}

/// The glyph staircase and its floor, for fixtures whose point is the
/// mechanism rather than the shipped volume: the word ladder is a tenth of
/// the glyph one, and a forty-word sample cannot reach three basis points.
fn loose() -> JudgingConfig {
    JudgingConfig {
        word_support_floor: 5,
        word_bands: Staircase::default(),
        ..JudgingConfig::default()
    }
}

fn judged(books: &[Book], config: &JudgingConfig) -> Vec<Pattern> {
    analyzed(books, config)
        .patterns()
        .iter()
        .filter(|row| row.channel.is_word())
        .copied()
        .collect()
}

/// The spans a word channel sited, in book order.
fn sited<'a>(books: &'a [Book], findings: &Findings, reason: Reasons) -> Vec<&'a str> {
    findings
        .rows()
        .iter()
        .filter(|row| match row.kind() {
            FindingKind::Convention(digest) => digest.reasons().contains(reason),
            _ => false,
        })
        .map(|row| {
            let text = &books[usize::from(row.book_idx().get())].text;
            &text[row.from() as usize..row.to() as usize]
        })
        .collect()
}

/// `David` x40 against `david` x2: the two are the row.
#[test]
fn the_minority_form_of_a_word_in_free_positions_fires() {
    let mut text = "and David went ".repeat(40);
    text.push_str("and david went and david went");
    let patterns = judged(&[book(b"MRK", text)], &loose());
    let casing: Vec<_> = patterns
        .iter()
        .map(|row| (row.word_hash(), row.key, row.numerator, row.denominator))
        .collect();
    assert_eq!(
        casing,
        vec![(
            Some(hash_of("david")),
            PatternKey::Casing {
                hash: hash_of("david"),
                form: Form::Lower
            },
            2,
            42
        )]
    );
    assert_eq!(patterns[0].channel, Channel::Casing);
    assert_eq!(patterns[0].glyph, crate::ScalarKey::NONE);
    assert_eq!(patterns[0].share_bp, 476);
}

/// A word common in both forms is convention, not a slip, so bivariance
/// needs no rule of its own.
#[test]
fn a_bivariant_word_fires_nothing() {
    let text = "and David went and david went ".repeat(20);
    assert!(judged(&[book(b"MRK", text)], &loose()).is_empty());
}

/// Forty `The`s stand where this corpus's own `.` puts a capital, so they are
/// not evidence: the one `The` that chose it is, against forty free `the`s.
/// Counting the forced forty would make the row 41/81 and fire nothing.
#[test]
fn a_forced_position_is_not_evidence() {
    let text = "The word. ".repeat(40) + &"and the word ".repeat(40) + "and The word";
    let patterns = judged(&[book(b"MRK", text)], &loose());
    assert_eq!(patterns.len(), 1);
    assert_eq!(
        patterns[0].key,
        PatternKey::Casing {
            hash: hash_of("the"),
            form: Form::Title
        }
    );
    assert_eq!((patterns[0].numerator, patterns[0].denominator), (1, 41));
}

/// The same words, and a corpus whose `.` does not predict a capital: nothing
/// is forced there, so the forty `The`s and the forty `the`s are one
/// bivariant word.
#[test]
fn a_glyph_this_corpus_does_not_capitalize_after_leaves_its_words_free() {
    let text = "The word. ".repeat(40) + &"and the word. ".repeat(40) + "and The word";
    assert!(judged(&[book(b"MRK", text)], &loose()).is_empty());
}

#[test]
fn a_word_under_the_support_floor_abstains() {
    let quiet = JudgingConfig {
        word_support_floor: 100,
        ..loose()
    };
    let text = "and David went ".repeat(40) + "and david went";
    assert!(judged(&[book(b"MRK", text.clone())], &quiet).is_empty());
    assert_eq!(judged(&[book(b"MRK", text)], &loose()).len(), 1);
}

#[test]
fn an_uncased_corpus_emits_nothing_and_the_channel_switches_off() {
    let hebrew = "\u{5d0}\u{5d1}\u{5d2} \u{5d3}\u{5d4}\u{5d5} ".repeat(40);
    assert!(judged(&[book(b"MRK", hebrew)], &loose()).is_empty());

    let off = JudgingConfig {
        channels: Channels {
            casing: false,
            ..Channels::default()
        },
        letters: LetterRoster::Never,
        ..loose()
    };
    let text = "and David went ".repeat(40) + "and david went";
    assert!(judged(&[book(b"MRK", text)], &off).is_empty());
}

/// The word channels read the terminal table out of the sink, so a pass that
/// judges words with no substrate beside it abstains rather than invent a
/// forced rule of its own.
#[test]
fn words_alone_abstain_because_nothing_published_a_terminal_table() {
    let text = "and David went ".repeat(40) + "and david went and david went";
    let books = [book(b"MRK", text)];
    let corpus = Corpus::try_new(&books).unwrap();
    let alone = analyze_with(&corpus, &Words, &loose());
    assert!(alone.terminals().is_none());
    assert!(alone.patterns().is_empty());
    assert_eq!(judged(&books, &loose()).len(), 1);
}

/// Dispersion is a count of the books holding part of the numerator, the same
/// claim the substrate's rows make.
#[test]
fn a_casing_row_names_the_books_its_numerator_came_from() {
    let books = [
        book(b"MRK", "and David went ".repeat(30)),
        book(
            b"GEN",
            "and David went ".repeat(30) + "and david went and david went",
        ),
    ];
    let patterns = judged(&books, &loose());
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].books, 1, "only GEN holds a lowercase david");
    assert_eq!((patterns[0].numerator, patterns[0].denominator), (2, 62));
}

/// The rescan places what the counts decided, and only in free positions.
#[test]
fn locate_sites_every_free_occurrence_the_numerator_counted() {
    // This corpus capitalizes after `.`, so the forty `David`s the stop put
    // there are sited nowhere; the two that chose it are.
    let text = "David went and david went. ".repeat(40) + "and David rose and David rose";
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &loose());
    let pattern = *findings
        .patterns()
        .iter()
        .find(|row| row.channel == Channel::Casing)
        .expect("one casing row");
    assert_eq!((pattern.numerator, pattern.denominator), (2, 42));
    assert_eq!(
        sited(&books, &findings, Reasons::CASING),
        vec!["David", "David"]
    );
    let table = findings.terminals().expect("the substrate published one");
    assert!(table.forces(ScalarKey::of('.')));
    assert_eq!(free_in(&fold(&[&books[0].text]), &pattern, table), 2);
}

// -- The word length channel --------------------------------------------

/// A word standing four deviations past the corpus's own mean, sited on its
/// every occurrence: length is a property of the word, not of a position.
#[test]
fn a_word_far_longer_than_the_corpus_fires_on_its_length() {
    let long = JudgingConfig {
        channels: Channels {
            casing: false,
            word_length: true,
            ..Channels::default()
        },
        word_support_floor: 3,
        ..JudgingConfig::default()
    };
    let text = "and the cat sat ".repeat(60)
        + "abcdefghijklmnopqrst abcdefghijklmnopqrst abcdefghijklmnopqrst";
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &long);
    let rows: Vec<_> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::WordLength)
        .collect();
    assert_eq!(rows.len(), 1);
    let PatternKey::WordLength { hash, sigma } = rows[0].key else {
        panic!("a length key")
    };
    assert_eq!(hash, hash_of("abcdefghijklmnopqrst"));
    assert!(sigma >= 4, "{sigma} deviations above the mean");
    assert_eq!(rows[0].numerator, 3);
    assert_eq!(rows[0].books, 1);
    assert_eq!(
        sited(&books, &findings, Reasons::WORD_LENGTH),
        vec![
            "abcdefghijklmnopqrst",
            "abcdefghijklmnopqrst",
            "abcdefghijklmnopqrst"
        ]
    );
}

/// The shipped ladder, on a corpus big enough for it: a form under three
/// basis points of its own word's free positions.
#[test]
fn the_shipped_word_ladder_is_a_tenth_of_the_glyph_ladder() {
    let shipped = JudgingConfig::default();
    assert_eq!(shipped.word_support_floor, 20);
    assert_eq!(shipped.word_bands.steps, Staircase::WORD_STEPS);
    assert!(shipped.channels.casing);
    for (word, glyph) in shipped
        .word_bands
        .steps
        .iter()
        .zip(Staircase::DEFAULT_STEPS)
    {
        assert_eq!(word.share_bp * 10, glyph.share_bp);
    }

    // 20,000 free `David` against one `david`: under the last rung's 3 bp.
    let text = "and David went. ".repeat(20_000) + "and david went";
    let patterns = judged(&[book(b"MRK", text)], &shipped);
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].numerator, 1);
    assert_eq!(patterns[0].band, Some(4));
}

/// Off by default: names and loanwords are this tail, and they are not slips.
#[test]
fn the_length_channel_ships_off() {
    assert!(!JudgingConfig::default().channels.word_length);
    let text = "and the cat sat ".repeat(60) + "abcdefghijklmnopqrst abcdefghijklmnopqrst";
    assert!(
        judged(&[book(b"MRK", text)], &loose())
            .iter()
            .all(|row| row.channel != Channel::WordLength)
    );
}

// ── The doubled channel ─────────────────────────────────────────────────

/// The recusal off, so a fixture whose point is the band is not answered by
/// the corpus statistic instead.
fn always(config: &JudgingConfig) -> JudgingConfig {
    JudgingConfig {
        doubles: DoublesPolicy::Always,
        ..*config
    }
}

fn doubled_rows(books: &[Book], config: &JudgingConfig) -> Vec<Pattern> {
    judged(books, config)
        .into_iter()
        .filter(|row| row.channel == Channel::Doubled)
        .collect()
}

/// French `vous vous` is a construction, not a slip: 300 doublings against
/// 9,000 uses is 3.3%, far above band 3's 10 basis points, so the word
/// excuses itself against its own count and no allow-list is needed.
#[test]
fn vous_vous_three_hundred_times_is_convention_and_silent() {
    let mut text = String::new();
    for index in 0..300 {
        text.push_str(&format!("w{} vous vous ", index % 100));
    }
    for index in 0..8_400 {
        text.push_str(&format!("w{} vous ", index % 100));
    }
    let books = [book(b"MRK", text)];
    assert!(doubled_rows(&books, &JudgingConfig::default()).is_empty());
    // Not the recusal: a hundred other words keep the corpus's doubling share
    // at 99 bp, and forcing the channel on changes nothing.
    assert!(doubled_rows(&books, &always(&JudgingConfig::default())).is_empty());
}

/// The other half of the same rule: one `the the` against two thousand
/// ordinary `the`s is 4 basis points and stays reviewable.
#[test]
fn the_the_once_is_flagged_bare() {
    let mut text = "and the word ".repeat(2_000);
    text.push_str("and the the word");
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &JudgingConfig::default());
    let rows: Vec<Pattern> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::Doubled)
        .copied()
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].key,
        PatternKey::Doubled {
            hash: hash_of("the"),
            separated: false,
        }
    );
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 2_002));
    assert_eq!(rows[0].books, 1);
    // The span covers both words and nothing else.
    assert_eq!(sited(&books, &findings, Reasons::DOUBLED_BARE), ["the the"]);
}

/// `na, na` is a second key with its own denominator, never pooled with the
/// adjacent one: a comma between them is a different claim about the text.
#[test]
fn na_comma_na_is_a_separate_claim() {
    let mut text = "and na now ".repeat(2_000);
    text.push_str("and na, na now");
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &JudgingConfig::default());
    let rows: Vec<Pattern> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::Doubled)
        .copied()
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].key,
        PatternKey::Doubled {
            hash: hash_of("na"),
            separated: true,
        }
    );
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 2_002));
    // The span covers both words AND the separator.
    assert_eq!(
        sited(&books, &findings, Reasons::DOUBLED_SEPARATED),
        ["na, na"]
    );
    assert!(sited(&books, &findings, Reasons::DOUBLED_BARE).is_empty());
}

/// Charter invariant 1: a verse start is an address, not a sentence boundary,
/// so word state crosses it and a pair straddling the seam is a real pair.
#[test]
fn a_double_across_a_verse_seam_counts() {
    let lead = "and na now ".repeat(2_000) + "and na";
    let books = [versed(b"MRK", &[&lead, "na now"])];
    let findings = analyzed(&books, &JudgingConfig::default());
    let rows: Vec<Pattern> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::Doubled)
        .copied()
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].numerator, 1);
    assert_eq!(sited(&books, &findings, Reasons::DOUBLED_BARE), ["na na"]);
}

/// A chapter seam is an edge of text for a word, which the fold's own rule
/// already says: the walk is per chapter, so the pair simply never forms.
#[test]
fn a_double_across_a_chapter_seam_does_not() {
    let lead = "and na now ".repeat(2_000) + "and na";
    let books = [chaptered(b"MRK", &[&lead, "na now"])];
    assert!(doubled_rows(&books, &JudgingConfig::default()).is_empty());
}

/// Genuinely productive reduplication is a corpus-level fact, not a band: a
/// language that doubles three words in ten abstains entirely, and `Always`
/// is the host's override.
#[test]
fn a_reduplicating_corpus_recuses_itself() {
    let mut text = String::new();
    for _ in 0..100 {
        for word in 0..10 {
            text.push_str(&format!("w{word} "));
        }
    }
    for word in 0..3 {
        text.push_str(&format!("w{word} w{word} w5 w{word} w{word} w5 "));
    }
    let books = [book(b"MRK", text)];
    assert!(doubled_rows(&books, &loose()).is_empty(), "3 of 10 double");
    let forced = doubled_rows(&books, &always(&loose()));
    assert_eq!(forced.len(), 3);
    assert!(forced.iter().all(|row| row.numerator == 2));

    let never = JudgingConfig {
        doubles: DoublesPolicy::Never,
        ..loose()
    };
    assert!(doubled_rows(&books, &never).is_empty());
}

/// Doubling has nothing to do with case, so an uncased script — which pays
/// nothing for the casing channel — is still judged here. Its denominator
/// comes from the doubles lane's own `uncased` count, because the casing lane
/// refuses every one of its words.
#[test]
fn an_uncased_script_still_counts_doubles() {
    let word = "\u{5d0}\u{5d1}";
    let mut text = format!("{word} \u{5d2}\u{5d3} ").repeat(200);
    text.push_str(&format!("{word} {word}"));
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &loose());
    let rows: Vec<Pattern> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel.is_word())
        .copied()
        .collect();
    assert_eq!(rows.len(), 1, "the casing channel says nothing here");
    assert_eq!(rows[0].channel, Channel::Doubled);
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 202));
    assert_eq!(
        sited(&books, &findings, Reasons::DOUBLED_BARE),
        [format!("{word} {word}")]
    );
}

/// It ships on: the volume is low and the claim is cheap.
#[test]
fn the_doubled_channel_ships_on() {
    assert!(JudgingConfig::default().channels.doubled);
    assert_eq!(JudgingConfig::default().doubles, DoublesPolicy::Auto);
    assert_eq!(JudgingConfig::default().doubles_productive_bp, 300);

    let mut text = "and the word ".repeat(2_000);
    text.push_str("and the the word");
    let off = JudgingConfig {
        channels: Channels {
            doubled: false,
            ..Channels::default()
        },
        ..JudgingConfig::default()
    };
    assert!(doubled_rows(&[book(b"MRK", text)], &off).is_empty());
}

// ── The corpus tally ────────────────────────────────────────────────────

/// Five books whose words overlap in every way that matters: shared hashes,
/// hashes one book alone holds, and a word only ever in a forced position.
fn corpus_books() -> Vec<WordAggregate> {
    [
        "David went. david wept. DAVID sang.",
        "Solomon spoke. david slept. na, na.",
        "David rose and david ran and Ruth wept. na na.",
        "Ruth. ruth. RUTH gleaned in david's field. \u{5d0}\u{5d1} \u{5d0}\u{5d1}",
        "Boaz.",
    ]
    .iter()
    .map(|text| fold(&[text]))
    .collect()
}

fn totals_of(books: &[&WordAggregate]) -> WordTotals {
    WordTotals::merge(books)
}

#[test]
fn adding_every_book_one_at_a_time_equals_one_merge() {
    let books = corpus_books();
    let views: Vec<&WordAggregate> = books.iter().collect();
    let mut built = WordTotals::default();
    for book in &views {
        built.add(&[book]);
    }
    assert_eq!(built, totals_of(&views));
}

#[test]
fn removing_a_book_leaves_the_merge_of_the_books_left() {
    let books = corpus_books();
    let views: Vec<&WordAggregate> = books.iter().collect();
    for dropped in 0..views.len() {
        let mut tally = totals_of(&views);
        tally.remove(&[views[dropped]]);
        let left: Vec<&WordAggregate> = views
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != dropped)
            .map(|(_, book)| *book)
            .collect();
        assert_eq!(tally, totals_of(&left), "book {dropped} removed");
    }
}

/// The resident update a host makes on a keystroke: one book out at its old
/// rows, back in at its new ones.
#[test]
fn swapping_one_books_rows_equals_a_merge_of_the_corpus_after() {
    let books = corpus_books();
    let views: Vec<&WordAggregate> = books.iter().collect();
    let mut tally = totals_of(&views);

    let edited = fold(&["David rose and DAVID ran and Ruth wept and Boaz sowed."]);
    tally.remove(&[views[2]]);
    tally.add(&[&edited]);

    let mut after: Vec<&WordAggregate> = views.clone();
    after[2] = &edited;
    assert_eq!(tally, totals_of(&after));
}

/// Two books holding the same text are two contributors, so the row's
/// dispersion counts both and dropping one leaves the other.
#[test]
fn one_aggregate_tallied_twice_is_two_books() {
    let book = fold(&["David went. david wept."]);
    let mut tally = WordTotals::default();
    tally.add(&[&book, &book]);
    assert_eq!(tally, totals_of(&[&book, &book]));
    tally.remove(&[&book]);
    assert_eq!(tally, totals_of(&[&book]));
    tally.remove(&[&book]);
    assert!(tally.is_empty(), "no book holds any word");
}

/// The doubles lane rides the same two updates as the casing lane, and the
/// recusal statistic is a share of the union of the two vocabularies.
#[test]
fn the_doubles_lane_adds_and_removes_like_the_casing_lane() {
    let books = corpus_books();
    let views: Vec<&WordAggregate> = books.iter().collect();
    let tally = totals_of(&views);
    let na = tally
        .doubles()
        .iter()
        .find(|row| row.hash == hash_of("na"))
        .expect("two books hold na");
    assert_eq!((na.bare, na.separated, na.holders), (1, 1, 2));

    // An uncased word is in the doubles lane alone, so the union counts it.
    let hebrew = tally
        .doubles()
        .iter()
        .find(|row| row.hash == hash_of("\u{5d0}\u{5d1}"))
        .expect("one book holds it");
    assert_eq!((hebrew.uncased, hebrew.bare), (2, 1));
    assert!(!tally.by_word().any(|word| word[0].hash == hebrew.hash));
    assert!(tally.doubling_share_bp() > 0);

    let mut built = WordTotals::default();
    for book in &views {
        built.add(&[book]);
    }
    assert_eq!(built, tally);
    built.remove(&[views[1]]);
    let left: Vec<&WordAggregate> = views
        .iter()
        .enumerate()
        .filter(|(at, _)| *at != 1)
        .map(|(_, book)| *book)
        .collect();
    assert_eq!(built, totals_of(&left));
}

/// A word only ever in a forced position has an all-zero row, and a fresh
/// merge has it too — so the row lives while a book holds the word at all.
#[test]
fn a_forced_only_word_keeps_its_row_and_leaves_with_its_last_book() {
    let forced = fold(&["Solomon reigned."]);
    let other = fold(&["and david went and david ran"]);
    let mut tally = totals_of(&[&forced, &other]);
    let solomon = hash_of("Solomon");
    let row = tally
        .rows()
        .iter()
        .find(|row| row.hash == solomon)
        .expect("the merge keeps the row");
    assert_eq!(row.counts[Form::Lower as usize], 0, "no free lowercase");
    assert_eq!(row.holders, 1);

    tally.remove(&[&forced]);
    assert_eq!(tally, totals_of(&[&other]));
    assert!(!tally.rows().iter().any(|row| row.hash == solomon));
}
