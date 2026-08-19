//! Lint's half of the fix model: the offered repair, the sequence fixes, and
//! the oracle. The byte-splice primitives it is built from are `crate::edit`.

use std::ops::Range;

use super::walk::span_of;
use super::{Code, Emit, LINT_ROWS, LintReport, Observation, lint};
use crate::designator::{self, Designator};
use crate::edit::{Edit, apply};
use crate::tables::generated;
use crate::tables::schema::MarkerKind;
use crate::{Token, TokenKind};

// ---------------------------------------------------------------------------
// The fix model: byte-splice edit lists (model A, ruled 2026-08-19)
// ---------------------------------------------------------------------------

/// One offered repair: a label and a range into [`LintReport::edit_list`].
///
/// The edits inside a fix are sorted by `from` and non-overlapping, and their
/// total order is (`from`, sequence) — equal `from` is legal and means
/// CONCATENATION, which is how text longer than a [`FixStr`] is carried. Apply
/// them RIGHT TO LEFT ([`apply`]) and every offset stays valid.
///
/// **Offered, never applied.** Never-synthesize governs TOKENS; a fix is
/// proposed TEXT. Nothing in this module rewrites a byte — once a user accepts
/// a fix the bytes are real and the next re-lex is honest about them.
///
/// [`FixStr`]: crate::edit::FixStr
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// From [`LintRow::fix_label`] — the row is where a rule's affordances are
    /// declared, so the label is never authored twice. Static, like every other
    /// string here: the digits of "renumber to 12" live in the EDIT, which is
    /// why the label stays generic.
    ///
    /// [`LintRow::fix_label`]: super::LintRow::fix_label
    pub label: &'static str,
    pub edits: Range<u32>,
}

/// A sequence finding, plus "renumber to the expected number" WHEN that is a
/// splice and not an interpretation.
///
/// All four anomaly codes come through here and only the duplicate and
/// out-of-order pairs are ever fixed; the two GAP codes are refused by the row
/// column itself, which declares no label for them (closing a hole would
/// renumber the rest of the chapter, and no single splice can do that). The row
/// stays the one place a rule's affordances are declared.
///
/// Two further conditions, both learned from real text:
///
/// - The designator must be PLAIN DIGITS. `\v 12-14` renumbered to one number
///   silently drops the verses the range covered, and `\v 2a` loses its segment
///   — both are interpretations of what the author meant, not repairs of how
///   they wrote it.
/// - The next number in the sequence must be strictly ABOVE the one we would
///   write. bdf_reg ROM 3 is the case that earned this: it writes `\v 10` twice
///   and then `\v 11`, so renumbering the duplicate to 11 would only move the
///   duplicate one verse along. The same guard covers the mirror shape for
///   out-of-order (`\v 5 \v 2 \v 3`, where renumbering the 2 to 6 would make
///   the following 3 the one going backwards). The fix oracle catches both, and
///   this is the fix admitting it rather than the oracle catching us.
pub(super) fn renumber(
    source: &[u8],
    tokens: &[Token],
    out: &mut Emit,
    observation: Observation,
    verse: bool,
) {
    let token = &tokens[observation.anchor as usize];
    let expected = observation.aux;
    let renumberable = observation.code.row().fix_label.is_some()
        && span_of(source, token).iter().all(u8::is_ascii_digit)
        && next_number(source, tokens, observation.anchor, verse)
            .is_none_or(|next| next > expected);
    if !renumberable {
        out.push(observation);
        return;
    }
    let mut buf = [0u8; 10];
    out.push_fixed(
        observation,
        token.start,
        token.end(),
        decimal(expected, &mut buf),
    );
}

/// The FIRST number of the next designator in this sequence — the one the
/// renumber has to stay clear of.
///
/// The only lookahead anywhere in lint, and it is affordable for the reason
/// every fix computation is: it runs on a FINDING, never on a token. It also
/// stops at the very next `\v`/`\c`, so the walk is a handful of tokens except
/// for the last finding in a book.
///
/// `None` covers three cases a renumber may treat alike: no next designator, a
/// next one that is malformed (which resyncs the sequence, so it compares
/// against nothing), and — for verses — an intervening `\c`, which resets the
/// verse sequence entirely.
fn next_number(source: &[u8], tokens: &[Token], from: u32, verse: bool) -> Option<u32> {
    let wanted = if verse {
        MarkerKind::Verse
    } else {
        MarkerKind::Chapter
    };
    let mut awaiting = false;
    for token in &tokens[from as usize + 1..] {
        match token.kind() {
            // Stepped over, exactly as the pass itself steps over it.
            TokenKind::AttrList => {}
            TokenKind::Designator if awaiting => {
                let span = span_of(source, token);
                let parsed = if verse {
                    designator::verse(span)
                } else {
                    designator::chapter(span)
                };
                return match parsed {
                    Designator::Wellformed { first, .. } => Some(first),
                    Designator::Malformed => None,
                };
            }
            TokenKind::Marker { .. } => {
                let kind = generated::kind(token.marker_idx);
                if verse && kind == MarkerKind::Chapter {
                    return None;
                }
                awaiting = kind == wanted;
            }
            _ => awaiting = false,
        }
    }
    None
}

/// A number as ASCII digits, written back to front into the caller's buffer
/// (ten digits holds any u32).
fn decimal(number: u32, buf: &mut [u8; 10]) -> &[u8] {
    let mut at = buf.len();
    let mut rest = number;
    loop {
        at -= 1;
        buf[at] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            return &buf[at..];
        }
    }
}

// ---------------------------------------------------------------------------
// Applying a fix, and the oracle that proves one
// ---------------------------------------------------------------------------

/// THE FIX ORACLE: the composability rule made executable.
///
/// Applies the fixes of the given observations (indices into
/// `report.observations`, one for a single fix, many for a "fix all of code X"
/// dispatch), re-lexes, re-builds and re-lints, and demands all four of:
///
/// 1. the edits are sorted, non-overlapping, and inside the source;
/// 2. each fixed finding is GONE — no finding of that code survives at that
///    anchor's position, mapped through the edits that moved it;
/// 3. NO code's count rises. A fix may not hand back a new finding;
/// 4. the partition oracle still holds on the fixed text.
///
/// Condition 2 is per-SITE and not a count, and the reason is a real corpus
/// case rather than fastidiousness. `missing-paragraph` reports once per
/// paragraph-less RUN; in en_ulb a run is repeatedly re-opened by `\s5`, whose
/// row-0 pop-all kills whatever paragraph is standing — so inserting the
/// proposed `\p` repairs the reported site and UNMASKS the next segment of the
/// same run, which was damaged all along and merely aggregated away. The total
/// does not move (36 findings before, 36 after in PHM), no damage is created,
/// and a strict "the count went down" test would have called that a failure. A
/// fix answers for its own site; it does not answer for what the rule's
/// aggregation was hiding behind it.
///
/// A verification tool, not part of the reporting path: it re-runs the whole
/// pipeline and returns an allocated message. It is public because the rule it
/// enforces is a property of the LIBRARY's fixes, and the tests that run it over
/// 226 books live outside this module.
pub fn check_fixes(
    source: &str,
    tokens: &[Token],
    report: &LintReport,
    observations: &[u32],
) -> Result<(), String> {
    let mut edits: Vec<Edit> = Vec::new();
    // (code, the anchor's byte offset) — the site each fix answers for.
    let mut targets: Vec<(Code, u32)> = Vec::new();
    for index in observations {
        let index = *index as usize;
        let fix = report
            .fix(index)
            .ok_or_else(|| format!("observation {index} offers no fix"))?;
        let own = report.edits(fix);
        for pair in own.windows(2) {
            if pair[1].from < pair[0].to || pair[1].from < pair[0].from {
                return Err(format!(
                    "{}: edits are out of order or overlap: {pair:?}",
                    fix.label
                ));
            }
        }
        let observation = report.observations[index];
        targets.push((observation.code, tokens[observation.anchor as usize].start));
        edits.extend_from_slice(own);
    }
    // Stable, so a fix's own concatenation chain keeps its (from, sequence)
    // order after several fixes are merged into one dispatch.
    edits.sort_by_key(|edit| edit.from);
    for pair in edits.windows(2) {
        if pair[1].from < pair[0].to {
            return Err(format!("merged fixes overlap: {pair:?}"));
        }
    }
    if let Some(edit) = edits.iter().find(|edit| {
        edit.to as usize > source.len()
            || edit.from > edit.to
            || !source.is_char_boundary(edit.to as usize)
    }) {
        return Err(format!(
            "edit is not a valid splice of the source: {edit:?}"
        ));
    }

    let fixed = apply(source.as_bytes(), &edits);
    let fixed = String::from_utf8(fixed).map_err(|error| format!("fix broke UTF-8: {error}"))?;
    let tokens_after = crate::lex(&fixed);
    let cst = crate::cst::build(&tokens_after);
    let after = lint(fixed.as_bytes(), &tokens_after, &cst);

    let mut before_counts = [0u32; LINT_ROWS.len()];
    let mut after_counts = [0u32; LINT_ROWS.len()];
    for obs in &report.observations {
        before_counts[obs.code as usize] += 1;
    }
    for obs in &after.observations {
        after_counts[obs.code as usize] += 1;
    }
    for (code, site) in &targets {
        // Where that anchor's first byte ended up: every edit that lands wholly
        // at or before it moves it, and nothing else does. An insertion exactly
        // AT the anchor (`missing-paragraph`, `marker-not-ws-preceded`) pushes
        // it right; a replacement OF the anchor (a renumber) leaves it where it
        // was, which is the same rule read the other way.
        let mut moved = i64::from(*site);
        for edit in &edits {
            if edit.to <= *site {
                moved += edit.insert.as_bytes().len() as i64 - i64::from(edit.to - edit.from);
            }
        }
        let survived = after.observations.iter().any(|obs| {
            obs.code == *code && i64::from(tokens_after[obs.anchor as usize].start) == moved
        });
        if survived {
            return Err(format!(
                "{} was not repaired: it still fires at byte {moved}",
                code.row().name
            ));
        }
    }
    for (slot, row) in LINT_ROWS.iter().enumerate() {
        if after_counts[slot] > before_counts[slot] {
            return Err(format!(
                "the fix introduced {}: {} findings before, {} after",
                row.name, before_counts[slot], after_counts[slot]
            ));
        }
    }

    // The standing invariant, re-asserted on text nobody has lexed before: the
    // spans of the fixed source still concatenate back to it.
    let mut cursor = 0usize;
    for token in &tokens_after {
        if token.start as usize != cursor {
            return Err(format!(
                "the fixed text does not partition: token at {} follows a span ending at {cursor}",
                token.start
            ));
        }
        cursor += token.len as usize;
    }
    if cursor != fixed.len() {
        return Err(format!(
            "the fixed text does not partition: spans end at {cursor} of {}",
            fixed.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::build;
    use crate::edit::FixStr;
    use crate::lex;

    // -----------------------------------------------------------------
    // Phase 4: fixes
    // -----------------------------------------------------------------

    /// One document's report, for the fix tests — which state their snippet in
    /// full (no `\id` prefix is added) because the repaired TEXT is the
    /// assertion and a hidden prefix would not appear in it.
    fn report_of(usfm: &str) -> (Vec<Token>, LintReport) {
        let tokens = lex(usfm);
        let cst = build(&tokens);
        let report = lint(usfm.as_bytes(), &tokens, &cst);
        (tokens, report)
    }

    fn slots_for(report: &LintReport, code: Code) -> Vec<u32> {
        report
            .observations
            .iter()
            .enumerate()
            .filter(|(_, obs)| obs.code == code)
            .map(|(index, _)| index as u32)
            .collect()
    }

    /// Runs the ORACLE over the fixes for `code` and returns the repaired text.
    ///
    /// Every fix test below goes through here, so each one is also an oracle
    /// test; what the test itself then states is the repaired document, which is
    /// the only form in which an edit list is checkable by eye.
    fn repaired(usfm: &str, code: Code) -> String {
        let (tokens, report) = report_of(usfm);
        let slots = slots_for(&report, code);
        assert!(!slots.is_empty(), "no {} in {usfm:?}", code.row().name);
        check_fixes(usfm, &tokens, &report, &slots)
            .unwrap_or_else(|error| panic!("{} on {usfm:?}: {error}", code.row().name));
        let mut edits: Vec<Edit> = slots
            .iter()
            .flat_map(|slot| {
                report
                    .edits(report.fix(*slot as usize).expect("the fix exists"))
                    .to_vec()
            })
            .collect();
        edits.sort_by_key(|edit| edit.from);
        String::from_utf8(apply(usfm.as_bytes(), &edits)).expect("fixes are ASCII")
    }

    /// Does this code offer a fix on this snippet at all?
    fn offers_fix(usfm: &str, code: Code) -> bool {
        let (_, report) = report_of(usfm);
        slots_for(&report, code)
            .iter()
            .any(|slot| report.fix(*slot as usize).is_some())
    }

    #[test]
    fn text_longer_than_a_fixstr_splits_into_concatenating_edits() {
        // No fix in the table is this long (the worst is `\table-e\*`, ten
        // bytes), so the splitter is exercised directly: it is the reason the
        // inline string needs no cap.
        let mut out = Emit::default();
        let long = b"0123456789abcdefghijklmnopqr";
        out.push_fixed(Observation::one(Code::OrphanCloser, 0), 4, 7, long);
        let fix = out.fixes[0].clone();
        let edits = &out.edit_list[fix.edits.start as usize..fix.edits.end as usize];
        assert_eq!(edits.len(), 2);
        // Only the FIRST edit carries the replaced range; the tail is a pure
        // insertion at its end, so right-to-left application concatenates.
        assert_eq!(edits[0].from, 4);
        assert_eq!(edits[0].to, 7);
        assert_eq!(
            edits[1],
            Edit {
                from: 7,
                to: 7,
                insert: FixStr::new(&long[15..])
            }
        );
        assert_eq!(
            String::from_utf8(apply(b"....xyz....", edits)).unwrap(),
            format!("....{}....", core::str::from_utf8(long).unwrap())
        );
    }

    #[test]
    fn a_missing_closer_lands_at_the_last_content_byte() {
        // The corpus shape: a `\c` inside a footnote. The closer goes where the
        // note's content ends, in front of what displaced it.
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\ft note\\c 2\n\\p b",
                Code::UnclosedNote
            ),
            "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\ft note\\f*\\c 2\n\\p b"
        );

        // …and when the displacer is on the NEXT LINE, the closer stays on this
        // one: the extent ends after the newline, the fix backs off over it.
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\ft note\n\\c 2\n\\p b",
                Code::UnclosedNote
            ),
            "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\ft note\\f*\n\\c 2\n\\p b"
        );

        // A character marker keeps the author's own SPELLING — `\+nd` is closed
        // by `\+nd*`, which the canonical row name could not have said.
        assert_eq!(
            repaired("\\id GEN\n\\p \\add a \\+nd b\\add*", Code::UnclosedChar),
            "\\id GEN\n\\p \\add a \\+nd b\\+nd*\\add*"
        );

        // `\ca` is one of these since Will's 2026-08-19 ruling — a character
        // scope like any other, so an unclosed one is repaired the same way.
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\ca 2\n\\c 2\n\\p \\v 1 a",
                Code::UnclosedChar
            ),
            "\\id GEN\n\\c 1\n\\ca 2\\ca*\n\\c 2\n\\p \\v 1 a"
        );

        // At EOF there is nothing to insert in front of, so the closer simply
        // ends the file.
        assert_eq!(
            repaired("\\id GEN\n\\p \\add a", Code::UnclosedAtEof),
            "\\id GEN\n\\p \\add a\\add*"
        );
        // A trailing newline is content the closer belongs in FRONT of…
        assert_eq!(
            repaired("\\id GEN\n\\p \\add a\n", Code::UnclosedAtEof),
            "\\id GEN\n\\p \\add a\\add*\n"
        );
        // …but the backoff never reaches into the opening marker's own span.
        assert_eq!(
            repaired("\\id GEN\n\\p \\add ", Code::UnclosedAtEof),
            "\\id GEN\n\\p \\add \\add*"
        );
    }

    #[test]
    fn a_missing_terminator_and_a_missing_end_milestone() {
        assert_eq!(
            repaired(
                "\\id GEN\n\\p a \\qt-s |who=\"Levi\"",
                Code::UnterminatedMilestone
            ),
            "\\id GEN\n\\p a \\qt-s |who=\"Levi\"\\*"
        );

        // The container's end milestone is built from the ROW name — the `-s`
        // and `-e` spellings are the row's, not the author's.
        assert_eq!(
            repaired(
                "\\id GEN\n\\list-s\\*\n\\li a\n\\p prose",
                Code::UnterminatedContainer
            ),
            "\\id GEN\n\\list-s\\*\n\\li a\\list-e\\*\n\\p prose"
        );
    }

    #[test]
    fn an_orphan_is_deleted_span_and_nothing_more() {
        // The two spaces left behind are deliberate: extra horizontal
        // whitespace is legal everywhere, and eating one would be a second edit
        // nobody asked for.
        assert_eq!(
            repaired("\\id GEN\n\\p text \\w* more", Code::OrphanCloser),
            "\\id GEN\n\\p text  more"
        );
        assert_eq!(
            repaired("\\id GEN\n\\p text \\* more", Code::OrphanTerminator),
            "\\id GEN\n\\p text  more"
        );
    }

    #[test]
    fn the_missing_paragraph_fix_writes_the_p_usfmtc_fabricates() {
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\v 1 no paragraph",
                Code::MissingParagraph
            ),
            "\\id GEN\n\\c 1\n\\p\n\\v 1 no paragraph"
        );

        // Glued to the preceding word, the `\p` needs a line break of its own —
        // without it the repair would trade one finding for another
        // (`marker-not-ws-preceded`), which the oracle refuses.
        assert_eq!(
            repaired("\\id GEN\n\\c 1\ntext\\v 1 a", Code::MissingParagraph),
            "\\id GEN\n\\c 1\ntext\n\\p\n\\v 1 a"
        );
    }

    #[test]
    fn the_book_identifier_and_the_glued_paragraph_marker() {
        assert_eq!(
            repaired("\\id gen\n\\c 1\n\\p \\v 1 a", Code::BookCodeNotUppercase),
            "\\id GEN\n\\c 1\n\\p \\v 1 a"
        );
        // A code that is no identifier in any casing is NOT guessed at.
        assert!(!offers_fix(
            "\\id ZZZ\n\\c 1\n\\p \\v 1 a",
            Code::BookCodeUnknown
        ));

        assert_eq!(
            repaired(
                "\\id GEN\n\\p text\\s1 heading\n\\p more",
                Code::MarkerNotWsPreceded
            ),
            "\\id GEN\n\\p text\n\\s1 heading\n\\p more"
        );
    }

    #[test]
    fn renumbering_is_offered_only_where_it_is_a_splice() {
        assert_eq!(
            repaired("\\id GEN\n\\c 1\n\\c 1\n", Code::ChapterDuplicate),
            "\\id GEN\n\\c 1\n\\c 2\n"
        );
        assert_eq!(
            repaired("\\id GEN\n\\c 2\n\\c 1\n", Code::ChapterOutOfOrder),
            "\\id GEN\n\\c 2\n\\c 3\n"
        );
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 1 b \\v 3 c",
                Code::VerseDuplicate
            ),
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b \\v 3 c"
        );
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 3 b \\v 2 c",
                Code::VerseOutOfOrder
            ),
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 3 b \\v 4 c"
        );

        // The bdf_reg ROM 3 shape: `\v 10` twice with `\v 11` after it.
        // Renumbering the duplicate to 11 would only move it along, so no fix
        // is offered at all — the finding stands on its own.
        assert!(!offers_fix(
            "\\id GEN\n\\c 1\n\\p \\v 10 a \\v 10 b \\v 11 c",
            Code::VerseDuplicate
        ));
        // The mirror shape for out-of-order.
        assert!(!offers_fix(
            "\\id GEN\n\\c 1\n\\p \\v 5 a \\v 2 b \\v 3 c",
            Code::VerseOutOfOrder
        ));
        // A RANGE is never renumbered: writing one number over `12-14` would
        // drop the verses it covers, which is interpretation, not a splice.
        assert!(!offers_fix(
            "\\id GEN\n\\c 1\n\\p \\v 1-11 a \\v 11-13 b",
            Code::VerseDuplicate
        ));
        // Neither is a segment (`\v 2a`).
        assert!(!offers_fix(
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 1a b \\v 3 c",
            Code::VerseDuplicate
        ));
        // A gap is never renumbered either — its row declares no label.
        assert!(!offers_fix(
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 5 b",
            Code::VerseGap
        ));
    }

    #[test]
    fn fix_all_of_one_code_dispatches_as_a_single_edit_list() {
        // Two findings of one code, concatenated, sorted, applied once — the
        // "fix all X" affordance, and the composability rule at its sharpest.
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 1 b \\v 3 c \\v 3 d\n",
                Code::VerseDuplicate
            ),
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b \\v 3 c \\v 4 d\n"
        );
        assert_eq!(
            repaired(
                "\\id GEN\n\\c 1\n\\v 1 a\n\\c 2\n\\v 1 b\n",
                Code::MissingParagraph
            ),
            "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\\c 2\n\\p\n\\v 1 b\n"
        );
    }

    #[test]
    fn a_fix_is_offered_exactly_where_the_row_declares_one() {
        // One snippet per code that DECLARES a label, so no label is a promise
        // nothing keeps. (The converse — a code emitting a fix its row does not
        // declare — is asserted on every snippet in this file, inside
        // `findings`, and again over the whole corpus.)
        let cases: [(Code, &str); 14] = [
            (
                Code::UnclosedNote,
                "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\ft n\\c 2\n\\p b",
            ),
            (Code::UnclosedChar, "\\id GEN\n\\p \\add a \\w b\\add*"),
            (Code::UnclosedAtEof, "\\id GEN\n\\p \\add a"),
            (
                Code::UnterminatedContainer,
                "\\id GEN\n\\list-s\\*\n\\li a\n\\p prose",
            ),
            (
                Code::UnterminatedMilestone,
                "\\id GEN\n\\p a \\qt-s |who=\"Levi\"",
            ),
            (Code::OrphanCloser, "\\id GEN\n\\p text\\w* more"),
            (Code::OrphanTerminator, "\\id GEN\n\\p text \\* more"),
            (Code::MissingParagraph, "\\id GEN\n\\c 1\n\\v 1 a"),
            (Code::ChapterDuplicate, "\\id GEN\n\\c 1\n\\c 1\n"),
            (Code::ChapterOutOfOrder, "\\id GEN\n\\c 2\n\\c 1\n"),
            (
                Code::VerseDuplicate,
                "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 1 b \\v 3 c",
            ),
            (
                Code::VerseOutOfOrder,
                "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 3 b \\v 2 c",
            ),
            (Code::BookCodeNotUppercase, "\\id gen\n\\c 1\n\\p \\v 1 a"),
            (
                Code::MarkerNotWsPreceded,
                "\\id GEN\n\\p text\\s1 heading\n\\p more",
            ),
        ];
        let mut demonstrated: Vec<Code> = Vec::new();
        for (code, usfm) in cases {
            assert!(
                offers_fix(usfm, code),
                "{} declares a fix but offered none on {usfm:?}",
                code.row().name
            );
            // Each one also passes the oracle (`repaired` runs it).
            repaired(usfm, code);
            demonstrated.push(code);
        }
        for row in LINT_ROWS.iter() {
            assert_eq!(
                row.fix_label.is_some(),
                demonstrated.contains(&row.code),
                "{} is not demonstrated by a case above",
                row.name
            );
        }
    }
}
