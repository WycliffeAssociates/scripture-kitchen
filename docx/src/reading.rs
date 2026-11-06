//! What a .docx or .odt says, before any reading of verses: both readers
//! produce this one shape, so the exact path (our styles) and the
//! best-effort path (everything else) are each written once.
//!
//! ```text
//! <w:p pStyle="Quote"><w:r><w:rPr><w:i/></w:rPr><w:t>Blessed</w:t></w:r>…
//!   ->  Paragraph { style: "Quote", role: Quote, indent: 36.0,
//!                   inlines: [Text { text: "Blessed", italic: true, .. }] }
//! ```
//!
//! A run keeps both its character style's NAME (the exact path reads it) and
//! its formatting resolved through the style chain (the best-effort path
//! reads that). `direct` is the formatting the run itself sets, so a word
//! bolded inside a `USFM p` paragraph can still become `\bd`.

use std::collections::BTreeMap;

use crate::comments::Comment;
use crate::dom::Element;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Docx,
    Odt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Formatting {
    pub superscript: bool,
    pub italic: bool,
    pub bold: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text {
        text: String,
        /// The character style's name, when the run names one.
        style: Option<String>,
        format: Formatting,
        direct: Formatting,
    },
    /// A line break inside the paragraph (Shift+Enter).
    Break,
    /// Where a note's mark sits; the body is in [`Reading::notes`].
    Note(String),
    CommentStart(String),
    CommentEnd(String),
}

/// What the paragraph style's name says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Title,
    Heading,
    Quote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Justify,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Paragraph {
    /// The paragraph style's display name: "Heading 1", "Quote", "USFM q1".
    pub style: Option<String>,
    pub role: Option<Role>,
    /// Heading level from the style or the outline level, 1 for the top.
    pub level: u32,
    /// Left indent in points, through the style chain.
    pub indent: f64,
    pub align: Option<Align>,
    pub list: bool,
    pub table: bool,
    pub inlines: Vec<Inline>,
}

impl Paragraph {
    /// The paragraph's text, a line break as a space.
    pub fn plain(&self) -> String {
        let mut out = String::new();
        for i in &self.inlines {
            match i {
                Inline::Text { text, .. } => out.push_str(text),
                Inline::Break => out.push(' '),
                _ => {}
            }
        }
        out
    }

    pub fn has_note(&self) -> bool {
        self.inlines.iter().any(|i| matches!(i, Inline::Note(_)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Footnote,
    Endnote,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub kind: NoteKind,
    pub paragraphs: Vec<Paragraph>,
}

impl Note {
    /// The note's words, whitespace collapsed.
    pub fn text(&self) -> String {
        let all: Vec<String> = self.paragraphs.iter().map(Paragraph::plain).collect();
        all.join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reading {
    pub format: Format,
    pub paragraphs: Vec<Paragraph>,
    pub notes: BTreeMap<String, Note>,
    pub comments: Vec<Comment>,
    /// Insertions, deletions and moves still tracked. The reading accepts them all.
    pub tracked_changes: usize,
    /// Every custom document property, as written.
    pub properties: Vec<(String, String)>,
}

/// Title, heading or quote, from a style's name and outline level.
pub fn role_of(name: Option<&str>, outline: Option<u32>) -> (Option<Role>, u32) {
    let n = name.unwrap_or("").trim().to_lowercase();
    let word = |w: &str| {
        n.starts_with(w) && !n[w.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    };
    if word("title") {
        return (Some(Role::Title), 1);
    }
    if word("subtitle") {
        return (Some(Role::Title), 2);
    }
    if let Some(rest) = n.strip_prefix("heading") {
        let digit = rest
            .trim_start()
            .chars()
            .next()
            .and_then(|c| c.to_digit(10));
        return (Some(Role::Heading), digit.unwrap_or(1));
    }
    let quote = n
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w.starts_with("quote") || w.starts_with("quotation"));
    if quote {
        return (Some(Role::Quote), 1);
    }
    match outline {
        Some(l) => (Some(Role::Heading), l),
        None => (None, 0),
    }
}

// --- Word ---------------------------------------------------------------------

/// Style properties, each `None` when the style does not set it.
#[derive(Debug, Clone, Default)]
struct StyleProps {
    name: Option<String>,
    based_on: Option<String>,
    indent: Option<f64>,
    align: Option<Align>,
    outline: Option<u32>,
    list: Option<bool>,
    italic: Option<bool>,
    bold: Option<bool>,
    superscript: Option<bool>,
}

impl StyleProps {
    /// `self` over `under`: nearest wins.
    fn over(&self, under: &StyleProps) -> StyleProps {
        StyleProps {
            name: self.name.clone(),
            based_on: self.based_on.clone(),
            indent: self.indent.or(under.indent),
            align: self.align.or(under.align),
            outline: self.outline.or(under.outline),
            list: self.list.or(under.list),
            italic: self.italic.or(under.italic),
            bold: self.bold.or(under.bold),
            superscript: self.superscript.or(under.superscript),
        }
    }
}

pub struct WordStyles {
    by_id: BTreeMap<String, StyleProps>,
    default_paragraph: Option<String>,
}

impl WordStyles {
    pub fn read(root: Option<&Element>) -> Self {
        let mut by_id = BTreeMap::new();
        let mut default_paragraph = None;
        for style in root
            .iter()
            .flat_map(|r| r.elements())
            .filter(|e| e.name == "w:style")
        {
            let Some(id) = style.attr("w:styleId") else {
                continue;
            };
            if style.attr("w:type") == Some("paragraph") && style.attr("w:default") == Some("1") {
                default_paragraph = Some(id.to_owned());
            }
            let mut props = StyleProps {
                name: Some(
                    style
                        .child("w:name")
                        .and_then(|n| n.attr("w:val"))
                        .unwrap_or(id)
                        .to_owned(),
                ),
                based_on: style
                    .child("w:basedOn")
                    .and_then(|b| b.attr("w:val"))
                    .map(str::to_owned),
                ..StyleProps::default()
            };
            if let Some(ppr) = style.child("w:pPr") {
                paragraph_props(ppr, &mut props);
            }
            if let Some(rpr) = style.child("w:rPr") {
                run_props(rpr, &mut props);
            }
            by_id.insert(id.to_owned(), props);
        }
        Self {
            by_id,
            default_paragraph,
        }
    }

    fn resolve(&self, id: Option<&str>) -> StyleProps {
        let mut chain = Vec::new();
        let mut at = id.map(str::to_owned);
        while let Some(id) = at.take() {
            let Some(own) = self.by_id.get(&id) else {
                break;
            };
            if chain.len() > 20 {
                break;
            }
            chain.push(own);
            at = own.based_on.clone();
        }
        let mut merged = StyleProps::default();
        for own in chain.iter().rev() {
            merged = own.over(&merged);
        }
        merged
    }
}

fn paragraph_props(ppr: &Element, props: &mut StyleProps) {
    if let Some(ind) = ppr.child("w:ind")
        && let Some(left) = ind.attr("w:left").or_else(|| ind.attr("w:start"))
        && let Ok(twips) = left.parse::<f64>()
    {
        props.indent = Some(twips / 20.0);
    }
    if let Some(jc) = ppr.child("w:jc").and_then(|j| j.attr("w:val")) {
        props.align = Some(match jc {
            "center" => Align::Center,
            "right" | "end" => Align::End,
            "both" | "distribute" => Align::Justify,
            _ => Align::Start,
        });
    }
    if let Some(level) = ppr
        .child("w:outlineLvl")
        .and_then(|o| o.attr("w:val"))
        .and_then(|v| v.parse::<u32>().ok())
        && level < 9
    {
        props.outline = Some(level);
    }
    if let Some(num) = ppr
        .child("w:numPr")
        .and_then(|n| n.child("w:numId"))
        .and_then(|n| n.attr("w:val"))
    {
        props.list = Some(num != "0");
    }
}

fn run_props(rpr: &Element, props: &mut StyleProps) {
    if let Some(i) = rpr.child("w:i") {
        props.italic = Some(toggle(i));
    }
    if let Some(b) = rpr.child("w:b") {
        props.bold = Some(toggle(b));
    }
    if let Some(v) = rpr.child("w:vertAlign").and_then(|v| v.attr("w:val")) {
        props.superscript = Some(v == "superscript");
    }
    // A raised baseline (half-points) is how some exports fake superscript.
    if props.superscript.is_none()
        && let Some(p) = rpr
            .child("w:position")
            .and_then(|p| p.attr("w:val"))
            .and_then(|v| v.parse::<f64>().ok())
    {
        props.superscript = Some(p > 0.0);
    }
}

/// `<w:i/>` is on; `<w:i w:val="0"/>` (or false, off) is off.
fn toggle(e: &Element) -> bool {
    !matches!(e.attr("w:val"), Some("0" | "false" | "off"))
}

pub struct WordReader<'a> {
    pub styles: &'a WordStyles,
    pub tracked_changes: usize,
}

impl WordReader<'_> {
    pub fn blocks(&mut self, parent: &Element, table: bool, out: &mut Vec<Paragraph>) {
        // Comment starts between paragraphs belong to the next one.
        let mut pending: Vec<Inline> = Vec::new();
        for e in parent.elements() {
            match e.name.as_str() {
                "w:p" => {
                    let mut p = self.paragraph(e, table);
                    if !pending.is_empty() {
                        pending.append(&mut p.inlines);
                        p.inlines = std::mem::take(&mut pending);
                    }
                    out.push(p);
                }
                "w:tbl" => self.blocks(e, true, out),
                "w:del" | "w:moveFrom" => self.tracked_changes += 1,
                "w:commentRangeStart" => {
                    if let Some(id) = e.attr("w:id") {
                        pending.push(Inline::CommentStart(id.to_owned()));
                    }
                }
                "w:commentRangeEnd" => {
                    if let Some(id) = e.attr("w:id") {
                        match out.last_mut() {
                            Some(p) => p.inlines.push(Inline::CommentEnd(id.to_owned())),
                            None => pending.push(Inline::CommentEnd(id.to_owned())),
                        }
                    }
                }
                "w:sectPr" | "w:tblPr" | "w:tblGrid" | "w:trPr" | "w:tcPr" | "w:sdtPr" => {}
                name => {
                    if name == "w:ins" || name == "w:moveTo" {
                        self.tracked_changes += 1;
                    }
                    self.blocks(e, table, out);
                }
            }
        }
        if let Some(last) = out.last_mut() {
            last.inlines.append(&mut pending);
        }
    }

    fn paragraph(&mut self, p: &Element, table: bool) -> Paragraph {
        let ppr = p.child("w:pPr");
        let id = ppr
            .and_then(|ppr| ppr.child("w:pStyle"))
            .and_then(|s| s.attr("w:val"));
        let style = self
            .styles
            .resolve(id.or(self.styles.default_paragraph.as_deref()));
        let mut direct = StyleProps::default();
        if let Some(ppr) = ppr {
            paragraph_props(ppr, &mut direct);
        }
        let outline = direct.outline.or(style.outline);
        let (role, level) = role_of(style.name.as_deref(), outline.map(|o| o + 1));
        let mut inlines = Vec::new();
        self.inline(p, &style, &mut inlines);
        Paragraph {
            style: style.name.clone(),
            role,
            level,
            indent: direct.indent.or(style.indent).unwrap_or(0.0),
            align: direct.align.or(style.align),
            list: direct.list.or(style.list).unwrap_or(false),
            table,
            inlines,
        }
    }

    fn inline(&mut self, parent: &Element, style: &StyleProps, out: &mut Vec<Inline>) {
        let mut drop_mark = false;
        for e in parent.elements() {
            match e.name.as_str() {
                "w:r" => self.run(e, style, out, &mut drop_mark),
                "w:commentRangeStart" => {
                    if let Some(id) = e.attr("w:id") {
                        out.push(Inline::CommentStart(id.to_owned()));
                    }
                }
                "w:commentRangeEnd" => {
                    if let Some(id) = e.attr("w:id") {
                        out.push(Inline::CommentEnd(id.to_owned()));
                    }
                }
                "w:del" | "w:moveFrom" => self.tracked_changes += 1,
                "w:pPr"
                | "w:proofErr"
                | "w:bookmarkStart"
                | "w:bookmarkEnd"
                | "mc:AlternateContent" => {}
                name => {
                    if name == "w:ins" || name == "w:moveTo" {
                        self.tracked_changes += 1;
                    }
                    self.inline(e, style, out);
                }
            }
        }
    }

    fn run(
        &mut self,
        r: &Element,
        paragraph: &StyleProps,
        out: &mut Vec<Inline>,
        drop_mark: &mut bool,
    ) {
        let rpr = r.child("w:rPr");
        if rpr
            .and_then(|rpr| rpr.child("w:vanish"))
            .is_some_and(toggle)
        {
            return;
        }
        let char_id = rpr
            .and_then(|rpr| rpr.child("w:rStyle"))
            .and_then(|s| s.attr("w:val"));
        let char_style = char_id
            .map(|id| self.styles.resolve(Some(id)))
            .unwrap_or_default();
        let mut own = StyleProps::default();
        if let Some(rpr) = rpr {
            run_props(rpr, &mut own);
        }
        let format = Formatting {
            superscript: own
                .superscript
                .or(char_style.superscript)
                .or(paragraph.superscript)
                .unwrap_or(false),
            italic: own
                .italic
                .or(char_style.italic)
                .or(paragraph.italic)
                .unwrap_or(false),
            bold: own
                .bold
                .or(char_style.bold)
                .or(paragraph.bold)
                .unwrap_or(false),
        };
        let direct = Formatting {
            superscript: own.superscript.unwrap_or(false),
            italic: own.italic.unwrap_or(false),
            bold: own.bold.unwrap_or(false),
        };
        let style = char_id.map(|id| char_style.name.clone().unwrap_or_else(|| id.to_owned()));
        let text = |out: &mut Vec<Inline>, drop_mark: &mut bool, value: &str| {
            if *drop_mark {
                *drop_mark = false;
                return;
            }
            push_text(out, value, style.clone(), format, direct);
        };
        for e in r.elements() {
            match e.name.as_str() {
                "w:t" => text(out, drop_mark, &e.text()),
                "w:tab" => text(out, drop_mark, "\t"),
                "w:noBreakHyphen" => text(out, drop_mark, "\u{2011}"),
                "w:br" => match e.attr("w:type") {
                    Some("page" | "column") => text(out, drop_mark, " "),
                    _ => out.push(Inline::Break),
                },
                "w:cr" => out.push(Inline::Break),
                "w:footnoteReference" | "w:endnoteReference" => {
                    let prefix = if e.name == "w:footnoteReference" {
                        "fn"
                    } else {
                        "en"
                    };
                    if let Some(id) = e.attr("w:id") {
                        out.push(Inline::Note(format!("{prefix}:{id}")));
                    }
                    // A custom mark is the next run's text, not the paragraph's.
                    *drop_mark = matches!(e.attr("w:customMarkFollows"), Some("1" | "true" | "on"));
                }
                _ => {}
            }
        }
    }
}

/// Appends text, joining the run before when it has the same style and look.
pub fn push_text(
    out: &mut Vec<Inline>,
    value: &str,
    style: Option<String>,
    format: Formatting,
    direct: Formatting,
) {
    if value.is_empty() {
        return;
    }
    if let Some(Inline::Text {
        text,
        style: s,
        format: f,
        direct: d,
    }) = out.last_mut()
        && *s == style
        && *f == format
        && *d == direct
    {
        text.push_str(value);
        return;
    }
    out.push(Inline::Text {
        text: value.to_owned(),
        style,
        format,
        direct,
    });
}

pub fn word_notes(
    reader: &mut WordReader<'_>,
    root: Option<&Element>,
    kind: NoteKind,
    notes: &mut BTreeMap<String, Note>,
) {
    let (element, prefix) = match kind {
        NoteKind::Footnote => ("w:footnote", "fn"),
        NoteKind::Endnote => ("w:endnote", "en"),
    };
    for n in root
        .iter()
        .flat_map(|r| r.elements())
        .filter(|e| e.name == element)
    {
        if n.attr("w:type").is_some_and(|t| t != "normal") {
            continue; // separators and continuation notices
        }
        let Some(id) = n.attr("w:id") else { continue };
        let mut paragraphs = Vec::new();
        reader.blocks(n, false, &mut paragraphs);
        notes.insert(format!("{prefix}:{id}"), Note { kind, paragraphs });
    }
}
