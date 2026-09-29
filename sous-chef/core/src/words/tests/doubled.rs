//! The doubled channel: a repeat judged against the corpus's own habit.

use super::*;

/// The recusal off, so a fixture whose point is one word's verdict is not
/// answered by the corpus statistic instead.
/// Judged whatever the recusal says, with both keys on.
fn always(config: &JudgingConfig) -> JudgingConfig {
    JudgingConfig {
        doubles: DoublesPolicy::Always,
        doubled_separated: true,
        ..*config
    }
}

fn doubled_rows(books: &[Book], config: &JudgingConfig) -> Vec<Pattern> {
    judged(books, config)
        .into_iter()
        .filter(|row| row.channel == Channel::Doubled)
        .collect()
}

/// French `vous vous` is a construction, not a slip: a word doubled
/// `support_floor` times or more excuses itself, so no allow-list is needed.
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

/// The other half of the same rule: one `the the` stays reviewable.
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

/// A single doubling fires whatever the word's frequency: `surface` used 47
/// times sits under `word_support_floor` and in a band a 2% share never
/// reaches, and neither applies here.
#[test]
fn one_doubling_of_a_word_used_47_times_fires() {
    let mut text = "and the surface ".repeat(45);
    text.push_str("and the surface surface word");
    let books = [book(b"GEN", text)];
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
            hash: hash_of("surface"),
            separated: false,
        }
    );
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 47));
    assert_eq!(
        sited(&books, &findings, Reasons::DOUBLED_BARE),
        ["surface surface"]
    );
}

/// The habit is an absolute count per key: four doublings fire, five are the
/// language. The separated key keeps its own count.
#[test]
fn a_word_doubled_five_times_is_excused() {
    let fixture = |bare: usize| {
        let mut text = "w0 w1 w2 w3 ".repeat(100);
        text.push_str(&"na na w1 ".repeat(bare));
        text.push_str("na, na w1 ");
        [book(b"MRK", text)]
    };
    let keys = |books: &[Book]| -> Vec<bool> {
        doubled_rows(books, &always(&JudgingConfig::default()))
            .iter()
            .map(|row| match row.key {
                PatternKey::Doubled { separated, .. } => separated,
                _ => unreachable!("a doubled row"),
            })
            .collect()
    };
    assert_eq!(keys(&fixture(4)), [false, true]);
    assert_eq!(
        keys(&fixture(5)),
        [true],
        "bare is a habit, separated is not"
    );
}

/// `na, na` is a second key with its own denominator, never pooled with the
/// adjacent one: a comma between them is a different claim about the text.
#[test]
fn na_comma_na_is_a_separate_claim() {
    let mut text = "and na now ".repeat(2_000);
    text.push_str("and na, na now");
    let books = [book(b"MRK", text)];
    let config = JudgingConfig {
        doubled_separated: true,
        ..JudgingConfig::default()
    };
    let findings = analyzed(&books, &config);
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

/// Each kind of gap has its own switch, and a switch that is off emits
/// nothing for its key.
#[test]
fn the_bare_and_separated_switches_are_independent() {
    let mut text = "and na now ".repeat(100);
    text.push_str("and na na now and na, na now");
    let books = [book(b"MRK", text)];
    let keys = |bare: bool, separated: bool| -> Vec<bool> {
        let config = JudgingConfig {
            doubled_bare: bare,
            doubled_separated: separated,
            ..always(&JudgingConfig::default())
        };
        doubled_rows(&books, &config)
            .iter()
            .map(|row| match row.key {
                PatternKey::Doubled { separated, .. } => separated,
                _ => unreachable!("a doubled row"),
            })
            .collect()
    };
    assert_eq!(keys(true, true), [false, true]);
    assert_eq!(keys(true, false), [false]);
    assert_eq!(keys(false, true), [true]);
    assert!(keys(false, false).is_empty());
    assert!(JudgingConfig::default().doubled_bare);
    assert!(!JudgingConfig::default().doubled_separated);
}

/// A separated pair whose separator's context forces a capital in this
/// corpus's own terminal table is a sentence terminal, not a doubled word:
/// `go. Go` is two sentences. Learned, not listed — the same corpus with the
/// same pair but without the forcing evidence keeps the row.
#[test]
fn a_pair_across_a_sentence_terminal_is_not_a_double() {
    // Five free `go`s, none of them adjacent, plus the pair: seven total,
    // over `loose()`'s support floor of five.
    let free_go = "I go there. She will go home too. They go far away. \
                    We go near. He can go too. ";

    // Plenty of `.` evidence, all of it a capital: the period forces here.
    let mut forcing = String::new();
    for word in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta"] {
        forcing.push_str(&format!("Word. {word} thing. "));
    }
    let mut text = forcing.clone();
    text.push_str(free_go);
    text.push_str("go. Go home.");
    let books = [book(b"MRK", text)];
    assert!(
        doubled_rows(&books, &always(&loose())).is_empty(),
        "the period forces a capital here, so `go. Go` is not a double"
    );

    // The same pair and the same `go` denominator, but a corpus whose periods
    // hand off to lowercase almost always: the period does not force, and the
    // pair is a real doubling.
    let mut not_forcing = String::new();
    for word in ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"] {
        not_forcing.push_str(&format!("word. {word} thing. "));
    }
    let mut text = not_forcing;
    text.push_str(free_go);
    text.push_str("go. Go home.");
    let books = [book(b"MRK", text)];
    let rows = doubled_rows(&books, &always(&loose()));
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].key,
        PatternKey::Doubled {
            hash: hash_of("go"),
            separated: true,
        }
    );
    let findings = analyzed(&books, &always(&loose()));
    assert_eq!(
        sited(&books, &findings, Reasons::DOUBLED_SEPARATED),
        ["go. Go"]
    );
}

/// A pair whose two words lie in different verses is not a doubling: JOB
/// 20:7-8 ends one verse on `he` and starts the next on `He`.
#[test]
fn a_double_across_a_verse_boundary_does_not_count() {
    let lead = "and na now ".repeat(2_000) + "and na";
    let books = [versed(b"MRK", &[&lead, "na now"])];
    assert!(doubled_rows(&books, &always(&JudgingConfig::default())).is_empty());
}

/// Within one verse a line break is whitespace, and two segments of one verse
/// are still one verse: both pairs count, bare.
#[test]
fn a_line_break_inside_a_verse_still_counts() {
    let text = "and na now ".repeat(200) + "and na\nna now so so";
    let books = [book(b"PSA", text.clone())];
    let findings = analyzed(&books, &always(&JudgingConfig::default()));
    assert_eq!(
        sited(&books, &findings, Reasons::DOUBLED_BARE),
        ["na\nna", "so so"]
    );

    let seam = text.len() as u32 - 2;
    let key = VerseKey::new(1, 1, 1).unwrap();
    let segmented = Book {
        verses: vec![
            Verse::new(key, TextRange::new(0, seam).unwrap()),
            Verse::new(key, TextRange::new(seam, text.len() as u32).unwrap()),
        ],
        ..book(b"PSA", text)
    };
    let books = [segmented];
    let findings = analyzed(&books, &always(&JudgingConfig::default()));
    assert_eq!(
        sited(&books, &findings, Reasons::DOUBLED_BARE),
        ["na\nna", "so so"]
    );
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
