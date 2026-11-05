//! The best-effort rules: a paragraph in no style of ours, read the way
//! `@sefer/document-import` reads a draft written any old way.
//!
//! ```text
//! "Chapter 3"                       \c 3
//! ¹Paul, a servant…  (superscript)  \p ⏎ \v 1 Paul, a servant…
//! an indented or all-italic block   \q1 / \q2 by indent depth
//! a short bold line, no verse       \s1
//! text before the first verse       \rem
//! *italic* and **bold** spans       \it …\it*, \bd …\bd*
//! a footnote                        \f + \fr 3:16 \ft its text\f*
//! ```
//!
//! Every paragraph stays a paragraph, in order. What each one becomes is a
//! [`Guess`] in the report, with the reason, and each kind of guess has a
//! setting that turns it off. Nothing is merged, reordered or repaired.
//!
//! One pass serves a whole draft and the guessed paragraphs of a mixed one:
//! `scan` and `emit` take one paragraph at a time and share the chapter and
//! verse with the exact path, so a heading pasted between two of our
//! paragraphs knows which verse it follows.

use std::collections::BTreeMap;

use crate::import::Out;
use crate::reading::{Align, Inline, Paragraph, Role};
use crate::verses::{self, Counts, Kind, Step, VerseMode};
use crate::{Guess, Keep, Settings, Skip, SkipReason};

/// Paragraph markers a style name may carry straight through (Paratext
/// exports, house templates): "q2", "s1 - Heading".
const STYLE_MARKERS: &[&str] = &[
    "p", "m", "pi", "pi1", "pi2", "mi", "nb", "pc", "pm", "pmo", "pmc", "pmr", "cls", "q", "q1", "q2", "q3",
    "q4", "qc", "qr", "qm", "qm1", "qm2", "qa", "qd", "b", "s", "s1", "s2", "s3", "s4", "ms", "ms1", "ms2",
    "mr", "sr", "r", "d", "sp", "cl", "mt", "mt1", "mt2", "mt3", "mt4", "imt", "is", "ip", "io1", "li", "li1",
    "li2", "li3", "lh", "lf",
];

/// `^\\?([a-z]+\d?)(?:$|\s|\s?-|_)` over the lowercased, trimmed style name.
pub fn style_marker(style: Option<&str>) -> Option<String> {
    let s = style?.trim().to_lowercase();
    let s = s.strip_prefix('\\').unwrap_or(&s);
    let letters = s.bytes().take_while(u8::is_ascii_lowercase).count();
    if letters == 0 {
        return None;
    }
    let mut end = letters;
    let ends_ok = |at: usize| {
        let rest = &s[at..];
        rest.is_empty()
            || rest.starts_with(char::is_whitespace) && (true)
            || rest.starts_with('-')
            || rest.starts_with('_')
    };
    if s[end..].starts_with(|c: char| c.is_ascii_digit()) && ends_ok(end + 1) {
        end += 1;
    } else if !ends_ok(end) {
        return None;
    }
    let m = &s[..end];
    STYLE_MARKERS.contains(&m).then(|| m.to_owned())
}

/// Text formatting carried into a piece.
#[derive(Debug, Clone)]
pub enum Piece {
    Text { text: String, italic: bool, bold: bool },
    Verse { chapter: u32, sid: String },
    Chapter(u32),
    Line,
    Note(String),
    CommentStart(String),
    CommentEnd(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Front,
    Heading,
    Chapter,
    Para,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    pub marker: String,
    /// The `\q` level when the block is poetry; each line break opens another line at it.
    pub poetry: u32,
    /// A chapter this block opens, written before its marker.
    pub chapter: Option<u32>,
    pub pieces: Vec<Piece>,
}

/// Whitespace-only runs take the formatting before them, so "¹" + " " reads
/// as one superscript "1 "; then equal neighbours merge.
pub fn tidy(p: &Paragraph) -> Paragraph {
    let mut out: Vec<Inline> = Vec::new();
    for i in &p.inlines {
        if let (Inline::Text { text, format, .. }, Some(Inline::Text { text: last, format: lf, .. })) = (i, out.last_mut())
            && (format == lf || text.trim().is_empty())
        {
            last.push_str(text);
            continue;
        }
        out.push(i.clone());
    }
    Paragraph { inlines: out, ..p.clone() }
}

/// What the whole document says, worked out before any paragraph is read.
pub struct Guesser<'s> {
    settings: &'s Settings,
    pub mode: VerseMode,
    pub detected: VerseMode,
    pub counts: Counts,
    levels: Vec<(f64, u32)>,
    labelled: bool,
}

impl<'s> Guesser<'s> {
    /// `paragraphs` are the ones this pass will read, already tidied.
    /// `our_verses` counts the verse numbers in our own `USFM v` style
    /// elsewhere in the document: they are set in superscript, so they say
    /// how this document writes its verse numbers.
    pub fn new(paragraphs: &[&Paragraph], our_verses: usize, settings: &'s Settings) -> Self {
        let mut counts = count_candidates(paragraphs);
        counts.superscript += our_verses;
        let detected = verses::detect(&counts);
        let mode = settings.verses.unwrap_or(detected);
        let labelled = paragraphs.iter().any(|p| chapter_label(&settings.chapter_words, &p.plain()).is_some());
        Self { settings, mode, detected, counts, levels: indent_levels(paragraphs), labelled }
    }

    fn level(&self, indent: f64) -> u32 {
        self.levels.iter().find(|(i, _)| *i == indent).map_or(0, |(_, l)| *l)
    }

    /// One paragraph into blocks: a chapter it opens, then itself.
    pub fn scan(&self, p: &Paragraph, out: &mut Out<'_>) -> Vec<Block> {
        let s = self.settings;
        let mut blocks = Vec::new();
        let text = p.plain();
        if text.trim().is_empty() && !p.has_note() {
            return blocks;
        }
        let excerpt = excerpt(&text);
        let mut guess = |out: &mut Out<'_>, marker: &str, reason: String| {
            let r = out.verse_ref();
            out.report.guesses.push(Guess { r#ref: r, marker: marker.to_owned(), reason, excerpt: excerpt.clone() });
        };

        // "Chapter 3", "Chapter", or a paragraph that is only the next
        // chapter's number. A paragraph carrying a footnote is never a label:
        // the label is dropped, the note must not be.
        let noted = p.has_note();
        let label = if noted { None } else { chapter_label(&s.chapter_words, &text) };
        let bare = if noted { None } else { bare_number(&text) };
        let bare_next = bare.is_some_and(|n| {
            self.mode != VerseMode::PerLine && n == out.chapter + 1 && (out.verse > 0 || out.chapter == 0)
        });
        if label.is_some() || bare_next {
            let n = match label {
                Some(Some(n)) => n,
                Some(None) => out.chapter + 1,
                None => bare.unwrap_or(out.chapter + 1),
            };
            match label {
                None => guess(out, "c", "a paragraph holding only the next chapter's number".to_owned()),
                Some(None) => guess(
                    out,
                    "c",
                    format!("\"{}\" without a number, counted on from the last chapter", text.trim()),
                ),
                Some(Some(_)) => {}
            }
            if n != out.chapter {
                out.chapter = n;
                out.verse = 0;
                blocks.push(Block { kind: BlockKind::Chapter, marker: "c".to_owned(), poetry: 0, chapter: Some(n), pieces: Vec::new() });
            }
            return blocks;
        }

        let mut pieces: Vec<Piece> = Vec::new();
        let mut opens: Option<u32> = None;
        let start_verse = |out: &mut Out<'_>, pieces: &mut Vec<Piece>, opens: &mut Option<u32>, from: u32, to: u32, restart: bool, why: &str, guess: &mut dyn FnMut(&mut Out<'_>, &str, String)| {
            if restart || out.chapter == 0 {
                out.chapter = if restart { out.chapter + 1 } else { 1 };
                if pieces.iter().any(|x| !matches!(x, Piece::CommentStart(_))) {
                    pieces.push(Piece::Chapter(out.chapter));
                } else {
                    *opens = Some(out.chapter);
                }
                if restart {
                    guess(out, "c", why.to_owned());
                }
            }
            out.verse = to;
            out.record(from, to);
            let sid = if from == to { from.to_string() } else { format!("{from}-{to}") };
            pieces.push(Piece::Verse { chapter: out.chapter, sid });
        };
        const RESTART: &str = "verse 1 again after a later verse, with no chapter label";

        if self.mode == VerseMode::PerLine {
            let heading = matches!(p.role, Some(Role::Heading | Role::Title));
            if !heading && !(self.labelled && out.chapter == 0) {
                let max = out.max_verse(out.chapter.max(1));
                let full = !self.labelled && out.chapter > 0 && max.is_some_and(|m| out.verse >= m);
                let n = if full { 1 } else { out.verse + 1 };
                start_verse(out, &mut pieces, &mut opens, n, n, full, "the versification's verse count was reached", &mut guess);
            }
        }

        let mut at_line_start = true;
        for inline in &p.inlines {
            let (itext, format) = match inline {
                Inline::Break => {
                    pieces.push(Piece::Line);
                    at_line_start = true;
                    continue;
                }
                Inline::Note(id) => {
                    pieces.push(Piece::Note(id.clone()));
                    continue;
                }
                Inline::CommentStart(id) => {
                    pieces.push(Piece::CommentStart(id.clone()));
                    continue;
                }
                Inline::CommentEnd(id) => {
                    pieces.push(Piece::CommentEnd(id.clone()));
                    continue;
                }
                Inline::Text { text, format, .. } => (text, format),
            };
            let mut cursor = 0;
            let push = |pieces: &mut Vec<Piece>, cursor: usize, to: usize| {
                if to > cursor {
                    pieces.push(Piece::Text { text: itext[cursor..to].to_owned(), italic: format.italic, bold: format.bold });
                }
            };
            for c in verses::candidates(itext, format.superscript, at_line_start) {
                if !verses::accepts(self.mode, c.kind) {
                    continue;
                }
                let written = itext[c.start..c.end].trim().to_owned();
                let r = out.verse_ref();
                if c.thousands {
                    out.report.skipped.push(Skip { r#ref: r, number: written, reason: SkipReason::Thousands });
                    continue;
                }
                let max = out.max_verse(out.chapter.max(1));
                let step = verses::next_verse(out.verse, c.from, c.to, verses::gap_for(c.kind), c.kind != Kind::Inline, max);
                let Some(step) = step else {
                    let over = max.is_some_and(|m| c.to > m) && c.from > out.verse;
                    if c.kind != Kind::Inline {
                        let reason = if over { SkipReason::Versification } else { SkipReason::Sequence };
                        out.report.skipped.push(Skip { r#ref: r, number: written, reason });
                    }
                    continue;
                };
                push(&mut pieces, cursor, c.start);
                start_verse(out, &mut pieces, &mut opens, c.from, c.to, step == Step::Restart, RESTART, &mut guess);
                cursor = c.end;
            }
            push(&mut pieces, cursor, itext.len());
            if !itext.trim().is_empty() {
                at_line_start = false;
            }
        }

        let has_verse = pieces.iter().any(|x| matches!(x, Piece::Verse { .. }));
        if has_verse {
            out.started = true;
        }
        let mut block = Block { kind: BlockKind::Para, marker: "p".to_owned(), poetry: 0, chapter: opens, pieces };
        if !out.started {
            block.kind = BlockKind::Front;
            block.marker = "rem".to_owned();
        } else if !has_verse && matches!(p.role, Some(Role::Heading | Role::Title)) {
            block.kind = BlockKind::Heading;
            block.marker = format!("s{}", p.level.clamp(1, 3));
        } else if !has_verse && looks_like_heading(p, &text) {
            block.kind = BlockKind::Heading;
            block.marker = "s1".to_owned();
            let why = if p.align == Some(Align::Center) { "short, centred, no verse number" } else { "short, bold, no verse number" };
            guess(out, "s1", why.to_owned());
        } else {
            self.paragraph_marker(p, &mut block, &mut |out, m, r| guess(out, m, r), out);
        }
        if block.kind == BlockKind::Para && block.poetry == 0 {
            for piece in &mut block.pieces {
                if matches!(piece, Piece::Line) {
                    *piece = Piece::Text { text: " ".to_owned(), italic: false, bold: false };
                }
            }
        }
        blocks.push(block);
        blocks
    }

    fn paragraph_marker(
        &self,
        p: &Paragraph,
        block: &mut Block,
        guess: &mut dyn FnMut(&mut Out<'_>, &str, String),
        out: &mut Out<'_>,
    ) {
        let s = self.settings;
        if s.style_markers == Keep::Keep
            && let Some(m) = style_marker(p.style.as_deref())
        {
            let marker = if matches!(m.as_str(), "q" | "li" | "mt" | "s") { format!("{m}1") } else { m };
            block.marker = marker.clone();
            if let Some(q) = marker.strip_prefix('q').and_then(|d| d.parse::<u32>().ok())
                && marker.len() == 2
            {
                block.poetry = q;
            }
            let heading = marker.len() == 2 && marker.starts_with('s') && marker.as_bytes()[1].is_ascii_digit();
            if heading && !block.pieces.iter().any(|x| matches!(x, Piece::Verse { .. })) {
                block.kind = BlockKind::Heading;
            }
            guess(out, &marker, format!("paragraph style \"{}\"", p.style.as_deref().unwrap_or("")));
            return;
        }
        if p.list && s.lists == Keep::Keep {
            block.marker = "li1".to_owned();
            guess(out, "li1", "a numbered or bulleted paragraph".to_owned());
            return;
        }
        let lines = line_lengths(&block.pieces);
        let level = self.level(p.indent);
        let italic = all_italic(&block.pieces);
        let reason = if level > 0 {
            Some(format!("indented {}pt", p.indent.round()))
        } else if p.role == Some(Role::Quote) {
            Some(format!("paragraph style \"{}\"", p.style.as_deref().unwrap_or("")))
        } else if italic {
            Some("the whole paragraph is italic".to_owned())
        } else if lines.len() >= 2 && lines.iter().all(|&n| n <= 60) {
            Some("short lines broken by hand".to_owned())
        } else {
            None
        };
        let Some(reason) = reason else { return };
        let q = level.max(1);
        if s.poetry == Keep::Unwrap {
            return;
        }
        block.marker = format!("q{q}");
        block.poetry = q;
        // The italics said "poetry"; they are not also emphasis.
        if italic {
            for piece in &mut block.pieces {
                if let Piece::Text { italic, .. } = piece {
                    *italic = false;
                }
            }
        }
        guess(out, &format!("q{q}"), reason);
    }

    /// Writes the blocks of one paragraph.
    pub fn emit(&self, blocks: &[Block], out: &mut Out<'_>) {
        let s = self.settings;
        let rename = |m: &str| s.markers.iter().find(|(from, _)| from == m).map_or_else(|| m.to_owned(), |(_, to)| to.clone());
        for block in blocks {
            if block.kind == BlockKind::Chapter {
                out.chapter_line(block.chapter.unwrap_or(0));
                continue;
            }
            if let Some(c) = block.chapter {
                out.chapter_line(c);
            }
            if matches!(block.kind, BlockKind::Front | BlockKind::Heading) {
                let mut text = String::new();
                for x in &block.pieces {
                    match x {
                        Piece::Text { text: t, .. } => text.push_str(t),
                        Piece::Line => text.push(' '),
                        Piece::Note(id) => text.push_str(&self.footnote(id, out)),
                        _ => {}
                    }
                }
                let text = collapse(&text).replace(" \\f +", "\\f +").trim().to_owned();
                let choice = if block.kind == BlockKind::Front { s.front.name() } else { s.headings.name() };
                if choice == "drop" || text.is_empty() {
                    continue;
                }
                let marker = if choice == "rem" {
                    "rem".to_owned()
                } else if block.kind == BlockKind::Front {
                    out.mt_count += 1;
                    if out.mt_count == 1 { "mt1".to_owned() } else { "mt2".to_owned() }
                } else {
                    block.marker.clone()
                };
                out.flush();
                let line = format!("\\{} {text}", rename(&marker));
                out.lines().push(line);
                continue;
            }

            out.flush();
            let open = format!("\\{}", rename(&block.marker));
            out.lines().push(open);
            out.anchors().text(" ");
            for piece in &block.pieces {
                match piece {
                    Piece::Text { text, italic, bold } => {
                        out.anchors().text(text);
                        let italic = *italic && s.italic == Keep::Keep;
                        let bold = *bold && s.bold == Keep::Keep;
                        if !italic && !bold {
                            out.line.push_str(text);
                            continue;
                        }
                        let core = text.trim();
                        if core.is_empty() {
                            out.line.push_str(text);
                            continue;
                        }
                        let lead = &text[..text.len() - text.trim_start().len()];
                        let trail = &text[text.trim_end().len()..];
                        let mut wrapped = core.to_owned();
                        if italic {
                            wrapped = format!("\\it {wrapped}\\it*");
                        }
                        if bold {
                            wrapped = format!("\\bd {wrapped}\\bd*");
                        }
                        out.line.push_str(lead);
                        out.line.push_str(&wrapped);
                        out.line.push_str(trail);
                    }
                    Piece::Verse { chapter, sid } => {
                        out.flush();
                        out.anchors().at(&chapter.to_string(), sid);
                        out.line = format!("\\v {sid} ");
                    }
                    Piece::Chapter(c) => {
                        out.chapter_line(*c);
                        let open = format!("\\{}", rename(&block.marker));
                        out.lines().push(open);
                    }
                    Piece::Line => {
                        out.flush();
                        let open = format!("\\{}", rename(&format!("q{}", block.poetry)));
                        out.lines().push(open);
                        out.anchors().text(" ");
                    }
                    Piece::Note(id) => {
                        let marker = self.footnote(id, out);
                        if !marker.is_empty() {
                            let kept = out.line.trim_end().len();
                            out.line.truncate(kept);
                            out.line.push_str(&marker);
                        }
                    }
                    Piece::CommentStart(id) => out.anchors().start(id),
                    Piece::CommentEnd(id) => out.anchors().end(id),
                }
            }
            out.flush();
        }
    }

    fn footnote(&self, id: &str, out: &mut Out<'_>) -> String {
        if self.settings.notes == crate::KeepDrop::Drop {
            return String::new();
        }
        let Some(note) = out.reading.notes.get(id) else {
            out.report.missing_notes += 1;
            return String::new();
        };
        let fr = match out.anchors_ref() {
            Some(r) => {
                let r = match r.rsplit_once('-') {
                    Some((head, tail)) if tail.bytes().all(|b| b.is_ascii_digit()) && !tail.is_empty() => head.to_owned(),
                    _ => r,
                };
                format!("\\fr {r} ")
            }
            None => String::new(),
        };
        format!("\\f + {fr}\\ft {}\\f*", note.text())
    }
}

/// `^\s*(?:words)\.?\s*(\d{1,3})?\s*[.:]?\s*$`, case-insensitive: a chapter
/// label, with its number when it has one.
pub fn chapter_label(words: &[String], text: &str) -> Option<Option<u32>> {
    let t = text.trim_start();
    let lower = t.to_lowercase();
    for w in words.iter().filter(|w| !w.is_empty()) {
        let w = w.to_lowercase();
        let Some(rest) = lower.strip_prefix(&w) else { continue };
        let rest = rest.strip_prefix('.').unwrap_or(rest).trim_start();
        let n = rest.bytes().take_while(u8::is_ascii_digit).count();
        if n > 3 {
            continue;
        }
        let number = (n > 0).then(|| rest[..n].parse().unwrap_or(0));
        let tail = rest[n..].trim_start();
        let tail = tail.strip_prefix(['.', ':']).unwrap_or(tail);
        if tail.trim().is_empty() {
            return Some(number);
        }
    }
    None
}

/// `^\s*(\d{1,3})\s*$`
fn bare_number(text: &str) -> Option<u32> {
    let t = text.trim();
    (!t.is_empty() && t.len() <= 3 && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.parse().unwrap_or(0))
}

/// Left indents deeper than the body text's, ranked: the first step in is
/// `\q1`, the next `\q2`, then `\q3`.
fn indent_levels(paragraphs: &[&Paragraph]) -> Vec<(f64, u32)> {
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for p in paragraphs {
        if p.plain().trim().is_empty() {
            continue;
        }
        match counts.iter_mut().find(|(i, _)| *i == p.indent) {
            Some((_, n)) => *n += 1,
            None => counts.push((p.indent, 1)),
        }
    }
    // The most common indent is the body's; the first listed wins a tie.
    let mut body = 0.0;
    let mut best = 0;
    for &(i, n) in &counts {
        if n > best {
            best = n;
            body = i;
        }
    }
    let mut deeper: Vec<f64> = counts.iter().map(|(i, _)| *i).filter(|&i| i > body + 4.0).collect();
    deeper.sort_by(f64::total_cmp);
    deeper.into_iter().enumerate().map(|(n, i)| (i, (n as u32 + 1).min(3))).collect()
}

fn looks_like_heading(p: &Paragraph, text: &str) -> bool {
    let t = text.trim();
    if t.encode_utf16().count() > 80 || t.ends_with(['.', ',', ';', ':', '!', '?', '"', '”', '’']) {
        return false;
    }
    let texts: Vec<bool> = p
        .inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Text { text, format, .. } if !text.trim().is_empty() => Some(format.bold),
            _ => None,
        })
        .collect();
    let bold = !texts.is_empty() && texts.iter().all(|&b| b);
    bold || p.align == Some(Align::Center)
}

/// A document that already holds USFM (pasted into Word, say).
pub fn looks_like_usfm(paragraphs: &[&Paragraph]) -> bool {
    let mut markers = 0;
    let mut verses = false;
    for p in paragraphs {
        let text = p.plain();
        if starts_with_marker(&text) {
            markers += 1;
        }
        if has_verse_marker(&text) {
            verses = true;
        }
    }
    verses && markers >= 3
}

/// `^\s*\\(id|c|v|p|m|q\d?|s\d?|h|mt\d?|toc\d)(\s|$)`
fn starts_with_marker(text: &str) -> bool {
    let Some(rest) = text.trim_start().strip_prefix('\\') else { return false };
    let name_len = rest.bytes().take_while(u8::is_ascii_alphanumeric).count();
    let name = &rest[..name_len];
    let after_ok = rest[name_len..].is_empty() || rest[name_len..].starts_with(char::is_whitespace);
    let digit = |prefix: &str, required: bool| {
        name.strip_prefix(prefix).is_some_and(|d| (d.is_empty() && !required) || (d.len() == 1 && d.as_bytes()[0].is_ascii_digit()))
    };
    let known = matches!(name, "id" | "c" | "v" | "p" | "m" | "h")
        || digit("q", false)
        || digit("s", false)
        || digit("mt", false)
        || digit("toc", true);
    after_ok && known
}

/// `\\v\s+\d`
fn has_verse_marker(text: &str) -> bool {
    text.match_indices("\\v").any(|(i, _)| {
        let rest = &text[i + 2..];
        let ws = rest.len() - rest.trim_start().len();
        ws > 0 && rest[ws..].starts_with(|c: char| c.is_ascii_digit())
    })
}

fn line_lengths(pieces: &[Piece]) -> Vec<usize> {
    let mut lengths = vec![0];
    for piece in pieces {
        match piece {
            Piece::Line => lengths.push(0),
            Piece::Text { text, .. } => *lengths.last_mut().expect("one") += text.trim().encode_utf16().count(),
            _ => {}
        }
    }
    if lengths.len() > 1 { lengths.into_iter().filter(|&n| n > 0).collect() } else { Vec::new() }
}

fn all_italic(pieces: &[Piece]) -> bool {
    let texts: Vec<bool> = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Text { text, italic, .. } if !text.trim().is_empty() => Some(*italic),
            _ => None,
        })
        .collect();
    !texts.is_empty() && texts.iter().all(|&i| i)
}

fn count_candidates(paragraphs: &[&Paragraph]) -> Counts {
    let mut counts = Counts::default();
    for p in paragraphs {
        let mut at_line_start = true;
        for i in &p.inlines {
            match i {
                Inline::Break => at_line_start = true,
                Inline::Text { text, format, .. } => {
                    for c in verses::candidates(text, format.superscript, at_line_start) {
                        if !c.thousands {
                            counts.add(c.kind);
                        }
                    }
                    if !text.trim().is_empty() {
                        at_line_start = false;
                    }
                }
                _ => {}
            }
        }
    }
    counts
}

/// Every run of whitespace as one space.
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ") + if text.ends_with(char::is_whitespace) && !text.trim().is_empty() { " " } else { "" }
}

fn excerpt(text: &str) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.encode_utf16().count() > 60 {
        let cut: String = t.chars().take(57).collect();
        format!("{cut}…")
    } else {
        t
    }
}

/// How many paragraphs carry a style named after a marker.
pub fn count_style_markers(paragraphs: &[&Paragraph]) -> usize {
    paragraphs.iter().filter(|p| style_marker(p.style.as_deref()).is_some()).count()
}

/// The verses seen in each chapter, as document-import summarises them.
pub fn summarise(seen: &BTreeMap<u32, std::collections::BTreeSet<u32>>) -> Vec<crate::ChapterSummary> {
    seen.iter()
        .map(|(&chapter, verses)| {
            let last = verses.iter().next_back().copied().unwrap_or(0);
            let missing = (1..=last).filter(|n| !verses.contains(n)).collect();
            crate::ChapterSummary { chapter, verses: verses.len(), last, missing }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_that_carry_a_marker() {
        assert_eq!(style_marker(Some("q2")), Some("q2".to_owned()));
        assert_eq!(style_marker(Some("s1 - Heading")), Some("s1".to_owned()));
        assert_eq!(style_marker(Some("\\p")), Some("p".to_owned()));
        assert_eq!(style_marker(Some("Heading 1")), None);
        assert_eq!(style_marker(Some("pi_Indent")), Some("pi".to_owned()));
    }

    #[test]
    fn chapter_labels() {
        let words = vec!["chapter".to_owned()];
        assert_eq!(chapter_label(&words, "Chapter 3"), Some(Some(3)));
        assert_eq!(chapter_label(&words, " CHAPTER. "), Some(None));
        assert_eq!(chapter_label(&words, "Chapter 3:"), Some(Some(3)));
        assert_eq!(chapter_label(&words, "Chapters 3"), None);
        assert_eq!(chapter_label(&words, "Chapter 1000"), None);
    }

    #[test]
    fn usfm_in_a_paragraph() {
        assert!(starts_with_marker("\\q2 line"));
        assert!(starts_with_marker("\\toc1 Gen"));
        assert!(!starts_with_marker("\\toc Gen"));
        assert!(has_verse_marker("\\c 1 \\v  3 x"));
    }
}
