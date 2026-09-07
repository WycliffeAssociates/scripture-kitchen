//! The row: one chapter's words, counted by hash.

use super::*;

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
    assert_eq!(
        u32::from(hebrew.doubles()[0].bare) + hebrew.doubles()[0].separated_total(),
        0
    );
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
