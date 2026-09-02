//! Grapheme-safe emission (charter invariant 6): snap a projected range out
//! to atom edges so a finding never splits a rendered grapheme.
//!
//! ```text
//! widen_to_atoms("qx\u{0301}", 2..4)                 → 1..4   // the mark keeps its base
//! widen_to_atoms("a\r\nb", 2..3)                     → 1..3   // CRLF stays whole
//! widen_to_atoms("\u{915}\u{94D}\u{937}", 6..9)      → 0..9   // the conjunct stays whole
//! widen_to_atoms("ab\0\0cd", 2..4)                   → 2..4   // controls are their own atoms
//! ```
//!
//! An *atom* is a base scalar plus everything that cannot stand without it.
//! [`is_atom_boundary`] is the whole rule. Complex runs widen as one atom;
//! over-wide is allowed, split is not.
//!
//! The claim, the conformance argument, and why the runtime carries no
//! segmenter: README.md.

use crate::TextRange;

use super::class_of;

/// Snaps a projected UTF-8 range outward to atom boundaries.
///
/// `range` must lie inside `text` on char boundaries; the result does too.
pub fn widen_to_atoms(text: &str, range: TextRange) -> TextRange {
    let mut from = range.from() as usize;
    let mut to = range.to() as usize;
    debug_assert!(text.is_char_boundary(from) && text.is_char_boundary(to));
    while !is_atom_boundary(text, from) {
        from -= text[..from]
            .chars()
            .next_back()
            .expect("position 0 is a boundary")
            .len_utf8();
    }
    while !is_atom_boundary(text, to) {
        to += text[to..]
            .chars()
            .next()
            .expect("the end of the text is a boundary")
            .len_utf8();
    }
    TextRange::new(from as u32, to as u32).expect("widening only moves the edges apart")
}

/// Whether `at` may be an emitted span edge.
///
/// UAX #29's rules, conservatively fused: where the algorithm needs state
/// this seals the boundary instead, because sealing can only widen.
pub fn is_atom_boundary(text: &str, at: usize) -> bool {
    if at == 0 || at == text.len() {
        return true;
    }
    let Some(prev) = text[..at].chars().next_back() else {
        return true;
    };
    let Some(next) = text[at..].chars().next() else {
        return true;
    };
    // GB3: CRLF is one atom.
    if prev == '\r' && next == '\n' {
        return false;
    }
    let (before, after) = (class_of(prev), class_of(next));
    // GB4/GB5: Control, CR, and LF break on both sides, ahead of everything
    // below. This keeps a hygiene run next to a newline exactly as wide.
    if before.is_gcb_control() || after.is_gcb_control() {
        return true;
    }
    // GB9/GB9a plus every General_Category Mark: glue owns the base behind it.
    if after.is_glue() {
        return false;
    }
    // GB9c without the InCB lanes: a virama anywhere in the glue run behind
    // `at` joins what follows. Wider than the conjunct rule, never narrower.
    for c in text[..at].chars().rev() {
        let class = class_of(c);
        if !class.is_glue() {
            break;
        }
        if class.is_linker() {
            return false;
        }
    }
    // GB9b: a Prepend owns what follows it.
    if before.is_prepend() {
        return false;
    }
    // GB6-GB8, GB11, GB12/GB13: one rule for every scalar joining forward.
    !((before.is_complex() || before.is_glue()) && after.is_complex())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widen(text: &str, from: u32, to: u32) -> (u32, u32) {
        let out = widen_to_atoms(text, TextRange::new(from, to).unwrap());
        (out.from(), out.to())
    }

    #[test]
    fn module_doc_examples_are_exact() {
        assert_eq!(widen("qx\u{301}", 2, 4), (1, 4));
        assert_eq!(widen("a\r\nb", 2, 3), (1, 3));
        assert_eq!(widen("\u{915}\u{94d}\u{937}", 6, 9), (0, 9));
        assert_eq!(widen("ab\0\0cd", 2, 4), (2, 4));
    }

    #[test]
    fn a_devanagari_conjunct_is_one_atom_from_either_end() {
        // क ् ष — GB9c joins across the virama; no sub-range splits it.
        let text = "\u{915}\u{94d}\u{937}";
        for from in [0usize, 3, 6] {
            for to in [3usize, 6, 9] {
                if to <= from {
                    continue;
                }
                assert_eq!(widen(text, from as u32, to as u32), (0, 9));
            }
        }
    }

    #[test]
    fn an_emoji_zwj_sequence_and_a_flag_widen_whole() {
        let family = "\u{1f6d1}\u{200d}\u{1f6d1}";
        assert_eq!(widen(family, 4, 7), (0, family.len() as u32));
        let flag = "\u{1f1fa}\u{1f1f8}";
        assert_eq!(widen(flag, 4, 8), (0, 8));
    }

    #[test]
    fn a_control_run_next_to_a_newline_does_not_widen() {
        // The byte-level hygiene classes survive widening untouched.
        assert_eq!(widen("abc\n\0\0\0def", 4, 7), (4, 7));
        assert_eq!(widen("\u{fffd}\n", 0, 3), (0, 3));
        assert_eq!(widen("a\r\rb", 1, 3), (1, 3));
    }

    #[test]
    fn a_prepend_keeps_the_scalar_it_introduces() {
        // U+0600 ARABIC NUMBER SIGN is GCB Prepend.
        let text = "\u{600}7";
        assert_eq!(widen(text, 2, 3), (0, 3));
    }

    #[test]
    fn an_empty_range_snaps_to_the_nearest_boundaries() {
        // Inside a decomposed grapheme an empty probe still widens.
        assert_eq!(widen("e\u{301}", 1, 1), (0, 3));
        assert_eq!(widen("ab", 1, 1), (1, 1));
    }
}
