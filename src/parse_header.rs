//! `ParseHeader`: what a book's own token stream says about its structure.
//!
//! ```text
//! \id GEN     book  = "GEN"
//! \c 1        runs  = [ rows 1.., label "1", occurrence 0,
//! \p a
//! \c 1                  rows 4.., label "1", occurrence 1 ]
//! ```
//!
//! A separate pass, not bookkeeping inside the scan: chapter bookkeeping in
//! `push_token` would tax every token in a book to serve ~150 of them, and a
//! caller that only wants tokens pays nothing. Everything it needs is already
//! in the stream — the scan carves `\id`'s code as `BookCode` and `\c`'s label
//! as `Designator` — so it reads rows and re-reads the source only to compare
//! label bytes.
//!
//! It owns no position and emits no tokens; it only INDEXES rows the scanner
//! already emitted, at ~1 ns per token. So the LEX stays the wall and later
//! passes need not fuse into the scan to be affordable — but read that as a
//! FLOOR: this walk has no stack, where the walker pushes frames and lint
//! allocates findings.

use std::collections::HashMap;

use crate::tables::generated;
use crate::tables::schema::MarkerKind;
use crate::token::{Token, TokenKind};

/// What one scan of a book discovers about its structure, beyond the tokens
/// themselves.
///
/// Named `ParseHeader` and not `Header` because the crate already has three
/// other "header" meanings — `MarkerKind::Header`, `ScopeKind::Header` (the
/// `\h` running-head marker) and `SpecContext::BookHeaders` (the region
/// `\toc#`/`\mt#` live in).
///
/// Three jobs, zero extra fields: nav toc, materialize-one-chapter index, and
/// the book↔slot COORDINATE ADAPTER — the run bases convert book-absolute
/// spans to slot-relative (subtract; find the run by binary search over
/// bases) and back, so a store derives its slot view without re-lexing.
#[derive(Debug, Clone, Default)]
pub struct ParseHeader {
    /// The book code after the FIRST `\id` — a SLICE, any length, kept
    /// verbatim; validation is lint's. `None` for no `\id`, or an `\id` with
    /// nothing after it.
    ///
    /// ```text
    /// \id GEN Some description   →  "GEN"   (the BookCode span; the rest is Text)
    /// \id gen                    →  "gen"
    /// \id GEN … \id EXO          →  "GEN"   later `\id`s are ordinary tokens
    /// ```
    pub book: Option<(u32, u16)>,
    /// One entry per `\c` run, in source order. Empty for a book with no
    /// chapters (a peripheral, or front matter alone) — not an error.
    pub runs: Vec<ChapterRun>,
}

/// One chapter run: the row range it covers, where its label text lives, and
/// which repeat of that label this is (reopened/duplicate chapters are real
/// data — the ordinal is derived and positional, never typed).
#[derive(Debug, Clone)]
pub struct ChapterRun {
    /// Token row indices `[first, last)`. `first` is the `\c` marker's own
    /// row, so a run OWNS its opener and the runs tile the stream from the
    /// first `\c` to EOF with no gaps.
    ///
    /// The `\id`/`\toc#`/`\h`/`\mt#` block before the first `\c` belongs to NO
    /// run — it is not a chapter and has no label. A consumer that wants a
    /// slot for it derives `0..runs[0].rows.start`, or all rows when `runs`
    /// is empty.
    pub rows: core::ops::Range<u32>,
    /// Span of the label after `\c` — the `Designator` the scan carved.
    /// ZERO-LENGTH at the marker's end when there was none (`\c` alone on its
    /// line, `\c` at EOF): the span stays useful as a coordinate and
    /// `len == 0` answers "was there one" without a second type. A missing or
    /// malformed label is lint's finding.
    pub label: (u32, u16),
    /// 0 for the first occurrence of this label in the book, 1 for the next…
    ///
    /// Keyed on the label BYTES EXACTLY: `\c 1` and `\c 01` are DIFFERENT
    /// labels and each starts its own count. Normalizing them is lint's call
    /// to make, not this index's to guess.
    pub occurrence: u8,
}

impl ParseHeader {
    /// Indexes an already-lexed book.
    ///
    /// `source` must be the same string the tokens were lexed from — it is
    /// read only to compare label bytes for `occurrence`.
    pub fn from_tokens(tokens: &[Token], source: &str) -> ParseHeader {
        let bytes = source.as_bytes();
        let mut parsed = ParseHeader::default();
        // Label bytes → how many runs have already carried them. A map rather
        // than a scan over `runs`, so a file with 100k `\c` markers stays
        // linear.
        let mut seen: HashMap<&[u8], u8> = HashMap::new();

        for (row, token) in tokens.iter().enumerate() {
            match token.kind() {
                // FIND `\id`, never index token 0 — a Byte Order Mark is legal
                // at the start of a file and lexes as Text. No marker lookup
                // needed: `Payload::BookCode` belongs to exactly one row, so
                // the first `BookCode` in the stream IS the first `\id`'s code.
                TokenKind::BookCode if parsed.book.is_none() => {
                    parsed.book = Some((token.start, token.len));
                }
                // Both conditions are load-bearing: the ROW is the authority on
                // what opens a chapter, not the payload — `\ca`/`\cp` carve a
                // `Designator` too, and `\+c` is not a chapter marker at all.
                TokenKind::Marker { nested: false }
                    if generated::kind(token.marker_idx) == MarkerKind::Chapter =>
                {
                    let row = row as u32;
                    // The previous run ends where this one starts, and the run
                    // being pushed claims everything to EOF — so the LAST run
                    // is already correct and the pass needs no epilogue.
                    if let Some(open) = parsed.runs.last_mut() {
                        open.rows.end = row;
                    }
                    let label = chapter_label(tokens, row as usize, token);
                    let count = seen
                        .entry(&bytes[label.0 as usize..label.0 as usize + label.1 as usize])
                        .or_insert(0);
                    let occurrence = *count;
                    *count = count.saturating_add(1);
                    parsed.runs.push(ChapterRun {
                        rows: row..tokens.len() as u32,
                        label,
                        occurrence,
                    });
                }
                _ => {}
            }
        }
        parsed
    }
}

/// The label span for a `\c` at `marker_row`.
fn chapter_label(tokens: &[Token], marker_row: usize, marker: &Token) -> (u32, u16) {
    let mut next = marker_row + 1;
    // A front-position attribute list belongs to the marker and so sits
    // between it and its payload (`\c |x="y"| 3`, the U25001 form). Exactly
    // one kind can intervene, so this is a step, not a search.
    if matches!(tokens.get(next).map(Token::kind), Some(TokenKind::AttrList)) {
        next += 1;
    }
    match tokens.get(next) {
        Some(t) if t.kind() == TokenKind::Designator => (t.start, t.len),
        _ => (marker.end(), 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    fn parsed(source: &str) -> ParseHeader {
        ParseHeader::from_tokens(&lex(source), source)
    }

    /// Labels as text, so the assertions read like the source they came from.
    fn labels(source: &str) -> Vec<(&str, u8)> {
        let h = parsed(source);
        h.runs
            .iter()
            .map(|r| {
                (
                    &source[r.label.0 as usize..r.label.0 as usize + r.label.1 as usize],
                    r.occurrence,
                )
            })
            .collect()
    }

    fn book(source: &str) -> Option<&str> {
        parsed(source)
            .book
            .map(|(start, len)| &source[start as usize..start as usize + len as usize])
    }

    #[test]
    fn the_book_code_is_found_not_indexed() {
        assert_eq!(book("\\id GEN\n"), Some("GEN"));
        // A BOM lexes as Text, so `\id` is not token 0 — the case the search
        // exists for.
        assert_eq!(book("\u{FEFF}\\id GEN\n"), Some("GEN"));
        assert_eq!(book("\\id GEN Some description\n"), Some("GEN"));
        // Invalid codes are kept verbatim — validation is lint's.
        assert_eq!(book("\\id gen\n"), Some("gen"));
        assert_eq!(book("\\id GENESIS\n"), Some("GENESIS"));
    }

    #[test]
    fn a_book_with_no_id_has_no_code() {
        assert_eq!(book("\\p text\n"), None);
        // `\id` with nothing after it carves no payload — no span to point at,
        // and none is invented.
        assert_eq!(book("\\id\n"), None);
    }

    #[test]
    fn only_the_first_id_supplies_the_code() {
        assert_eq!(book("\\id GEN\n\\id EXO\n"), Some("GEN"));
    }

    #[test]
    fn runs_tile_the_stream_from_the_first_chapter() {
        let source = "\\id GEN\n\\c 1\n\\p a\n\\c 2\n\\p b\n";
        let tokens = lex(source);
        let h = ParseHeader::from_tokens(&tokens, source);
        assert_eq!(h.runs.len(), 2);
        for run in &h.runs {
            let opener = tokens[run.rows.start as usize];
            assert_eq!(opener.kind(), TokenKind::Marker { nested: false });
            assert_eq!(
                &source[opener.start as usize..opener.end() as usize],
                "\\c "
            );
        }
        assert_eq!(h.runs[0].rows.end, h.runs[1].rows.start);
        assert_eq!(h.runs[1].rows.end, tokens.len() as u32);
        // The `\id` block precedes every run, so it belongs to none.
        assert!(h.runs[0].rows.start > 0);
    }

    #[test]
    fn a_book_with_no_chapters_has_no_runs() {
        assert!(parsed("\\id FRT\n\\periph Title Page\n").runs.is_empty());
    }

    #[test]
    fn duplicate_chapter_labels_are_counted_not_merged() {
        assert_eq!(
            labels("\\c 1\n\\c 2\n\\c 1\n\\c 1\n"),
            vec![("1", 0), ("2", 0), ("1", 1), ("1", 2)]
        );
    }

    /// The ordinal keys on the exact bytes; normalizing `1` and `01` into one
    /// chapter is lint's call, not this index's.
    #[test]
    fn labels_are_compared_byte_for_byte() {
        assert_eq!(labels("\\c 1\n\\c 01\n"), vec![("1", 0), ("01", 0)]);
    }

    /// The designator's absence is an empty span WHERE it would have been, not
    /// a missing field.
    #[test]
    fn a_chapter_with_no_label_gets_an_empty_span_at_its_marker() {
        // `ws_run_end` never folds a newline, so `\c` alone on its line carves
        // no designator.
        let source = "\\c\n\\p a\n";
        let h = parsed(source);
        assert_eq!(h.runs.len(), 1);
        assert_eq!(h.runs[0].label, (2, 0));
        assert_eq!(labels(source), vec![("", 0)]);
        // Two empty labels are the SAME label, so they count as repeats.
        assert_eq!(labels("\\c\n\\c\n"), vec![("", 0), ("", 1)]);
    }

    /// Keyed on the ROW's kind, not the payload: `\ca`/`\cp` carve a
    /// `Designator` too, and an alternate or published number is not a chapter.
    #[test]
    fn alternate_and_published_chapter_numbers_open_no_run() {
        assert_eq!(labels("\\c 1\n\\ca 2\\ca*\n\\cp A\n"), vec![("1", 0)]);
    }

    /// `\+c` opens NO run: only character markers nest, so the nesting
    /// spelling on a chapter is not a chapter marker. The scanner resolves it
    /// to `c`'s own row, making this pass the first place that can refuse it —
    /// which matches the walker, where it is an illegal nested marker. The
    /// finding is lint's.
    #[test]
    fn the_nesting_spelling_does_not_open_a_chapter() {
        assert_eq!(labels("\\+c 1\n"), vec![]);
        // `\+c` stays inside the run already open, so nothing is lost.
        assert_eq!(labels("\\c 1\n\\+c 2\n\\c 3\n"), vec![("1", 0), ("3", 0)]);
    }

    /// A front-position attribute list belongs to the `\c` and sits between
    /// it and its label, so the label is still found.
    #[test]
    fn a_front_position_attribute_list_does_not_hide_the_label() {
        assert_eq!(labels("\\c |x-note=\"a\"| 1\n"), vec![("1", 0)]);
    }
}
