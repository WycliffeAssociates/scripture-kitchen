//! The casing and word-length channels over the folded rows.

use super::*;

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
