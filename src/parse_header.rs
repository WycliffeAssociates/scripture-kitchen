//! `ParseHeader`: what a book's own token stream says about its structure.
//!
//! This is the first CONSUMER of tokens, and it is deliberately a separate
//! pass rather than bookkeeping inside the scan:
//!
//! - **The hot loop stays untouched.** Stop density is the measured wall
//!   (NEXT-STEPS "Perf, what is already known"), and chapter bookkeeping in
//!   `push_token` would tax every token in the book to serve ~150 of them.
//! - **It is opt-in.** A caller that only wants tokens — the partition
//!   oracle, a syntax highlighter — pays nothing.
//! - **Everything it needs is already in the stream.** The scan carves
//!   `\id`'s code as `BookCode` and `\c`'s label as `Designator`, and the
//!   partition oracle guarantees the stream is complete, so this pass reads
//!   rows and never re-reads the source except to compare label bytes.
//!
//! It owns no position and emits no tokens; it only INDEXES rows the scanner
//! already emitted.
//!
//! ## What a second pass costs (measured 2026-08-17, load ~15, max-of-8
//! `--iters 10`, `playground --parse-header-only` vs `serial`)
//!
//! | corpus | pass alone | ns/token | lex ns/token | pass as % of lex |
//! |---|---|---|---|---|
//! | en_ulb (prose, 4.5 MB, 255k tokens) | 11238 MiB/s | 1.50 | 10.3 | 14.6% |
//! | en_ult (aligned, 99 MB, 6.6M tokens) | 13921 MiB/s | 1.08 | 15.7 | 6.9% |
//!
//! Cross-checked: lexing en_ulb and then indexing it measured 1419 MiB/s
//! against 1637 for lex alone, and `1/(1/1637 + 1/11238) = 1428` — so the
//! two passes are simply additive, with no cache interaction worth a term.
//!
//! Those are WHOLE-CORPUS figures (0.382 ms for 66 books, 7.13 ms for 67).
//! Per book — the number an interactive caller pays — it is ~5.8 µs for
//! prose and ~106 µs for a heavily aligned book, so a pass is free at
//! interactive scale and only shows up in whole-corpus batch work, which
//! parallelizes over books anyway (Will, 2026-08-17).
//!
//! **The generalizable number is ~1 ns per token** — a load, a compare, a
//! branch, ~4 cycles. It means the LEX stays the wall, so the walker and the
//! lint listener do not have to fuse into the scan to be affordable. But read
//! it as a FLOOR, not a prediction: this pass is a read-only walk with no
//! stack, whereas the walker pushes/pops frames and reads table columns per
//! marker and lint allocates findings. Same shape, more work per row — so the
//! honest use of this number is "the walk itself is not the cost; measure
//! what you add on top of it."
//!
//! Two facts about the table above worth keeping straight:
//!
//! - **Aligned text is CHEAPER per source byte, not dearer.** A `\zaln-s`
//!   attribute list is ~150 bytes in ONE token, so en_ult's density is
//!   15.7 bytes/token against prose's 17.7 — nearly the same. Alignment
//!   inflates bytes far more than it inflates rows.
//! - **Prose's higher ns/token is FIXED cost showing through**, not slower
//!   walking: both corpora have the same 1189 chapters, so en_ulb pays the
//!   same map inserts, run pushes, and per-book allocations over 22x fewer
//!   bytes. Read `1.08` as the marginal per-token cost and the prose gap as
//!   the per-chapter/per-book overhead.

use std::collections::HashMap;

use crate::tables::generated;
use crate::tables::schema::MarkerKind;
use crate::token::{Token, TokenKind};

/// What one scan of a book discovers about its structure, beyond the tokens
/// themselves.
///
/// Named `ParseHeader` and not `Header` on purpose: this crate already has
/// three other "header" meanings — `MarkerKind::Header` and
/// `ScopeKind::Header` (the `\h` running-head marker) and
/// `SpecContext::BookHeaders` (the region `\toc#`/`\mt#` live in). This is
/// none of those; it is what PARSING a book yields about the book.
///
/// Three jobs, zero extra fields: nav toc,
/// materialize-one-chapter index, and the book↔slot COORDINATE ADAPTER —
/// the run table's base offsets convert book-absolute spans to
/// slot-relative (subtract; find the run by binary search over bases) and
/// back (add), so the store derives its slot view from one spec parse
/// without re-lexing (planning/ideas/committed/braidv2.md).
#[derive(Debug, Clone, Default)]
pub struct ParseHeader {
    /// The book code after the FIRST `\id` — a SLICE, any length, invalid
    /// codes kept verbatim, never validated or truncated ("is this three
    /// uppercase letters", "is it a known code" are both lint's). `None`
    /// when the book has no `\id`, or an `\id` with nothing after it.
    ///
    /// This is the `BookCode` token's span, so it stops at the first space:
    /// `\id GEN Some description` gives `GEN` and leaves the description as
    /// ordinary Text. Later `\id` occurrences are ordinary tokens and lint's
    /// business.
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
    /// Token row indices `[first, last)` belonging to this run. `first` is
    /// the `\c` marker's own row, so a run OWNS its opener and the runs
    /// tile the stream from the first `\c` to EOF with no gaps.
    ///
    /// Rows before the first `\c` — the BOM, `\id`, `\toc#`, `\h`, `\mt#`
    /// block — belong to NO run, because they are not a chapter and have no
    /// label. A consumer that wants a slot for them derives it:
    /// `0..runs[0].rows.start`, or all rows when `runs` is empty.
    pub rows: core::ops::Range<u32>,
    /// Span of the label text after `\c` — the `Designator` the scan carved.
    /// ZERO-LENGTH, positioned at the marker's end, when the scan carved no
    /// designator (`\c` alone on its line, `\c` at EOF): that is where the
    /// label would have gone, so the span stays useful as a coordinate and
    /// `len == 0` answers "was there one" without a second type. Whether a
    /// missing or malformed label is a finding is lint's, against the spec's
    /// `VERSE` pattern.
    pub label: (u32, u16),
    /// 0 for the first occurrence of this label in the book, 1 for the next…
    ///
    /// Keyed on the label BYTES EXACTLY, because normalization is never
    /// ours: `\c 1` and `\c 01` are DIFFERENT labels here and each starts
    /// its own count. A file that means them as the same chapter has a lint
    /// finding, not an index that guessed.
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
        // Label bytes → how many runs have already carried them. A map
        // rather than a scan over `runs` so a pathological file (junk with
        // 100k `\c` markers) stays linear.
        let mut seen: HashMap<&[u8], u8> = HashMap::new();

        for (row, token) in tokens.iter().enumerate() {
            match token.kind() {
                // FIND `\id`, never index token 0 — a Byte Order Mark is
                // allowed at the start of a file and lexes as Text. No marker
                // lookup is needed to do it: `Payload::BookCode` belongs to
                // exactly one row, so the first `BookCode` token in the
                // stream IS the first `\id`'s code.
                TokenKind::BookCode if parsed.book.is_none() => {
                    parsed.book = Some((token.start, token.len));
                }
                // TWO conditions, and both are load-bearing.
                //
                // The row is the authority on what opens a chapter, not the
                // payload: `\ca`/`\cp` also carve a `Designator` and must
                // never start a run.
                //
                TokenKind::Marker { nested: false }
                    if generated::kind(token.marker_idx) == MarkerKind::Chapter =>
                {
                    let row = row as u32;
                    // The previous run ends where this one starts. The run
                    // being pushed claims everything to EOF, so the LAST run
                    // is already correct and this pass needs no epilogue.
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
    // A front-position attribute list BELONGS to the marker and therefore
    // sits between it and its payload (`\c |x="y"| 3`, the U25001 form — the
    // scanner keeps the payload owed across it on purpose). Exactly one kind
    // can intervene, so this is a step, not a search.
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
        // A BOM is legal at the start of a file and lexes as Text, so `\id`
        // is not token 0. This is the case the search exists for.
        assert_eq!(book("\u{FEFF}\\id GEN\n"), Some("GEN"));
        // The description after the code is content, not part of it.
        assert_eq!(book("\\id GEN Some description\n"), Some("GEN"));
        // Invalid codes are kept verbatim — validation is lint's.
        assert_eq!(book("\\id gen\n"), Some("gen"));
        assert_eq!(book("\\id GENESIS\n"), Some("GENESIS"));
    }

    #[test]
    fn a_book_with_no_id_has_no_code() {
        assert_eq!(book("\\p text\n"), None);
        // `\id` with nothing after it carves no payload, so there is no span
        // to point at — and none is invented.
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
        // Each run opens on its own `\c` marker…
        for run in &h.runs {
            let opener = tokens[run.rows.start as usize];
            assert_eq!(opener.kind(), TokenKind::Marker { nested: false });
            assert_eq!(
                &source[opener.start as usize..opener.end() as usize],
                "\\c "
            );
        }
        // …they are contiguous, and the last reaches EOF.
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

    /// Normalization is never ours, so the ordinal keys on the exact bytes:
    /// a file that means `1` and `01` as one chapter gets a lint finding, not
    /// a header that decided for it.
    #[test]
    fn labels_are_compared_byte_for_byte() {
        assert_eq!(labels("\\c 1\n\\c 01\n"), vec![("1", 0), ("01", 0)]);
    }

    /// The designator's absence is recorded as an empty span WHERE it would
    /// have been, not as a missing field.
    #[test]
    fn a_chapter_with_no_label_gets_an_empty_span_at_its_marker() {
        // `\c` alone on its line: the scan carves no designator, because
        // `ws_run_end` never folds a newline.
        let source = "\\c\n\\p a\n";
        let h = parsed(source);
        assert_eq!(h.runs.len(), 1);
        assert_eq!(h.runs[0].label, (2, 0));
        assert_eq!(labels(source), vec![("", 0)]);
        // Two of them are the same (empty) label, so they count as repeats —
        // which is the honest reading: both say nothing.
        assert_eq!(labels("\\c\n\\c\n"), vec![("", 0), ("", 1)]);
    }

    /// Keyed on the ROW's kind, not on the payload: `\ca`/`\cp` carve a
    /// `Designator` too, and an alternate or published number is not a
    /// chapter (their rows carry empty context slices for the same reason —
    /// they are adjacency facts, see NEXT-STEPS).
    #[test]
    fn alternate_and_published_chapter_numbers_open_no_run() {
        assert_eq!(labels("\\c 1\n\\ca 2\\ca*\n\\cp A\n"), vec![("1", 0)]);
    }

    /// `\+c` opens NO run. Only character markers nest, so the nesting
    /// spelling on a chapter is not a chapter marker at all — and the scanner
    /// resolves it to `c`'s own row, so this pass is the first place that can
    /// refuse it. Refusing keeps it consistent with the walker, which sees an
    /// illegal nested marker; the finding is lint's.
    #[test]
    fn the_nesting_spelling_does_not_open_a_chapter() {
        assert_eq!(labels("\\+c 1\n"), vec![]);
        // Its neighbours are unaffected: the run `\+c` sits inside is the one
        // that was already open, so nothing is lost — it just isn't a chapter.
        assert_eq!(labels("\\c 1\n\\+c 2\n\\c 3\n"), vec![("1", 0), ("3", 0)]);
    }

    /// A front-position attribute list belongs to the `\c` and sits between
    /// it and its label, so the label is still found.
    #[test]
    fn a_front_position_attribute_list_does_not_hide_the_label() {
        assert_eq!(labels("\\c |x-note=\"a\"| 1\n"), vec![("1", 0)]);
    }
}
