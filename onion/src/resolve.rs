//! An address to its place in one text: found, missing or ambiguous.
//!
//! ```text
//! \c 1\n\p\n\v 1-2 a b\n\v 3 c     LUK 1:2  → Found 8..19, opening 5, covers LUK 1:1-2
//! \c 1\n\p\n\v 1,3 a\n\v 4 b       LUK 1:2  → Missing verse, within 0..24
//! \c 3\n\p a\n\c 3\n\p b\n         LUK 3    → Ambiguous [0..10, 10..20]
//! ```
//!
//! The rules, one example each, and where they depart from Sefer's: `resolve.md`.

use core::ops::Range;

pub use mise::address::{Address, Point};

use crate::scanner::lex;
use crate::tables::generated::{self, MarkerIdx};
use crate::tables::schema::{Category, MarkerKind};
use crate::toc::{ChapterRow, Toc, VerseAnchor, VerseMember};
use crate::token::TokenKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Found {
        span: Range<u32>,
        /// Where the run of openers before the first `\v` starts; `span.start`
        /// mid-paragraph and for every answer that is not a verse.
        opening: u32,
        /// What the span is: `LUK 1:1-2` when `LUK 1:2` landed in a bridge.
        covers: Address,
        /// A segment found as its whole verse, because the text does not divide it.
        coarser: bool,
    },
    Missing {
        missing: Missing,
        /// The chapter a missing verse would be in.
        within: Option<Range<u32>>,
    },
    Ambiguous(Vec<Candidate>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    Intro,
    Chapter,
    Verse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub span: Range<u32>,
    pub opening: u32,
}

impl Toc {
    /// `source` is the text this `Toc` was built from.
    pub fn resolve(&self, source: &str, address: &Address) -> Resolution {
        match address {
            Address::Verses { from, to } => self
                .verses_span(source, from, to)
                .unwrap_or_else(|refusal| refusal),
            _ => self.resolve_rows(address).expect("not a verse address"),
        }
    }

    /// The answer the chapter rows alone give, for a holder of the `Toc`
    /// without its text: a book, an intro or chapters. `None` for verses,
    /// which need the text's segment letters and opening walk.
    pub fn resolve_rows(&self, address: &Address) -> Option<Resolution> {
        let answer = match address {
            Address::Book => {
                let end = self.chapters.last().map_or(0, |row| row.end);
                Ok(found(0..end, Address::Book, false))
            }
            Address::Intro => {
                let front = &self.chapters[0];
                if front.end <= front.start {
                    Err(missing(Missing::Intro, None))
                } else {
                    Ok(found(front.span(), Address::Intro, false))
                }
            }
            Address::Chapters { from, to } => self.chapters_span(*from, *to, address),
            Address::Verses { .. } => return None,
        };
        Some(answer.unwrap_or_else(|refusal| refusal))
    }

    fn chapters_span(&self, from: u16, to: u16, address: &Address) -> Answer {
        let first = self.one_chapter(from)?.span();
        let last = self.one_chapter(to)?.span();
        let span = first.start.min(last.start)..first.end.max(last.end);
        Ok(found(span, address.clone(), false))
    }

    fn verses_span(&self, source: &str, from: &Point, to: &Point) -> Answer {
        let (first, first_coarser) = self.one_verse(source, from)?;
        let (last, last_coarser) = self.one_verse(source, to)?;
        let span = first.start.min(last.start)..first.end.max(last.end);
        let covered = self.addresses_covering(source, &span);
        let covers = match (covered.first(), covered.last()) {
            (Some(Address::Verses { .. }), Some(Address::Verses { .. })) => {
                let mut ends: Option<(Point, Point)> = None;
                for one in &covered {
                    let Address::Verses { from, to } = one else {
                        continue;
                    };
                    ends = Some(match ends {
                        None => (from.clone(), to.clone()),
                        Some((low, high)) => (
                            if from < &low { from.clone() } else { low },
                            if to.cmp_ends(&high).is_gt() {
                                to.clone()
                            } else {
                                high
                            },
                        ),
                    });
                }
                let (from, to) = ends.expect("two verse addresses were covered");
                Address::Verses { from, to }
            }
            _ => Address::Verses {
                from: from.clone(),
                to: to.clone(),
            },
        };
        Ok(Resolution::Found {
            opening: self.opening(source, span.start),
            span,
            covers,
            coarser: first_coarser || last_coarser,
        })
    }

    /// The one chapter row numbered `number`; row 0 is the front matter and
    /// never answers for a number.
    fn one_chapter(&self, number: u16) -> Result<&ChapterRow, Resolution> {
        let mut rows = self.chapters[1..].iter().filter(|row| row.number == number);
        let Some(only) = rows.next() else {
            return Err(missing(Missing::Chapter, None));
        };
        if rows.next().is_none() {
            return Ok(only);
        }
        Err(Resolution::Ambiguous(
            self.chapters[1..]
                .iter()
                .filter(|row| row.number == number)
                .map(|row| Candidate {
                    span: row.span(),
                    opening: row.start,
                })
                .collect(),
        ))
    }

    /// Where one point is: the one unit covering it, or, for a whole verse the
    /// text divides into segments, its first segment to its last.
    fn one_verse(&self, source: &str, point: &Point) -> Result<(Range<u32>, bool), Resolution> {
        let chapter = self.one_chapter(point.chapter)?;
        let mut whole = Vec::new();
        let mut parts = Vec::new();
        let mut coarser = false;
        for (verse, span) in self.verse_units(chapter) {
            if verse.first == 0 {
                continue;
            }
            match self.meet_verse(source.as_bytes(), verse, point) {
                Meet::None => {}
                Meet::Part => parts.push((verse, span)),
                Meet::Whole => whole.push(span),
                Meet::Coarser => {
                    whole.push(span);
                    coarser = true;
                }
            }
        }
        let ambiguous = |spans: Vec<Range<u32>>| {
            Resolution::Ambiguous(
                spans
                    .into_iter()
                    .map(|span| Candidate {
                        opening: self.opening(source, span.start),
                        span,
                    })
                    .collect(),
            )
        };
        match (whole.len(), parts.len()) {
            (1, 0) => Ok((whole.remove(0), coarser)),
            (0, 0) => Err(missing(Missing::Verse, Some(chapter.span()))),
            (0, _) => {
                if self.parts_overlap(source.as_bytes(), &parts, point) {
                    return Err(ambiguous(parts.into_iter().map(|(_, span)| span).collect()));
                }
                let from = parts.iter().map(|(_, span)| span.start).min();
                let to = parts.iter().map(|(_, span)| span.end).max();
                Ok((from.unwrap_or(0)..to.unwrap_or(0), false))
            }
            _ => {
                let mut spans = whole;
                spans.extend(parts.into_iter().map(|(_, span)| span));
                spans.sort_by_key(|span| span.start);
                Err(ambiguous(spans))
            }
        }
    }

    /// Whether two units' parts of one whole verse share a place: `\v 3a` and
    /// `\v 3a-3b` both hold 3a.
    fn parts_overlap(
        &self,
        source: &[u8],
        parts: &[(&VerseAnchor, Range<u32>)],
        point: &Point,
    ) -> bool {
        let pieces: Vec<(usize, Piece)> = parts
            .iter()
            .enumerate()
            .flat_map(|(unit, (verse, _))| {
                self.members_of(verse)
                    .iter()
                    .filter(move |member| meet(source, member, point) == Meet::Part)
                    .map(move |member| (unit, piece(source, member, point.verse)))
            })
            .collect();
        pieces.iter().enumerate().any(|(at, (unit, one))| {
            pieces[..at]
                .iter()
                .any(|(other_unit, other)| other_unit != unit && one.overlaps(other))
        })
    }

    /// Each verse of one chapter row with its extent, to the next `\v` or the
    /// row's end.
    fn verse_units<'t>(
        &'t self,
        chapter: &'t ChapterRow,
    ) -> impl Iterator<Item = (&'t VerseAnchor, Range<u32>)> + 't {
        let low = self.verses.partition_point(|v| v.at < chapter.start);
        let high = self.verses.partition_point(|v| v.at < chapter.end);
        let verses = &self.verses[low..high];
        verses.iter().enumerate().map(move |(at, verse)| {
            let end = verses.get(at + 1).map_or(chapter.end, |next| next.at);
            (verse, verse.at..end)
        })
    }

    /// The best any of a verse's members does: one covering the point wins.
    fn meet_verse(&self, source: &[u8], verse: &VerseAnchor, point: &Point) -> Meet {
        let members = self.members_of(verse);
        if members.is_empty() {
            if point.verse < verse.first || point.verse > verse.last {
                return Meet::None;
            }
            return if point.segment.is_none() {
                Meet::Whole
            } else {
                Meet::Coarser
            };
        }
        let mut best = Meet::None;
        for member in members {
            match meet(source, member, point) {
                Meet::Whole => return Meet::Whole,
                Meet::None => {}
                other => best = other,
            }
        }
        best
    }

    /// The address one verse unit names: its hull for a bridge or a list, its
    /// segments' first start and last end for a segmented one.
    fn unit_address(&self, source: &[u8], chapter: &ChapterRow, verse: &VerseAnchor) -> Address {
        if chapter.number == 0 {
            return Address::Book;
        }
        if verse.first == 0 {
            return Address::Chapters {
                from: chapter.number,
                to: chapter.number,
            };
        }
        let members = self.members_of(verse);
        if members.is_empty() {
            return Address::Verses {
                from: Point::verse(chapter.number, verse.first),
                to: Point::verse(chapter.number, verse.last),
            };
        }
        let place = |number: u16, start: u32, end: u32| Point {
            chapter: chapter.number,
            verse: number,
            segment: segment(source, start, end).map(|s| String::from_utf8_lossy(s).into_owned()),
        };
        let mut from: Option<Point> = None;
        let mut to: Option<Point> = None;
        for member in members {
            let start = place(
                member.from,
                member.from_segment_start,
                member.from_segment_end,
            );
            let end = place(member.to, member.to_segment_start, member.to_segment_end);
            if from.as_ref().is_none_or(|low| &start < low) {
                from = Some(start);
            }
            if to.as_ref().is_none_or(|high| end.cmp_ends(high).is_gt()) {
                to = Some(end);
            }
        }
        Address::Verses {
            from: from.expect("a verse with members"),
            to: to.expect("a verse with members"),
        }
    }

    /// The address of every unit a span touches, in source order: the
    /// intro, a chapter's head before its first verse, and each verse.
    fn addresses_covering(&self, source: &str, span: &Range<u32>) -> Vec<Address> {
        let touches = |unit: &Range<u32>| unit.end > span.start && unit.start < span.end;
        let mut out = Vec::new();
        for (row, chapter) in self.chapters.iter().enumerate() {
            if !touches(&chapter.span()) {
                continue;
            }
            let mut verses = self.verse_units(chapter).peekable();
            let head_end = verses.peek().map_or(chapter.end, |(verse, _)| verse.at);
            if touches(&(chapter.start..head_end)) {
                out.push(match (row, chapter.number) {
                    (0, _) => Address::Intro,
                    (_, 0) => Address::Book,
                    (_, number) => Address::Chapters {
                        from: number,
                        to: number,
                    },
                });
            }
            for (verse, unit) in verses {
                if touches(&unit) {
                    out.push(self.unit_address(source.as_bytes(), chapter, verse));
                }
            }
        }
        out
    }

    /// Where the run of openers before the `\v` at `at` starts, walking back
    /// no further than the previous `\v` or the chapter's `\c`. Only that
    /// window is lexed.
    fn opening(&self, source: &str, at: u32) -> u32 {
        let chapter = &self.chapters[self.chapter_row(at)];
        let before = self.verses.partition_point(|v| v.at < at);
        let low = before
            .checked_sub(1)
            .map(|previous| self.verses[previous].at)
            .filter(|previous| *previous >= chapter.start)
            .unwrap_or(chapter.start);
        let window = &source[low as usize..at as usize];
        let mut opening = at;
        let mut pending_text = false;
        for token in lex(window).iter().rev() {
            let bytes = &window.as_bytes()[token.start as usize..token.end() as usize];
            let role = match token.kind() {
                TokenKind::Marker { nested: false } => opener(token.marker_idx),
                _ => None,
            };
            match (token.kind(), role) {
                (_, Some(Opener::Heading)) => {
                    opening = low + token.start;
                    pending_text = false;
                }
                (_, Some(Opener::Line)) if !pending_text => opening = low + token.start,
                (_, Some(Opener::Line)) => break,
                (_, Some(Opener::ChapterHead)) => pending_text = false,
                (
                    TokenKind::Newline
                    | TokenKind::Pad
                    | TokenKind::Milestone { .. }
                    | TokenKind::MilestoneTerminator
                    | TokenKind::AttrList,
                    None,
                ) => {}
                (TokenKind::Text, None) if bytes.iter().all(u8::is_ascii_whitespace) => {}
                (TokenKind::Marker { .. }, None)
                    if generated::kind(token.marker_idx) == MarkerKind::Milestone => {}
                (TokenKind::Marker { .. }, None)
                    if matches!(
                        generated::kind(token.marker_idx),
                        MarkerKind::Character | MarkerKind::Note
                    ) =>
                {
                    pending_text = true;
                }
                (
                    TokenKind::Text
                    | TokenKind::ClosingMarker { .. }
                    | TokenKind::NoteCaller
                    | TokenKind::OptBreak
                    | TokenKind::Designator,
                    None,
                ) => pending_text = true,
                _ => break,
            }
        }
        opening
    }
}

type Answer = Result<Resolution, Resolution>;

fn found(span: Range<u32>, covers: Address, coarser: bool) -> Resolution {
    Resolution::Found {
        opening: span.start,
        span,
        covers,
        coarser,
    }
}

fn missing(missing: Missing, within: Option<Range<u32>>) -> Resolution {
    Resolution::Missing { missing, within }
}

/// How one member meets one point of the same chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Meet {
    /// It covers the point entirely.
    Whole,
    /// The point is a whole verse and the member only some of it: `3a` for 3.
    Part,
    /// The point is a segment and the member its whole verse.
    Coarser,
    None,
}

fn meet(source: &[u8], member: &VerseMember, point: &Point) -> Meet {
    let verse = point.verse;
    if verse < member.from || verse > member.to {
        return Meet::None;
    }
    let from_segment = segment(source, member.from_segment_start, member.from_segment_end)
        .filter(|_| verse == member.from);
    let to_segment = segment(source, member.to_segment_start, member.to_segment_end)
        .filter(|_| verse == member.to);
    let cut = from_segment.is_some() || to_segment.is_some();
    let Some(asked) = point.segment.as_deref().map(str::as_bytes) else {
        return if cut { Meet::Part } else { Meet::Whole };
    };
    if from_segment.is_some_and(|low| asked < low) || to_segment.is_some_and(|high| asked > high) {
        return Meet::None;
    }
    if cut { Meet::Whole } else { Meet::Coarser }
}

/// What one member holds of a whole verse it cuts, as segment bounds; `None`
/// is the verse's open end on that side.
struct Piece<'s> {
    low: Option<&'s [u8]>,
    high: Option<&'s [u8]>,
}

impl Piece<'_> {
    fn overlaps(&self, other: &Piece<'_>) -> bool {
        let at_most = |low: Option<&[u8]>, high: Option<&[u8]>| match (low, high) {
            (Some(low), Some(high)) => low <= high,
            _ => true,
        };
        at_most(self.low, other.high) && at_most(other.low, self.high)
    }
}

fn piece<'s>(source: &'s [u8], member: &VerseMember, verse: u16) -> Piece<'s> {
    Piece {
        low: segment(source, member.from_segment_start, member.from_segment_end)
            .filter(|_| verse == member.from),
        high: segment(source, member.to_segment_start, member.to_segment_end)
            .filter(|_| verse == member.to),
    }
}

/// A member's segment letters, `None` when none is written.
fn segment(source: &[u8], start: u32, end: u32) -> Option<&[u8]> {
    (end > start).then(|| &source[start as usize..end as usize])
}

enum Opener {
    /// Joins the run only with nothing but whitespace after it.
    Line,
    /// Joins the run with its text.
    Heading,
    /// Its text is its own; the run passes over it without starting there.
    ChapterHead,
}

fn opener(marker: MarkerIdx) -> Option<Opener> {
    match (generated::category(marker), generated::name(marker)) {
        (_, "cl" | "cp" | "ca" | "cd") => Some(Opener::ChapterHead),
        (_, "qa") => Some(Opener::Heading),
        (Category::ParaTitlesSections, "mt" | "mte") => None,
        (Category::ParaTitlesSections, _) => Some(Opener::Heading),
        (Category::ParaBody | Category::ParaPoetry | Category::ParaLists, _) => Some(Opener::Line),
        _ => None,
    }
}
