//! The doubles lane: a word repeated, adjacent or separated.

use super::*;

/// One word's doubles row, or the zero row when the chapter holds none.
fn doubles_of(text: &str, word: &str) -> DoubleCount {
    let hash = hash_of(word);
    row(text)
        .doubles()
        .iter()
        .find(|row| row.hash == hash)
        .cloned()
        .unwrap_or(DoubleCount::new(hash))
}

#[test]
fn the_double_count_is_thirty_two_bytes() {
    assert_eq!(size_of::<DoubleCount>(), 32);
}

/// Two claims, kept apart, and compared by the case fold: `The the` is a
/// double.
#[test]
fn adjacent_and_separated_doubles_are_different_counters() {
    assert_eq!(doubles_of("go go on", "go").bare, 1);
    assert_eq!(doubles_of("go go on", "go").separated_total(), 0);
    assert_eq!(doubles_of("na, na now", "na").separated_total(), 1);
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
    assert_eq!(doubles_of("na 3 na", "na").separated_total(), 0);
    assert_eq!(doubles_of("na 3 na", "na").bare, 0);
    // `a--b` is two words, so `go--go` is a separated double.
    assert_eq!(doubles_of("go--go on", "go").separated_total(), 1);
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
    // No doubled word and no repeated letter, so the other two lanes are
    // empty and the row is its casing rows plus the inline header.
    let observed = row("one two six");
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
