//! The word scan: one pass per chapter, three lanes out of it.
//!
//! ```text
//! walk("He said. \u{201C}Go,\u{201D} said david. David went. Go, go. Theee.", &[])
//!   casing lane
//!     he     Title  Start        // the chapter's first word
//!     said   Lower  None
//!     go     Title  Glyph('.')   // the quote is transparent; the terminal is not
//!     said   Lower  Glyph(',')
//!     david  Lower  None
//!     david  Title  Glyph('.')
//!     went   Lower  None
//!   doubles lane
//!     hash(go)   bare 0  separated [(',', 1)]   // `Go, go`: a comma stood between
//!   letter-run lane
//!     'e'  [0, 1, 0, 0, 0, 0, 0]                // one run of three, in `Theee`
//! ```
//!
//! The walk decides nothing about capitals. It records what stood before each
//! word and leaves forced or free to the judge, which reads the corpus's own
//! terminal table ([`crate::judge::TerminalTable`]) — and the doubles lane
//! leans on the same table: `separated` is keyed by the separator's own last
//! glyph rather than a bare count, because a pair a comma splits and a pair a
//! period-then-capital splits are different claims. The judge folds only the
//! glyphs that do not force a capital into the doubling numerator, so
//! `go. Go` is two sentences and never a doubled word.
//!
//! A word is a maximal run of letters and glue, extended through ONE nonletter
//! with a letter immediately on both sides (`ng'ombe`, `don't`,
//! `mother-in-law`; `a--b` is two words). A digit beside a letter joins the
//! word (`3rd`, `1Ki`) — splitting there would invent a word — and a run of
//! digits holding no letter is not a word at all.
//!
//! The case fold is simple: the first scalar of `char::to_lowercase`. `\u{df}`
//! folds to itself and `\u{130}` loses its dot, so `STRASSE` and `Stra\u{df}e`
//! stay two words here. This is a convention check, not a collator.

use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::xxh3_64;

use super::{
    Before, DoubleCount, Form, LETTER_RUN_LANES, LETTER_RUN_MAX, LETTER_RUN_MIN, WordCount,
    WordRow, letter_run_lane,
};
use crate::Verse;
use crate::substrate::{ScalarKey, is_run_atom};
use crate::unicode::{Class, Pool, class_of, pool_of};

/// One word as the walk saw it, in the coordinates of the text scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    pub from: u32,
    pub to: u32,
    /// xxh3-64 of the case-folded scalars. Every word is hashed, uncased
    /// ones included: the doubled lane compares by hash and doubling has
    /// nothing to do with case.
    pub hash: u64,
    pub form: Form,
    /// What stood before the word, quotes and brackets ridden through.
    pub before: Before,
    /// Scalar count, saturating.
    pub len: u8,
}

/// Whether an atom is transparent to what stands behind it.
///
/// The one thing the walk still reads a pool for. An opening quote hides the
/// terminal in front of it, and it is the terminal the capital answers to.
fn rides(scalar: char) -> bool {
    matches!(pool_of(scalar), Pool::Quote | Pool::Bracket)
}

/// What a word is built from before the joiner rule extends it.
#[inline]
const fn is_core(class: Class) -> bool {
    class.is_alphabetic() || class.is_glue() || class.is_decimal_digit()
}

/// Glue rides its base, so it answers `Letter` when the joiner rule asks —
/// the reading `OuterClass::of` already gives a neighbour.
#[inline]
const fn is_letterish(class: Class) -> bool {
    class.is_alphabetic() || class.is_glue()
}

/// One word under construction.
#[derive(Clone, Copy)]
struct Building {
    from: u32,
    before: Before,
    scalars: u32,
    letters: bool,
    upper: u32,
    cased: u32,
    /// Whether the word's FIRST alphabetic scalar is uppercase.
    first_upper: Option<bool>,
}

impl Building {
    const fn new(from: u32, before: Before) -> Self {
        Self {
            from,
            before,
            scalars: 0,
            letters: false,
            upper: 0,
            cased: 0,
            first_upper: None,
        }
    }

    fn add(&mut self, class: Class) {
        self.scalars += 1;
        if !class.is_alphabetic() {
            return;
        }
        self.letters = true;
        let upper = class.is_uppercase();
        if upper || class.is_lowercase() {
            self.cased += 1;
            self.upper += u32::from(upper);
        }
        if self.first_upper.is_none() {
            self.first_upper = Some(upper);
        }
    }

    const fn form(self) -> Form {
        if self.cased == 0 {
            Form::Uncased
        } else if self.upper == 0 {
            Form::Lower
        } else if self.upper == self.cased && self.cased >= 2 {
            Form::Upper
        } else if self.upper == 1 && matches!(self.first_upper, Some(true)) {
            Form::Title
        } else {
            Form::Mixed
        }
    }
}

/// The scan state; nothing here outlives one call.
struct Scan<'a> {
    text: &'a str,
    /// The case-folded word, refilled per cased word and never per scalar.
    scratch: String,
    /// What stands behind the cursor, through quotes, brackets, and space.
    chain: Before,
    /// A chapter or verse started and no word has claimed it yet.
    opened: bool,
    word: Option<Building>,
    /// A run atom with a letter before it that may yet get one after.
    joiner: Option<u32>,
    prev_letterish: bool,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            scratch: String::new(),
            chain: Before::None,
            opened: true,
            word: None,
            joiner: None,
            prev_letterish: false,
        }
    }

    fn step(&mut self, at: u32, scalar: char, visit: &mut impl FnMut(Occurrence)) {
        let class = class_of(scalar);
        let letterish = is_letterish(class);

        if is_core(class) {
            if let Some(gap) = self.joiner.take() {
                if letterish {
                    // Confirmed: a letter on both sides, so the word keeps it.
                    self.word.as_mut().expect("a joiner needs a word").scalars += 1;
                } else {
                    // `a-3`: a digit does not confirm a joiner.
                    self.close(gap, visit);
                }
            }
            if self.word.is_none() {
                let before = if self.opened {
                    Before::Start
                } else {
                    self.chain
                };
                self.word = Some(Building::new(at, before));
                self.opened = false;
                self.chain = Before::None;
            }
            self.word.as_mut().expect("just opened").add(class);
        } else if class.is_whitespace() {
            let to = self.joiner.take().unwrap_or(at);
            self.close(to, visit);
        } else {
            debug_assert!(is_run_atom(class), "the remaining scalars are run atoms");
            match self.joiner.take() {
                // `a--b`: the first atom ended the word after all.
                Some(gap) => self.close(gap, visit),
                None if self.word.is_some() && self.prev_letterish => self.joiner = Some(at),
                None => self.close(at, visit),
            }
            if !rides(scalar) {
                self.chain = Before::Glyph(ScalarKey::of(scalar));
            }
        }
        self.prev_letterish = letterish;
    }

    fn close(&mut self, to: u32, visit: &mut impl FnMut(Occurrence)) {
        let Some(built) = self.word.take() else {
            return;
        };
        if !built.letters {
            return;
        }
        let source: &'a str = self.text;
        let word = &source[built.from as usize..to as usize];
        let form = built.form();
        let hash = fold_hash(word, &mut self.scratch);
        visit(Occurrence {
            from: built.from,
            to,
            hash,
            form,
            before: built.before,
            len: u8::try_from(built.scalars).unwrap_or(u8::MAX),
        });
    }
}

/// Calls `visit` with every word of `text`, in order.
///
/// `verses` are rebased to `text`, as [`crate::ChapterInput`] hands them over;
/// an empty slice leaves verse starts out of the forced rule and nothing else.
pub fn for_each_word(text: &str, verses: &[Verse], mut visit: impl FnMut(Occurrence)) {
    let mut scan = Scan::new(text);
    let mut verse = 0usize;
    for (at, scalar) in text.char_indices() {
        let at = at as u32;
        while verses.get(verse).is_some_and(|row| row.text().from() <= at) {
            scan.opened = true;
            verse += 1;
        }
        scan.step(at, scalar, &mut visit);
    }
    let to = scan.joiner.take().unwrap_or(text.len() as u32);
    scan.close(to, &mut visit);
}

/// The word span holding the scalar at `at`, by the same rule
/// [`for_each_word`] builds one.
///
/// ```text
/// word_around("he said, don\u{2019}t go", 13)   → 9..15     // the joiner is inside
/// word_around("go. he went", 4)               → 4..6      // `he`
/// ```
///
/// `at` is the byte offset of a scalar the word rule counts as core, and the
/// span is clipped to `text` — one chapter's slice in practice, which is what
/// the walk itself scans. It exists so a site that names a word can draw the
/// same span the word lane would, without a second pass over the chapter.
pub fn word_around(text: &str, at: u32) -> (u32, u32) {
    (word_start(text, at), word_end(text, at))
}

/// Forward from `at` through core scalars, riding at most one nonletter that
/// has a letter on both sides.
fn word_end(text: &str, at: u32) -> u32 {
    let mut to = at;
    let mut joiner = false;
    let mut letterish = false;
    for (offset, scalar) in text[at as usize..].char_indices() {
        let class = class_of(scalar);
        if is_core(class) {
            // `a-3`: a digit does not confirm a joiner, so the word ended.
            if joiner && !is_letterish(class) {
                break;
            }
            joiner = false;
            to = at + offset as u32 + scalar.len_utf8() as u32;
            letterish = is_letterish(class);
        } else if is_run_atom(class) && !joiner && letterish {
            joiner = true;
            letterish = false;
        } else {
            break;
        }
    }
    to
}

/// The same walk backwards; `a--b` ends at the first atom either way.
fn word_start(text: &str, at: u32) -> u32 {
    let mut from = at;
    let mut joiner = false;
    let mut letterish = text[at as usize..]
        .chars()
        .next()
        .is_some_and(|scalar| is_letterish(class_of(scalar)));
    let mut cursor = at;
    for scalar in text[..at as usize].chars().rev() {
        let class = class_of(scalar);
        let start = cursor - scalar.len_utf8() as u32;
        if is_core(class) {
            if joiner && !is_letterish(class) {
                break;
            }
            joiner = false;
            from = start;
            letterish = is_letterish(class);
        } else if is_run_atom(class) && !joiner && letterish {
            joiner = true;
            letterish = false;
        } else {
            break;
        }
        cursor = start;
    }
    from
}

/// Calls `visit` with every same-letter run of two or more inside one word.
///
/// ```text
/// for_each_letter_run("theee")   → ('e', 3)
/// for_each_letter_run("Eel")     → ('e', 2)      // the same simple fold
/// for_each_letter_run("aaaaaaaaah") → ('a', 8)   // saturating at the lane
/// ```
///
/// Glue rides its base, so a combining mark neither breaks a run nor
/// lengthens it; a digit, or the one nonletter a word rides through, breaks
/// one. The letter is the first scalar of `char::to_lowercase`, the same fold
/// the hash uses, so `Ee` is a run of two and an uncased script counts here
/// like any other.
pub fn for_each_letter_run(word: &str, mut visit: impl FnMut(ScalarKey, u8)) {
    let mut held: Option<(ScalarKey, u32)> = None;
    for scalar in word.chars() {
        let class = class_of(scalar);
        if class.is_glue() {
            continue;
        }
        let letter = class
            .is_alphabetic()
            .then(|| ScalarKey::of(scalar.to_lowercase().next().unwrap_or(scalar)));
        if let Some((key, scalars)) = &mut held
            && letter == Some(*key)
        {
            *scalars += 1;
            continue;
        }
        if let Some((key, scalars)) = held.take()
            && let Some(length) = run_length(scalars)
        {
            visit(key, length);
        }
        held = letter.map(|key| (key, 1u32));
    }
    if let Some((key, scalars)) = held
        && let Some(length) = run_length(scalars)
    {
        visit(key, length);
    }
}

/// A repeat, saturating into the last lane; `None` for a letter standing once.
const fn run_length(scalars: u32) -> Option<u8> {
    if scalars < LETTER_RUN_MIN as u32 {
        None
    } else if scalars >= LETTER_RUN_MAX as u32 {
        Some(LETTER_RUN_MAX)
    } else {
        Some(scalars as u8)
    }
}

/// What stood between two occurrences of one word, when they are a double.
///
/// Two claims, not one: `na na` and `na, na` have different denominators and
/// different reasons to be a slip, so they never share a key. `Separated`
/// carries the run's LAST scalar as a [`ScalarKey`] — the same key
/// [`Before::Glyph`](super::Before::Glyph) stores — because that is the atom
/// the judge's terminal table reads to decide whether this is a sentence
/// boundary rather than a doubled word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    /// Whitespace only.
    Bare,
    /// A nonletter run, with or without whitespace around it. The last glyph
    /// of the run.
    Separated(ScalarKey),
}

/// How two adjacent word occurrences are separated, or `None` when something
/// stood between them that a double may not ride through.
///
/// Only a letter, glue, or digit disqualifies — and each of those means the
/// walk dropped a token between the two words (a digit run holds no letter, so
/// it is no word at all), which is exactly the case a double must not claim.
pub fn gap_between(text: &str, from: u32, to: u32) -> Option<Gap> {
    let mut last_glyph: Option<ScalarKey> = None;
    for scalar in text[from as usize..to as usize].chars() {
        let class = class_of(scalar);
        if is_core(class) {
            return None;
        }
        if !class.is_whitespace() {
            last_glyph = Some(ScalarKey::of(scalar));
        }
    }
    Some(match last_glyph {
        None => Gap::Bare,
        Some(glyph) => Gap::Separated(glyph),
    })
}

/// One chapter's word counts: the casing lane sorted by `(hash, before)`, and
/// the doubles lane sorted by hash.
///
/// An uncased chapter's casing lane is empty — a word with no cased letter can
/// hold no casing convention — but its doubles lane is not: doubling has
/// nothing to do with case, so every word is hashed and every uncased
/// occurrence is counted here, which is where the doubled channel's
/// denominator comes from when the casing lane holds nothing.
pub(crate) fn walk(text: &str, verses: &[Verse]) -> WordRow {
    let mut rows: Vec<WordCount> = Vec::new();
    let mut slots: FxHashMap<(u64, u32), u32> = FxHashMap::default();
    let mut doubles: Vec<DoubleCount> = Vec::new();
    let mut lanes: FxHashMap<u64, u32> = FxHashMap::default();
    let mut runs: Vec<(ScalarKey, [u16; LETTER_RUN_LANES])> = Vec::new();
    let mut run_slots: FxHashMap<ScalarKey, u32> = FxHashMap::default();
    let mut cased = false;
    let mut previous: Option<(u64, u32)> = None;

    for_each_word(text, verses, |word| {
        let mut lane_of = |hash: u64, doubles: &mut Vec<DoubleCount>| {
            *lanes.entry(hash).or_insert_with(|| {
                doubles.push(DoubleCount::new(hash));
                doubles.len() as u32 - 1
            }) as usize
        };
        // Before the uncased return: a repeat is a repeat in any script.
        for_each_letter_run(
            &text[word.from as usize..word.to as usize],
            |letter, length| {
                let slot = *run_slots.entry(letter).or_insert_with(|| {
                    runs.push((letter, [0; LETTER_RUN_LANES]));
                    runs.len() as u32 - 1
                });
                let lane = &mut runs[slot as usize].1[letter_run_lane(length)];
                *lane = lane.saturating_add(1);
            },
        );
        if let Some((hash, to)) = previous.replace((word.hash, word.to))
            && hash == word.hash
            && let Some(gap) = gap_between(text, to, word.from)
        {
            let slot = lane_of(hash, &mut doubles);
            doubles[slot].add(gap);
        }
        if word.form == Form::Uncased {
            let slot = lane_of(word.hash, &mut doubles);
            doubles[slot].uncased = doubles[slot].uncased.saturating_add(1);
            return;
        }
        cased = true;
        let key = (word.hash, word.before.raw());
        let slot = *slots.entry(key).or_insert_with(|| {
            rows.push(WordCount::new(word.hash, word.before, word.len));
            rows.len() as u32 - 1
        });
        let lane = &mut rows[slot as usize].counts[word.form as usize];
        *lane = lane.saturating_add(1);
    });

    rows.sort_unstable_by_key(|row| (row.hash, row.before().raw()));
    doubles.sort_unstable_by_key(|row| row.hash);
    runs.sort_unstable_by_key(|row| row.0);
    WordRow {
        cased,
        words: rows.into_boxed_slice(),
        doubles: doubles.into_boxed_slice(),
        letter_runs: runs.into_boxed_slice(),
        released: false,
    }
}

/// The word's scalars, simple-folded to lowercase, hashed.
fn fold_hash(word: &str, scratch: &mut String) -> u64 {
    scratch.clear();
    for scalar in word.chars() {
        scratch.push(scalar.to_lowercase().next().unwrap_or(scalar));
    }
    xxh3_64(scratch.as_bytes())
}
