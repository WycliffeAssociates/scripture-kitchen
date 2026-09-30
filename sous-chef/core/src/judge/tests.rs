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

/// `."'"` x3, `?"'"` and `!"'"` are `T"'"` x5 with the sentence ends read as
/// one, so all three recur; `.'?"` keeps another order and is what is left.
/// `;` is a separator, not a terminal, so `;'` pools with nothing and fires.
#[test]
fn a_run_recurs_when_its_sentence_ends_pooled_recur() {
    let texts = [
        "a\" b; c ".repeat(3_000),
        format!(
            "{}{}{}",
            "go.\"'\" x ".repeat(3),
            "go?\"'\" y go!\"'\" z go.'?\" w ",
            "(x); ".repeat(8) + &"c;' d ".repeat(3)
        ),
    ];
    let findings = judged(&texts, &JudgingConfig::default());
    let row = |glyph: char, bucket: u8| {
        findings.patterns().iter().position(|row| {
            row.glyph == ScalarKey::of(glyph)
                && row.key
                    == PatternKey::RunShape {
                        pure: false,
                        bucket,
                    }
        })
    };
    let quote = row('"', 4).expect("`.'?\"` is left");
    let pattern = &findings.patterns()[quote];
    assert_eq!((pattern.numerator, pattern.denominator), (1, 3_006));
    let listed: Vec<(String, u32, bool)> = findings
        .clusters()
        .iter()
        .filter(|cluster| usize::from(cluster.pattern.get()) == quote)
        .map(|cluster| {
            let text = cluster.atoms.iter().filter_map(|atom| atom.scalar()).collect();
            (text, cluster.count, cluster.recurring)
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            (".\"'\"".to_string(), 3, true),
            ("!\"'\"".to_string(), 1, true),
            (".'?\"".to_string(), 1, false),
            ("?\"'\"".to_string(), 1, true),
        ]
    );
    let semicolon = &findings.patterns()[row(';', 2).expect("`;'` still fires")];
    assert_eq!(semicolon.numerator, 3);
}

/// A row lists its clusters: the swap it counts beside the order that recurs.
#[test]
fn a_run_shape_row_lists_its_clusters() {
    let texts = [
        "a; b ".repeat(3_000),
        format!("{}{}", "c'; d ".repeat(7), "e;' f ".repeat(3)),
    ];
    let findings = judged(&texts, &JudgingConfig::default());
    let listed: Vec<(String, u32, bool)> = findings
        .clusters()
        .iter()
        .map(|cluster| {
            let row = &findings.patterns()[usize::from(cluster.pattern.get())];
            assert_eq!(row.glyph, ScalarKey::of(';'));
            let text = cluster
                .atoms
                .iter()
                .filter_map(|atom| atom.scalar())
                .collect();
            (text, cluster.count, cluster.recurring)
        })
        .collect();
    assert_eq!(
        listed,
        vec![("';".to_string(), 7, true), (";'".to_string(), 3, false)]
    );
}

/// A run past the atoms a cluster keeps is cut there and says so.
#[test]
fn a_long_run_is_listed_truncated() {
    let texts = [format!("{}x{} y", "a. b ".repeat(3_000), ".".repeat(20))];
    let findings = judged(&texts, &JudgingConfig::default());
    let long = findings
        .clusters()
        .iter()
        .find(|cluster| cluster.truncated)
        .expect("the twenty-period run is listed");
    assert_eq!(long.atoms.len(), Cluster::ATOMS);
    assert_eq!(long.count, 1);
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

/// A rare letter, or a rare mark no named pool holds, has no kin to name: the
/// space and the letters that share `Pool::Other` with it are not usual marks.
#[test]
fn a_rarity_usual_is_never_a_letter_or_a_space() {
    let texts = [format!("{} Q \u{a7}", "ab cd. ".repeat(2_000))];
    let config = JudgingConfig {
        letters: LetterRoster::Always,
        ..JudgingConfig::default()
    };
    let findings = judged(&texts, &config);
    let usual = |glyph: char| {
        findings
            .patterns()
            .iter()
            .find(|row| row.glyph == ScalarKey::of(glyph) && row.channel == Channel::Rarity)
            .map(|row| row.usual)
    };
    let none = Some(Usual::Rarity {
        glyph: None,
        count: 0,
    });
    assert_eq!(usual('Q'), none, "a letter");
    assert_eq!(usual('\u{a7}'), none, "`§` pools as Other");
}

// ── One book breaks from the rest ───────────────────────────────────────

/// A book of `commas` commas, `spaced` of them after a space.
fn comma_book(commas: usize, spaced: usize) -> String {
    format!(
        "{}{}",
        "a, b ".repeat(commas - spaced),
        "a ,b ".repeat(spaced)
    )
}

/// `(book, numerator, denominator, usual)` of every `, prev=Space` book row.
fn book_rows(findings: &Findings) -> Vec<(u16, u32, u32, Usual)> {
    findings
        .patterns()
        .iter()
        .filter_map(|row| match row.key {
            PatternKey::BookRate {
                side: Side::Prev,
                class: OuterClass::Space,
                book,
            } if row.glyph == ScalarKey::of(',') => {
                Some((book.get(), row.numerator, row.denominator, row.usual))
            }
            _ => None,
        })
        .collect()
}

/// nya 1SA's shape: one book writes most of its commas after a space.
#[test]
fn a_book_whose_rate_breaks_from_the_rest_fires() {
    let mut texts: Vec<String> = (0..5).map(|_| comma_book(200, 1)).collect();
    texts.insert(2, comma_book(1_526, 1_183));
    let findings = judged(&texts, &JudgingConfig::default());
    assert_eq!(
        book_rows(&findings),
        vec![(
            2,
            1_183,
            1_526,
            Usual::BookRate {
                baseline_bp: 50,
                books: 5,
            }
        )]
    );
    let row = findings
        .patterns()
        .iter()
        .find(|row| row.channel == Channel::BookRate)
        .unwrap();
    assert_eq!((row.band, row.books, row.share_bp), (None, 1, 7_752));
}

/// Three judged books are one under test and two to compare: too few.
#[test]
fn under_four_judged_books_nothing_fires() {
    let texts = [
        comma_book(1_526, 1_183),
        comma_book(200, 1),
        comma_book(200, 1),
        // Four books, but this one holds too few commas to be judged.
        comma_book(4, 0),
    ];
    assert!(book_rows(&judged(&texts, &JudgingConfig::default())).is_empty());
}

/// 5% against 0.1% is fifty times the baseline, and still under 40%.
#[test]
fn a_rate_under_the_minimum_is_silent() {
    let mut texts: Vec<String> = (0..4).map(|_| comma_book(1_000, 1)).collect();
    texts.push(comma_book(100, 5));
    assert!(book_rows(&judged(&texts, &JudgingConfig::default())).is_empty());
    let config = JudgingConfig {
        book_rate_min_bp: 400,
        ..JudgingConfig::default()
    };
    assert_eq!(book_rows(&judged(&texts, &config)).len(), 1);
}

/// 80% of 99 commas is a book too small to fire, and the same book at 100
/// fires. A book under `book_rate_min_uses` still sits in the others' median.
#[test]
fn a_book_under_the_minimum_uses_is_silent_but_still_compared() {
    let mut texts: Vec<String> = (0..4).map(|_| comma_book(200, 1)).collect();
    texts.push(comma_book(99, 80));
    assert!(book_rows(&judged(&texts, &JudgingConfig::default())).is_empty());
    texts.push(comma_book(100, 80));
    assert_eq!(
        book_rows(&judged(&texts, &JudgingConfig::default())),
        vec![(
            5,
            80,
            100,
            Usual::BookRate {
                baseline_bp: 50,
                books: 5,
            }
        )],
        "the 99-comma book is one of the five it is compared against"
    );
}

/// With the tested book in its own baseline the median of 1%, 1%, 20% and 50%
/// is 10.5%, and 50% is not ten times that. Left out, the median is 1%.
#[test]
fn the_baseline_leaves_the_tested_book_out() {
    let texts = [
        comma_book(100, 50),
        comma_book(100, 1),
        comma_book(100, 1),
        comma_book(100, 20),
    ];
    let rows = book_rows(&judged(&texts, &JudgingConfig::default()));
    assert!(rows.contains(&(
        0,
        50,
        100,
        Usual::BookRate {
            baseline_bp: 100,
            books: 3,
        }
    )));
}
