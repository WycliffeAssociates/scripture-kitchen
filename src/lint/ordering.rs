//! The ORDERING machine: the chapter/verse designator sequence. The
//! designator SPAN interpreter itself is `crate::designator`.
//!
//! ```text
//! \c 1 \v 1 a \v 3 b     →  verse-gap @ \v 3                  (fix: renumber to 2)
//! \c 1 \v Then He said   →  verse-without-designator @ \v      (fixless: the
//!                            verse is THERE and only the human can name it)
//! \c 1 \v \v \v 1 text   →  two verse-without-designator, ONE fix on the first
//!                            (delete `\v \v `: an EMPTY one names nothing)
//! ```

use super::carried::{Carried, ChapterEdge, ChapterEvent, ChapterExit};
use super::fix::renumber;
use super::walk::{is_structural_ws, span_of};
use super::{Code, Doc, Emit, NO_TOKEN, Observation};
use crate::designator::{self, Designator};
use crate::tables::generated;
use crate::tables::schema::MarkerKind;
use crate::{Token, TokenKind};

/// The Ordering subsystem: one linear sweep over the tokens, ignoring the CST.
///
/// Its own pass because it is the only part of lint carrying CROSS-TOKEN state —
/// the previous chapter, the previous verse, whether a `\c` has been seen at all.
///
/// Two adjacency facts it depends on, both read off the scanner (scanner.rs
/// `text_arm`/`newline_arm`): a `\c`/`\v` marker and its `Designator` are
/// SEPARATE, adjacent tokens; and only an attribute list can sit between them
/// (`\v |script="Arab"| 1`), since anything else abandons the payload.
///
/// Sequence policy, in one place:
///
/// - Malformed designators are FLAGGED and then excluded — they update no
///   state, so a single typo never cascades into a gap plus a duplicate.
/// - Exactly ONE code per anomaly: equal → duplicate, smaller → out of order,
///   larger by more than one → gap.
/// - A designator-less `\v` that HOLDS something resyncs like a malformed one;
///   an EMPTY one is a stray marker the sequence steps over, which is what lets
///   the deletion fix change no verdict.
/// - Verse state resets at every `\c`; chapter state runs for the whole book.
/// - Ranges count as their span: after `\v 12-14` the sequence expects 15.
///
/// Two of the four anomaly codes offer a renumber fix, both through `renumber`.
///
/// Which marker is still owed its `Designator` token.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Awaiting {
    None,
    /// The `\c` marker's token index — the anchor if no number arrives.
    Chapter(u32),
    /// The `\v` marker's token index, for the same reason.
    Verse(u32),
}

pub(crate) struct Ordering {
    awaiting: Awaiting,
    /// (number, the designator token that carried it) — `second` on a finding.
    prev_chapter: Option<(u32, u32)>,
    prev_verse: Option<(u32, u32)>,
    /// True until this chapter's first verse has been read (well-formed or not):
    /// the window in which `missing-verse-one` can fire. Starts FALSE, because
    /// the rule is about a CHAPTER's first verse and a verse ahead of any `\c`
    /// belongs to `verse-before-first-chapter` alone.
    first_verse_slot: bool,
    seen_chapter: bool,
    first_verse_token: Option<u32>,
    first_pre_chapter_verse: Option<u32>,
    /// `(observation slot, the `\v` token, the token that ends its emptiness)`
    /// per EMPTY designator-less verse, resolved into fixes at
    /// [`Self::finish_chunk`] — the repair depends on the whole RUN of empties
    /// this one opens, which the abandon event has not reached. The only
    /// allocation in this machine, and it stays empty on a book with no such
    /// verse.
    empty_verses: Vec<(u32, u32, u32)>,
    /// The fold's observations (chunk-fold.md): the chunk's first chapter
    /// event with its renumber material, the number after it, and whether any
    /// chapter-designator event happened at all (the exit-state distinction
    /// between "resynced" and "untouched").
    first_event: ChapterEvent,
    second_number: Option<Option<u32>>,
    touched: bool,
}

impl Ordering {
    pub(crate) fn new() -> Self {
        Self {
            awaiting: Awaiting::None,
            prev_chapter: None,
            prev_verse: None,
            first_verse_slot: false,
            seen_chapter: false,
            first_verse_token: None,
            first_pre_chapter_verse: None,
            empty_verses: Vec::new(),
            first_event: ChapterEvent::None,
            second_number: None,
            touched: false,
        }
    }

    #[inline]
    pub(crate) fn on_leaf(
        &mut self,
        doc: &Doc,
        idx: u32,
        token: &Token,
        kind: TokenKind,
        out: &mut Emit,
    ) {
        let (source, tokens) = (doc.source, doc.tokens);
        match kind {
            // An attribute list does not end the payload expectation, and it
            // is not the payload either — step over it.
            TokenKind::AttrList => {}
            TokenKind::Designator => match self.awaiting {
                Awaiting::Chapter(_) => {
                    self.awaiting = Awaiting::None;
                    self.touched = true;
                    match designator::chapter(span_of(source, token)) {
                        Designator::Malformed => {
                            out.push(Observation::one(Code::DesignatorMalformed, idx));
                            // RESYNC, not just skip: the number is unknown, so
                            // the NEXT chapter has nothing to compare against.
                            // Dropping state keeps one typo to one finding.
                            self.prev_chapter = None;
                            // The fold's observations: a malformed FIRST event
                            // means the seam judges nothing; a malformed SECOND
                            // constrains no seam renumber.
                            match self.first_event {
                                ChapterEvent::None => self.first_event = ChapterEvent::Malformed,
                                ChapterEvent::Wellformed(_) if self.second_number.is_none() => {
                                    self.second_number = Some(None);
                                }
                                _ => {}
                            }
                        }
                        Designator::Wellformed { first: number, .. } => {
                            // The fold's observations: the first wellformed
                            // event is the seam's right-hand party, with the
                            // splice window a seam renumber would need — reduce
                            // holds no tokens, so the bytes-facts travel.
                            match self.first_event {
                                ChapterEvent::None => {
                                    let span = span_of(source, token);
                                    let label = designator::label(span);
                                    self.first_event = ChapterEvent::Wellformed(ChapterEdge {
                                        number,
                                        anchor: idx,
                                        splice_at: token.start,
                                        label_len: label.len() as u32,
                                        plain_digits: label.iter().all(u8::is_ascii_digit),
                                    });
                                }
                                ChapterEvent::Wellformed(_) if self.second_number.is_none() => {
                                    self.second_number = Some(Some(number));
                                }
                                _ => {}
                            }
                            if let Some((previous, previous_token)) = self.prev_chapter {
                                let expected = previous.saturating_add(1);
                                let code = if number == previous {
                                    Some(Code::ChapterDuplicate)
                                } else if number < previous {
                                    Some(Code::ChapterOutOfOrder)
                                } else if number > expected {
                                    Some(Code::ChapterGap)
                                } else {
                                    None
                                };
                                if let Some(code) = code {
                                    renumber(
                                        source,
                                        tokens,
                                        out,
                                        Observation {
                                            code,
                                            anchor: idx,
                                            second: previous_token,
                                            aux: expected,
                                        },
                                        false,
                                    );
                                }
                            }
                            self.prev_chapter = Some((number, idx));
                        }
                    }
                }
                Awaiting::Verse(_) => {
                    self.awaiting = Awaiting::None;
                    match designator::verse(span_of(source, token)) {
                        Designator::Malformed => {
                            out.push(Observation::one(Code::DesignatorMalformed, idx));
                            // RESYNC: both slots drop, so the verse AFTER the
                            // bad one compares against nothing. Skipping only
                            // the malformed token is not enough — en_ulb ZEC
                            // 12:7 writes `\v 7"`, and keeping `prev_verse` at
                            // 6 makes the good `\v 8` look like a gap.
                            self.prev_verse = None;
                            self.first_verse_slot = false;
                        }
                        Designator::Wellformed { first, last } => {
                            if let Some((previous_last, previous_token)) = self.prev_verse {
                                let expected = previous_last.saturating_add(1);
                                let code = if first == previous_last {
                                    Some(Code::VerseDuplicate)
                                } else if first < previous_last {
                                    Some(Code::VerseOutOfOrder)
                                } else if first > expected {
                                    Some(Code::VerseGap)
                                } else {
                                    None
                                };
                                if let Some(code) = code {
                                    renumber(
                                        source,
                                        tokens,
                                        out,
                                        Observation {
                                            code,
                                            anchor: idx,
                                            second: previous_token,
                                            aux: expected,
                                        },
                                        true,
                                    );
                                }
                            } else if self.first_verse_slot && first != 1 {
                                out.push(Observation {
                                    code: Code::MissingVerseOne,
                                    anchor: idx,
                                    second: NO_TOKEN,
                                    aux: 1,
                                });
                            }
                            self.first_verse_slot = false;
                            self.prev_verse = Some((last, idx));
                        }
                    }
                }
                Awaiting::None => {}
            },
            TokenKind::Marker { .. } => {
                self.abandon(doc, out);
                self.awaiting = match generated::kind(token.marker_idx) {
                    MarkerKind::Chapter => {
                        self.seen_chapter = true;
                        self.prev_verse = None;
                        self.first_verse_slot = true;
                        Awaiting::Chapter(idx)
                    }
                    MarkerKind::Verse => {
                        self.first_verse_token.get_or_insert(idx);
                        if !self.seen_chapter {
                            self.first_pre_chapter_verse.get_or_insert(idx);
                        }
                        Awaiting::Verse(idx)
                    }
                    _ => Awaiting::None,
                };
            }
            // Text, a newline, a closer, anything else: the payload window is
            // over, exactly as the scanner's is. Guarded rather than
            // unconditional because nearly every token takes this arm, and a
            // perfectly-predicted branch beats a store.
            _ if self.awaiting != Awaiting::None => self.abandon(doc, out),
            _ => {}
        }
    }

    /// The pending `\c`/`\v` never got a `Designator` token: its line ended, or
    /// content started. Under the scanner's designator gate that includes
    /// `\v Then He declared` — prose after `\v ` is Text, so a verse with no
    /// NUMBER and a verse with no designator token are one fact.
    fn abandon(&mut self, doc: &Doc, out: &mut Emit) {
        match self.awaiting {
            Awaiting::Chapter(marker) => {
                out.push(Observation::one(Code::ChapterWithoutDesignator, marker))
            }
            Awaiting::Verse(marker) => {
                let slot = out.observations.len() as u32;
                out.push(Observation::one(Code::VerseWithoutDesignator, marker));
                match empty_through(doc, marker) {
                    // An EMPTY one names no verse AND holds none: a stray
                    // marker the sequence steps OVER rather than resyncs
                    // around. That is what makes deleting it safe — the fix
                    // below changes no sequence verdict, where a resync would
                    // have hidden the gap it unmasks (`\v 1 a` `\v` `\v 3 b`).
                    Some(boundary) => self.empty_verses.push((slot, marker, boundary)),
                    // RESYNC, exactly as a malformed designator does: the
                    // number is unknown but the verse is THERE, so the next
                    // verse must compare against nothing or one typo reads as
                    // a gap too.
                    None => {
                        self.prev_verse = None;
                        self.first_verse_slot = false;
                    }
                }
            }
            Awaiting::None => {}
        }
        self.awaiting = Awaiting::None;
    }

    /// End of this CHUNK's input: the chunk-local closes (a `\c`/`\v` as the
    /// very last token, the empty-verse runs), then the fold observations.
    /// The whole-book verdicts this machine used to emit here —
    /// missing-chapter, verse-before-first-chapter — are REDUCE's now
    /// (map observes, reduce judges; carried.rs `finish`), which is what
    /// makes the summary equal-in / equal-out with the whole-book walk.
    pub(crate) fn finish_chunk(&mut self, doc: &Doc, out: &mut Emit, carried: &mut Carried) {
        self.abandon(doc, out);
        self.collapse_empty_verses(doc, out);

        carried.first_chapter = core::mem::take(&mut self.first_event);
        carried.second_chapter_number = self.second_number;
        carried.exit_chapter = if !self.touched {
            ChapterExit::Untouched
        } else {
            match self.prev_chapter {
                None => ChapterExit::Reset,
                Some((number, anchor)) => ChapterExit::At { number, anchor },
            }
        };
        carried.has_chapter = self.seen_chapter;
        carried.first_verse = self.first_verse_token;
        carried.first_pre_chapter_verse = self.first_pre_chapter_verse;
    }

    /// The FIX half of `verse-without-designator`, at finish for the reason
    /// pass 12's empty-paragraph chain is: a RUN of empties is one repair, and
    /// whether this empty verse opens one is a fact about the tokens after the
    /// abandon that reported it.
    ///
    /// A consecutive run collapses under ONE fix, filed on its FIRST member —
    /// per-member deletions would each fail "the site is repaired", since
    /// deleting one leaves the next empty `\v` at that byte. The other members
    /// keep their fixless findings: the same transaction repairs them.
    fn collapse_empty_verses(&mut self, doc: &Doc, out: &mut Emit) {
        let empties = core::mem::take(&mut self.empty_verses);
        let mut at = 0;
        while at < empties.len() {
            let mut last = at;
            // Adjacency read off the SOURCE: the token that ended this one's
            // emptiness IS the next empty verse's marker.
            while last + 1 < empties.len() && empties[last].2 == empties[last + 1].1 {
                last += 1;
            }
            let (slot, first, _) = empties[at];
            let boundary = empties[last].2;
            if !empties_the_paragraph(doc, first, boundary) {
                let from = doc.tokens[first as usize].start;
                // Everything from the first marker to the displacer is the run
                // and the whitespace between its members — including the line
                // ending each sat alone on, which is pass 12's extent.
                let to = doc
                    .tokens
                    .get(boundary as usize)
                    .map_or(doc.source.len() as u32, |token| token.start);
                out.attach_fixed(slot, from, to, b"");
            }
            at = last + 1;
        }
    }
}

/// Is everything between this designator-less `\v` and the next verse, chapter
/// or paragraph marker WHITESPACE — and if so, which token ends it? (End of
/// input ends it too, and is reported as `tokens.len()`.)
///
/// The one lookahead this machine does, and it runs on a FINDING, never on a
/// token: `fix::next_number`'s affordance, for the same reason.
fn empty_through(doc: &Doc, marker: u32) -> Option<u32> {
    for (offset, token) in doc.tokens[marker as usize + 1..].iter().enumerate() {
        match token.kind() {
            TokenKind::Newline => {}
            TokenKind::Text if span_of(doc.source, token).iter().all(|b| is_structural_ws(*b)) => {}
            TokenKind::Marker { .. }
                if matches!(
                    generated::kind(token.marker_idx),
                    MarkerKind::Verse | MarkerKind::Chapter | MarkerKind::Paragraph
                ) =>
            {
                return Some(marker + 1 + offset as u32);
            }
            // A note, an attribute list, a `\tr`, an `\s5` — anything else is
            // either content or a marker whose own displacement rules this fix
            // does not model. Not empty, no fix.
            _ => return None,
        }
    }
    Some(doc.tokens.len() as u32)
}

/// Would deleting the run leave the paragraph holding it EMPTY? Then the fix is
/// declined: `\p \v \v \p b` would collapse to `\p \p b`, handing back an
/// `empty-paragraph` finding, and a fix may not do that. The empty paragraph's
/// own chain fix is the next transaction's business.
fn empties_the_paragraph(doc: &Doc, first: u32, boundary: u32) -> bool {
    // The paragraph survives if it still holds the verse the run runs into.
    if doc.tokens.get(boundary as usize).is_some_and(|token| {
        matches!(token.kind(), TokenKind::Marker { .. })
            && generated::kind(token.marker_idx) == MarkerKind::Verse
    }) {
        return false;
    }
    for token in doc.tokens[..first as usize].iter().rev() {
        match token.kind() {
            TokenKind::Newline => {}
            TokenKind::Text if span_of(doc.source, token).iter().all(|b| is_structural_ws(*b)) => {}
            TokenKind::Marker { .. } => {
                return generated::kind(token.marker_idx) == MarkerKind::Paragraph;
            }
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::tests::{codes, findings, token_named};

    /// The index of the nth `Designator` token — the anchor every sequence rule
    /// uses (the marker and its number are separate tokens).
    fn designator_at(tokens: &[Token], nth: usize) -> u32 {
        tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| token.kind() == TokenKind::Designator)
            .map(|(idx, _)| idx as u32)
            .nth(nth)
            .unwrap_or_else(|| panic!("no designator #{nth}"))
    }

    #[test]
    fn a_clean_ordered_book_yields_nothing() {
        let (_, obs) = findings(
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b \\v 3-4 c \\v 5 d\n\\c 2\n\\p \\v 1 e\n",
        );
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn designator_malformed_flags_once_and_resyncs() {
        // The en_ulb ZEC 12:7 shape: `\v 2"` glues the quote to the number, so
        // the span is `2"`. ONE finding — the good `\v 3` is not a gap.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\v 2\" b\n\\v 3 c\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DesignatorMalformed,
                designator_at(&tokens, 2)
            )]
        );

        // A chapter designator is judged by the CHAPTER pattern: no segments.
        let (tokens, obs) = findings("\\c 12b\n\\p \\v 1 a\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::DesignatorMalformed,
                designator_at(&tokens, 0)
            )]
        );
    }

    #[test]
    fn chapter_sequence_reports_one_code_per_anomaly() {
        let cases = [
            ("\\c 1\n\\c 1\n", Code::ChapterDuplicate, 2),
            ("\\c 2\n\\c 1\n", Code::ChapterOutOfOrder, 3),
            ("\\c 1\n\\c 4\n", Code::ChapterGap, 2),
        ];
        for (usfm, code, expected) in cases {
            let (tokens, obs) = findings(usfm);
            assert_eq!(
                obs,
                vec![Observation {
                    code,
                    anchor: designator_at(&tokens, 1),
                    second: designator_at(&tokens, 0),
                    aux: expected,
                }],
                "{usfm:?}"
            );
        }
    }

    #[test]
    fn verse_sequence_reports_one_code_per_anomaly() {
        let cases = [
            ("\\c 1\n\\p \\v 1 a \\v 1 b", Code::VerseDuplicate, 2),
            (
                "\\c 1\n\\p \\v 1 a \\v 3 b \\v 2 c",
                Code::VerseOutOfOrder,
                4,
            ),
            ("\\c 1\n\\p \\v 1 a \\v 3 b", Code::VerseGap, 2),
        ];
        for (usfm, code, expected) in cases {
            let (tokens, obs) = findings(usfm);
            let last = obs.last().copied().unwrap_or_else(|| panic!("{usfm:?}"));
            assert_eq!(last.code, code, "{usfm:?}");
            assert_eq!(last.aux, expected, "{usfm:?}");
            // `second` always names the designator this one was compared to.
            let count = tokens
                .iter()
                .filter(|t| t.kind() == TokenKind::Designator)
                .count();
            assert_eq!(last.anchor, designator_at(&tokens, count - 1));
            assert_eq!(last.second, designator_at(&tokens, count - 2));
        }
    }

    #[test]
    fn ranges_count_as_their_whole_span() {
        // 12-14 covers 15's predecessor, so `\v 15` is contiguous.
        let (_, obs) = findings("\\c 1\n\\p \\v 1-11 a \\v 12-14 b \\v 15 c");
        assert_eq!(obs, vec![]);

        // Landing ON the range's last verse is a DUPLICATE…
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1-11 a \\v 12-14 b \\v 14 c");
        assert_eq!(codes(&obs), vec![Code::VerseDuplicate]);
        assert_eq!(obs[0].anchor, designator_at(&tokens, 3));
        assert_eq!(obs[0].aux, 15);

        // …and landing INSIDE it is out of order.
        let (_, obs) = findings("\\c 1\n\\p \\v 1-11 a \\v 12-14 b \\v 13 c");
        assert_eq!(codes(&obs), vec![Code::VerseOutOfOrder]);

        // A list covers its endpoints the same way.
        let (_, obs) = findings("\\c 1\n\\p \\v 1,3 a \\v 4 b");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn segments_and_rtl_marks_are_ordinary_designators() {
        // Two segments of one verse are the SAME verse: the suffix takes no
        // part in the comparison.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a \\v 2a b \\v 3 c");
        assert_eq!(obs, vec![]);

        // U+200F before the separator, as RTL scripts write it.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a \\v 2\u{200F}-3 b \\v 4 c");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn missing_verse_one_fires_instead_of_verse_gap() {
        let (tokens, obs) = findings("\\c 1\n\\p \\v 2 a \\v 3 b");
        assert_eq!(
            obs,
            vec![Observation {
                code: Code::MissingVerseOne,
                anchor: designator_at(&tokens, 1),
                second: NO_TOKEN,
                aux: 1,
            }]
        );

        // Starting well above 1 is still that one finding, never a gap too.
        let (_, obs) = findings("\\c 1\n\\p \\v 7 a");
        assert_eq!(codes(&obs), vec![Code::MissingVerseOne]);
    }

    #[test]
    fn a_chapter_resets_the_verse_sequence() {
        // Chapter 2 starting again at 1 is not a duplicate or a step back.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a \\v 2 b\n\\c 2\n\\p \\v 1 c \\v 2 d");
        assert_eq!(obs, vec![]);

        // …and the missing-verse-one window reopens with it.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\c 2\n\\p \\v 4 b");
        assert_eq!(codes(&obs), vec![Code::MissingVerseOne]);
        assert_eq!(obs[0].anchor, designator_at(&tokens, 3));
    }

    #[test]
    fn verse_before_first_chapter_fires_once_and_only_with_a_chapter() {
        // ONE finding for the whole run, at its first verse.
        let (tokens, obs) = findings("\\p \\v 1 a \\v 2 b\n\\c 1\n\\p \\v 1 c");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::VerseBeforeFirstChapter,
                token_named(&tokens, "v", 0)
            )]
        );

        // With no `\c` at all it is `missing-chapter` instead: never both.
        let (tokens, obs) = findings("\\p \\v 1 a \\v 2 b");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingChapter,
                token_named(&tokens, "v", 0)
            )]
        );

        // A pre-chapter verse numbered other than 1 is still only this code:
        // `missing-verse-one` is about a CHAPTER's first verse.
        let (tokens, obs) = findings("\\p \\v 5 a\n\\c 1\n\\p \\v 1 c");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::VerseBeforeFirstChapter,
                token_named(&tokens, "v", 0)
            )]
        );
    }

    #[test]
    fn a_book_with_no_verses_is_never_asked_for_a_chapter() {
        // Front matter: no `\c`, no `\v`, and nothing to say.
        let (_, obs) = findings("\\id FRT\n\\mt1 Front Matter\n\\p Some introduction.\n");
        assert_eq!(obs, vec![]);
    }

    #[test]
    fn chapter_without_designator() {
        // The scanner abandons the payload at a newline: this `\c` owns no
        // number.
        let (tokens, obs) = findings("\\c\n\\p \\v 1 a");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::ChapterWithoutDesignator,
                token_named(&tokens, "c", 0)
            )]
        );

        // `\c` as the very last token of the file.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\c");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::ChapterWithoutDesignator,
                token_named(&tokens, "c", 1)
            )]
        );

        // An attribute list is stepped over, not mistaken for the payload. (An
        // `x-` name: `\v` defines no attributes, so any other would draw an
        // `attr-unknown-name` hint this test is not about.)
        let (_, obs) = findings("\\c 1\n\\p \\v |x-script=\"Arab\"| 1 a");
        assert_eq!(obs, vec![]);
    }

    /// The verse counterpart. Under the designator gate all three spellings of
    /// "this `\v` names no verse" are ONE token shape, so they are one code.
    #[test]
    fn verse_without_designator() {
        for source in [
            "\\c 1\n\\p \\v Then He declared\n",
            "\\c 1\n\\p \\v \\p x\n",
            "\\c 1\n\\p \\v\n",
        ] {
            let (tokens, obs) = findings(source);
            assert_eq!(
                obs,
                vec![Observation::one(
                    Code::VerseWithoutDesignator,
                    token_named(&tokens, "v", 0)
                )],
                "{source:?}"
            );
        }

        // ONE finding, not two: the resync keeps `\v 3` from also reading as a
        // gap — the same policy a malformed designator gets.
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\v Then He declared\n\\v 3 c\n");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::VerseWithoutDesignator,
                token_named(&tokens, "v", 1)
            )]
        );
    }

    /// An EMPTY designator-less `\v` is a stray marker, not a mis-numbered
    /// verse: the sequence steps over it instead of resyncing, so the gap the
    /// document really has is reported whether or not the empty is deleted.
    #[test]
    fn an_empty_designator_less_verse_is_sequence_transparent() {
        let (tokens, obs) = findings("\\c 1\n\\p \\v 1 a\n\\v \n\\v 3 c\n");
        assert_eq!(
            codes(&obs),
            vec![Code::VerseWithoutDesignator, Code::VerseGap]
        );
        assert_eq!(obs[0].anchor, token_named(&tokens, "v", 1));

        // …and the well-formed neighbours still see each other.
        let (_, obs) = findings("\\c 1\n\\p \\v 1 a\n\\v \n\\v 2 c\n");
        assert_eq!(codes(&obs), vec![Code::VerseWithoutDesignator]);
    }

    #[test]
    fn an_empty_verse_run_collapses_under_one_fix() {
        // Will's shape: two empty markers ahead of the real verse, one repair,
        // filed on the FIRST.
        let usfm = "\\id GEN\n\\c 1\n\\p \\v \\v \\v 1 Put the caret in the slot";
        let tokens = crate::lex(usfm);
        let cst = crate::cst::build(&tokens);
        let report = crate::lint::lint(usfm.as_bytes(), &tokens, &cst);
        assert_eq!(
            codes(&report.observations),
            vec![Code::VerseWithoutDesignator; 2]
        );
        let fixed: Vec<usize> = (0..2).filter(|slot| report.fix(*slot).is_some()).collect();
        assert_eq!(fixed, vec![0]);
        let edits = report.edits(report.fix(0).expect("the run's first member is fixed"));
        assert_eq!(edits.len(), 1);
        assert_eq!(
            crate::edit::apply(usfm.as_bytes(), edits),
            b"\\id GEN\n\\c 1\n\\p \\v 1 Put the caret in the slot".to_vec()
        );
    }

    /// The two refusals. A designator-less `\v` that HOLDS something is the
    /// human's to name; a run that is its paragraph's whole content would be
    /// repaired into an `empty-paragraph`, and a fix may not hand back a
    /// finding.
    #[test]
    fn a_verse_that_holds_something_and_a_run_that_empties_its_paragraph_stay_fixless() {
        let fixless = |usfm: &str| {
            let usfm = format!("\\id GEN\n{usfm}");
            let tokens = crate::lex(&usfm);
            let cst = crate::cst::build(&tokens);
            let report = crate::lint::lint(usfm.as_bytes(), &tokens, &cst);
            report
                .observations
                .iter()
                .enumerate()
                .filter(|(_, obs)| obs.code == Code::VerseWithoutDesignator)
                .all(|(slot, _)| report.fix(slot).is_none())
        };
        assert!(fixless("\\c 1\n\\p \\v Then He declared\n"));
        assert!(fixless("\\c 1\n\\p \\v \\f + \\ft note\\f*\n"));
        assert!(fixless("\\c 1\n\\p \\v \\v \n\\p b\n"));
        assert!(fixless("\\c 1\n\\p \\v \n"));

        // The paragraph SURVIVES when it holds anything else — before the run…
        assert!(!fixless("\\c 1\n\\p a\n\\v \\v \n\\p b\n"));
        // …or after it.
        assert!(!fixless("\\c 1\n\\p \\v \\v 2 b\n"));
    }
}
