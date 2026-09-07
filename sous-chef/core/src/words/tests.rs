//! Coverage for the word walk, the fold, and the casing channel over them.

use super::*;
use crate::judge::{Channel, Channels, LetterRoster};
use crate::pass::analyze_with;
use crate::{BookKey, Chapter, Corpus, ProjectedBook, VerseKey};

// ── The walk ────────────────────────────────────────────────────────────

fn words(text: &str) -> Vec<(&str, Form, bool)> {
    let mut out = Vec::new();
    for_each_word(text, &[], |word| {
        out.push((
            &text[word.from as usize..word.to as usize],
            word.form,
            word.forced,
        ))
    });
    out
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
            ("He", Form::Title, true),
            ("said", Form::Lower, false),
            ("Go", Form::Title, true),
            ("said", Form::Lower, false),
            ("david", Form::Lower, false),
            ("David", Form::Title, true),
            ("went", Form::Lower, false),
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

#[test]
fn a_position_forces_a_capital_or_leaves_the_word_free() {
    let forced = |text: &str| -> Vec<bool> { words(text).into_iter().map(|w| w.2).collect() };
    // Chapter start, then a terminal, then a comma that forces nothing.
    assert_eq!(
        forced("one. two, three four. five"),
        vec![true, true, false, false, true]
    );
    // An opening quote rides a terminal through; a dash after one does not.
    assert_eq!(
        forced("a. \u{201C}b\u{201D} c. \u{2014}d"),
        vec![true, true, false, false]
    );
    // A closing bracket after the terminal is still a ride.
    assert_eq!(forced("a. (b"), vec![true, true]);
    // A separator, not a terminal, so nothing after the quote is forced.
    assert_eq!(forced("a, \u{201C}b"), vec![true, false]);
}

#[test]
fn a_verse_start_forces_its_first_word() {
    let text = "one two three four";
    let verse = |chapter, number, from, to| {
        Verse::new(
            VerseKey::new(chapter, number, number).unwrap(),
            TextRange::new(from, to).unwrap(),
        )
    };
    let verses = [verse(1, 1, 0, 7), verse(1, 2, 8, 18)];
    let mut forced = Vec::new();
    for_each_word(text, &verses, |word| forced.push(word.forced));
    assert_eq!(forced, vec![true, false, true, false]);
}

// ── The row ─────────────────────────────────────────────────────────────

fn row(text: &str) -> WordRow {
    walk::walk(text, &[])
}

#[test]
fn a_word_row_counts_free_forms_and_forced_positions_by_hash() {
    let observed = row("Then david went and David wept and DAVID sang. david ran.");
    let david = observed
        .words()
        .iter()
        .find(|word| word.hash == hash_of("david"))
        .expect("the row holds david");
    assert_eq!(david.free_of(Form::Lower), 1);
    assert_eq!(david.free_of(Form::Title), 1);
    assert_eq!(david.free_of(Form::Upper), 1);
    assert_eq!(david.forced, 1, "the david the terminal put a capital on");
    assert_eq!(david.len, 5);
    assert!(
        observed
            .words()
            .windows(2)
            .all(|pair| pair[0].hash < pair[1].hash),
        "the lane is sorted and distinct"
    );
}

#[test]
fn an_uncased_chapter_stores_nothing_and_hashes_nothing() {
    let hebrew = row("\u{5d0}\u{5d1}\u{5d2} \u{5d3}\u{5d4}. \u{5d0}\u{5d1}\u{5d2}");
    assert!(hebrew.words().is_empty());
    assert!(!hebrew.cased());
    assert_eq!(hebrew.resident_bytes(), size_of::<WordRow>());
    assert!(row("David").cased());
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
    assert_eq!(word.free_of(Form::Lower), u16::MAX);
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
    let david = joined.get(hash_of("david")).expect("merged");
    // Both chapters' first words are forced by their own chapter start.
    assert_eq!(david.forced, 2);
    assert_eq!(david.free_of(Form::Title), 0);
    assert!(joined.cased());

    // A word is never split across a masked `\c`, so order cannot matter.
    let reversed = fold(&["David wept", "David went"]);
    assert_eq!(joined.words(), reversed.words());
    assert!(!fold(&["\u{5d0}\u{5d1}", "\u{5d2}"]).cased());
}

// ── The channel ─────────────────────────────────────────────────────────

struct Book {
    key: BookKey,
    text: String,
}

impl ProjectedBook for Book {
    fn key(&self) -> BookKey {
        self.key
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn chapters(&self) -> impl Iterator<Item = Chapter> {
        std::iter::once(
            Chapter::new(1, TextRange::new(0, self.text.len() as u32).unwrap()).unwrap(),
        )
    }

    fn verses(&self) -> impl Iterator<Item = Verse> {
        std::iter::once(Verse::new(
            VerseKey::new(1, 1, 1).unwrap(),
            TextRange::new(0, self.text.len() as u32).unwrap(),
        ))
    }
}

fn book(key: &[u8; 3], text: impl Into<String>) -> Book {
    Book {
        key: BookKey::new(*key),
        text: text.into(),
    }
}

fn judged(books: &[Book], config: &JudgingConfig) -> Vec<Pattern> {
    let corpus = Corpus::try_new(books).expect("a synthetic corpus is valid");
    analyze_with(&corpus, &Words, config).patterns().to_vec()
}

/// `David` ×40 against `david` ×2: the two are the row.
#[test]
fn the_minority_form_of_a_word_in_free_positions_fires() {
    let mut text = "and David went ".repeat(40);
    text.push_str("and david went and david went");
    let patterns = judged(&[book(b"MRK", text)], &JudgingConfig::default());
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
    assert!(judged(&[book(b"MRK", text)], &JudgingConfig::default()).is_empty());
}

/// Forced positions never reach the numerator, so `The` at the head of forty
/// sentences is not a minority of anything.
#[test]
fn a_forced_position_is_not_evidence() {
    let text = "The word. ".repeat(40) + &"and the word. ".repeat(40);
    assert!(judged(&[book(b"MRK", text)], &JudgingConfig::default()).is_empty());
}

#[test]
fn a_word_under_the_support_floor_abstains() {
    let quiet = JudgingConfig {
        word_support_floor: 100,
        ..JudgingConfig::default()
    };
    let text = "and David went ".repeat(40) + "and david went";
    assert!(judged(&[book(b"MRK", text.clone())], &quiet).is_empty());
    assert_eq!(
        judged(&[book(b"MRK", text)], &JudgingConfig::default()).len(),
        1
    );
}

#[test]
fn an_uncased_corpus_emits_nothing_and_the_channel_switches_off() {
    let hebrew = "\u{5d0}\u{5d1}\u{5d2} \u{5d3}\u{5d4}\u{5d5} ".repeat(40);
    assert!(judged(&[book(b"MRK", hebrew)], &JudgingConfig::default()).is_empty());

    let off = JudgingConfig {
        channels: Channels {
            casing: false,
            ..Channels::default()
        },
        letters: LetterRoster::Never,
        ..JudgingConfig::default()
    };
    let text = "and David went ".repeat(40) + "and david went";
    assert!(judged(&[book(b"MRK", text)], &off).is_empty());
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
    let patterns = judged(&books, &JudgingConfig::default());
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].books, 1, "only GEN holds a lowercase david");
    assert_eq!((patterns[0].numerator, patterns[0].denominator), (2, 62));
}

/// The rescan places what the counts decided, and only in free positions.
#[test]
fn locate_sites_every_free_occurrence_the_numerator_counted() {
    let text = "and David went ".repeat(40) + "and david went. david ran";
    let books = [book(b"MRK", text)];
    let corpus = Corpus::try_new(&books).unwrap();
    let findings = analyze_with(&corpus, &Words, &JudgingConfig::default());
    let pattern = findings.patterns()[0];
    assert_eq!(pattern.numerator, 1, "the second david is forced");
    let sites: Vec<_> = findings
        .rows()
        .iter()
        .map(|row| &books[0].text[row.from() as usize..row.to() as usize])
        .collect();
    assert_eq!(sites, vec!["david"]);
    let FindingKind::Convention(digest) = findings.rows()[0].kind() else {
        panic!("convention kind")
    };
    assert_eq!(digest.reasons(), Reasons::CASING);
    assert_eq!(digest.pattern().get(), 0);
    assert_eq!(free_in(&fold(&[&books[0].text]), &pattern), 1);
}

// ── The corpus tally ────────────────────────────────────────────────────

/// Five books whose words overlap in every way that matters: shared hashes,
/// hashes one book alone holds, and a word only ever in a forced position.
fn corpus_books() -> Vec<WordAggregate> {
    [
        "David went. david wept. DAVID sang.",
        "Solomon spoke. david slept.",
        "David rose and david ran and Ruth wept.",
        "Ruth. ruth. RUTH gleaned in david's field.",
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
    assert_eq!(row.free, [0; 4], "every occurrence was forced");
    assert_eq!(row.holders, 1);

    tally.remove(&[&forced]);
    assert_eq!(tally, totals_of(&[&other]));
    assert!(!tally.rows().iter().any(|row| row.hash == solomon));
}
