//! The word scan: one pass per chapter, and no hashing where nothing is cased.
//!
//! ```text
//! walk("He said. \u{201C}Go,\u{201D} said david. David went.", &[])
//!   he     Title  Start        // the chapter's first word
//!   said   Lower  None
//!   go     Title  Glyph('.')   // the quote is transparent; the terminal is not
//!   said   Lower  Glyph(',')
//!   david  Lower  None
//!   david  Title  Glyph('.')
//!   went   Lower  None
//! ```
//!
//! The walk decides nothing about capitals. It records what stood before each
//! word and leaves forced or free to the judge, which reads the corpus's own
//! terminal table ([`crate::judge::TerminalTable`]).
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

use super::{Before, Form, WordCount, WordRow};
use crate::Verse;
use crate::substrate::{ScalarKey, is_run_atom};
use crate::unicode::{Class, Pool, class_of, pool_of};

/// One word as the walk saw it, in the coordinates of the text scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    pub from: u32,
    pub to: u32,
    /// xxh3-64 of the case-folded scalars; 0 for [`Form::Uncased`], which is
    /// never hashed.
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
        let hash = if form == Form::Uncased {
            0
        } else {
            fold_hash(word, &mut self.scratch)
        };
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

/// One chapter's word counts, sorted by hash and then by what preceded them.
/// An uncased chapter hashes nothing and returns an empty row.
pub(crate) fn walk(text: &str, verses: &[Verse]) -> WordRow {
    let mut rows: Vec<WordCount> = Vec::new();
    let mut slots: FxHashMap<(u64, u32), u32> = FxHashMap::default();
    for_each_word(text, verses, |word| {
        if word.form == Form::Uncased {
            return;
        }
        let key = (word.hash, word.before.raw());
        let slot = *slots.entry(key).or_insert_with(|| {
            rows.push(WordCount::new(word.hash, word.before, word.len));
            rows.len() as u32 - 1
        });
        let lane = &mut rows[slot as usize].counts[word.form as usize];
        *lane = lane.saturating_add(1);
    });
    rows.sort_unstable_by_key(|row| (row.hash, row.before().raw()));
    WordRow {
        cased: !rows.is_empty(),
        words: rows.into_boxed_slice(),
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
