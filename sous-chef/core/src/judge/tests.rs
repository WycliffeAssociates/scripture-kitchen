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
        support_floor: 2,
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
        assert_eq!(
            books_touched(&views, pattern, &config),
            pattern.books,
            "{pattern:?}"
        );
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

// ── Both sides ──────────────────────────────────────────────────────────

/// Judges one book per text under `config`.
fn judged(texts: &[String], config: &JudgingConfig) -> Findings {
    use crate::pass::ChapterObs;
    use crate::substrate::{Edge, fold_book, walk};

    let rows: Vec<_> = texts.iter().map(|text| walk::walk(text, &[])).collect();
    let aggregates: Vec<BookAggregate> = rows
        .iter()
        .map(|obs| fold_book(&[ChapterObs { start: 0, obs }], &mut Edge::default()))
        .collect();
    let views: Vec<&BookAggregate> = aggregates.iter().collect();
    let mut findings = Findings::new(texts.iter().map(|text| text.len() as u32).collect());
    judge_corpus(&views, config, &mut findings);
    findings
}

fn placement_row(
    findings: &Findings,
    glyph: char,
    side: Side,
    class: OuterClass,
) -> Option<(u32, u32, u8)> {
    findings
        .patterns()
        .iter()
        .find(|row| {
            row.glyph == ScalarKey::of(glyph) && row.key == PatternKey::Placement { side, class }
        })
        .map(|row| (row.numerator, row.denominator, row.books))
}

/// `),` ten times is `)`'s to judge; the one `],` has an unentitled leader.
#[test]
fn a_pair_an_entitled_leader_judges_leaves_the_placement_numerator() {
    let texts = [
        "a, b ".repeat(2_000),
        format!("{}c], d", "(x), ".repeat(10)),
    ];
    let findings = judged(&texts, &JudgingConfig::default());
    assert_eq!(
        placement_row(&findings, ',', Side::Prev, OuterClass::Nonletter),
        Some((1, 2_011, 1)),
        "only the `],` is left, against every comma"
    );
    assert_eq!(findings.explained().leaders(), &[ScalarKey::of(')')]);

    let without = judged(
        &texts[..1]
            .iter()
            .cloned()
            .chain([format!("{}c, d", "(x), ".repeat(10))])
            .collect::<Vec<_>>(),
        &JudgingConfig::default(),
    );
    assert_eq!(
        placement_row(&without, ',', Side::Prev, OuterClass::Nonletter),
        None,
        "every comma after a mark is explained, so the row is silent"
    );
}

/// With ExactNeighbor off, nothing judges the pair, so Placement keeps it.
#[test]
fn a_leader_whose_channel_is_off_explains_nothing() {
    let texts = ["a, b ".repeat(2_000), "(x), ".repeat(10)];
    let mut config = JudgingConfig::default();
    config.channels.exact_neighbor = false;
    let findings = judged(&texts, &config);
    assert_eq!(
        placement_row(&findings, ',', Side::Prev, OuterClass::Nonletter),
        Some((10, 2_010, 1))
    );
    assert!(findings.explained().leaders().is_empty());
}

/// `7.` is 20 of 5,020 periods but 20 of 30 number ends: ordinary there.
#[test]
fn a_digit_row_ordinary_among_number_ends_is_silent() {
    let texts = [
        format!("{}{}", "a. ".repeat(5_000), "7. ".repeat(20)),
        "7 ".repeat(10),
    ];
    let findings = judged(&texts, &JudgingConfig::default());
    assert_eq!(
        placement_row(&findings, '.', Side::Prev, OuterClass::Digit),
        None
    );

    // Against 10,020 number ends the same 20 are 19 bp, under band 4's 30.
    let texts = [
        format!("{}{}", "a. ".repeat(5_000), "7. ".repeat(20)),
        "7 ".repeat(10_000),
    ];
    let findings = judged(&texts, &JudgingConfig::default());
    assert_eq!(
        placement_row(&findings, '.', Side::Prev, OuterClass::Digit),
        Some((20, 5_020, 1))
    );
}

/// `);` eight times is a convention; the three `;'` are what is left.
#[test]
fn a_run_that_recurs_exactly_leaves_the_run_shape_numerator() {
    let shape = PatternKey::RunShape {
        pure: false,
        bucket: 2,
    };
    let row = |findings: &Findings| {
        findings
            .patterns()
            .iter()
            .find(|row| row.glyph == ScalarKey::of(';') && row.key == shape)
            .map(|row| (row.numerator, row.denominator, row.books))
    };
    let texts = [
        "a; b ".repeat(3_000),
        format!("{}{}", "(x); ".repeat(8), "c;' d ".repeat(3)),
    ];
    let findings = judged(&texts, &JudgingConfig::default());
    assert_eq!(row(&findings), Some((3, 3_011, 1)));
    let cluster: Box<[ScalarKey]> = [ScalarKey::of(')'), ScalarKey::of(';')].into();
    assert_eq!(findings.explained().clusters(), &[cluster]);

    let texts = ["a; b ".repeat(3_000), "(x); ".repeat(8)];
    let findings = judged(&texts, &JudgingConfig::default());
    assert_eq!(row(&findings), None, "every mixed pair recurs");
}

// ── What is usual instead ───────────────────────────────────────────────

/// Each row names the corpus's own majority beside its minority.
#[test]
fn every_row_names_what_is_usual_instead() {
    let texts = [format!(
        "{}{}{}{}{} \u{2019}",
        "a, b ".repeat(2_000),
        "c ,d ".repeat(3),
        "e.' ".repeat(975),
        "f'. ".repeat(3),
        "g'\" ".repeat(400),
    )];
    let config = JudgingConfig {
        letters: LetterRoster::Never,
        ..JudgingConfig::default()
    };
    let findings = judged(&texts, &config);
    let usual = |glyph: char, key: PatternKey| {
        findings
            .patterns()
            .iter()
            .find(|row| row.glyph == ScalarKey::of(glyph) && row.key == key)
            .map(|row| row.usual)
    };
    assert_eq!(
        usual(
            ',',
            PatternKey::Placement {
                side: Side::Prev,
                class: OuterClass::Space
            }
        ),
        Some(Usual::Placement {
            class: OuterClass::Letter,
            count: 2_000,
        })
    );
    assert_eq!(
        usual('\'', PatternKey::ExactNeighbor(ScalarKey::of('.'))),
        Some(Usual::ExactNeighbor {
            neighbor: ScalarKey::of('"'),
            count: 400,
            reversed: 975,
        }),
        "`'` is usually followed by `\"`, and `.'` is the swap of `'.`"
    );
    assert_eq!(
        usual('\u{2019}', PatternKey::Rarity),
        Some(Usual::Rarity {
            glyph: Some(ScalarKey::of('\'')),
            count: 1_378,
        }),
        "the most common other Quote"
    );
}
