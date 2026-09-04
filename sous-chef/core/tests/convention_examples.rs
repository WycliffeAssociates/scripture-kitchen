//! The required examples of `rules/character-inventory.md`, pinned at claim
//! level over synthetic corpora.
//!
//! Each corpus is one book of one chapter, so what a test builds is exactly
//! what the judge sees. The assertions name a pattern's channel, key, and
//! fraction and nothing else: the emission order is `judge.md`'s business and
//! the wire layout is the codec's.

use sous_core::{
    BookKey, Channel, Chapter, Corpus, JudgingConfig, LetterRoster, Pattern, PatternKey,
    ProjectedBook, ScalarKey, Side, Substrate, TextRange, Verse, VerseKey, analyze_with,
    substrate::OuterClass,
};

// ── The harness ─────────────────────────────────────────────────────────

struct Book {
    key: BookKey,
    text: String,
    chapters: Vec<Chapter>,
    verses: Vec<Verse>,
}

impl ProjectedBook for Book {
    fn key(&self) -> BookKey {
        self.key
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn chapters(&self) -> impl Iterator<Item = Chapter> {
        self.chapters.iter().copied()
    }

    fn verses(&self) -> impl Iterator<Item = Verse> {
        self.verses.iter().copied()
    }
}

fn book(key: &[u8; 3], text: String) -> Book {
    let len = u32::try_from(text.len()).expect("a synthetic corpus is small");
    let span = TextRange::new(0, len).unwrap();
    Book {
        key: BookKey::new(*key),
        text,
        chapters: vec![Chapter::new(1, span).unwrap()],
        verses: vec![Verse::new(VerseKey::new(1, 1, 1).unwrap(), span)],
    }
}

fn patterns_with(text: impl Into<String>, config: &JudgingConfig) -> Vec<Pattern> {
    let books = vec![book(b"MRK", text.into())];
    let corpus = Corpus::try_new(&books).expect("a one-book corpus is valid");
    analyze_with(&corpus, &Substrate, config)
        .patterns()
        .to_vec()
}

fn patterns(text: impl Into<String>) -> Vec<Pattern> {
    patterns_with(text, &JudgingConfig::default())
}

/// Every pattern for one glyph on one channel.
fn on(rows: &[Pattern], glyph: char, channel: Channel) -> Vec<Pattern> {
    rows.iter()
        .filter(|row| row.glyph == ScalarKey::of(glyph) && row.channel == channel)
        .copied()
        .collect()
}

fn rostered(rows: &[Pattern], glyph: char) -> bool {
    !on(rows, glyph, Channel::Rarity).is_empty()
}

/// Every roster row whose glyph is a letter.
fn letter_roster(rows: &[Pattern]) -> Vec<char> {
    rows.iter()
        .filter(|row| row.channel == Channel::Rarity)
        .filter_map(|row| row.glyph.scalar())
        .filter(|scalar| sous_core::unicode::class_of(*scalar).is_alphabetic())
        .collect()
}

fn judged(rows: &[Pattern]) -> Vec<Pattern> {
    rows.iter()
        .filter(|row| row.channel != Channel::Rarity)
        .copied()
        .collect()
}

// ── Convention stays silent ─────────────────────────────────────────────

/// A word-medial apostrophe with a letter either side is what the language
/// does, whatever its count.
#[test]
fn a_medial_apostrophe_used_everywhere_is_convention_and_silent() {
    let rows = patterns("ng'ombe ".repeat(851).trim_end());
    assert!(on(&rows, '\'', Channel::Placement).is_empty());
    assert!(on(&rows, '\'', Channel::RunShape).is_empty());
    assert!(!rostered(&rows, '\''));
}

/// French guillemet spacing, Amharic punctuation, and a glottal-stop
/// apostrophe: no allow-list, no rows.
#[test]
fn conventions_self_abstain_without_allow_lists() {
    let french = format!(
        "{}fin",
        "Il a dit \u{ab}\u{a0}mot\u{a0}\u{bb} ici. ".repeat(20)
    );
    assert_eq!(judged(&patterns(french)), Vec::new());

    // U+1361 wordspace between words, U+1362 full stop at the end.
    let amharic = format!(
        "{}\u{1240}\u{120d}",
        "\u{1240}\u{120d}\u{1361}\u{1240}\u{120d}\u{1362} ".repeat(20)
    );
    assert_eq!(judged(&patterns(amharic)), Vec::new());

    let glottal = format!("{}mwisho", "ng'ombe wa ng'ambo ".repeat(20));
    assert_eq!(judged(&patterns(glottal)), Vec::new());
}

/// An ellipsis-writing corpus has its own run history, and it excuses the
/// three dots.
#[test]
fn an_ellipsis_corpus_excuses_three_dots() {
    let text = format!("{}{}end", "a... ".repeat(300), "b. ".repeat(3_000));
    let rows = patterns(text);
    assert!(on(&rows, '.', Channel::RunShape).is_empty());
}

// ── Slips fire ──────────────────────────────────────────────────────────

/// `word?.` fires on the exact pair even though both glyphs are common,
/// because the pair's own opportunity set is strong.
#[test]
fn an_exact_pair_fires_when_its_opportunity_set_is_strong() {
    let text = format!("{}{}end", "a?. ".repeat(3), "b?\" ".repeat(400));
    let rows = patterns(text);
    let fired = on(&rows, '?', Channel::ExactNeighbor);
    assert_eq!(
        fired
            .iter()
            .map(|row| (
                row.key,
                row.numerator,
                row.denominator,
                row.band,
                row.share_bp
            ))
            .collect::<Vec<_>>(),
        vec![(
            PatternKey::ExactNeighbor(ScalarKey::of('.')),
            3,
            403,
            Some(2),
            74
        )]
    );
}

/// `,..,` against six hundred lone commas fires on run length.
#[test]
fn a_run_shape_fires_when_placement_is_ordinary() {
    let text = format!("{}c,..,d", "a, ".repeat(600));
    let rows = patterns(text);
    let fired = on(&rows, ',', Channel::RunShape);
    assert_eq!(
        fired
            .iter()
            .map(|row| (row.key, row.numerator, row.denominator))
            .collect::<Vec<_>>(),
        vec![(
            PatternKey::RunShape {
                pure: false,
                bucket: 4
            },
            1,
            601
        )]
    );
}

/// The common comma has no entitled exact-pair evidence, so the rare member
/// carries the pair through the roster.
#[test]
fn a_rare_member_carries_the_pair() {
    let text = format!("{}b,`c", "a, ".repeat(5_000));
    let rows = patterns(text);
    assert!(rostered(&rows, '`'));
    assert!(
        on(&rows, ',', Channel::ExactNeighbor).is_empty(),
        "one in-run position is under the support floor"
    );
}

/// A 1-of-6 comma is a cheap review rather than a small-sample silence.
#[test]
fn a_ten_verse_draft_judges_at_the_small_band() {
    let rows = patterns("aa, bb, cc, dd, ee, ff,gg");
    let fired = on(&rows, ',', Channel::Placement);
    assert_eq!(
        fired
            .iter()
            .map(|row| (
                row.key,
                row.numerator,
                row.denominator,
                row.band,
                row.share_bp
            ))
            .collect::<Vec<_>>(),
        vec![(
            PatternKey::Placement {
                side: Side::Next,
                class: OuterClass::Letter
            },
            1,
            6,
            Some(0),
            1_666
        )]
    );
}

/// Under the support floor a channel abstains; rarity owns the roster there.
#[test]
fn four_commas_abstain_under_the_support_floor() {
    let rows = patterns("aa, bb, cc, dd,ee");
    assert!(on(&rows, ',', Channel::Placement).is_empty());
}

// ── The rarity roster ───────────────────────────────────────────────────

/// Low Line used 28 times is not rare; `}` used twice is.
#[test]
fn a_glyph_used_twice_is_rostered_and_low_line_used_28_times_is_not() {
    let text = format!(
        "{}{}end",
        "word _ word ".repeat(28),
        "word } word ".repeat(2)
    );
    let rows = patterns(text);
    assert!(rostered(&rows, '}'));
    assert!(!rostered(&rows, '_'));
}

/// In a ten-verse draft `q`, `x`, `z` are rare by sample size, not by
/// convention; the same text at chapter scale rosters a genuine stray.
#[test]
fn a_ten_verse_draft_rosters_no_letters() {
    let verse = "and the people of the land heard the word of the prophet in that day ";
    let draft = format!("{}quiz box zeal", verse.repeat(8));
    assert!(draft.chars().filter(|c| c.is_alphabetic()).count() < 5_000);
    assert_eq!(letter_roster(&patterns(draft)), Vec::<char>::new());

    let long = format!("{}z", verse.repeat(120));
    assert!(long.chars().filter(|c| c.is_alphabetic()).count() >= 5_000);
    assert_eq!(letter_roster(&patterns(long)), vec!['z']);
}

/// A thirteen-letter alphabet rosters its stray; three thousand distinct
/// letters would be noise, so above the bound the roster abstains.
#[test]
fn letters_join_the_roster_for_an_alphabet_and_leave_it_above_the_bound() {
    // Hawaiian: five vowels, seven consonants, the okina.
    let hawaiian = "ka mea ho pono lani ale nui wahi ".repeat(200);
    let alphabet = format!("{hawaiian}z");
    assert!(
        alphabet
            .chars()
            .filter(|c| c.is_alphabetic())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            <= 14
    );
    assert_eq!(letter_roster(&patterns(alphabet.clone())), vec!['z']);

    // The same corpus plus six hundred Han characters used once each.
    let logographic: String = (0x4e00u32..0x4e00 + 600)
        .map(|cp| char::from_u32(cp).expect("a CJK code point"))
        .collect();
    let wide = format!("{alphabet}{logographic}");
    assert_eq!(letter_roster(&patterns(wide.clone())), Vec::<char>::new());

    let always = JudgingConfig {
        letters: LetterRoster::Always,
        ..JudgingConfig::default()
    };
    assert!(letter_roster(&patterns_with(wide, &always)).len() > 500);

    let never = JudgingConfig {
        letters: LetterRoster::Never,
        ..JudgingConfig::default()
    };
    assert_eq!(
        letter_roster(&patterns_with(alphabet, &never)),
        Vec::<char>::new()
    );
}

/// Charter invariant 8: glue is never a scalar, so it is never a pattern.
#[test]
fn zwj_never_reaches_the_inventory_or_the_roster() {
    let rows = patterns("\u{915}\u{94d}\u{200d}\u{937} \u{915}\u{94d}\u{200c}\u{937} word");
    assert!(!rostered(&rows, '\u{200d}'));
    assert!(!rostered(&rows, '\u{200c}'));
    assert!(!rostered(&rows, '\u{94d}'));
    assert!(
        rows.iter()
            .all(|row| row.glyph.scalar() != Some('\u{200d}'))
    );
}

// ── The config reaches judging and nothing else ─────────────────────────

/// Raising the floor rosters more; turning a channel off removes exactly its
/// rows.
#[test]
fn the_config_moves_the_roster_and_the_channels() {
    let text = format!("{}b,`c", "a, ".repeat(5_000));
    let default = patterns(text.clone());
    let raised = JudgingConfig {
        rarity_floor: 5_001,
        ..JudgingConfig::default()
    };
    assert!(
        patterns_with(text.clone(), &raised).len() > default.len(),
        "a floor above every count rosters every glyph"
    );

    let quiet = JudgingConfig {
        channels: sous_core::Channels {
            rarity: false,
            ..sous_core::Channels::default()
        },
        ..JudgingConfig::default()
    };
    assert!(
        patterns_with(text, &quiet)
            .iter()
            .all(|row| row.channel != Channel::Rarity)
    );
}
