//! Instrument: SHAPES — hand-built aggregates, chosen so one channel fires
//! at a time.

use super::*;

#[test]
fn the_default_staircase_is_the_rule_documents_table() {
    let bands = Staircase::default();
    assert_eq!(bands.band_for(0), None);
    assert_eq!(bands.band_for(1), Some((0, 2_500)));
    assert_eq!(bands.band_for(10), Some((0, 2_500)));
    assert_eq!(bands.band_for(11), Some((1, 1_000)));
    assert_eq!(bands.band_for(100), Some((1, 1_000)));
    assert_eq!(bands.band_for(1_000), Some((2, 300)));
    assert_eq!(bands.band_for(10_000), Some((3, 100)));
    assert_eq!(bands.band_for(10_001), Some((4, 30)));
    assert_eq!(bands.band_for(u32::MAX), Some((4, 30)));
}

#[test]
fn a_staircase_whose_bounds_do_not_ascend_is_refused() {
    let mut steps = Staircase::DEFAULT_STEPS;
    steps.swap(1, 2);
    assert_eq!(Staircase::new(steps), None);
    let mut open = Staircase::DEFAULT_STEPS;
    open[4].up_to = 10_000_000;
    assert_eq!(Staircase::new(open), None);
    assert_eq!(
        Staircase::new(Staircase::DEFAULT_STEPS),
        Some(Staircase::default())
    );
}

/// `books_touched` recounts dispersion from the retained aggregates; the
/// merge that produced each numerator must agree with it row for row.
#[test]
fn books_touched_is_the_oracle_for_the_merge_time_count() {
    use crate::pass::{ChapterObs, Findings};
    use crate::substrate::{Edge, fold_book, walk};

    let texts = [
        format!("{}c;d ,,, 12,345", "a; b ".repeat(40)),
        "e;f ... `rare` ;; 7,8 quiz".to_string(),
        "no punctuation at all in this one".to_string(),
        "x; y; z, w ,, q?. r?\" s".to_string(),
    ];
    let rows: Vec<_> = texts.iter().map(|text| walk::walk(text, &[])).collect();
    let aggregates: Vec<BookAggregate> = rows
        .iter()
        .map(|obs| fold_book(&[ChapterObs { start: 0, obs }], &mut Edge::default()))
        .collect();
    let views: Vec<&BookAggregate> = aggregates.iter().collect();
    let config = JudgingConfig {
        support_floor: 1,
        letters: LetterRoster::Always,
        ..JudgingConfig::default()
    };
    let mut findings = Findings::new(texts.iter().map(|text| text.len() as u32).collect());
    judge_corpus(&views, &config, &mut findings);
    assert!(
        findings.patterns().len() > 20,
        "the sample must judge something: {} rows",
        findings.patterns().len()
    );
    let mut dispersed = 0;
    for pattern in findings.patterns() {
        assert_eq!(books_touched(&views, pattern), pattern.books, "{pattern:?}");
        dispersed += usize::from(pattern.books > 1);
    }
    assert!(dispersed > 0, "no pattern reached two books");
}

/// A ten-million-count corpus stays inside its wire widths.
#[test]
fn a_ten_million_count_corpus_does_not_wrap() {
    assert_eq!(share_bp(10_000_000, 10_000_000), 10_000);
    assert_eq!(share_bp(1, 10_000_000), 0);
    assert_eq!(share_bp(3_000, 10_000_000), 3);
    assert_eq!(share_bp(u64::MAX, 1), 10_000);
    assert_eq!(saturate(u64::from(u32::MAX) + 1), u32::MAX);
    assert_eq!(saturate(10_000_000), 10_000_000);
}
