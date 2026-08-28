//! The CARRIED seam: map/reduce pattern.
//! reduce judges.** Every cross-chunk fact a machine used to judge in-walk is
//! RECORDED here instead — a first/last chapter edge, once-per-book
//! occurrences, a family's spelling firsts, the sid a later `-e` may owe —
//! and [`reduce`] is the one place those observations become findings: the
//! seam judgments between adjacent summaries, then the `finish()` facts over
//! the fold's accumulator.
//!
//! Whole-book [`lint`](super::lint) IS the single-unit fold — one map, one
//! reduce over one summary — so every judgment exists exactly once and the
//! corpus pins hold it still. Everything in a summary is CHUNK-RELATIVE;
//! [`reduce`] rebases through the same `token_bases`/`byte_bases` prefix sums
//! as every other fold product. Reduce is a LEFT FOLD in document order and
//! needs no source, no tokens, no CST: the map recorded the bytes-facts (a
//! designator's splice window, a duplicate's line extent) while it had them.

use super::walk::Emit;
use super::{Code, NO_TOKEN, Observation, UsfmVersion};
use crate::lint::LintReport;
use crate::tables::generated;
use crate::tables::schema::Numbering;

/// What one chunk's walk OBSERVED across its boundary — the fold's per-chunk
/// summary, beside the chunk-local [`LintReport`]. Chunk-relative throughout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carried {
    // --- ordering: the chapter sequence --------------------------------
    pub(crate) first_chapter: ChapterEvent,
    /// The chapter number FOLLOWING the first event within this chunk —
    /// `fix::next_number`'s in-chunk answer for a seam renumber.
    /// `None` = no second designator here; `Some(None)` = the second is
    /// malformed (which resyncs, so it constrains nothing).
    pub(crate) second_chapter_number: Option<Option<u32>>,
    pub(crate) exit_chapter: ChapterExit,
    /// Any chapter MARKER (ordering's unconditional bit).
    pub(crate) has_chapter: bool,
    pub(crate) first_verse: Option<u32>,
    /// The first verse before this chunk's first chapter marker.
    pub(crate) first_pre_chapter_verse: Option<u32>,

    // --- flat: once-per-book, families, the band's finish fact ----------
    /// Every `\id` occurrence: (anchor, delete-line extent).
    pub(crate) ids: Vec<LineOccurrence>,
    pub(crate) usfms: Vec<LineOccurrence>,
    /// Numbered families sighted here: spelling firsts per row.
    pub(crate) families: Vec<FamilyFirsts>,
    pub(crate) pbfc_pending: Option<u32>,
    /// A `\c` at a POSITIONAL position (flat's gated bit).
    pub(crate) positional_chapter: bool,
    /// A verse before this chunk's first positional `\c`.
    pub(crate) verse_pre_chapter: bool,
    pub(crate) has_markers: bool,

    // --- attrs: the sid/eid obligation ----------------------------------
    /// The LAST sid-carrying point per row (matching `sid_at`'s
    /// most-recent-wins semantics): (row, anchor).
    pub(crate) sid_last: Vec<(u8, u32)>,
    /// `-e` points that owe an `eid` and found no sid IN-CHUNK before them:
    /// (row, the anchor the emission would use — list or milestone).
    pub(crate) eid_candidates: Vec<(u8, u32)>,

    // --- chunk 0's carry-outs (the fold's per-book context) -------------
    pub(crate) declared_version: Option<UsfmVersion>,
    pub(crate) book_is_scripture: bool,
}

impl Carried {
    /// Chunk 0's carry-out: the `\usfm` version its header declared. Later
    /// chunks receive it through [`ChunkContext`](super::ChunkContext) and a
    /// cache folds it into their keys.
    pub fn declared_version(&self) -> Option<UsfmVersion> {
        self.declared_version
    }

    /// Chunk 0's other carry-out: does the `\id` code name a scripture book
    /// (the gate on judging container-licensed paragraphs positionally).
    pub fn book_is_scripture(&self) -> bool {
        self.book_is_scripture
    }
}

/// One chapter DESIGNATOR plus the material a seam renumber-fix needs,
/// recorded at map time because reduce holds no tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChapterEdge {
    pub(crate) number: u32,
    /// The designator token index.
    pub(crate) anchor: u32,
    /// The number label's byte window (`designator::label`) — what a
    /// renumber replaces.
    pub(crate) splice_at: u32,
    pub(crate) label_len: u32,
    pub(crate) plain_digits: bool,
}

/// The chunk's FIRST chapter-designator event — the seam's right-hand party.
/// A malformed first resyncs (compares against nothing), exactly as in-walk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ChapterEvent {
    #[default]
    None,
    Malformed,
    Wellformed(ChapterEdge),
}

/// The chapter-sequence state the chunk EXITS with — the seam's left-hand
/// party for its successor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ChapterExit {
    /// No chapter-designator event at all: the fold's running state passes
    /// through unchanged.
    #[default]
    Untouched,
    /// The last event resynced (malformed): the successor compares against
    /// nothing.
    Reset,
    At {
        number: u32,
        anchor: u32,
    },
}

/// A once-per-book marker occurrence and the delete-the-line fix's extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineOccurrence {
    pub(crate) anchor: u32,
    pub(crate) from: u32,
    pub(crate) to: u32,
}

/// A numbered family's spelling firsts within one chunk. `numbering-mix`
/// cannot be judged at map time at all — a carry-in mask changes WHERE the
/// mix completes — so the map only records and reduce owns the whole rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyFirsts {
    pub(crate) row: u8,
    pub(crate) first_any: u32,
    pub(crate) first_bare: Option<u32>,
    pub(crate) first_numbered: Option<u32>,
}

/// THE REDUCE: seam judgments between adjacent summaries in document order
/// (a left fold — chunk-fold.md law 2), then the finish() facts over the
/// accumulator. Emits in GLOBAL coordinates; `book`/`declared_version` stamp
/// the report (they are chunk 0's header facts, which the caller holds).
pub fn reduce(
    units: &[&Carried],
    token_bases: &[u32],
    byte_bases: &[u32],
    book: Option<u32>,
    declared_version: Option<UsfmVersion>,
) -> LintReport {
    assert_eq!(units.len(), token_bases.len());
    assert_eq!(units.len(), byte_bases.len());
    let mut out = Emit::default();

    chapter_seams(units, token_bases, byte_bases, &mut out);
    once_per_book(units, token_bases, byte_bases, &mut out);
    families(units, token_bases, &mut out);
    sid_eid(units, token_bases, &mut out);
    finish(units, token_bases, &mut out);

    out.finish(book, declared_version)
}

/// The chapter sequence across seams: each unit's first wellformed designator
/// judged against the fold's running exit state — the same one-code-per-
/// anomaly policy the walk applies to interior pairs, renumber fix included.
fn chapter_seams(units: &[&Carried], token_bases: &[u32], byte_bases: &[u32], out: &mut Emit) {
    let mut prev: Option<(u32, u32)> = None; // (number, global designator)
    for (at, unit) in units.iter().enumerate() {
        let token_base = token_bases[at];
        if let ChapterEvent::Wellformed(edge) = &unit.first_chapter
            && let Some((previous, previous_token)) = prev
        {
            let expected = previous.saturating_add(1);
            let code = if edge.number == previous {
                Some(Code::ChapterDuplicate)
            } else if edge.number < previous {
                Some(Code::ChapterOutOfOrder)
            } else if edge.number > expected {
                Some(Code::ChapterGap)
            } else {
                None
            };
            if let Some(code) = code {
                let observation = Observation {
                    code,
                    anchor: edge.anchor + token_base,
                    second: previous_token,
                    aux: expected,
                };
                // The same three renumber guards as `fix::renumber`, answered
                // from summaries: the row declares a label, the label is plain
                // digits, and the NEXT number in the sequence stays above what
                // we would write.
                let renumberable = code.row().fix_label.is_some()
                    && edge.plain_digits
                    && next_chapter_number(units, at, unit).is_none_or(|next| next > expected);
                if renumberable {
                    let mut buf = [0u8; 10];
                    let text = decimal(expected, &mut buf);
                    let from = edge.splice_at + byte_bases[at];
                    out.push_fixed(observation, from, from + edge.label_len, text);
                } else {
                    out.push(observation);
                }
            }
        }
        prev = match unit.exit_chapter {
            ChapterExit::Untouched => prev,
            ChapterExit::Reset => None,
            ChapterExit::At { number, anchor } => Some((number, anchor + token_base)),
        };
    }
}

/// The first chapter number AFTER `unit`'s first designator — in-chunk when
/// the map recorded one, else the following units' first events. `None`
/// covers no-next and malformed-next alike (a resync constrains nothing),
/// matching `fix::next_number`.
fn next_chapter_number(units: &[&Carried], at: usize, unit: &Carried) -> Option<u32> {
    if let Some(second) = unit.second_chapter_number {
        return second;
    }
    for later in &units[at + 1..] {
        match &later.first_chapter {
            ChapterEvent::None => continue,
            ChapterEvent::Malformed => return None,
            ChapterEvent::Wellformed(edge) => return Some(edge.number),
        }
    }
    None
}

/// `duplicate-id` / `duplicate-usfm`: the global FIRST is the identity, every
/// later occurrence is the finding, pointed back at it, with the
/// delete-the-line fix the map recorded.
fn once_per_book(units: &[&Carried], token_bases: &[u32], byte_bases: &[u32], out: &mut Emit) {
    for (code, pick) in [
        (
            Code::DuplicateId,
            (|unit: &Carried| &unit.ids) as fn(&Carried) -> &Vec<LineOccurrence>,
        ),
        (Code::DuplicateUsfm, |unit: &Carried| &unit.usfms),
    ] {
        let mut first = NO_TOKEN;
        for (at, unit) in units.iter().enumerate() {
            for occurrence in pick(unit) {
                let anchor = occurrence.anchor + token_bases[at];
                if first == NO_TOKEN {
                    first = anchor;
                    continue;
                }
                out.push_fixed(
                    Observation {
                        code,
                        anchor,
                        second: first,
                        aux: 0,
                    },
                    occurrence.from + byte_bases[at],
                    occurrence.to + byte_bases[at],
                    b"",
                );
            }
        }
    }
}

/// `numbering-mix`, once per family per book: the mix completes at the LATER
/// of the two global spelling-firsts, pointed at the family's global first.
fn families(units: &[&Carried], token_bases: &[u32], out: &mut Emit) {
    let mut first_any = [NO_TOKEN; generated::ROW_COUNT];
    let mut first_bare = [NO_TOKEN; generated::ROW_COUNT];
    let mut first_numbered = [NO_TOKEN; generated::ROW_COUNT];
    let mut rows: Vec<u8> = Vec::new();
    for (at, unit) in units.iter().enumerate() {
        let token_base = token_bases[at];
        for family in &unit.families {
            let slot = family.row as usize;
            if first_any[slot] == NO_TOKEN {
                rows.push(family.row);
                first_any[slot] = family.first_any + token_base;
            }
            if let Some(bare) = family.first_bare
                && first_bare[slot] == NO_TOKEN
            {
                first_bare[slot] = bare + token_base;
            }
            if let Some(numbered) = family.first_numbered
                && first_numbered[slot] == NO_TOKEN
            {
                first_numbered[slot] = numbered + token_base;
            }
        }
    }
    let mut mixes: Vec<Observation> = rows
        .into_iter()
        .filter(|row| {
            first_bare[*row as usize] != NO_TOKEN && first_numbered[*row as usize] != NO_TOKEN
        })
        .map(|row| Observation {
            code: Code::NumberingMix,
            anchor: first_bare[row as usize].max(first_numbered[row as usize]),
            second: first_any[row as usize],
            aux: match generated::numbering(row) {
                Numbering::UpTo(cap) => u32::from(cap),
                _ => 0,
            },
        })
        .collect();
    // Deterministic regardless of sighting order across chunks.
    mixes.sort_unstable_by_key(|observation| observation.anchor);
    for mix in mixes {
        out.push(mix);
    }
}

/// The cross-chunk half of the sid/eid obligation: an `-e` point with no
/// in-chunk sid before it owes one iff an EARLIER chunk opened the family —
/// most-recent sid wins, matching `sid_at`'s overwrite semantics.
fn sid_eid(units: &[&Carried], token_bases: &[u32], out: &mut Emit) {
    let mut sid_at = [NO_TOKEN; generated::ROW_COUNT];
    for (at, unit) in units.iter().enumerate() {
        let token_base = token_bases[at];
        for (row, anchor) in &unit.eid_candidates {
            let sid = sid_at[*row as usize];
            if sid != NO_TOKEN {
                out.push(Observation {
                    code: Code::AttrRequiredIf,
                    anchor: anchor + token_base,
                    second: sid,
                    aux: 0,
                });
            }
        }
        for (row, anchor) in &unit.sid_last {
            sid_at[*row as usize] = anchor + token_base;
        }
    }
}

/// The finish() facts — the rules only the whole fold can judge.
fn finish(units: &[&Carried], token_bases: &[u32], out: &mut Emit) {
    // missing-id: no `\id` anywhere, markers somewhere.
    if units.iter().all(|unit| unit.ids.is_empty()) && units.iter().any(|unit| unit.has_markers) {
        out.push(Observation::one(Code::MissingId, 0));
    }

    // missing-chapter / verse-before-first-chapter (ordering's finish pair):
    // exactly one of the two, never both.
    let first_chapter_unit = units.iter().position(|unit| unit.has_chapter);
    match first_chapter_unit {
        None => {
            if let Some((at, verse)) = units
                .iter()
                .enumerate()
                .find_map(|(at, unit)| unit.first_verse.map(|verse| (at, verse)))
            {
                out.push(Observation::one(
                    Code::MissingChapter,
                    verse + token_bases[at],
                ));
            }
        }
        Some(chapter_at) => {
            let before = units[..chapter_at]
                .iter()
                .enumerate()
                .find_map(|(at, unit)| unit.first_verse.map(|verse| (at, verse)));
            let within = units[chapter_at]
                .first_pre_chapter_verse
                .map(|verse| (chapter_at, verse));
            if let Some((at, verse)) = before.or(within) {
                out.push(Observation::one(
                    Code::VerseBeforeFirstChapter,
                    verse + token_bases[at],
                ));
            }
        }
    }

    // paragraph-before-first-chapter (flat's finish): only the fold knows
    // whether the book HAS a positional chapter, and whether verses sat up
    // there too (then the run is verse-before-first-chapter's, never both).
    let first_positional = units.iter().position(|unit| unit.positional_chapter);
    if let Some(chapter_at) = first_positional
        && let Some((at, pending)) = units
            .iter()
            .enumerate()
            .find_map(|(at, unit)| unit.pbfc_pending.map(|pending| (at, pending)))
    {
        let verses_up_there = units[..=chapter_at]
            .iter()
            .any(|unit| unit.verse_pre_chapter);
        if !verses_up_there {
            out.push(Observation::one(
                Code::ParagraphBeforeFirstChapter,
                pending + token_bases[at],
            ));
        }
    }
}

/// `expected` as decimal bytes (the seam renumber's insert text).
fn decimal(mut number: u32, buf: &mut [u8; 10]) -> &[u8] {
    let mut at = buf.len();
    loop {
        at -= 1;
        buf[at] = b'0' + (number % 10) as u8;
        number /= 10;
        if number == 0 {
            break;
        }
    }
    &buf[at..]
}
