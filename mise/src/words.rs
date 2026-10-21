//! `words/1`: where a word starts and ends, the one rule every crate splits by.
//!
//! ```text
//! for_each_word("He said, don't-3rd \u{5d0}\u{5d1}\u{5d2} a--b 12,345")
//!   0..2    He
//!   3..7    said
//!   9..14   don't         a joiner with a letter on both sides
//!   15..18  3rd           a digit confirms no joiner, so `-` closes `don't`
//!   19..25  אבג
//!   26..27  a             `--` is two scalars: the second confirms nothing
//!   29..30  b
//!                         `12,345` holds no letter, so it is no word
//!
//! word,word  → word | word        a comma is no joiner
//! a–b        → a | b              nor an en dash
//! ng'ombe  l'homme  father-in-law  col·lecció  בֶּן־הֲדַד   → one word each
//! ```
//!
//! A word is a maximal run of letters, glue and digits holding at least one
//! letter, extended through ONE [`is_joiner`] scalar with a letter or glue
//! immediately on both sides. [`fold`] is the simple case fold. Changing the
//! joiner set, the digit rule, the fold, or the Unicode version of
//! [`crate::unicode`] is `words/2`: a consumer that hashes words sees
//! different hashes.
//!
//! [`Splitter`] is the rule as a state machine a caller feeds one scalar at a
//! time, so a walk that keeps its own state per word asks it every boundary
//! question without a second pass.

use crate::unicode::{Class, class_of};

pub const VERSION: &str = "words/1";

/// The scalars that join two letters into one word.
///
/// ```text
/// '  ’          apostrophes      don't  ng'ombe  l’homme
/// -  ‐  ‑       hyphens          father-in-law  Ben-Hadad
/// ־  ֊  ᐀       maqaf, Armenian and Canadian syllabics hyphens
/// ·             middle dot       col·lecció
/// ```
#[inline]
pub const fn is_joiner(scalar: char) -> bool {
    matches!(
        scalar,
        '\u{27}'
            | '\u{2019}'
            | '\u{2D}'
            | '\u{2010}'
            | '\u{2011}'
            | '\u{5BE}'
            | '\u{58A}'
            | '\u{1400}'
            | '\u{B7}'
    )
}

/// What a word is built from: letters, glue and digits.
#[inline]
pub const fn is_core(class: Class) -> bool {
    class.is_alphabetic() || class.is_glue() || class.is_decimal_digit()
}

/// What may stand on either side of a joiner: a letter, or glue riding one.
#[inline]
pub const fn is_letter(class: Class) -> bool {
    class.is_alphabetic() || class.is_glue()
}

/// The simple case fold: the first scalar of `char::to_lowercase`, so `ß`
/// stays itself and `İ` loses its dot.
#[inline]
pub fn fold(scalar: char) -> char {
    scalar.to_lowercase().next().unwrap_or(scalar)
}

/// A closed run of core scalars and joiners: `from..to` in bytes, and whether
/// it held a letter, which is what makes it a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub from: u32,
    pub to: u32,
    pub word: bool,
}

/// What one scalar did.
///
/// ```text
/// "a-b c"   a Start   - Hold   b Join   ␠ Close(0..3 word)   c Start
/// "a-3"     a Start   - Hold   3 Restart(0..1 word)
/// "a--b"    a Start   - Hold   - Close(0..1 word)   b Start
/// "12 a"    1 Start   2 Extend ␠ Close(0..2)   a Start          no letter, no word
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Outside a word, and still outside.
    Outside,
    /// This scalar starts a word.
    Start,
    /// This scalar is in the word.
    Extend,
    /// This scalar may join the word to a letter after it.
    Hold,
    /// This scalar confirmed the held joiner: both are in the word.
    Join,
    /// The word closed; this scalar is outside it.
    Close(Run),
    /// The word closed at the held joiner, and this scalar starts a new one.
    Restart(Run),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Outside,
    Word,
    Held,
}

/// The word rule, one scalar at a time.
///
/// ```text
/// let mut split = Splitter::default();
/// for (at, scalar) in text.char_indices() {
///     split.step(at as u32, scalar, class_of(scalar));
/// }
/// split.finish(text.len() as u32);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Splitter {
    state: State,
    from: u32,
    joiner: u32,
    after_letter: bool,
    letter: bool,
}

impl Splitter {
    /// `at` is the scalar's byte offset; `class` is `class_of(scalar)`.
    #[inline]
    pub fn step(&mut self, at: u32, scalar: char, class: Class) -> Step {
        let _ = scalar;
        let letter = is_letter(class);
        let step = if letter || class.is_decimal_digit() {
            let step = match self.state {
                State::Outside => {
                    self.open(at);
                    Step::Start
                }
                State::Word => Step::Extend,
                State::Held if letter => {
                    self.state = State::Word;
                    Step::Join
                }
                State::Held => {
                    let closed = self.closed(self.joiner);
                    self.open(at);
                    Step::Restart(closed)
                }
            };
            self.letter |= class.is_alphabetic();
            step
        } else if self.state == State::Word && self.after_letter && !class.is_whitespace() {
            self.state = State::Held;
            self.joiner = at;
            Step::Hold
        } else {
            match self.state {
                State::Outside => Step::Outside,
                State::Word => Step::Close(self.close(at)),
                State::Held => Step::Close(self.close(self.joiner)),
            }
        };
        self.after_letter = letter;
        step
    }

    /// Closes the word standing at the end of the text, if one is.
    #[inline]
    pub fn finish(&mut self, len: u32) -> Option<Run> {
        match self.state {
            State::Outside => None,
            State::Word => Some(self.close(len)),
            State::Held => Some(self.close(self.joiner)),
        }
    }

    #[inline]
    fn open(&mut self, at: u32) {
        self.state = State::Word;
        self.from = at;
        self.letter = false;
    }

    #[inline]
    fn close(&mut self, to: u32) -> Run {
        self.state = State::Outside;
        self.closed(to)
    }

    #[inline]
    fn closed(&self, to: u32) -> Run {
        Run {
            from: self.from,
            to,
            word: self.letter,
        }
    }
}

/// Calls `visit` with every word's byte span, in order.
pub fn for_each_word(text: &str, mut visit: impl FnMut(u32, u32)) {
    let mut split = Splitter::default();
    for (at, scalar) in text.char_indices() {
        if let Step::Close(run) | Step::Restart(run) =
            split.step(at as u32, scalar, class_of(scalar))
            && run.word
        {
            visit(run.from, run.to);
        }
    }
    if let Some(run) = split.finish(text.len() as u32)
        && run.word
    {
        visit(run.from, run.to);
    }
}

/// The span holding the scalar at `at`, by the rule [`for_each_word`] splits
/// by, letter or no.
///
/// ```text
/// word_around("he said, don\u{2019}t go", 15)   → (9, 16)    the joiner is inside
/// word_around("go. he went", 4)               → (4, 6)
/// word_around("a-3rd", 2)                     → (2, 5)
/// ```
///
/// `at` is the byte offset of an [`is_core`] scalar. The rule reads the same
/// in either direction, so the start is a [`Splitter`] fed the text backwards,
/// each scalar at its end offset.
pub fn word_around(text: &str, at: u32) -> (u32, u32) {
    let here = text[at as usize..]
        .chars()
        .next()
        .map_or(at, |scalar| at + scalar.len_utf8() as u32);
    let forward = text[at as usize..]
        .char_indices()
        .map(|(offset, scalar)| (at + offset as u32, scalar));
    let backward = text[..here as usize]
        .char_indices()
        .rev()
        .map(|(offset, scalar)| ((offset + scalar.len_utf8()) as u32, scalar));
    (edge(backward, 0), edge(forward, text.len() as u32))
}

fn edge(scalars: impl Iterator<Item = (u32, char)>, end: u32) -> u32 {
    let mut split = Splitter::default();
    for (at, scalar) in scalars {
        if let Step::Close(run) | Step::Restart(run) = split.step(at, scalar, class_of(scalar)) {
            return run.to;
        }
    }
    split.finish(end).map_or(end, |run| run.to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        let mut out = Vec::new();
        for_each_word(text, |from, to| out.push(&text[from as usize..to as usize]));
        out
    }

    #[test]
    fn the_module_example_is_exact() {
        let text = "He said, don't-3rd \u{5d0}\u{5d1}\u{5d2} a--b 12,345";
        assert_eq!(
            words(text),
            [
                "He",
                "said",
                "don't",
                "3rd",
                "\u{5d0}\u{5d1}\u{5d2}",
                "a",
                "b"
            ]
        );
    }

    #[test]
    fn digits_ride_letters_and_a_digit_run_is_no_word() {
        assert_eq!(words("3rd 1Ki x2"), ["3rd", "1Ki", "x2"]);
        assert_eq!(words("in 12,345 and 7"), ["in", "and"]);
        assert_eq!(words("a-3 4-b"), ["a", "b"]);
        assert_eq!(words("a-3rd"), ["a", "3rd"]);
        assert_eq!(words("12,345"), Vec::<&str>::new());
    }

    #[test]
    fn a_joiner_needs_a_letter_on_both_sides() {
        assert_eq!(words("a--b"), ["a", "b"]);
        assert_eq!(words("a- b"), ["a", "b"]);
        assert_eq!(words("-ab-"), ["ab"]);
        assert_eq!(words("e\u{301}-e"), ["e\u{301}-e"], "glue rides its letter");
    }

    #[test]
    fn the_steps_name_every_boundary() {
        let steps = |text: &str| {
            let mut split = Splitter::default();
            let mut out: Vec<Step> = text
                .char_indices()
                .map(|(at, scalar)| split.step(at as u32, scalar, class_of(scalar)))
                .collect();
            out.extend(split.finish(text.len() as u32).map(Step::Close));
            out
        };
        let run = |from, to, word| Run { from, to, word };
        assert_eq!(
            steps("a-b c"),
            [
                Step::Start,
                Step::Hold,
                Step::Join,
                Step::Close(run(0, 3, true)),
                Step::Start,
                Step::Close(run(4, 5, true)),
            ]
        );
        assert_eq!(
            steps("a-3"),
            [
                Step::Start,
                Step::Hold,
                Step::Restart(run(0, 1, true)),
                Step::Close(run(2, 3, false)),
            ]
        );
        assert_eq!(
            steps("a--b"),
            [
                Step::Start,
                Step::Hold,
                Step::Close(run(0, 1, true)),
                Step::Start,
                Step::Close(run(3, 4, true)),
            ]
        );
        assert_eq!(
            steps("12 a"),
            [
                Step::Start,
                Step::Extend,
                Step::Close(run(0, 2, false)),
                Step::Start,
                Step::Close(run(3, 4, true)),
            ]
        );
        assert_eq!(
            steps("a-"),
            [Step::Start, Step::Hold, Step::Close(run(0, 1, true))]
        );
    }

    #[test]
    fn word_around_draws_the_span_for_each_word() {
        assert_eq!(word_around("he said, don\u{2019}t go", 15), (9, 16));
        assert_eq!(word_around("go. he went", 4), (4, 6));
        assert_eq!(word_around("a-3rd", 2), (2, 5));
        assert_eq!(word_around("3-ab", 2), (2, 4));
        assert_eq!(word_around("a--b", 0), (0, 1));
        assert_eq!(word_around("a--b", 3), (3, 4));
        assert_eq!(word_around("x father-in-law", 9), (2, 15));
        let text = "He said, don't-3rd \u{5d0}\u{5d1}\u{5d2} a--b 12,345 col\u{b7}lecci\u{f3}";
        for_each_word(text, |from, to| {
            for (offset, _) in text[from as usize..to as usize].char_indices() {
                let at = from + offset as u32;
                if is_core(class_of(text[at as usize..].chars().next().unwrap())) {
                    assert_eq!(word_around(text, at), (from, to), "at {at}");
                }
            }
        });
    }

    #[test]
    fn the_fold_is_simple() {
        assert_eq!(fold('A'), 'a');
        assert_eq!(fold('\u{df}'), '\u{df}');
        assert_eq!(fold('\u{130}'), 'i');
        assert_eq!(fold('\u{5d0}'), '\u{5d0}');
    }
}
