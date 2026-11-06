//! An SRS reference located in one text, and a span spelled as one.
//!
//! ```text
//! locate(jonah, "JON 2:8!2-4")     -> the bytes of "who cling to"
//! locate(jonah, "JON 1:4!s1!2")    -> "Great", in the heading 1:4 opens
//! locate(jonah, "JON 1:13!f!5")    -> "dug", in 1:13's footnote
//! locate(text, "MAT 1:1!5+2")      -> the source character holding NFD character 2
//! spell(jonah, span of "cling to") -> "JON 2:8!3-4"
//! ```
//!
//! The rules are the Scripture Referencing Standard's (`srs` crate), on
//! onion's tokens:
//!
//! - A WORD is a run of non-space characters inside one text run: a marker of
//!   any kind ends a word (`anthro\f + \ft human\f*pomorphic` is two words).
//!   Spaces are the standard's `ws` plus ASCII whitespace and USFM's `~`.
//! - A verse's words are its scripture: notes are counted on their own, and a
//!   heading paragraph's words are its own (`!s1!2`), never the verse's.
//! - A CHARACTER is counted in NFD (`canon::nfd_len`), so `é` is two. A
//!   reference to either half answers the source character holding it.
//! - An ELEMENT step (`!f`, `!s1`, `!q1[2]`, `!w`) finds the nth element of
//!   that marker: for a paragraph marker, first the headings and titles just
//!   before the paragraph the range starts in, then elements starting inside
//!   the range, and last the paragraph containing the range's start (so
//!   `LUK 8:10!p` is the part of 8:10 in its `\p`). A negative index counts
//!   back from the range's start. `s` matches `\s1`; `s1` matches `\s`.
//! - A word's range runs on to the next word, so the note after it is "its"
//!   note (`1:13!5!f`).
//! - An ATTRIBUTE step (`@strong`, `@caller`, `@number`) is read off the
//!   element in hand, or the nearest element containing it that has one.
//!
//! Named references (`k:`, `a:`, `t:`) are refused by name: onion holds no
//! `aid` anchors yet.

use core::ops::Range;

use srs::{CharStep, Index, Place, SrsRange, SrsRef, Step};

use crate::onion::Toc;
use crate::onion::extensions::Extensions;
use crate::onion::resolve::{Address, Point, Resolution};
use crate::onion::tables::generated::{self, MarkerIdx};
use crate::onion::tables::schema::{Category, MarkerKind};
use crate::onion::{Token, TokenKind};

/// Why a reference has no place in this text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocateError {
    /// The text has no such place: `"verse"`, `"word"`, `"character"`,
    /// `"element"`, `"attribute"`, `"chapter"`.
    Missing(&'static str),
    /// The text holds the place twice (a repeated chapter or verse).
    Ambiguous,
    /// Something this locator doesn't read: `"named reference"`, `"book"`
    /// (a reference into another book than the text's).
    Unsupported(&'static str),
}

/// Where `range` is in `source`, in bytes. The book is the caller's to match:
/// a reference to another book is [`LocateError::Unsupported`].
pub fn locate(source: &str, ext: &Extensions, range: &SrsRange) -> Result<Range<u32>, LocateError> {
    let model = Model::new(source, ext);
    let first = model.locate_one(&range.first)?;
    let Some(last) = &range.last else {
        return Ok(first);
    };
    let last = model.locate_one(last)?;
    Ok(first.start.min(last.start)..first.end.max(last.end))
}

/// The SRS spelling of a span of `source`: the verse each end is in, and the
/// words (and characters, when an end falls inside a word) it runs between, in
/// a verse's scripture, one of its notes, or a heading it opens. A span of
/// whole verses is the verses; an empty span (a caret) is one place. `None`
/// outside any verse.
pub fn spell(source: &str, ext: &Extensions, span: Range<u32>) -> Option<SrsRange> {
    Model::new(source, ext).spell(span)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Para,
    Note,
    Char,
    Verse,
    Chapter,
    Milestone,
}

#[derive(Debug, Clone)]
struct Element {
    kind: Kind,
    marker: MarkerIdx,
    /// The marker as spelled, without its backslash: `s1`, `f`, `w`.
    name: String,
    /// The marker's first byte to the element's end.
    span: Range<u32>,
    /// What the element holds: after its marker, caller and leading
    /// attributes, to its closing marker.
    content: Range<u32>,
    attrs: Option<Token>,
    /// A note's caller, a chapter's or verse's number.
    caller: Option<Range<u32>>,
    number: Option<Range<u32>>,
    parent: Option<usize>,
}

/// A word: source bytes, and where it sits.
#[derive(Debug, Clone)]
struct Word {
    span: Range<u32>,
    /// The innermost note holding it.
    note: Option<usize>,
    /// The paragraph holding it.
    para: Option<usize>,
}

struct Model<'s> {
    source: &'s str,
    toc: Toc,
    elements: Vec<Element>,
    words: Vec<Word>,
}

/// What a step refines: a span, and the element it is, if one.
#[derive(Debug, Clone)]
struct Scope {
    span: Range<u32>,
    /// Where word and element searches may look: past a word, to the next.
    reach: Range<u32>,
    element: Option<usize>,
    /// A verse's (or chapter's) scripture: headings and notes are not its words.
    scripture: bool,
    /// The word in hand, for a character step.
    word: Option<usize>,
}

impl<'s> Model<'s> {
    fn new(source: &'s str, ext: &Extensions) -> Model<'s> {
        let tokens = crate::onion::lex_with(source, ext);
        let toc = crate::onion::toc(source.as_bytes(), &tokens);
        let mut model = Model {
            source,
            toc,
            elements: Vec::new(),
            words: Vec::new(),
        };
        model.build(&tokens);
        model
    }

    /// One walk over the tokens: elements with their extents, and words.
    fn build(&mut self, tokens: &[Token]) {
        let len = self.source.len() as u32;
        // Open containers, innermost last: indices into `elements`.
        let mut open: Vec<usize> = Vec::new();
        let mut para: Option<usize> = None;
        // The current text run: words never cross a marker.
        let mut run: Option<u32> = None;
        let mut i = 0;
        while i < tokens.len() {
            let token = tokens[i];
            let kind = token.kind();
            let textual = matches!(kind, TokenKind::Text | TokenKind::Newline | TokenKind::Pad);
            if textual {
                run.get_or_insert(token.start);
                i += 1;
                continue;
            }
            if let Some(start) = run.take() {
                self.words_in(start..token.start, &open, para);
            }
            match kind {
                TokenKind::Marker { .. } => {
                    let marker = token.marker_idx;
                    let element_kind = match generated::kind(marker) {
                        MarkerKind::Paragraph
                        | MarkerKind::Periph
                        | MarkerKind::TableRow
                        | MarkerKind::Sidebar
                        | MarkerKind::Header
                        | MarkerKind::Meta => Some(Kind::Para),
                        MarkerKind::Chapter => Some(Kind::Chapter),
                        MarkerKind::Verse => Some(Kind::Verse),
                        MarkerKind::Note => Some(Kind::Note),
                        MarkerKind::Character | MarkerKind::TableCell | MarkerKind::Figure => {
                            Some(Kind::Char)
                        }
                        _ => None,
                    };
                    if matches!(element_kind, Some(Kind::Para | Kind::Chapter)) {
                        // A paragraph or chapter closes everything open.
                        for at in open.drain(..).rev() {
                            self.elements[at].span.end = token.start;
                            self.elements[at].content.end =
                                self.elements[at].content.end.min(token.start);
                        }
                        if let Some(p) = para.take() {
                            self.elements[p].span.end = token.start;
                            self.elements[p].content.end = token.start;
                        }
                    }
                    let name = self.source[token.start as usize + 1..token.end() as usize]
                        .trim_end()
                        .trim_start_matches('+')
                        .to_string();
                    let mut element = Element {
                        kind: element_kind.unwrap_or(Kind::Milestone),
                        marker,
                        name,
                        span: token.start..len,
                        content: token.end()..len,
                        attrs: None,
                        caller: None,
                        number: None,
                        parent: open.last().copied().or(para),
                    };
                    // What follows the marker and belongs to it.
                    let mut next = i + 1;
                    while let Some(t) = tokens.get(next) {
                        match t.kind() {
                            TokenKind::NoteCaller => {
                                // The caller, less the delimiter space folded onto it.
                                let written = &self.source[t.start as usize..t.end() as usize];
                                let len = written.trim_end().len() as u32;
                                element.caller = Some(t.start..t.start + len);
                            }
                            TokenKind::Designator => {
                                let written = &self.source[t.start as usize..t.end() as usize];
                                let len = written.trim_end().len() as u32;
                                element.number = Some(t.start..t.start + len);
                            }
                            TokenKind::AttrList => element.attrs = Some(*t),
                            TokenKind::Pad => {}
                            _ => break,
                        }
                        element.content.start = t.end();
                        next += 1;
                    }
                    // A note's own markers (`\\fr`, `\\ft`, `\\xo`, …) close the one
                    // before them: `\\fr 1:1 \\ft text` is two spans, not nested.
                    if element.kind == Kind::Char && note_content(marker) {
                        while let Some(&top) = open.last() {
                            let open_one = &mut self.elements[top];
                            if open_one.kind != Kind::Char || !note_content(open_one.marker) {
                                break;
                            }
                            open_one.span.end = token.start;
                            open_one.content.end = open_one.content.end.min(token.start);
                            open.pop();
                        }
                    }
                    let at = self.elements.len();
                    match element.kind {
                        Kind::Para => {
                            self.elements.push(element);
                            para = Some(at);
                        }
                        Kind::Note | Kind::Char => {
                            self.elements.push(element);
                            open.push(at);
                        }
                        Kind::Chapter | Kind::Verse | Kind::Milestone => {
                            let end = tokens[next - 1].end();
                            element.span.end = end;
                            element.content = end..end;
                            self.elements.push(element);
                        }
                    }
                    i = next;
                    continue;
                }
                TokenKind::ClosingMarker { .. } => {
                    if let Some(pos) = open
                        .iter()
                        .rposition(|&at| self.elements[at].marker == token.marker_idx)
                    {
                        for at in open.drain(pos..).rev() {
                            // An attribute list before the closer already ended it.
                            let content = &mut self.elements[at].content;
                            content.end = content.end.min(token.start);
                            self.elements[at].span.end = token.end();
                        }
                    }
                }
                TokenKind::AttrList => {
                    // Attributes belong to the innermost open element.
                    if let Some(&at) = open.last() {
                        self.elements[at].attrs.get_or_insert(token);
                        self.elements[at].content.end = token.start;
                    }
                }
                TokenKind::Milestone { .. } => {
                    let mut end = token.end();
                    let mut attrs = None;
                    let mut next = i + 1;
                    while let Some(t) = tokens.get(next) {
                        match t.kind() {
                            TokenKind::AttrList => attrs = Some(*t),
                            TokenKind::MilestoneTerminator => {}
                            _ => break,
                        }
                        end = t.end();
                        next += 1;
                    }
                    self.elements.push(Element {
                        kind: Kind::Milestone,
                        marker: token.marker_idx,
                        name: self.source[token.start as usize + 1..token.end() as usize]
                            .trim_end()
                            .to_string(),
                        span: token.start..end,
                        content: end..end,
                        attrs,
                        caller: None,
                        number: None,
                        parent: open.last().copied().or(para),
                    });
                    i = next;
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        if let Some(start) = run.take() {
            self.words_in(start..len, &open, para);
        }
        for at in open {
            self.elements[at].span.end = len;
        }
        if let Some(p) = para {
            self.elements[p].span.end = len;
            self.elements[p].content.end = len;
        }
    }

    /// The words of one text run.
    fn words_in(&mut self, run: Range<u32>, open: &[usize], para: Option<usize>) {
        let note = open
            .iter()
            .rev()
            .copied()
            .find(|&at| self.elements[at].kind == Kind::Note);
        let text = &self.source[run.start as usize..run.end as usize];
        let mut start: Option<usize> = None;
        for (at, c) in text.char_indices().chain([(text.len(), ' ')]) {
            match (is_space(c), start) {
                (false, None) => start = Some(at),
                (true, Some(from)) => {
                    self.words.push(Word {
                        span: run.start + from as u32..run.start + at as u32,
                        note,
                        para,
                    });
                    start = None;
                }
                _ => {}
            }
        }
    }

    fn is_heading(&self, at: usize) -> bool {
        let element = &self.elements[at];
        element.kind == Kind::Para
            && (matches!(
                generated::category(element.marker),
                Category::ParaTitlesSections | Category::ParaIdentification
            ) || element.name == "qa")
    }

    // ── locate ─────────────────────────────────────────────────────────

    fn locate_one(&self, r: &SrsRef) -> Result<Range<u32>, LocateError> {
        if mise::books::BookKey::new(self.toc.book.map(|b| b.to_ascii_uppercase())) != r.book {
            return Err(LocateError::Unsupported("book"));
        }
        let mut scope = self.place(&r.place)?;
        for step in &r.steps {
            scope = match step {
                Step::Word { word, char } => {
                    let word = self.word(&scope, *word)?;
                    match char {
                        None => word,
                        Some(c) => self.char(&word, *c)?,
                    }
                }
                Step::Element { marker, index } => {
                    self.element(&scope, marker, index.unwrap_or(1))?
                }
                Step::Attribute(name) => self.attribute(&scope, name)?,
            };
        }
        Ok(scope.span)
    }

    fn place(&self, place: &Place) -> Result<Scope, LocateError> {
        let whole = |span: Range<u32>| Scope {
            reach: span.clone(),
            span,
            element: None,
            scripture: true,
            word: None,
        };
        let found = |answer: Resolution| match answer {
            Resolution::Found { span, .. } => Ok(span),
            Resolution::Missing { missing, .. } => Err(LocateError::Missing(match missing {
                crate::onion::resolve::Missing::Intro => "introduction",
                crate::onion::resolve::Missing::Chapter => "chapter",
                crate::onion::resolve::Missing::Verse => "verse",
            })),
            Resolution::Ambiguous(_) => Err(LocateError::Ambiguous),
        };
        let narrow = |n: u32| u16::try_from(n).map_err(|_| LocateError::Missing("chapter"));
        match place {
            Place::Book => Ok(whole(0..self.source.len() as u32)),
            Place::Chapter(0) => found(self.toc.resolve(self.source, &Address::Intro)).map(whole),
            Place::Chapter(c) => {
                let c = narrow(*c)?;
                found(
                    self.toc
                        .resolve(self.source, &Address::Chapters { from: c, to: c }),
                )
                .map(whole)
            }
            Place::Verse {
                chapter,
                verse,
                segment,
            } => {
                let chapter = narrow(*chapter)?;
                let verse = match verse {
                    Index::At(v) => u16::try_from(*v).map_err(|_| LocateError::Missing("verse"))?,
                    Index::End => self.last_verse(chapter)?,
                };
                let at = Point {
                    chapter,
                    verse,
                    segment: *segment,
                };
                let span = found(
                    self.toc
                        .resolve(self.source, &Address::Verses { from: at, to: at }),
                )?;
                let mut scope = whole(span);
                // Verse 0 is the title: its own words, not scripture's.
                if verse == 0 {
                    scope.scripture = false;
                    scope.element = self
                        .elements
                        .iter()
                        .position(|e| e.kind == Kind::Para && e.span.start == scope.span.start);
                }
                Ok(scope)
            }
            Place::Named { .. } => Err(LocateError::Unsupported("named reference")),
        }
    }

    /// The highest verse the text numbers in a chapter.
    fn last_verse(&self, chapter: u16) -> Result<u16, LocateError> {
        let span = self
            .toc
            .chapter_span(chapter)
            .ok_or(LocateError::Missing("chapter"))?;
        self.toc
            .verses
            .iter()
            .filter(|v| span.contains(&v.at) && v.last > 0)
            .map(|v| v.last)
            .max()
            .ok_or(LocateError::Missing("verse"))
    }

    /// The words a scope holds.
    fn scope_words(&self, scope: &Scope) -> Vec<usize> {
        let inside = |w: &Word| scope.span.start <= w.span.start && w.span.end <= scope.span.end;
        let element = scope.element.map(|at| &self.elements[at]);
        (0..self.words.len())
            .filter(|&at| {
                let w = &self.words[at];
                if !inside(w) {
                    return false;
                }
                match element.map(|e| e.kind) {
                    // A note's words are all of its text, past its caller.
                    Some(Kind::Note) => w.note == scope.element,
                    // An element's words: its text, notes inside it not.
                    Some(_) => {
                        let note = element.and_then(|e| self.note_of(e));
                        w.note == note
                    }
                    None if scope.scripture => {
                        w.note.is_none() && !w.para.is_some_and(|p| self.is_heading(p))
                    }
                    None => w.note.is_none(),
                }
            })
            .collect()
    }

    /// The note an element sits inside, if any.
    fn note_of(&self, element: &Element) -> Option<usize> {
        let mut parent = element.parent;
        while let Some(at) = parent {
            if self.elements[at].kind == Kind::Note {
                return Some(at);
            }
            parent = self.elements[at].parent;
        }
        None
    }

    fn word(&self, scope: &Scope, index: Index) -> Result<Scope, LocateError> {
        let words = self.scope_words(scope);
        let pick = match index {
            Index::At(0) => None,
            Index::At(n) => words.get(n as usize - 1),
            Index::End => words.last(),
        };
        let &at = pick.ok_or(LocateError::Missing("word"))?;
        let span = self.words[at].span.clone();
        // A word reaches on to the next word: a note after it is its note.
        let next = words
            .iter()
            .map(|&w| self.words[w].span.start)
            .find(|&start| start >= span.end)
            .unwrap_or(scope.reach.end.max(span.end));
        Ok(Scope {
            reach: span.start..next,
            span,
            element: scope
                .element
                .filter(|&e| self.elements[e].kind == Kind::Note),
            scripture: scope.scripture,
            word: Some(at),
        })
    }

    /// A character, counted in NFD: the source character holding it, or the
    /// gap after it.
    fn char(&self, word: &Scope, step: CharStep) -> Result<Scope, LocateError> {
        let span = &word.span;
        let text = &self.source[span.start as usize..span.end as usize];
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let total: usize = chars.iter().map(|(_, c)| canon::nfd_len(*c)).sum();
        let wanted = match step.at {
            Index::At(n) => n as usize,
            Index::End => total,
        };
        if wanted > total {
            return Err(LocateError::Missing("character"));
        }
        let at = |byte: usize| span.start + byte as u32;
        let (start, end) = if wanted == 0 {
            (span.start, span.start)
        } else {
            let mut seen = 0;
            let (byte, c) = chars
                .iter()
                .copied()
                .find(|(_, c)| {
                    seen += canon::nfd_len(*c);
                    seen >= wanted
                })
                .ok_or(LocateError::Missing("character"))?;
            (at(byte), at(byte + c.len_utf8()))
        };
        let span = if step.gap {
            // The gap after it: to the next character, or past the word to
            // the next word.
            let next = if end < span.end { end } else { word.reach.end };
            end..next.max(end)
        } else {
            start..end
        };
        Ok(Scope {
            reach: span.clone(),
            span,
            element: None,
            scripture: word.scripture,
            word: word.word,
        })
    }

    fn element(&self, scope: &Scope, wanted: &str, index: i32) -> Result<Scope, LocateError> {
        let named = |e: &Element| marker_matches(&e.name, wanted);
        let within = scope.reach.clone();
        let element_scope = |at: usize, span: Range<u32>| Scope {
            reach: span.clone(),
            span,
            element: Some(at),
            scripture: false,
            word: None,
        };
        if index < 0 {
            let back = index.unsigned_abs() as usize;
            let chapter_start = self.chapter_start(within.start);
            let before: Vec<usize> = (0..self.elements.len())
                .filter(|&at| {
                    let e = &self.elements[at];
                    named(e) && e.span.start < within.start && e.span.start >= chapter_start
                })
                .collect();
            let &at = before
                .iter()
                .rev()
                .nth(back - 1)
                .ok_or(LocateError::Missing("element"))?;
            return Ok(element_scope(at, self.elements[at].content.clone()));
        }
        let candidates = self.candidates(scope, wanted);
        if let Some(&at) = candidates.get(index as usize - 1) {
            return Ok(element_scope(at, self.elements[at].content.clone()));
        }
        // The paragraph the range starts in, where it is that marker: the part
        // of the range inside it.
        if index == 1
            && let Some(at) = self.paragraph_at(within.start)
            && named(&self.elements[at])
        {
            let content = &self.elements[at].content;
            // From the verse's text: past its `\\v` and number.
            let from = self
                .elements
                .iter()
                .find(|e| e.kind == Kind::Verse && e.span.start == scope.span.start)
                .map_or(scope.span.start, |verse| verse.span.end);
            let span = content.start.max(from)..content.end.min(scope.span.end);
            return Ok(element_scope(at, span));
        }
        Err(LocateError::Missing("element"))
    }

    /// The elements a positive `!marker[n]` counts, in order: for a verse or
    /// chapter, the headings and titles just before the paragraph the range
    /// starts in; then the elements starting inside the range.
    fn candidates(&self, scope: &Scope, wanted: &str) -> Vec<usize> {
        let named = |e: &Element| marker_matches(&e.name, wanted);
        let within = &scope.reach;
        let mut out: Vec<usize> = Vec::new();
        if scope.element.is_none() {
            out.extend(
                self.headings_before(within.start)
                    .into_iter()
                    .filter(|&at| named(&self.elements[at])),
            );
        }
        out.extend((0..self.elements.len()).filter(|&at| {
            let e = &self.elements[at];
            named(e)
                && within.start <= e.span.start
                && e.span.start < within.end
                && Some(at) != scope.element
                && (scope.element.is_some() || e.kind == Kind::Note || self.note_of(e).is_none())
        }));
        out
    }

    /// The heading and title paragraphs in a row just before the paragraph
    /// holding `byte`, in document order; none past a chapter.
    fn headings_before(&self, byte: u32) -> Vec<usize> {
        let Some(holder) = self.paragraph_at(byte) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut at = holder;
        while at > 0 {
            at -= 1;
            let e = &self.elements[at];
            // Notes, character spans and milestones inside a heading are
            // passed over; a verse, a chapter or a body paragraph ends the run.
            match e.kind {
                Kind::Chapter | Kind::Verse => break,
                Kind::Para if self.is_heading(at) => out.push(at),
                Kind::Para => break,
                Kind::Note | Kind::Char | Kind::Milestone => {}
            }
        }
        out.reverse();
        out
    }

    /// The innermost paragraph holding a byte.
    fn paragraph_at(&self, byte: u32) -> Option<usize> {
        (0..self.elements.len()).rev().find(|&at| {
            let e = &self.elements[at];
            e.kind == Kind::Para && e.span.start <= byte && byte < e.span.end
        })
    }

    fn chapter_start(&self, byte: u32) -> u32 {
        self.toc
            .chapters
            .iter()
            .rev()
            .find(|c| c.start <= byte)
            .map_or(0, |c| c.start)
    }

    fn attribute(&self, scope: &Scope, name: &str) -> Result<Scope, LocateError> {
        let value = |span: Range<u32>| Scope {
            reach: span.clone(),
            span,
            element: None,
            scripture: false,
            word: None,
        };
        // The element in hand; for a word or a verse, the elements around it.
        let mut candidates: Vec<usize> = Vec::new();
        if let Some(at) = scope.element {
            let mut next = Some(at);
            while let Some(at) = next {
                candidates.push(at);
                next = self.elements[at].parent;
            }
        } else {
            let start = scope.span.start;
            let mut around: Vec<usize> = (0..self.elements.len())
                .filter(|&at| {
                    let e = &self.elements[at];
                    e.kind != Kind::Para
                        && e.span.start <= start
                        && start < e.span.end.max(e.span.start + 1)
                })
                .collect();
            around.reverse();
            candidates.extend(around);
            // A verse's own number: `2:1@number`.
            if let Some(v) = self
                .elements
                .iter()
                .position(|e| {
                    e.kind == Kind::Verse
                        && e.span.end <= start + 1
                        && scope.span.start <= e.span.end
                })
                .or_else(|| {
                    (0..self.elements.len()).rev().find(|&at| {
                        self.elements[at].kind == Kind::Verse
                            && self.elements[at].span.start <= start
                    })
                })
            {
                candidates.push(v);
            }
        }
        for at in candidates {
            let e = &self.elements[at];
            let found = match name {
                "caller" => e.caller.clone(),
                "number" => e.number.clone(),
                "category" => self
                    .elements
                    .iter()
                    .find(|c| c.parent == Some(at) && c.name == "cat")
                    .map(|c| c.content.clone()),
                _ => e
                    .attrs
                    .and_then(|list| self.attr_value(e.marker, &list, name)),
            };
            if let Some(span) = found {
                return Ok(value(span));
            }
        }
        Err(LocateError::Missing("attribute"))
    }

    fn attr_value(&self, marker: MarkerIdx, list: &Token, name: &str) -> Option<Range<u32>> {
        let default = generated::default_attribute(marker);
        crate::onion::attributes::attrs(self.source.as_bytes(), list).find_map(
            |event| match event {
                crate::onion::attributes::AttrEvent::Attr(attr) => {
                    let named = if attr.name.is_empty() {
                        default.is_some_and(|d| d == name)
                    } else {
                        attr.name == name.as_bytes()
                    };
                    named.then(|| attr.value_span.clone())
                }
                _ => None,
            },
        )
    }

    // ── spell ──────────────────────────────────────────────────────────

    fn spell(&self, span: Range<u32>) -> Option<SrsRange> {
        let (book, first_place, first_verse) = self.verse_at(span.start)?;
        let reference = |place: &Place, steps: Vec<Step>| SrsRef {
            translation: None,
            book,
            place: place.clone(),
            steps,
        };
        let first_steps = self.word_steps(&first_verse, span.start, false)?;
        // A caret is one place.
        if span.end <= span.start {
            return Some(SrsRange {
                first: reference(&first_place, first_steps),
                last: None,
            });
        }
        // The end, in its own verse: a span may run on into the next.
        let (_, last_place, last_verse) = self.verse_at(span.end - 1)?;
        let last_steps = self.word_steps(&last_verse, span.end, true)?;
        // A span holding whole verses is the verses.
        let first_words = self.scope_words(&first_verse);
        let last_words = self.scope_words(&last_verse);
        let whole = first_words
            .first()
            .is_some_and(|&w| self.words[w].span.start >= span.start)
            && last_words
                .last()
                .is_some_and(|&w| self.words[w].span.end <= span.end)
            && span.end <= last_verse.span.end;
        let (first, last) = if whole {
            (
                reference(&first_place, Vec::new()),
                reference(&last_place, Vec::new()),
            )
        } else {
            (
                reference(&first_place, first_steps),
                reference(&last_place, last_steps),
            )
        };
        Some(SrsRange {
            last: (last != first).then_some(last),
            first,
        })
    }

    /// The verse holding a byte, as its book, its place and its scope.
    fn verse_at(&self, byte: u32) -> Option<(mise::books::BookKey, Place, Scope)> {
        let (book, address) = self.toc.address_at(self.source, byte);
        let Address::Verses { from, .. } = address else {
            return None;
        };
        let place = Place::Verse {
            chapter: u32::from(from.chapter),
            verse: Index::At(u32::from(from.verse)),
            segment: from.segment,
        };
        let scope = self.place(&place).ok()?;
        Some((book, place, scope))
    }

    /// The steps naming a byte inside a verse: a note's word, or the verse's.
    /// `end` names the word holding the byte before it (an exclusive end).
    fn word_steps(&self, verse: &Scope, byte: u32, end: bool) -> Option<Vec<Step>> {
        let probe = if end { byte.saturating_sub(1) } else { byte };
        let mut steps = Vec::new();
        let mut scope = verse.clone();
        // Inside a note: the note, then its word.
        if let Some(note) = (0..self.elements.len()).find(|&at| {
            let e = &self.elements[at];
            e.kind == Kind::Note && e.span.start <= probe && probe < e.span.end
        }) {
            let notes: Vec<usize> = (0..self.elements.len())
                .filter(|&at| {
                    let e = &self.elements[at];
                    e.kind == Kind::Note
                        && e.name == self.elements[note].name
                        && verse.span.start <= e.span.start
                        && e.span.start < verse.span.end
                })
                .collect();
            let nth = notes.iter().position(|&at| at == note)? + 1;
            steps.push(Step::Element {
                marker: self.elements[note].name.clone(),
                index: (nth > 1).then_some(nth as i32),
            });
            scope = Scope {
                span: self.elements[note].content.clone(),
                reach: self.elements[note].content.clone(),
                element: Some(note),
                scripture: false,
                word: None,
            };
        } else if let Some(heading) = self.paragraph_at(probe).filter(|&p| self.is_heading(p)) {
            // Inside a heading: the heading, counted as `!s1[n]` counts, then
            // its word.
            let name = self.elements[heading].name.clone();
            let nth = self
                .candidates(verse, &name)
                .iter()
                .position(|&at| at == heading)?
                + 1;
            steps.push(Step::Element {
                marker: name,
                index: (nth > 1).then_some(nth as i32),
            });
            scope = Scope {
                span: self.elements[heading].content.clone(),
                reach: self.elements[heading].content.clone(),
                element: Some(heading),
                scripture: false,
                word: None,
            };
        }
        let words = self.scope_words(&scope);
        let (n, word) = if end {
            words
                .iter()
                .enumerate()
                .rev()
                .find(|(_, w)| self.words[**w].span.start < byte)?
        } else {
            words
                .iter()
                .enumerate()
                .find(|(_, w)| self.words[**w].span.end > byte)?
        };
        let w = &self.words[*word];
        let text = &self.source[w.span.start as usize..w.span.end as usize];
        let nfd_upto = |to: u32| -> u32 {
            text.char_indices()
                .take_while(|(at, _)| w.span.start + (*at as u32) < to)
                .map(|(_, c)| canon::nfd_len(c) as u32)
                .sum()
        };
        let char = if end && byte < w.span.end {
            Some(CharStep {
                at: Index::At(nfd_upto(byte)),
                gap: false,
            })
        } else if !end && byte > w.span.start {
            Some(CharStep {
                at: Index::At(nfd_upto(byte) + 1),
                gap: false,
            })
        } else {
            None
        };
        steps.push(Step::Word {
            word: Index::At(n as u32 + 1),
            char,
        });
        Some(steps)
    }
}

/// A footnote's or cross reference's own character markers.
fn note_content(marker: MarkerIdx) -> bool {
    matches!(
        generated::category(marker),
        Category::CharNotesFootnote | Category::CharNotesCrossReference
    )
}

/// `s` names `\s1`, `\s2` …; `s1` names `\s` too (USFM's `\s` is `\s1`).
fn marker_matches(name: &str, wanted: &str) -> bool {
    if name == wanted {
        return true;
    }
    let base = |n: &str| n.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
    if !wanted.ends_with(|c: char| c.is_ascii_digit()) {
        return base(name) == wanted;
    }
    wanted.strip_suffix('1') == Some(name)
}

/// The standard's `ws`, ASCII whitespace, and USFM's `~` (a no-break space).
fn is_space(c: char) -> bool {
    c.is_ascii_whitespace()
        || c == '~'
        || matches!(c,
            '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200B}' | '\u{200E}' | '\u{200F}'
            | '\u{202A}'..='\u{202F}' | '\u{205F}' | '\u{2066}'..='\u{2069}' | '\u{3000}')
}

#[cfg(test)]
mod tests;
