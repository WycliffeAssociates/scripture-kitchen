//! `vref`: a line per verse, rendered from a [`Toc`] and a [`Mask`].
//!
//! ```text
//! \id GEN
//! \c 1
//! \p
//! \v 1 In the beginning\f + \ft a note\f*
//! \v 2-3 Formless, and void.
//! \v 4 Let there be light.
//!
//! let (toc, m) = (toc(source, &tokens), mask(source, &tokens, &cst, &Filter::verse_text()));
//! vref::joined(&toc, &m, source, true) ==
//!     "GEN 1:1\tIn the beginning
//!      GEN 1:2\tFormless, and void.
//!      GEN 1:3\t<range>
//!      GEN 1:4\tLet there be light."
//! ```
//!
//! Two parallel files is ebible's shape — [`keys`] alone, [`lines`] alone, keyed
//! by LINE NUMBER — so the core here is an iterator ([`verses`]) and every format
//! is a one-liner over it. Line N of every translation naming the same verse is
//! the whole point, which is why a bridge (`\v 2-3`) puts its text on the FIRST
//! verse's line and gives every verse it covers a [`RANGE`] line instead of
//! omitting them: an omitted verse slides every later verse onto the wrong line.
//!
//! Nothing here is a second filter. Which bytes are verse text was decided by
//! the [`Mask`] the caller hands in — `Filter::verse_text()` for a plain reading
//! text, the same with `kinds[Note] = Unwrap` to fold footnote prose in. The
//! renderer's only liberties are the ones the FORMAT demands: a line cannot
//! contain a newline and a column cannot contain a tab, so each becomes a space,
//! one byte for one byte. `trim` lives here for the same reason — a vref line's
//! edges are presentation, where the Mask never trims, because a mask offset has
//! to keep naming a real document byte.
//!
//! (Full alignment to ebible's master vref.txt also wants blank lines for verses
//! a book never had, which needs a versification table this crate does not own:
//! these renderers render what the file contains.)

use crate::mask::Mask;
use crate::toc::{Sid, Toc};

/// The text of a verse a bridge covers. ebible's spelling, kept verbatim.
pub const RANGE: &str = "<range>";

/// What [`joined`] puts between a key and its text. A tab, because verse text
/// carries spaces freely and tabs essentially never.
pub const JOIN: char = '\t';

/// One `(Sid, text)` per verse SLOT, in source order.
///
/// `toc` and `mask` must both be built from `source`. Each verse's text is the
/// masked bytes inside its extent (its `\v` to the next `\v`/`\c`/end of
/// chapter), with newlines flattened to spaces.
///
/// - a bridge yields one item per verse it names: the first carries the text,
///   the rest carry [`RANGE`].
/// - a verse whose extent survived the mask empty yields an EMPTY string, never
///   nothing — a dropped line would misalign every line after it.
/// - a `\v` before the first `\c` is front matter, and front matter is not a
///   verse: it yields no line at all.
/// - a `\v` whose designator the Toc could not number yields a chapter-only key
///   (`GEN 3`), keeping its text rather than inventing a verse number for it.
pub fn verses<'a>(toc: &'a Toc, mask: &'a Mask, source: &'a [u8]) -> Verses<'a> {
    Verses {
        toc,
        mask,
        source,
        row: 0,
        covered: None,
        range_at: 0,
    }
}

/// The keys file: one sid per line, the reference half of the parallel pair.
pub fn keys(toc: &Toc, mask: &Mask, source: &[u8]) -> String {
    join(verses(toc, mask, source).map(|(sid, _)| sid.to_string()))
}

/// The lines file: one verse text per line, keyed only by its line NUMBER.
/// `trim` drops each line's leading and trailing whitespace, never its interior.
pub fn lines(toc: &Toc, mask: &Mask, source: &[u8], trim: bool) -> String {
    join(verses(toc, mask, source).map(|(_, text)| trimmed(text, trim)))
}

/// Both halves on one line, `sid`, [`JOIN`], text — the human-readable form, and
/// what the playground's `--vref` prints.
pub fn joined(toc: &Toc, mask: &Mask, source: &[u8], trim: bool) -> String {
    join(verses(toc, mask, source).map(|(sid, text)| format!("{sid}{JOIN}{}", trimmed(text, trim))))
}

/// Lines into a file body: separated, not terminated, so a caller who wants a
/// trailing newline says so.
fn join(lines: impl Iterator<Item = String>) -> String {
    let mut out = String::new();
    for line in lines {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out
}

fn trimmed(text: String, trim: bool) -> String {
    if trim { text.trim().to_string() } else { text }
}

/// [`verses`]' iterator: a cursor over the verse anchors, plus the tail of a
/// bridge it is still paying out.
pub struct Verses<'a> {
    toc: &'a Toc,
    mask: &'a Mask,
    source: &'a [u8],
    /// The next verse anchor to read.
    row: usize,
    /// A bridge's remaining verses as `(chapter, next, last)` — the [`RANGE`]
    /// lines owed after its text line.
    covered: Option<(u16, u16, u16)>,
    /// A cursor into `mask.ranges`. Extents ascend, so it only ever moves
    /// forward: the whole book is one pass over the range set, not a search per
    /// verse.
    range_at: usize,
}

impl Iterator for Verses<'_> {
    type Item = (Sid, String);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((chapter, next, last)) = self.covered {
            self.covered = (next < last).then_some((chapter, next + 1, last));
            return Some((self.sid(chapter, next), RANGE.to_string()));
        }

        loop {
            let anchor = *self.toc.verses.get(self.row)?;
            self.row += 1;
            // Chapter 0 is the front matter: a `\v` there names no verse a vref
            // file has a slot for.
            if anchor.chapter == 0 {
                continue;
            }
            let text = self.extent_text(anchor.at, self.extent_end(anchor.at));
            if anchor.first < anchor.last {
                self.covered = Some((anchor.chapter, anchor.first + 1, anchor.last));
            }
            return Some((self.sid(anchor.chapter, anchor.first), text));
        }
    }
}

impl Verses<'_> {
    /// One line's reference. ONE verse per line even for a bridge — the whole
    /// reason `<range>` lines exist — where [`Toc::locate`] answers with the
    /// bridge's full span, because it is answering about a byte.
    fn sid(&self, chapter: u16, verse: u16) -> Sid {
        Sid {
            book: self.toc.book,
            chapter,
            first: verse,
            last: verse,
        }
    }

    /// Where the verse anchored at `at` stops: the next `\v`, or the end of its
    /// chapter when there is none inside it.
    fn extent_end(&self, at: u32) -> u32 {
        let chapter = self.toc.chapters[self
            .toc
            .chapters
            .partition_point(|row| row.start <= at)
            .saturating_sub(1)]
        .end;
        let next = self.toc.verses.get(self.row).map_or(chapter, |v| v.at);
        next.min(chapter)
    }

    /// The masked bytes of one extent, as a LINE: a newline or a tab becomes a
    /// space, because a line cannot hold a line break and a column cannot hold
    /// its own separator (en_ult PSA really has a tab mid-verse). One byte for
    /// one byte, so nothing else about the text the mask chose moves.
    fn extent_text(&mut self, start: u32, end: u32) -> String {
        while self
            .mask
            .ranges
            .get(self.range_at)
            .is_some_and(|range| range.end <= start)
        {
            self.range_at += 1;
        }
        let mut out = String::new();
        for range in &self.mask.ranges[self.range_at..] {
            if range.start >= end {
                break;
            }
            let from = range.start.max(start) as usize;
            let to = range.end.min(end) as usize;
            let part =
                std::str::from_utf8(&self.source[from..to]).expect("mask ranges are token spans");
            for ch in part.chars() {
                out.push(match ch {
                    '\n' | '\r' | '\t' => ' ',
                    other => other,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::{Filter, mask};
    use crate::{cst, lex, toc};

    /// The joined rendering of one book, trimmed — the shape a reader checks.
    fn rendered(source: &str) -> String {
        let bytes = source.as_bytes();
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        let toc = toc(bytes, &tokens);
        let m = mask(bytes, &tokens, &cst, &Filter::verse_text());
        joined(&toc, &m, bytes, true)
    }

    #[test]
    fn a_verse_renders_its_masked_text() {
        assert_eq!(
            rendered("\\id GEN\n\\c 1\n\\p \\v 1 In the \\add beginning\\add*\\f + \\ft n\\f*\n"),
            "GEN 1:1\tIn the beginning"
        );
    }

    #[test]
    fn a_bridge_puts_its_text_on_the_first_line_and_ranges_the_rest() {
        assert_eq!(
            rendered("\\id GEN\n\\c 1\n\\p \\v 1-3 One two three. \\v 4 Four.\n"),
            "GEN 1:1\tOne two three.\n\
             GEN 1:2\t<range>\n\
             GEN 1:3\t<range>\n\
             GEN 1:4\tFour."
        );
    }

    #[test]
    fn keys_and_lines_have_the_same_line_count() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1-3 a\n\\v 4 b\n\\c 2\n\\p \\v 1 c\n";
        let bytes = source.as_bytes();
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        let toc = toc(bytes, &tokens);
        let m = mask(bytes, &tokens, &cst, &Filter::verse_text());
        assert_eq!(
            keys(&toc, &m, bytes),
            "GEN 1:1\nGEN 1:2\nGEN 1:3\nGEN 1:4\nGEN 2:1"
        );
        assert_eq!(lines(&toc, &m, bytes, true), "a\n<range>\n<range>\nb\nc");
    }

    #[test]
    fn an_empty_verse_still_takes_its_line() {
        // Nothing but a note: the extent survives the mask empty, and the line
        // stays so verse 3 is still the third line.
        assert_eq!(
            rendered("\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 \\f + \\ft n\\f* \\v 3 c\n"),
            "GEN 1:1\ta\nGEN 1:2\t\nGEN 1:3\tc"
        );
    }

    #[test]
    fn front_matter_is_not_a_verse() {
        // A `\v` before any `\c` is chapter 0 — no slot, no line.
        assert_eq!(
            rendered("\\id GEN\n\\p \\v 1 stray\n\\c 1\n\\p \\v 1 real\n"),
            "GEN 1:1\treal"
        );
        // A book with no verses at all renders nothing.
        assert_eq!(rendered("\\id FRT\n\\c 1\n\\p intro\n"), "");
    }

    #[test]
    fn an_empty_chapter_contributes_no_lines() {
        assert_eq!(
            rendered("\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 2\n\\c 3\n\\p \\v 1 c\n"),
            "GEN 1:1\ta\nGEN 3:1\tc"
        );
    }

    #[test]
    fn a_verse_the_toc_could_not_number_keeps_its_text_under_a_chapter_key() {
        // The en_ulb ZEC shape: `\v 2"` carves a malformed designator, so the
        // anchor has no number. Its text is real, so the line is real.
        assert_eq!(
            rendered("\\id ZEC\n\\c 1\n\\p \\v 1 a \\v 2\" b \\v 3 c\n"),
            "ZEC 1:1\ta\nZEC 1\tb\nZEC 1:3\tc"
        );
    }

    #[test]
    fn newlines_inside_a_verse_become_spaces() {
        let source = "\\id PSA\n\\c 1\n\\q1 \\v 1 Blessed is the man\n\\q2 who walks not\n";
        assert_eq!(
            rendered(source),
            "PSA 1:1\tBlessed is the man who walks not"
        );
        // Untrimmed, the same line keeps the edge whitespace the mask kept and
        // its interior is untouched.
        let bytes = source.as_bytes();
        let tokens = lex(source);
        let cst = cst::build(&tokens);
        let toc = toc(bytes, &tokens);
        let m = mask(bytes, &tokens, &cst, &Filter::verse_text());
        assert_eq!(
            lines(&toc, &m, bytes, false),
            "Blessed is the man who walks not "
        );
    }

    #[test]
    fn a_tab_inside_a_verse_becomes_a_space() {
        // en_ult PSA has one: left alone it would forge a second column in the
        // joined form.
        assert_eq!(
            rendered("\\id PSA\n\\c 1\n\\p \\v 1 a\tb\n"),
            "PSA 1:1\ta b"
        );
    }

    #[test]
    fn trim_touches_only_the_edges() {
        // Two spaces mid-line (a `\add` unwrapped from between them) survive.
        let out = rendered("\\id GEN\n\\c 1\n\\p \\v 1  a  \\add \\add* b  \n");
        assert_eq!(out, "GEN 1:1\ta   b");
    }

    #[test]
    fn a_duplicated_chapter_renders_duplicated_keys() {
        // Real data (bdf_reg reopens a chapter). The renderer reports what the
        // file says; deduping would be a versification opinion.
        assert_eq!(
            rendered("\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 1\n\\p \\v 1 b\n"),
            "GEN 1:1\ta\nGEN 1:1\tb"
        );
    }

    #[test]
    fn a_verse_stops_at_its_chapters_end() {
        // Chapter 1's last verse must not swallow chapter 2's heading text.
        assert_eq!(
            rendered("\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 2\n\\s head\n\\p \\v 1 b\n"),
            "GEN 1:1\ta\nGEN 2:1\tb"
        );
    }
}
