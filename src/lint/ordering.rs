//! The ORDERING machine: the chapter/verse designator sequence. The
//! designator SPAN interpreter itself is `crate::designator`.

use super::fix::renumber;
use super::walk::span_of;
use super::{Code, Doc, Emit, NO_TOKEN, Observation};
use crate::designator::{self, Designator};
use crate::tables::generated;
use crate::tables::schema::MarkerKind;
use crate::{Token, TokenKind};

/// The Ordering subsystem: one linear sweep over the tokens, ignoring the CST
/// entirely
///
/// It is its own pass because it is the only part of lint that carries
/// CROSS-TOKEN state — the previous chapter, the previous verse, whether a
/// `\c` has been seen at all — and threading that through the row-lookup
/// sweep would entangle two unrelated shapes of rule.
///
/// Two adjacency facts it depends on, both read off the scanner (scanner.rs
/// `text_arm`/`newline_arm`): a `\c`/`\v` marker and its `Designator` are
/// SEPARATE, adjacent tokens; and the only thing that can sit between them is
/// an attribute list (`\v |script="Arab"| 1`), because a newline or any other
/// marker abandons the payload expectation.
///
/// Sequence policy, in one place:
///
/// - Malformed designators are FLAGGED and then excluded — they update no
///   state, so a single typo never cascades into a gap plus a duplicate.
/// - Exactly ONE code per anomaly: equal → duplicate, smaller → out of order,
///   larger by more than one → gap.
/// - Verse state resets at every `\c`; chapter state runs for the whole book.
/// - Ranges count as their span: after `\v 12-14` the sequence expects 15.
///
/// Two of its four anomaly codes offer a fix — renumber to what the sequence
/// expected — and both go through [`renumberable`], which is where the honest
/// half of that fix lives.
/// Which marker is still owed its `Designator` token.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Awaiting {
    None,
    /// The `\c` marker's token index — the anchor if no number arrives.
    Chapter(u32),
    Verse,
}

pub(crate) struct Ordering {
    awaiting: Awaiting,
    /// (number, the designator token that carried it) — `second` on a finding.
    prev_chapter: Option<(u32, u32)>,
    prev_verse: Option<(u32, u32)>,
    /// True until this chapter's first verse has been read (well-formed or
    /// not): the window in which `missing-verse-one` can fire. Starts FALSE —
    /// the rule is about a CHAPTER's first verse, so a verse ahead of any `\c`
    /// is verse-before-first-chapter's story alone, never also this one's.
    first_verse_slot: bool,
    seen_chapter: bool,
    first_verse_token: Option<u32>,
    first_pre_chapter_verse: Option<u32>,
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
                    match designator::chapter(span_of(source, token)) {
                        Designator::Malformed => {
                            out.push(Observation::one(Code::DesignatorMalformed, idx));
                            // RESYNC, not just skip: the number is unknown, so
                            // the NEXT chapter has nothing legitimate to be
                            // compared against either. Dropping the state is
                            // what keeps one bad number to one finding.
                            self.prev_chapter = None;
                        }
                        Designator::Wellformed { first: number, .. } => {
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
                Awaiting::Verse => {
                    self.awaiting = Awaiting::None;
                    match designator::verse(span_of(source, token)) {
                        Designator::Malformed => {
                            out.push(Observation::one(Code::DesignatorMalformed, idx));
                            // RESYNC. Both slots are dropped, so the verse
                            // AFTER the bad one is compared against nothing
                            // and the sequence restarts from it. Skipping only
                            // the malformed token itself is not enough: real
                            // data proves it. en_ulb ZEC 12:7 is written
                            // `\v 7"` (no space before the quote), so the
                            // designator is `7"`; leaving `self.prev_verse` at 6
                            // then made the perfectly good `\v 8` look like a
                            // gap — one typo, two findings, the second of them
                            // a lie.
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
                if let Awaiting::Chapter(marker) = self.awaiting {
                    out.push(Observation::one(Code::ChapterWithoutDesignator, marker));
                }
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
                        Awaiting::Verse
                    }
                    _ => Awaiting::None,
                };
            }
            // Text, a newline, a closer, anything else: the payload window is
            // over, exactly as the scanner's is. Guarded rather than
            // unconditional because this is the arm nearly every token in the
            // corpus takes, and a perfectly-predicted branch beats a store.
            _ if self.awaiting != Awaiting::None => {
                if let Awaiting::Chapter(marker) = self.awaiting {
                    out.push(Observation::one(Code::ChapterWithoutDesignator, marker));
                }
                self.awaiting = Awaiting::None;
            }
            _ => {}
        }
    }

    /// End of input: the whole-book facts, which are exactly the ones no token
    /// event could carry.
    pub(crate) fn finish(&mut self, out: &mut Emit) {
        // A `\c` as the very last token of the file.
        if let Awaiting::Chapter(marker) = self.awaiting {
            out.push(Observation::one(Code::ChapterWithoutDesignator, marker));
        }

        match (
            self.seen_chapter,
            self.first_verse_token,
            self.first_pre_chapter_verse,
        ) {
            // Verses and no chapter anywhere: one finding at the first verse.
            // (A book with neither — front matter, a glossary — says nothing.)
            (false, Some(verse), _) => out.push(Observation::one(Code::MissingChapter, verse)),
            // Verses ahead of the first `\c`: one finding for the whole run.
            (true, _, Some(verse)) => {
                out.push(Observation::one(Code::VerseBeforeFirstChapter, verse))
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::tests::{codes, findings, token_named};

    // -----------------------------------------------------------------
    // Phase 2: ordering
    // -----------------------------------------------------------------

    /// The index of the nth `Designator` token — the anchor every sequence
    /// rule uses (the marker and its number are separate tokens).
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
        // The en_ulb ZEC 12:7 shape: `\v 2"` glues the quote to the number,
        // so the carved span is `2"`. ONE finding — the perfectly good `\v 3`
        // that follows must NOT be reported as a gap.
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
        // Two segments of one verse are the SAME verse, not a duplicate: the
        // suffix takes no part in the comparison.
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

        // Starting well above 1 is still the SAME single finding — never a
        // gap as well.
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

        // With no `\c` at all it is `missing-chapter`'s story instead — the
        // two never both describe the same token.
        let (tokens, obs) = findings("\\p \\v 1 a \\v 2 b");
        assert_eq!(
            obs,
            vec![Observation::one(
                Code::MissingChapter,
                token_named(&tokens, "v", 0)
            )]
        );

        // A pre-chapter verse numbered other than 1 is still ONLY this rule's
        // story — `missing-verse-one` is about a CHAPTER's first verse and
        // stays quiet until a `\c` exists.
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
        // The scanner abandons the payload expectation at a newline, so this
        // `\c` really does own no number.
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

        // An attribute list is stepped over, not mistaken for the payload.
        // (The name is `x-`: `\v` defines no attributes of its own, so a
        // canonical-looking one would draw an `attr-unknown-name` hint from the
        // k/v rules and this test is not about that.)
        let (_, obs) = findings("\\c 1\n\\p \\v |x-script=\"Arab\"| 1 a");
        assert_eq!(obs, vec![]);
    }
}
