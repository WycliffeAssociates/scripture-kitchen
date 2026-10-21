//! The word scan: one pass per chapter, three lanes out of it.
//!
//! ```text
//! walk("He said. \u{201C}Go,\u{201D} said david. David went. Go, go. Theee.", &[])
//!   casing lane
//!     he     Title  Start        // the chapter's first word
//!     said   Lower  None
//!     go     Title  Glyph('.' quoted)   // the quote rides; the terminal leads
//!     said   Lower  Glyph(',' quoted)
//!     david  Lower  None
//!     david  Title  Glyph('.' bare)
//!     went   Lower  None
//!   doubles lane
//!     hash(go)   bare 0  separated [((',', bare), 1)]   // `Go, go`: a comma stood between
//!   letter-run lane
//!     'e'  [0, 1, 0, 0, 0, 0, 0]                // one run of three, in `Theee`
//! ```
//!
//! The walk decides nothing about capitals. It records what stood before each
//! word and leaves forced or free to the judge, which reads the corpus's own
//! terminal table ([`crate::judge::TerminalTable`]) — and the doubles lane
//! leans on the same table: `separated` is keyed by the separator's handoff
//! context rather than a bare count, because a pair a comma splits and a pair a
//! period-then-capital splits are different claims. The judge folds only the
//! contexts that do not force a capital into the doubling numerator, so
//! `go. Go` is two sentences and never a doubled word.
//!
//! The boundaries are `mise::words` (`words/1`), asked one scalar at a time
//! through its `Splitter`, so the walk keeps only its own state — the chain,
//! the verse, the word's capitals:
//!
//! ```text
//! ng'ombe  don't  mother-in-law       one word each: an apostrophe or hyphen joins
//! word,word  a\u{2013}b  a--b              two each: a comma, an en dash, a second atom
//! 3rd  1Ki                            a digit rides a letter
//! 12,345                              no letter, no word
//! ```
//!
//! The case fold is `mise::words::fold`, the first scalar of `char::to_lowercase`. `\u{df}`
//! folds to itself and `\u{130}` loses its dot, so `STRASSE` and `Stra\u{df}e`
//! stay two words here. This is a convention check, not a collator.

use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::xxh3_64;

use super::{
    Before, DoubleCount, Form, LETTER_RUN_LANES, LETTER_RUN_MAX, LETTER_RUN_MIN, WordCount,
    WordRow, letter_run_lane,
};
use crate::substrate::{FollowKey, Ride, ScalarKey, ride_of};
use crate::{Verse, VerseKey};
use mise::unicode::Class;
use mise::words::{Run, Splitter, Step, class_of, fold, is_core};

pub use mise::words::word_around;

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
    /// The verse the word starts in; `None` before the first verse or when
    /// the walk was handed none.
    pub verse: Option<VerseKey>,
}

/// The chain past one run atom: a leading glyph replaces it, a quote or a
/// closing bracket marks it, an opening bracket leaves it as it was.
fn chained(chain: Before, scalar: char) -> Before {
    match (ride_of(scalar), chain) {
        (Ride::Leads, _) => Before::Glyph(FollowKey::new(ScalarKey::of(scalar), false)),
        (Ride::Quote, Before::Glyph(key)) => Before::Glyph(key.through_quote()),
        (Ride::Closer, Before::Glyph(key)) => Before::Glyph(key.through_bracket()),
        _ => chain,
    }
}

/// One word under construction.
#[derive(Clone, Copy)]
struct Building {
    before: Before,
    verse: Option<VerseKey>,
    scalars: u32,
    upper: u32,
    cased: u32,
    /// Whether the word's FIRST alphabetic scalar is uppercase.
    first_upper: Option<bool>,
}

impl Building {
    const fn new(before: Before, verse: Option<VerseKey>) -> Self {
        Self {
            before,
            verse,
            scalars: 0,
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
    verse: Option<VerseKey>,
    word: Option<Building>,
    split: Splitter,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            scratch: String::new(),
            chain: Before::None,
            opened: true,
            verse: None,
            word: None,
            split: Splitter::default(),
        }
    }

    #[inline]
    fn step(&mut self, at: u32, scalar: char, visit: &mut impl FnMut(Occurrence)) {
        let class = class_of(scalar);
        // Every core scalar ends the chain, as it does the substrate's: the
        // letter after an inner joiner (`long-suffering`) is where that joiner
        // hands off.
        let core = match self.split.step(at, scalar, class) {
            Step::Outside | Step::Hold => false,
            Step::Start => {
                self.open(class);
                true
            }
            Step::Extend => {
                self.word.as_mut().expect("a word stands").add(class);
                true
            }
            Step::Join => {
                let word = self.word.as_mut().expect("a joiner needs a word");
                word.scalars += 1;
                word.add(class);
                true
            }
            Step::Close(run) => {
                self.close(run, visit);
                false
            }
            Step::Restart(run) => {
                self.close(run, visit);
                self.open(class);
                true
            }
        };
        if core {
            self.chain = Before::None;
        } else if !class.is_whitespace() {
            self.chain = chained(self.chain, scalar);
        }
    }

    #[inline]
    fn open(&mut self, class: Class) {
        let before = if self.opened {
            Before::Start
        } else {
            self.chain
        };
        let mut word = Building::new(before, self.verse);
        word.add(class);
        self.word = Some(word);
        self.opened = false;
    }

    fn close(&mut self, run: Run, visit: &mut impl FnMut(Occurrence)) {
        let Some(built) = self.word.take() else {
            return;
        };
        if !run.word {
            return;
        }
        let word = &self.text[run.from as usize..run.to as usize];
        let hash = fold_hash(word, &mut self.scratch);
        visit(Occurrence {
            from: run.from,
            to: run.to,
            hash,
            form: built.form(),
            before: built.before,
            len: u8::try_from(built.scalars).unwrap_or(u8::MAX),
            verse: built.verse,
        });
    }
}

/// Calls `visit` with every word of `text`, in order.
///
/// `verses` are rebased to `text`, as [`crate::ChapterInput`] hands them over;
/// an empty slice leaves verse starts out of the forced rule and every
/// `verse` `None`.
pub fn for_each_word(text: &str, verses: &[Verse], mut visit: impl FnMut(Occurrence)) {
    let mut scan = Scan::new(text);
    let mut verse = 0usize;
    for (at, scalar) in text.char_indices() {
        let at = at as u32;
        while verses.get(verse).is_some_and(|row| row.text().from() <= at) {
            // Only when no word is standing: a verse that starts inside one
            // does not make the NEXT word a verse-start capital.
            scan.opened = scan.word.is_none();
            scan.verse = Some(verses[verse].key());
            verse += 1;
        }
        scan.step(at, scalar, &mut visit);
    }
    if let Some(run) = scan.split.finish(text.len() as u32) {
        scan.close(run, &mut visit);
    }
}

/// Calls `visit` with every same-letter run of two or more inside one word.
///
/// ```text
/// for_each_letter_run("theee")   → ('e', 3)
/// for_each_letter_run("Eel")     → ('e', 2)      // the same simple fold
/// for_each_letter_run("aaaaaaaaah") → ('a', 8)   // saturating at the lane
/// ```
///
/// A run is identical atoms, a base and the glue riding it, so `त्त` and
/// `तित` are not runs of `त`; a digit, or a joiner, breaks one. The letter is
/// `mise::words::fold`, the same fold the hash uses, so `Ee` is a run of two and an uncased script counts here
/// like any other.
pub fn for_each_letter_run(word: &str, mut visit: impl FnMut(ScalarKey, u8)) {
    let mut held: Option<(ScalarKey, &str, u32)> = None;
    let mut scalars = word.char_indices().peekable();
    while let Some((at, scalar)) = scalars.next() {
        let class = class_of(scalar);
        if class.is_glue() {
            continue;
        }
        let start = at + scalar.len_utf8();
        let mut end = start;
        while let Some(&(next_at, next)) = scalars.peek()
            && class_of(next).is_glue()
        {
            end = next_at + next.len_utf8();
            scalars.next();
        }
        let glue = &word[start..end];
        let letter = class.is_alphabetic().then(|| ScalarKey::of(fold(scalar)));
        if let Some((key, held_glue, scalars)) = &mut held
            && letter == Some(*key)
            && glue == *held_glue
        {
            *scalars += 1;
            continue;
        }
        if let Some((key, _, scalars)) = held.take()
            && let Some(length) = run_length(scalars)
        {
            visit(key, length);
        }
        held = letter.map(|key| (key, glue, 1u32));
    }
    if let Some((key, _, scalars)) = held
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
/// carries the same context [`Before::Glyph`](super::Before::Glyph) would give
/// the second word, because the terminal table reads it to decide whether this
/// is a sentence boundary rather than a doubled word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    /// Whitespace only.
    Bare,
    /// Nonletters, with or without whitespace around them.
    Separated(FollowKey),
}

/// How two adjacent word occurrences are separated, or `None` when something
/// stood between them that a double may not ride through. A line break is
/// whitespace; a verse boundary is the caller's to refuse
/// ([`Occurrence::verse`]).
///
/// Only a letter, glue, or digit disqualifies — and each of those means the
/// walk dropped a token between the two words (a digit run holds no letter, so
/// it is no word at all), which is exactly the case a double must not claim.
pub fn gap_between(text: &str, from: u32, to: u32) -> Option<Gap> {
    let mut chain = Before::None;
    let mut last: Option<char> = None;
    for scalar in text[from as usize..to as usize].chars() {
        let class = class_of(scalar);
        if is_core(class) {
            return None;
        }
        if !class.is_whitespace() {
            chain = chained(chain, scalar);
            last = Some(scalar);
        }
    }
    Some(match (chain, last) {
        (Before::Glyph(key), _) => Gap::Separated(key),
        // Only quotes and brackets: keyed by the last, which no table learns,
        // so the pair stays free as `Before::None` would.
        (_, Some(atom)) => Gap::Separated(FollowKey::new(ScalarKey::of(atom), false)),
        (_, None) => Gap::Bare,
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
    let mut previous: Option<(u64, u32, Option<VerseKey>)> = None;

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
        if let Some((hash, to, verse)) = previous.replace((word.hash, word.to, word.verse))
            && hash == word.hash
            && verse == word.verse
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
        scratch.push(fold(scalar));
    }
    xxh3_64(scratch.as_bytes())
}
