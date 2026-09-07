//! The walk: what counts as a word, its form, and what stood before it.

use super::*;

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

/// A verse that starts inside a word: the word already stood, so the verse
/// start belongs to no word and the NEXT one is not a verse-start capital.
#[test]
fn a_verse_start_inside_a_word_does_not_travel_to_the_next_one() {
    let text = "onetwo Three";
    let verse = |number, from, to| {
        Verse::new(
            VerseKey::new(1, number, number).unwrap(),
            TextRange::new(from, to).unwrap(),
        )
    };
    // Verse 2 opens at the `t` of `two`, three bytes into the first word.
    let verses = [verse(1, 0, 3), verse(2, 3, 12)];
    let mut before = Vec::new();
    for_each_word(text, &verses, |word| before.push(word.before));
    assert_eq!(before, vec![Before::Start, Before::None]);
}
