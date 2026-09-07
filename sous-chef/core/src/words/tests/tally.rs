//! The corpus tally: books added and removed without a re-merge.

use super::*;

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
    let empty = TerminalTable::default();
    assert_eq!(
        (u64::from(na.bare), na.separated_free(&empty), na.holders),
        (1, 1, 2)
    );

    // An uncased word is in the doubles lane alone, so the union counts it.
    let hebrew = tally
        .doubles()
        .iter()
        .find(|row| row.hash == hash_of("\u{5d0}\u{5d1}"))
        .expect("one book holds it");
    assert_eq!((hebrew.uncased, hebrew.bare), (2, 1));
    assert!(!tally.by_word().any(|word| word[0].hash == hebrew.hash));
    assert!(tally.doubling_share_bp(&empty) > 0);

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
