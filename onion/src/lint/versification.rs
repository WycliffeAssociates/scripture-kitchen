//! The two findings a book can only get against a versification's counts.
//!
//! ```text
//! \c 3 … \v 21  and the scheme has 22   →  chapter-ends-early @ \v 21    aux 22
//! \c 3 … \v 18  and the scheme has 17   →  verse-beyond-scheme @ \v 18   aux 17
//! ```
//!
//! [`lint`](super::lint) never emits these, and onion never learns what a
//! scheme is: [`lint_against_counts`] takes the counts as a function, which a
//! caller fills from whatever it knows (canon's `Scheme::last_verse`, a
//! project's declared scheme). Not calling it is the off switch, and hiding
//! either code is the host's per-rule suppression, as for any other.
//!
//! A chapter the counts don't know (`None`) is skipped, and so is a verse
//! whose designator is malformed: `verse-without-designator` and friends
//! already report it.

use super::{Code, NO_TOKEN, Observation};
use crate::toc::Toc;

/// Every `chapter-ends-early` and `verse-beyond-scheme` in a book, sorted by
/// anchor. `expected_last(chapter)` is the versification's last verse of that
/// chapter, or `None` when it has no such chapter.
///
/// A reopened chapter (a second `\c 3`) is judged as one chapter: its verses
/// are pooled by number, as [`Toc::chapter_span`] answers about a number.
pub fn lint_against_counts(
    toc: &Toc,
    expected_last: impl Fn(u16) -> Option<u16>,
) -> Vec<Observation> {
    let mut out = Vec::new();
    // (chapter, highest verse so far, its anchor token, expected)
    let mut highest: Vec<(u16, u16, u32, u16)> = Vec::new();
    for verse in &toc.verses {
        if verse.chapter == 0 || verse.first == 0 {
            continue;
        }
        let Some(expected) = expected_last(verse.chapter) else {
            continue;
        };
        if verse.last > expected {
            out.push(Observation {
                code: Code::VerseBeyondScheme,
                anchor: verse.token,
                second: NO_TOKEN,
                aux: u32::from(expected),
            });
        }
        match highest
            .iter_mut()
            .find(|(chapter, ..)| *chapter == verse.chapter)
        {
            Some(row) if verse.last > row.1 => {
                row.1 = verse.last;
                row.2 = verse.token;
            }
            Some(_) => {}
            None => highest.push((verse.chapter, verse.last, verse.token, expected)),
        }
    }
    for (_, last, token, expected) in highest {
        if last < expected {
            out.push(Observation {
                code: Code::ChapterEndsEarly,
                anchor: token,
                second: NO_TOKEN,
                aux: u32::from(expected),
            });
        }
    }
    out.sort_by_key(|o| (o.anchor, o.code as u32));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lex, toc};

    /// `(code, aux)` per finding, for a book judged against `counts`
    /// (chapter `n` has `counts[n - 1]` verses).
    fn judged(source: &str, counts: &[u16]) -> Vec<(Code, u32)> {
        let toc = toc(source.as_bytes(), &lex(source));
        lint_against_counts(&toc, |chapter| {
            counts.get(usize::from(chapter).checked_sub(1)?).copied()
        })
        .into_iter()
        .map(|o| (o.code, o.aux))
        .collect()
    }

    #[test]
    fn a_chapter_one_verse_short_ends_early() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b\n\\c 2\n\\p \\v 1 c \\v 2 d \\v 3 e\n";
        assert_eq!(judged(source, &[3, 3]), [(Code::ChapterEndsEarly, 3)]);
        assert!(judged(source, &[2, 3]).is_empty());
    }

    #[test]
    fn a_verse_past_the_count_is_beyond_the_scheme() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b \\v 3 c \\v 4 d\n";
        assert_eq!(
            judged(source, &[2]),
            [(Code::VerseBeyondScheme, 2), (Code::VerseBeyondScheme, 2)]
        );
    }

    #[test]
    fn a_bridge_counts_by_its_top() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2-3 b\n";
        assert!(judged(source, &[3]).is_empty());
        assert_eq!(judged(source, &[2]), [(Code::VerseBeyondScheme, 2)]);
    }

    #[test]
    fn the_finding_sits_on_the_chapters_highest_verse_not_its_last_written() {
        // Out of order: the last `\v` written is 2, the highest is 3.
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 3 c \\v 2 b\n";
        let toc = toc(source.as_bytes(), &lex(source));
        let found = lint_against_counts(&toc, |_| Some(4));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].anchor, toc.verses[1].token, "on \\v 3");
    }

    #[test]
    fn chapters_the_counts_dont_know_and_malformed_verses_are_skipped() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 9\n\\p \\v 1 b\n\\c 2\n\\p \\v x c\n";
        assert_eq!(judged(source, &[5, 5]), [(Code::ChapterEndsEarly, 5)]);
    }

    #[test]
    fn neither_code_is_ever_a_plain_lint_finding() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b\n";
        let tokens = lex(source);
        let report = crate::lint::lint(source.as_bytes(), &tokens, &crate::cst::build(&tokens));
        assert!(
            !report
                .observations
                .iter()
                .any(|o| matches!(o.code, Code::ChapterEndsEarly | Code::VerseBeyondScheme))
        );
    }
}
