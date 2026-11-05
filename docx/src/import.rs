//! .docx to USFM, reading the named styles as markers.
//!
//! ```text
//! <w:p "USFM q1"><w:r "USFM v">3</w:r><w:r>He restores</w:r></w:p>
//!     ->  \q1 \v 3 He restores
//! <w:r "USFM wj">I am </w:r><w:r "USFM wj+nd">Lord</w:r>
//!     ->  \wj I am \+nd Lord\+nd*\wj*
//! <w:p "Normal">pasted text</w:p>    ->  \p pasted text      reported
//! <w:r "Emphasis">word</w:r>         ->  word                reported
//! <w:p "USFM id">GEN …</w:p>         ->  a new book starts here
//! ```
//!
//! Style NAMES are read, never ids. Pending tracked changes are taken as
//! accepted: inserted text is kept, deleted text is not, and each is counted.

use std::collections::BTreeMap;

use crate::comments::Anchors;
use crate::dom::{self, Element};
use crate::styles::{CALLER, LOOSE, NEST, NOTE_PEERS, PREFIX, RAW};
use crate::zip;
use crate::{ImportError, ImportReport, Imported, ImportedBook};

const OFFICE_DOCUMENT: &str = "/officeDocument";

pub fn import(docx: &[u8]) -> Result<Imported, ImportError> {
    let entries = zip::read(docx).map_err(ImportError::Zip)?;
    let package = Package { entries };

    let main = package
        .rels("_rels/.rels", "")?
        .into_iter()
        .find(|(t, _)| t.ends_with(OFFICE_DOCUMENT))
        .map_or_else(|| "word/document.xml".to_owned(), |(_, target)| target);
    let (dir, file) = main.rsplit_once('/').unwrap_or(("", main.as_str()));
    let rels = package.rels(&format!("{dir}/_rels/{file}.rels"), dir)?;
    let target = |kind: &str| rels.iter().find(|(t, _)| t.ends_with(kind)).map(|(_, p)| p.clone());

    let styles = match target("/styles") {
        Some(p) => style_names(&package.xml(&p)?.ok_or(ImportError::Missing(p))?),
        None => BTreeMap::new(),
    };
    let mut notes = BTreeMap::new();
    for (kind, element) in [("/footnotes", "w:footnote"), ("/endnotes", "w:endnote")] {
        if let Some(p) = target(kind)
            && let Some(root) = package.xml(&p)?
        {
            for n in root.elements().filter(|e| e.name == element) {
                if let Some(id) = n.attr("w:id") {
                    notes.insert((element == "w:endnote", id.to_owned()), n.clone());
                }
            }
        }
    }
    let props = match package.xml("docProps/custom.xml")? {
        Some(root) => crate::props::read(&root),
        None => Vec::new(),
    };
    let document = package.xml(&main)?.ok_or_else(|| ImportError::Missing(main.clone()))?;
    let body = document.child("w:body").ok_or_else(|| ImportError::Missing(format!("{main}: w:body")))?;

    let mut reader = Reader { styles: &styles, report: ImportReport::default(), pending: Vec::new() };
    let mut paragraphs = Vec::new();
    reader.blocks(body, &mut paragraphs);

    let mut printer = Printer { reader, notes: &notes, books: Vec::new() };
    for p in &paragraphs {
        printer.paragraph(p);
    }
    let Printer { reader, books, .. } = printer;
    let comments_root = match target("/comments") {
        Some(p) => package.xml(&p)?,
        None => None,
    };
    let extended_root = match target("/commentsExtended") {
        Some(p) => package.xml(&p)?,
        None => None,
    };
    let comments = crate::comments::read(comments_root.as_ref(), extended_root.as_ref());
    let anchored = |id: &str| books.iter().any(|b| b.anchors.has(id));
    let unanchored: Vec<String> = comments.iter().map(|c| c.id.clone()).filter(|id| !anchored(id)).collect();
    let books = books
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let source = b.code.as_deref().and_then(|c| crate::props::source(&props, c));
            let mut usfm = b.lines.join("\n");
            usfm.push('\n');
            // A thread anchored nowhere goes with the first book.
            let here = b.anchors.ids();
            let take = |root: &str| here.iter().any(|h| h == root) || (i == 0 && unanchored.iter().any(|u| u == root));
            let comments = b.anchors.threads(&comments, take);
            ImportedBook { code: b.code, usfm, source, comments }
        })
        .collect();
    Ok(Imported { books, report: reader.report, properties: props })
}

struct Package {
    entries: Vec<zip::Entry>,
}

impl Package {
    fn get(&self, name: &str) -> Option<&[u8]> {
        let name = name.trim_start_matches('/');
        self.entries.iter().find(|e| e.name == name).map(|e| e.data.as_slice())
    }

    fn xml(&self, name: &str) -> Result<Option<Element>, ImportError> {
        self.get(name)
            .map(|data| dom::parse(name, data).map_err(ImportError::Xml))
            .transpose()
    }

    /// A relationships part, as (type, target resolved against `dir`).
    fn rels(&self, name: &str, dir: &str) -> Result<Vec<(String, String)>, ImportError> {
        let Some(root) = self.xml(name)? else { return Ok(Vec::new()) };
        Ok(root
            .elements()
            .filter(|e| e.attr("TargetMode") != Some("External"))
            .filter_map(|e| {
                let target = e.attr("Target")?;
                let path = match target.strip_prefix('/') {
                    Some(abs) => abs.to_owned(),
                    None if dir.is_empty() => target.to_owned(),
                    None => format!("{dir}/{target}"),
                };
                Some((e.attr("Type")?.to_owned(), path))
            })
            .collect())
    }
}

/// Style id to style name, for every style in styles.xml.
fn style_names(root: &Element) -> BTreeMap<String, String> {
    root.elements()
        .filter(|e| e.name == "w:style")
        .filter_map(|s| {
            let id = s.attr("w:styleId")?.to_owned();
            let name = s.child("w:name").and_then(|n| n.attr("w:val")).unwrap_or(&id).to_owned();
            Some((id, name))
        })
        .collect()
}

/// One paragraph as read: its style name and what is in it.
struct Para {
    style: Option<String>,
    items: Vec<Item>,
}

#[derive(Debug)]
enum Item {
    Text { style: Option<String>, text: String },
    Note { endnote: bool, id: String, custom_mark: bool },
    CommentStart(String),
    CommentEnd(String),
}

struct Reader<'a> {
    styles: &'a BTreeMap<String, String>,
    report: ImportReport,
    /// Comment starts between paragraphs, for the next paragraph.
    pending: Vec<Item>,
}

impl Reader<'_> {
    fn name(&self, id: &str) -> String {
        self.styles.get(id).cloned().unwrap_or_else(|| id.to_owned())
    }

    fn blocks(&mut self, parent: &Element, out: &mut Vec<Para>) {
        for e in parent.elements() {
            match e.name.as_str() {
                "w:p" => {
                    let mut p = self.paragraph(e);
                    if !self.pending.is_empty() {
                        let mut items = std::mem::take(&mut self.pending);
                        items.append(&mut p.items);
                        p.items = items;
                    }
                    out.push(p);
                }
                "w:commentRangeStart" => {
                    if let Some(id) = e.attr("w:id") {
                        self.pending.push(Item::CommentStart(id.to_owned()));
                    }
                }
                "w:commentRangeEnd" => {
                    if let Some(id) = e.attr("w:id") {
                        match out.last_mut() {
                            Some(p) => p.items.push(Item::CommentEnd(id.to_owned())),
                            None => self.pending.push(Item::CommentEnd(id.to_owned())),
                        }
                    }
                }
                "w:tbl" | "w:tr" | "w:tc" | "w:sdt" | "w:sdtContent" | "w:customXml" | "w:ins"
                | "w:moveTo" => self.blocks(e, out),
                _ => {}
            }
        }
    }

    fn paragraph(&mut self, p: &Element) -> Para {
        let style = p
            .child("w:pPr")
            .and_then(|ppr| ppr.child("w:pStyle"))
            .and_then(|s| s.attr("w:val"))
            .map(|id| self.name(id));
        let mut items = Vec::new();
        self.inline(p, &mut items);
        Para { style, items: merge(items) }
    }

    fn inline(&mut self, parent: &Element, items: &mut Vec<Item>) {
        for e in parent.elements() {
            match e.name.as_str() {
                "w:r" => self.run(e, items),
                "w:del" | "w:moveFrom" => self.report.tracked_changes += 1,
                "w:ins" | "w:moveTo" => {
                    self.report.tracked_changes += 1;
                    self.inline(e, items);
                }
                "w:hyperlink" | "w:smartTag" | "w:sdt" | "w:sdtContent" | "w:customXml"
                | "w:fldSimple" | "w:dir" | "w:bdo" => self.inline(e, items),
                "w:commentRangeStart" => {
                    if let Some(id) = e.attr("w:id") {
                        items.push(Item::CommentStart(id.to_owned()));
                    }
                }
                "w:commentRangeEnd" => {
                    if let Some(id) = e.attr("w:id") {
                        items.push(Item::CommentEnd(id.to_owned()));
                    }
                }
                _ => {}
            }
        }
    }

    fn run(&mut self, r: &Element, items: &mut Vec<Item>) {
        let style = r
            .child("w:rPr")
            .and_then(|rpr| rpr.child("w:rStyle"))
            .and_then(|s| s.attr("w:val"))
            .map(|id| self.name(id));
        let mut text = String::new();
        let flush = |text: &mut String, items: &mut Vec<Item>| {
            if !text.is_empty() {
                items.push(Item::Text { style: style.clone(), text: std::mem::take(text) });
            }
        };
        for e in r.elements() {
            match e.name.as_str() {
                "w:t" => text.push_str(&e.text()),
                "w:tab" | "w:br" | "w:cr" => text.push(' '),
                "w:noBreakHyphen" => text.push('\u{2011}'),
                "w:softHyphen" => text.push('\u{AD}'),
                "w:footnoteReference" | "w:endnoteReference" => {
                    flush(&mut text, items);
                    if let Some(id) = e.attr("w:id") {
                        items.push(Item::Note {
                            endnote: e.name == "w:endnoteReference",
                            id: id.to_owned(),
                            custom_mark: matches!(e.attr("w:customMarkFollows"), Some("1" | "true" | "on")),
                        });
                    }
                }
                _ => {}
            }
        }
        flush(&mut text, items);
    }
}

/// Joins neighbouring text with the same style: Word splits runs freely.
fn merge(items: Vec<Item>) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    let mut drop_mark = false;
    for item in items {
        match item {
            Item::Text { .. } if drop_mark => drop_mark = false,
            Item::Text { style, text } => match out.last_mut() {
                Some(Item::Text { style: s, text: t }) if *s == style => t.push_str(&text),
                _ => out.push(Item::Text { style, text }),
            },
            Item::Note { custom_mark, .. } => {
                drop_mark = custom_mark;
                out.push(item);
            }
            Item::CommentStart(_) | Item::CommentEnd(_) => out.push(item),
        }
    }
    out
}

fn marker(style: Option<&str>) -> Option<&str> {
    style?.strip_prefix(PREFIX).filter(|m| !m.is_empty())
}

/// Character styles that say nothing about the text: Word's and LibreOffice's
/// own note-mark styles and the default font.
fn benign(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("footnote") || n.contains("endnote") || n == "default paragraph font"
}

struct Book {
    code: Option<String>,
    lines: Vec<String>,
    anchors: Anchors,
}

struct Printer<'a> {
    reader: Reader<'a>,
    notes: &'a BTreeMap<(bool, String), Element>,
    books: Vec<Book>,
}

impl Printer<'_> {
    fn lines(&mut self) -> &mut Vec<String> {
        if self.books.is_empty() {
            self.books.push(Book { code: None, lines: Vec::new(), anchors: Anchors::default() });
        }
        &mut self.books.last_mut().expect("a book").lines
    }

    fn anchors(&mut self) -> &mut Anchors {
        self.lines();
        &mut self.books.last_mut().expect("a book").anchors
    }

    fn paragraph(&mut self, p: &Para) {
        let text: String = p
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        match marker(p.style.as_deref()) {
            Some("id") => {
                let code = text.split_whitespace().next().map(str::to_owned);
                self.books.push(Book {
                    code,
                    lines: vec![format!("\\id {}", text.trim())],
                    anchors: Anchors::default(),
                });
                self.comment_marks(&p.items);
            }
            Some(LOOSE) => {
                self.anchors().text(" ");
                let line = self.inline(&p.items, false);
                if !line.is_empty() {
                    self.lines().push(line);
                }
            }
            Some("c") => {
                let line = format!("\\c {}", text.trim());
                self.lines().push(line);
                self.anchors().chapter(&text);
                self.comment_marks(&p.items);
            }
            Some(m) => {
                let line = self.line(m, &p.items);
                self.lines().push(line);
            }
            None => {
                let empty = text.trim().is_empty() && !p.items.iter().any(|i| matches!(i, Item::Note { .. }));
                if empty {
                    self.comment_marks(&p.items);
                    return;
                }
                let name = p.style.clone().unwrap_or_else(|| "(no style)".to_owned());
                *self.reader.report.unknown_paragraph_styles.entry(name).or_default() += 1;
                let line = self.line("p", &p.items);
                self.lines().push(line);
            }
        }
    }

    /// The comment anchors in a paragraph printed as one piece (`\\id`, `\\c`).
    fn comment_marks(&mut self, items: &[Item]) {
        for i in items {
            match i {
                Item::CommentStart(id) => self.anchors().start(id),
                Item::CommentEnd(id) => self.anchors().end(id),
                _ => {}
            }
        }
    }

    fn line(&mut self, marker: &str, items: &[Item]) -> String {
        self.anchors().text(" ");
        let inline = self.inline(items, false);
        if inline.is_empty() { format!("\\{marker}") } else { format!("\\{marker} {inline}") }
    }

    fn inline(&mut self, items: &[Item], in_note: bool) -> String {
        let mut spans = Spans::new(in_note);
        for (i, item) in items.iter().enumerate() {
            match item {
                Item::Text { style, text } => match marker(style.as_deref()) {
                    Some("v") => {
                        spans.close_all();
                        spans.push(&format!("\\v {} ", text.trim()));
                        if !in_note {
                            self.anchors().verse(text);
                        }
                    }
                    Some(RAW) => {
                        spans.close_all();
                        spans.push_markup(text);
                    }
                    Some(CALLER) => {}
                    Some(m) => {
                        let want: Vec<&str> = m.split(NEST).collect();
                        spans.to(&want);
                        spans.push(text);
                        if !in_note {
                            self.anchors().text(text);
                        }
                    }
                    None => {
                        if let Some(name) = style
                            && !benign(name)
                        {
                            *self.reader.report.unknown_character_styles.entry(name.clone()).or_default() += 1;
                        }
                        spans.close_all();
                        spans.push(text);
                        if !in_note {
                            self.anchors().text(text);
                        }
                    }
                },
                Item::Note { endnote, id, .. } => {
                    // A note inside a span only when the span goes on after it.
                    let goes_on = items[i + 1..].iter().find_map(|n| match n {
                        Item::Text { style, .. } => Some(style),
                        _ => None,
                    });
                    let inside = goes_on
                        .and_then(|s| marker(s.as_deref()))
                        .is_some_and(|m| m.split(NEST).eq(spans.stack.iter().copied()));
                    if !inside {
                        spans.close_all();
                    }
                    let note = self.note(*endnote, id);
                    spans.push_markup(&note);
                }
                Item::CommentStart(id) => self.anchors().start(id),
                Item::CommentEnd(id) => self.anchors().end(id),
            }
        }
        spans.close_all();
        spans.out
    }

    fn note(&mut self, endnote: bool, id: &str) -> String {
        let Some(element) = self.notes.get(&(endnote, id.to_owned())) else {
            self.reader.report.missing_notes += 1;
            return String::new();
        };
        let mut paragraphs = Vec::new();
        self.reader.blocks(element, &mut paragraphs);
        let style = paragraphs.first().and_then(|p| p.style.clone());
        let marker = match marker(style.as_deref()) {
            Some(m) => m.to_owned(),
            None => {
                let name = style.unwrap_or_else(|| "(no style)".to_owned());
                *self.reader.report.unknown_paragraph_styles.entry(name).or_default() += 1;
                if endnote { "fe".to_owned() } else { "f".to_owned() }
            }
        };
        let caller = paragraphs
            .iter()
            .flat_map(|p| &p.items)
            .find_map(|i| match i {
                Item::Text { style, text } if marker_is(style, CALLER) => Some(text.trim().to_owned()),
                _ => None,
            })
            .unwrap_or_else(|| "+".to_owned());
        let items: Vec<Item> = paragraphs
            .into_iter()
            .flat_map(|p| p.items)
            .filter(|i| matches!(i, Item::Text { .. }))
            .collect();
        let body = self.inline(&merge(items), true);
        let body = body.trim_start();
        format!("\\{marker} {caller} {body}\\{marker}*")
    }
}

fn marker_is(style: &Option<String>, m: &str) -> bool {
    marker(style.as_deref()) == Some(m)
}

/// The character markers open at a point in one paragraph or note, and the
/// USFM written so far.
struct Spans<'s> {
    out: String,
    stack: Vec<&'s str>,
    in_note: bool,
    /// A note peer closed at the top level whose closer is written only if
    /// plain text follows: `\fqa a\fqa* or`, but `\fr 1.1 \ft text`.
    pending: Option<&'s str>,
}

impl<'s> Spans<'s> {
    fn new(in_note: bool) -> Self {
        Self { out: String::new(), stack: Vec::new(), in_note, pending: None }
    }

    /// Text in the current spans.
    fn push(&mut self, text: &str) {
        if let Some(m) = self.pending.take()
            && self.stack.is_empty()
        {
            self.out.push('\\');
            self.out.push_str(m);
            self.out.push('*');
        }
        self.out.push_str(text);
    }

    /// Markup that is neither text nor a span: a whole note.
    fn push_markup(&mut self, markup: &str) {
        self.pending = None;
        self.out.push_str(markup);
    }

    fn close_to(&mut self, keep: usize) {
        while self.stack.len() > keep {
            let m = self.stack.pop().expect("non-empty");
            let depth = self.stack.len();
            if depth == 0 && self.in_note && NOTE_PEERS.contains(&m) {
                self.pending = Some(m);
                continue;
            }
            self.out.push('\\');
            if depth > 0 {
                self.out.push('+');
            }
            self.out.push_str(m);
            self.out.push('*');
        }
    }

    fn close_all(&mut self) {
        self.close_to(0);
    }

    fn to(&mut self, want: &[&'s str]) {
        let common = self.stack.iter().zip(want).take_while(|(a, b)| a == b).count();
        self.close_to(common);
        for m in &want[common..] {
            if self.stack.is_empty() {
                self.pending = None;
            }
            self.out.push('\\');
            if !self.stack.is_empty() {
                self.out.push('+');
            }
            self.out.push_str(m);
            self.out.push(' ');
            self.stack.push(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(steps: &[(&[&'static str], &str)], in_note: bool) -> String {
        let mut spans = Spans::new(in_note);
        for (want, text) in steps {
            spans.to(want);
            spans.push(text);
        }
        spans.close_all();
        spans.out
    }

    #[test]
    fn nesting_opens_and_closes_with_plus() {
        let out = write(&[(&["wj"], "I am "), (&["wj", "nd"], "Lord")], false);
        assert_eq!(out, r"\wj I am \+nd Lord\+nd*\wj*");
    }

    #[test]
    fn note_peers_close_only_before_plain_text() {
        assert_eq!(write(&[(&["fr"], "1.1 "), (&["ft"], "text")], true), r"\fr 1.1 \ft text");
        assert_eq!(write(&[(&["fqa"], "a"), (&[], " or "), (&["fqa"], "b")], true), r"\fqa a\fqa* or \fqa b");
    }

    /// An exported book with a comment and a reply added the way Word writes
    /// them.
    fn with_comment(usfm: &str, anchored: &str, inside: &str) -> Vec<u8> {
        let out = crate::export(usfm, &crate::ExportOptions::default());
        let mut parts: Vec<(String, Vec<u8>)> =
            zip::read(&out.docx).unwrap().into_iter().map(|e| (e.name, e.data)).collect();
        let (before, after) = anchored.split_once(inside).unwrap();
        let marked = format!(
            r#"{before}</w:t></w:r><w:commentRangeStart w:id="0"/><w:r><w:t xml:space="preserve">{inside}</w:t></w:r><w:commentRangeEnd w:id="0"/><w:r><w:t xml:space="preserve">{after}"#
        );
        for (name, data) in &mut parts {
            let text = String::from_utf8(data.clone()).unwrap();
            let text = match name.as_str() {
                "word/document.xml" => text.replacen(anchored, &marked, 1),
                "word/_rels/document.xml.rels" => text.replace(
                    "</Relationships>",
                    r#"<Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/><Relationship Id="rId9" Type="http://schemas.microsoft.com/office/2011/relationships/commentsExtended" Target="commentsExtended.xml"/></Relationships>"#,
                ),
                _ => text,
            };
            *data = text.into_bytes();
        }
        parts.push((
            "word/comments.xml".to_owned(),
            br#"<w:comments xmlns:w="w" xmlns:w14="w14"><w:comment w:id="0" w:author="Ana" w:date="2026-11-05T10:00:00Z"><w:p w14:paraId="AAA"><w:r><w:t>Is this the right word?</w:t></w:r></w:p></w:comment><w:comment w:id="1" w:author="Ben" w:date="1900-01-01T00:00:00Z"><w:p w14:paraId="BBB"><w:r><w:t>Yes.</w:t></w:r></w:p></w:comment></w:comments>"#.to_vec(),
        ));
        parts.push((
            "word/commentsExtended.xml".to_owned(),
            br#"<w15:commentsEx xmlns:w15="w15"><w15:commentEx w15:paraId="AAA" w15:done="1"/><w15:commentEx w15:paraId="BBB" w15:paraIdParent="AAA" w15:done="0"/></w15:commentsEx>"#.to_vec(),
        ));
        let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        zip::write(&refs)
    }

    #[test]
    fn comments_come_back_as_threads_on_their_verse() {
        let usfm = "\\id TIT\n\\c 1\n\\p\n\\v 1 Paul, a servant of God\n\\v 2 in hope\n";
        let docx = with_comment(usfm, "Paul, a servant of God", "servant");
        let back = crate::import(&docx).unwrap();
        let book = &back.books[0];
        assert_eq!(book.usfm, usfm.replace("\\p\n", "\\p ").replace("God\n", "God "));
        assert_eq!(book.comments.len(), 1);
        let t = &book.comments[0];
        assert_eq!(t.r#ref.as_deref(), Some("1:1"));
        assert_eq!(t.anchor_text, "servant");
        assert_eq!(t.start, Some(crate::VersePoint { r#ref: "1:1".to_owned(), offset: 8 }));
        assert_eq!(t.end, Some(crate::VersePoint { r#ref: "1:1".to_owned(), offset: 15 }));
        assert_eq!(t.comments.len(), 2);
        assert_eq!((t.comments[0].author.as_str(), t.comments[0].resolved), ("Ana", true));
        assert_eq!(t.comments[0].date.as_deref(), Some("2026-11-05T10:00:00Z"));
        assert_eq!(t.comments[1].parent_id.as_deref(), Some("0"));
        assert_eq!(t.comments[1].date, None);
    }

    #[test]
    fn style_ids_are_not_names() {
        assert_eq!(crate::styles::id("USFM q1"), "USFMq1");
    }
}
