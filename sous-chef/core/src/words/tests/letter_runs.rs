//! The letter-run channel: a letter repeated inside one word.

use super::*;

/// The rows this corpus fires on the letter-run channel. `judged` reads the
/// three hash-keyed channels; this one keys a real scalar.
fn sticky_rows(books: &[Book], config: &JudgingConfig) -> Vec<Pattern> {
    analyzed(books, config)
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::LetterRun)
        .copied()
        .collect()
}

/// The runs the walk sees inside one word, as `(letter, length)`.
fn runs_in(word: &str) -> Vec<(char, u8)> {
    let mut out = Vec::new();
    for_each_letter_run(word, |letter, length| {
        out.push((letter.scalar().expect("a letter is a scalar"), length));
    });
    out
}

/// The claim in one line: `theee` against two thousand ordinary `ee`, at the
/// shipped ladder, on the letter's own repeat history.
#[test]
fn theee_fires_where_ee_is_common() {
    let text = "the tree stood ".repeat(2_000) + "and theee end";
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &JudgingConfig::default());
    let rows: Vec<Pattern> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::LetterRun)
        .copied()
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].glyph, ScalarKey::of('e'));
    assert_eq!(rows[0].key, PatternKey::LetterRun { length: 3 });
    // 2,000 `ee` in `tree`, and the one `eee`.
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 2_001));
    assert_eq!(rows[0].books, 1);
    // `oo` in `stood` is two thousand runs of length two and no more, so the
    // letter that only ever doubles says nothing.
    assert!(!rows.iter().any(|row| row.glyph == ScalarKey::of('o')));
    // The site is the word the run sits inside, not the run.
    assert_eq!(sited(&books, &findings, Reasons::LETTER_RUN), ["theee"]);
}

/// A language that doubles vowels everywhere is judged against its own habit:
/// `aa` is never a row however common, and one `aaa` still is.
#[test]
fn a_language_that_doubles_vowels_is_silent() {
    let text = "maa tee ".repeat(2_000) + "maaa loppu";
    let books = [book(b"MRK", text)];
    let rows = sticky_rows(&books, &JudgingConfig::default());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].glyph, ScalarKey::of('a'));
    assert_eq!(rows[0].key, PatternKey::LetterRun { length: 3 });
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 2_001));
    // Two thousand `ee`, and the channel has nothing to say about them.
    assert!(!rows.iter().any(|row| row.glyph == ScalarKey::of('e')));
}

/// A letter the corpus barely repeats carries no history to be judged
/// against, and a length whose shorter runs are unattested is not evidence
/// either: `xxxx` where no `xxx` was ever written says nothing about `xxxx`.
#[test]
fn a_letter_with_no_repeat_history_says_nothing() {
    let base = "the tree stood ".repeat(2_000);
    let thin = base.clone() + "a boxx and a boxxx";
    let books = [book(b"MRK", thin)];
    assert!(
        sticky_rows(&books, &JudgingConfig::default()).is_empty(),
        "two runs of `x` are under the word support floor"
    );

    // The support gate on its own: two thousand `xx`, so the denominator and
    // the band would both pass, but the corpus never wrote `xxx`.
    let gapped = base + &"a boxx ".repeat(2_000) + "a boxxxx";
    let books = [book(b"MRK", gapped)];
    let rows = sticky_rows(&books, &JudgingConfig::default());
    assert!(
        rows.is_empty(),
        "no `xxx` stands between `xx` and `xxxx`: {rows:?}"
    );
}

/// The doc example, and the disposition it claims: an emphatic spelling FIRES
/// — it is a row for review, not a verdict — where the corpus has established
/// both shorter runs of that letter.
#[test]
fn emphasis_aaaah_is_reviewable_not_convicted() {
    let text = "haah ".repeat(25) + &"haaah ".repeat(25) + "haaaah";
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &loose());
    let rows: Vec<Pattern> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::LetterRun)
        .copied()
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].glyph, ScalarKey::of('a'));
    assert_eq!(rows[0].key, PatternKey::LetterRun { length: 4 });
    // 25 `aa`, 25 `aaa`, one `aaaa`: the row is one of fifty-one.
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 51));
    assert_eq!(sited(&books, &findings, Reasons::LETTER_RUN), ["haaaah"]);
}

/// Glue rides its base (charter invariant 8), so a combining mark neither
/// breaks a run nor lengthens it. A digit and the one nonletter a word rides
/// through do break one.
#[test]
fn glue_inside_a_repeat_does_not_break_it() {
    assert_eq!(runs_in("the\u{301}ee"), vec![('e', 3)]);
    assert_eq!(runs_in("thee"), vec![('e', 2)]);
    assert_eq!(runs_in("Eel"), vec![('e', 2)], "the same simple fold");
    assert_eq!(runs_in("the"), vec![]);
    assert_eq!(runs_in("e3e"), vec![], "a digit breaks the run");
    assert_eq!(runs_in("co-op"), vec![], "so does the joiner it rode");
    assert_eq!(runs_in("baaaaaaaaad"), vec![('a', 8)], "saturating");
    assert_eq!(runs_in("aabbaa"), vec![('a', 2), ('b', 2), ('a', 2)]);
}

/// An uncased script is in no casing row at all, and repeats there are
/// letters like any other: the lane counts them and the rescan sites them.
#[test]
fn uncased_scripts_count() {
    let text = "\u{5d1}\u{5d0}\u{5d0} ".repeat(40) + "\u{5d1}\u{5d0}\u{5d0}\u{5d0}";
    let books = [book(b"MRK", text)];
    let findings = analyzed(&books, &loose());
    let rows: Vec<Pattern> = findings
        .patterns()
        .iter()
        .filter(|row| row.channel == Channel::LetterRun)
        .copied()
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].glyph, ScalarKey::of('\u{5d0}'));
    assert_eq!(rows[0].key, PatternKey::LetterRun { length: 3 });
    assert_eq!((rows[0].numerator, rows[0].denominator), (1, 41));
    assert!(judged(&books, &loose()).is_empty(), "no cased letter");
    assert_eq!(
        sited(&books, &findings, Reasons::LETTER_RUN),
        ["\u{5d1}\u{5d0}\u{5d0}\u{5d0}"]
    );
}

/// The channel switches off, and its lane is still walked: the config decides
/// judging alone.
#[test]
fn the_letter_run_channel_ships_on_and_switches_off() {
    assert!(JudgingConfig::default().channels.letter_runs);
    let text = "the tree stood ".repeat(2_000) + "and theee end";
    let books = [book(b"MRK", text)];
    let off = JudgingConfig {
        channels: Channels {
            letter_runs: false,
            ..Channels::default()
        },
        ..JudgingConfig::default()
    };
    assert!(sticky_rows(&books, &off).is_empty());
    assert_eq!(sticky_rows(&books, &JudgingConfig::default()).len(), 1);
}

/// The corpus tally carries the lane too, so a resident host judges it from
/// the two updates instead of merging every book again.
#[test]
fn the_tally_carries_the_letter_run_lane_through_both_updates() {
    let first = fold(&["a tree stood"]);
    let second = fold(&["theee"]);
    let both = totals_of(&[&first, &second]);
    let row = both
        .letter_runs()
        .iter()
        .find(|row| row.letter == ScalarKey::of('e'))
        .expect("both books repeat it");
    assert_eq!(row.lengths[0], 1, "one `ee`");
    assert_eq!(row.lengths[1], 1, "one `eee`");
    assert_eq!((row.holders, row.runs()), (2, 2));

    let mut built = WordTotals::default();
    built.add(&[&first]);
    built.add(&[&second]);
    assert_eq!(built, both);
    built.remove(&[&second]);
    assert_eq!(built, totals_of(&[&first]));
}
