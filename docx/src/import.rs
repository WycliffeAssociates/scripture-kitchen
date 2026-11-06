//! A .docx or .odt into USFM: each paragraph read exactly when it is in one
//! of our styles, and by the best-effort rules when it is not.
//!
//! ```text
//! read(bytes)                 ->  Reading    once per file: parts unzipped and parsed
//! render(&reading, &settings) ->  Imported   cheap; rerun on every settings change
//!
//! for each paragraph:
//!   "USFM …" style    ->  strict::paragraph   the style is the marker
//!   anything else     ->  guess::scan + emit  document-import's rules, each guess reported
//! ```
//!
//! Both paths write into one [`Out`]: the same books, lines, chapter and
//! verse, so a heading pasted between two of our paragraphs follows the verse
//! before it, and a comment anchors the same way in either.

use std::collections::{BTreeMap, BTreeSet};

use crate::comments::Anchors;
use crate::dom::{self, Element};
use crate::guess::{self, Guesser};
use crate::reading::{Inline, NoteKind, Paragraph, Reading, WordReader, WordStyles, word_notes};
use crate::{
    Found, ImportError, ImportReport, Imported, ImportedBook, KeepDrop, Settings, strict, zip,
};

const OFFICE_DOCUMENT: &str = "/officeDocument";

/// Reads a .docx, or an .odt (told apart by the `mimetype` entry ODF puts first).
pub fn read(bytes: &[u8]) -> Result<Reading, ImportError> {
    let package = Package {
        archive: zip::Archive::open(bytes).map_err(ImportError::Zip)?,
    };
    let odt = package
        .get("mimetype")?
        .is_some_and(|m| m.starts_with(crate::odt::MIMETYPE.as_bytes()));
    if odt || (package.archive.has("content.xml") && !package.archive.has("word/document.xml")) {
        let content = package
            .xml("content.xml")?
            .ok_or_else(|| ImportError::Missing("content.xml".to_owned()))?;
        let styles = package.xml("styles.xml")?;
        let mut reading = crate::odt::read(&content, styles.as_ref())
            .ok_or_else(|| ImportError::Missing("content.xml: office:text".to_owned()))?;
        if let Some(meta) = package.xml("meta.xml")? {
            reading.properties = crate::odt::properties(&meta);
        }
        return Ok(reading);
    }

    let main = package
        .rels("_rels/.rels", "")?
        .into_iter()
        .find(|(t, _)| t.ends_with(OFFICE_DOCUMENT))
        .map_or_else(|| "word/document.xml".to_owned(), |(_, target)| target);
    let (dir, file) = main.rsplit_once('/').unwrap_or(("", main.as_str()));
    let rels = package.rels(&format!("{dir}/_rels/{file}.rels"), dir)?;
    let part = |kind: &str| -> Result<Option<Element>, ImportError> {
        match rels.iter().find(|(t, _)| t.ends_with(kind)) {
            Some((_, p)) => package.xml(p),
            None => Ok(None),
        }
    };

    let styles = WordStyles::read(part("/styles")?.as_ref());
    let document = package
        .xml(&main)?
        .ok_or_else(|| ImportError::Missing(main.clone()))?;
    let body = document
        .child("w:body")
        .ok_or_else(|| ImportError::Missing(format!("{main}: w:body")))?;
    let mut reader = WordReader {
        styles: &styles,
        tracked_changes: 0,
    };
    let mut paragraphs = Vec::new();
    reader.blocks(body, false, &mut paragraphs);
    let mut notes = BTreeMap::new();
    word_notes(
        &mut reader,
        part("/footnotes")?.as_ref(),
        NoteKind::Footnote,
        &mut notes,
    );
    word_notes(
        &mut reader,
        part("/endnotes")?.as_ref(),
        NoteKind::Endnote,
        &mut notes,
    );
    let comments = crate::comments::read(
        part("/comments")?.as_ref(),
        part("/commentsExtended")?.as_ref(),
    );
    let properties = match package.xml("docProps/custom.xml")? {
        Some(root) => crate::props::read(&root),
        None => Vec::new(),
    };
    Ok(Reading {
        format: crate::reading::Format::Docx,
        paragraphs,
        notes,
        comments,
        tracked_changes: reader.tracked_changes,
        properties,
    })
}

/// The package, its parts inflated only when read.
struct Package<'a> {
    archive: zip::Archive<'a>,
}

impl Package<'_> {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, ImportError> {
        self.archive.get(name).map_err(ImportError::Zip)
    }

    fn xml(&self, name: &str) -> Result<Option<Element>, ImportError> {
        self.get(name)?
            .map(|data| dom::parse(name, &data).map_err(ImportError::Xml))
            .transpose()
    }

    /// A relationships part, as (type, target resolved against `dir`).
    fn rels(&self, name: &str, dir: &str) -> Result<Vec<(String, String)>, ImportError> {
        let Some(root) = self.xml(name)? else {
            return Ok(Vec::new());
        };
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
                Some((e.attr("Type")?.to_owned(), normalise(&path)))
            })
            .collect())
    }
}

/// A part path with its `.` and `..` steps resolved: `word/../media/a.png` is `media/a.png`.
fn normalise(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for step in path.split('/') {
        match step {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

pub(crate) struct Book {
    code: Option<String>,
    lines: Vec<String>,
    anchors: Anchors,
}

/// Where both paths write: the books so far, and where the reading is.
pub(crate) struct Out<'a> {
    pub reading: &'a Reading,
    pub settings: &'a Settings,
    books: Vec<Book>,
    pub chapter: u32,
    /// The last verse number (a bridge's last), 0 before the first.
    pub verse: u32,
    /// A verse has been seen: text before it is front matter.
    pub started: bool,
    seen: BTreeMap<u32, BTreeSet<u32>>,
    pub report: ImportReport,
    /// The line the best-effort path is building.
    pub line: String,
    pub mt_count: usize,
}

impl<'a> Out<'a> {
    fn new(reading: &'a Reading, settings: &'a Settings) -> Self {
        Self {
            reading,
            settings,
            books: Vec::new(),
            chapter: 0,
            verse: 0,
            started: false,
            seen: BTreeMap::new(),
            report: ImportReport::default(),
            line: String::new(),
            mt_count: 0,
        }
    }

    fn book(&mut self) -> &mut Book {
        if self.books.is_empty() {
            let lines = self
                .settings
                .book
                .iter()
                .map(|b| format!("\\id {b}"))
                .collect();
            self.books.push(Book {
                code: self.settings.book.clone(),
                lines,
                anchors: Anchors::default(),
            });
        }
        self.books.last_mut().expect("a book")
    }

    pub fn lines(&mut self) -> &mut Vec<String> {
        &mut self.book().lines
    }

    pub fn anchors(&mut self) -> &mut Anchors {
        &mut self.book().anchors
    }

    /// The verse a comment or footnote would sit in: `chapter:verse`.
    pub fn anchors_ref(&mut self) -> Option<String> {
        self.anchors().current()
    }

    pub fn new_book(&mut self, code: Option<String>, id_line: String) {
        self.flush();
        self.books.push(Book {
            code,
            lines: vec![id_line],
            anchors: Anchors::default(),
        });
        self.chapter = 0;
        self.verse = 0;
        self.started = false;
        self.mt_count = 0;
    }

    /// The best-effort path's line, written out when it holds anything.
    pub fn flush(&mut self) {
        let line = std::mem::take(&mut self.line);
        // Runs of spaces and tabs as one space, then trimmed as JavaScript
        // trims: every kind of whitespace, no-break spaces included.
        let joined = line
            .split([' ', '\t'])
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let tidied = joined.trim();
        if !tidied.is_empty() {
            let tidied = tidied.to_owned();
            self.lines().push(tidied);
        }
    }

    pub fn chapter_line(&mut self, n: u32) {
        self.flush();
        self.lines().push(format!("\\c {n}"));
        self.anchors().chapter(&n.to_string());
    }

    /// `chapter:verse` of the last verse, as a report names it.
    pub fn verse_ref(&self) -> Option<String> {
        (self.verse > 0).then(|| format!("{}:{}", self.chapter, self.verse))
    }

    pub fn record(&mut self, from: u32, to: u32) {
        let set = self.seen.entry(self.chapter).or_default();
        set.extend(from..=to);
    }

    /// The chapter's verse count from the versification, when there is one.
    pub fn max_verse(&self, chapter: u32) -> Option<u32> {
        let scheme = canon::scheme(self.settings.versification.as_deref()?)?;
        let code = self
            .books
            .last()
            .and_then(|b| b.code.clone())
            .or_else(|| self.settings.book.clone())?;
        let book = canon::parse_book(&code)?;
        scheme
            .last_verse(book, u16::try_from(chapter).ok()?)
            .map(u32::from)
    }

    /// A `\c` in one of our styles.
    pub fn strict_chapter(&mut self, number: &str) {
        if let Some(n) = leading_number(number) {
            self.chapter = n;
            self.verse = 0;
        }
        self.anchors().chapter(number);
    }

    /// A `\v` in one of our styles.
    pub fn strict_verse(&mut self, number: &str) {
        let from = leading_number(number);
        let to = number
            .split_once(['-', '–'])
            .and_then(|(_, b)| leading_number(b.trim()))
            .or(from);
        if let (Some(from), Some(to)) = (from, to) {
            if self.chapter == 0 {
                self.chapter = 1;
            }
            self.verse = to;
            self.record(from, to.max(from));
        }
        self.started = true;
        let chapter = self.chapter.to_string();
        self.anchors().at(&chapter, number);
    }
}

fn leading_number(s: &str) -> Option<u32> {
    let n = s.bytes().take_while(u8::is_ascii_digit).count();
    s[..n].parse().ok()
}

pub fn render(reading: &Reading, settings: &Settings) -> Imported {
    let paragraphs: Vec<Paragraph> = reading
        .paragraphs
        .iter()
        .map(|p| {
            if strict::is_ours(p) {
                p.clone()
            } else {
                guess::tidy(p)
            }
        })
        .collect();
    let guessed: Vec<&Paragraph> = paragraphs.iter().filter(|p| !strict::is_ours(p)).collect();
    let any_ours = paragraphs.len() != guessed.len();
    let mut found = found(reading, &paragraphs);

    if !any_ours && guess::looks_like_usfm(&guessed) {
        return passthrough(reading, &paragraphs, found);
    }

    let our_verses = paragraphs
        .iter()
        .filter(|p| strict::is_ours(p))
        .flat_map(|p| &p.inlines)
        .filter(|i| matches!(i, Inline::Text { style: Some(s), .. } if strict::marker(Some(s)) == Some("v")))
        .count();
    let guesser = Guesser::new(&guessed, our_verses, settings);
    let mut out = Out::new(reading, settings);
    for p in &paragraphs {
        if strict::is_ours(p) {
            strict::paragraph(p, &mut out);
            continue;
        }
        let blocks = guesser.scan(p, &mut out);
        if !blocks.is_empty() {
            let name = p.style.clone().unwrap_or_else(|| "(no style)".to_owned());
            *out.report.guessed_paragraphs.entry(name).or_default() += 1;
        }
        for b in &blocks {
            match b.kind {
                guess::BlockKind::Front => found.front += 1,
                guess::BlockKind::Heading => found.headings += 1,
                guess::BlockKind::Para if b.poetry > 0 => found.poetry += 1,
                _ => {}
            }
        }
        guesser.emit(&blocks, &mut out);
    }
    out.flush();

    let mut report = std::mem::take(&mut out.report);
    report.tracked_changes = reading.tracked_changes;
    report.verses = guesser.mode;
    report.detected = guesser.detected;
    report.candidates = guesser.counts;
    report.chapters = guess::summarise(&out.seen);
    report.found = found;

    let books = finish(out.books, reading, settings);
    Imported {
        books,
        report,
        properties: reading.properties.clone(),
    }
}

/// Each book's USFM, source and comment threads.
fn finish(books: Vec<Book>, reading: &Reading, settings: &Settings) -> Vec<ImportedBook> {
    let comments = if settings.comments == KeepDrop::Keep {
        reading.comments.as_slice()
    } else {
        &[]
    };
    let anchored = |id: &str| books.iter().any(|b| b.anchors.has(id));
    let unanchored: Vec<String> = comments
        .iter()
        .map(|c| c.id.clone())
        .filter(|id| !anchored(id))
        .collect();
    books
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let source = b
                .code
                .as_deref()
                .and_then(|c| crate::props::source(&reading.properties, c));
            let mut usfm = b.lines.join("\n");
            usfm.push('\n');
            // A thread anchored nowhere goes with the first book.
            let here = b.anchors.ids();
            let take = |root: &str| {
                here.iter().any(|h| h == root) || (i == 0 && unanchored.iter().any(|u| u == root))
            };
            let comments = b.anchors.threads(comments, take);
            ImportedBook {
                code: b.code,
                usfm,
                source,
                comments,
            }
        })
        .collect()
}

/// A document that already holds USFM, copied through as text.
fn passthrough(reading: &Reading, paragraphs: &[Paragraph], found: Found) -> Imported {
    let text: Vec<String> = paragraphs
        .iter()
        .map(|p| {
            p.inlines
                .iter()
                .map(|i| match i {
                    Inline::Text { text, .. } => text.as_str(),
                    Inline::Break => "\n",
                    _ => "",
                })
                .collect()
        })
        .collect();
    let mut usfm = text.join("\n").trim_end().to_owned();
    usfm.push('\n');
    let code = usfm
        .lines()
        .find_map(|l| {
            l.trim_start()
                .strip_prefix("\\id ")
                .and_then(|r| r.split_whitespace().next())
        })
        .map(str::to_owned);
    let report = ImportReport {
        passthrough: true,
        verses: crate::VerseMode::Marked,
        detected: crate::VerseMode::Marked,
        tracked_changes: reading.tracked_changes,
        found,
        ..ImportReport::default()
    };
    let source = code
        .as_deref()
        .and_then(|c| crate::props::source(&reading.properties, c));
    Imported {
        books: vec![ImportedBook {
            code,
            usfm,
            source,
            comments: Vec::new(),
        }],
        report,
        properties: reading.properties.clone(),
    }
}

/// What the document holds, whatever the settings do with it: the switches a
/// host offers.
fn found(reading: &Reading, paragraphs: &[Paragraph]) -> Found {
    let mut f = Found {
        notes: reading.notes.len(),
        comments: reading.comments.len(),
        tracked_changes: reading.tracked_changes,
        ..Found::default()
    };
    for p in paragraphs {
        let has_text = !p.plain().trim().is_empty();
        if p.list && has_text {
            f.lists += 1;
        }
        if p.table && has_text {
            f.tables += 1;
        }
        if !has_text && !p.has_note() {
            f.blank_paragraphs += 1;
        }
        for i in &p.inlines {
            match i {
                Inline::Break => f.line_breaks += 1,
                Inline::Text { text, format, .. } if !text.trim().is_empty() => {
                    f.italic += usize::from(format.italic);
                    f.bold += usize::from(format.bold);
                }
                _ => {}
            }
        }
    }
    let refs: Vec<&Paragraph> = paragraphs.iter().collect();
    f.style_markers = guess::count_style_markers(&refs);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An exported book with a comment and a reply added the way Word writes
    /// them.
    fn with_comment(usfm: &str, anchored: &str, inside: &str) -> Vec<u8> {
        let out = crate::export(usfm, &crate::ExportOptions::default());
        let mut parts: Vec<(String, Vec<u8>)> = zip::read(&out.docx)
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.data))
            .collect();
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
        let refs: Vec<(&str, &[u8])> = parts
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect();
        zip::write(&refs)
    }

    #[test]
    fn comments_come_back_as_threads_on_their_verse() {
        let usfm = "\\id TIT\n\\c 1\n\\p\n\\v 1 Paul, a servant of God\n\\v 2 in hope\n";
        let docx = with_comment(usfm, "Paul, a servant of God", "servant");
        let back = crate::import(&docx, &Settings::default()).unwrap();
        let book = &back.books[0];
        assert_eq!(
            book.usfm,
            "\\id TIT\n\\c 1\n\\p \\v 1 Paul, a servant of God \\v 2 in hope\n"
        );
        assert_eq!(book.comments.len(), 1);
        let t = &book.comments[0];
        assert_eq!(t.r#ref.as_deref(), Some("1:1"));
        assert_eq!(t.anchor_text, "servant");
        assert_eq!(
            t.start,
            Some(crate::VersePoint {
                r#ref: "1:1".to_owned(),
                offset: 8
            })
        );
        assert_eq!(
            t.end,
            Some(crate::VersePoint {
                r#ref: "1:1".to_owned(),
                offset: 15
            })
        );
        assert_eq!(t.comments.len(), 2);
        assert_eq!(
            (t.comments[0].author.as_str(), t.comments[0].resolved),
            ("Ana", true)
        );
        assert_eq!(t.comments[0].date.as_deref(), Some("2026-11-05T10:00:00Z"));
        assert_eq!(t.comments[1].parent_id.as_deref(), Some("0"));
        assert_eq!(t.comments[1].date, None);
    }

    fn text(value: &str, superscript: bool) -> Inline {
        let format = crate::reading::Formatting {
            superscript,
            ..Default::default()
        };
        Inline::Text {
            text: value.to_owned(),
            style: None,
            format,
            direct: format,
        }
    }

    fn para(inlines: Vec<Inline>) -> Paragraph {
        Paragraph {
            inlines,
            ..Paragraph::default()
        }
    }

    fn verses(usfm: &str) -> Vec<String> {
        usfm.split('\\')
            .filter_map(|m| {
                let (name, rest) = m.split_once(' ')?;
                matches!(name, "c" | "v")
                    .then(|| format!("{name}{}", rest.split_whitespace().next().unwrap_or("")))
            })
            .collect()
    }

    #[test]
    fn only_the_next_number_written_the_documents_way_is_a_verse() {
        let reading = Reading {
            paragraphs: vec![
                para(vec![
                    text("1", true),
                    text("The army was 1,000 men ", false),
                    text("2", true),
                    text("and 3 sons. ", false),
                    text("3-4", true),
                    text("Bridged. ", false),
                    text("14", true),
                    text("Too far.", false),
                ]),
                para(vec![text("1", true), text("Again.", false)]),
            ],
            ..Reading::default()
        };
        let settings = Settings {
            book: Some("2JN".to_owned()),
            versification: Some("eng".to_owned()),
            ..Settings::default()
        };
        let out = render(&reading, &settings);
        let usfm = &out.books[0].usfm;
        assert_eq!(verses(usfm), ["c1", "v1", "v2", "v3-4", "c2", "v1"]);
        assert!(usfm.contains("The army was 1,000 men"), "{usfm}");
        assert!(usfm.contains("14Too far."), "{usfm}");
        assert_eq!(
            out.report.skipped,
            [crate::Skip {
                r#ref: Some("1:4".to_owned()),
                number: "14".to_owned(),
                reason: crate::SkipReason::Versification
            }]
        );
    }

    #[test]
    fn line_start_and_marked_numbers_are_each_detected() {
        let reading = |ps: Vec<Paragraph>| Reading {
            paragraphs: ps,
            ..Reading::default()
        };
        let line_start = render(
            &reading(vec![
                para(vec![text("Chapter 2", false)]),
                para(vec![text("1 One, 3 days.", false)]),
                para(vec![text("2 Two.", false)]),
            ]),
            &Settings::default(),
        );
        assert_eq!(line_start.report.detected, crate::VerseMode::LineStart);
        assert_eq!(verses(&line_start.books[0].usfm), ["c2", "v1", "v2"]);
        let marked = render(
            &reading(vec![para(vec![text(
                "v1 One. verse 2 Two. v 3 Three in 4 parts.",
                false,
            )])]),
            &Settings::default(),
        );
        assert_eq!(marked.report.detected, crate::VerseMode::Marked);
        assert_eq!(verses(&marked.books[0].usfm), ["c1", "v1", "v2", "v3"]);
    }

    /// Our export with two paragraphs pasted in from somewhere else.
    #[test]
    fn a_mixed_document_reads_ours_exactly_and_guesses_the_rest() {
        let usfm = "\\id TIT\n\\c 1\n\\p\n\\v 1 Paul, a servant\n\\v 2 in hope\n";
        let out = crate::export(usfm, &crate::ExportOptions::default());
        let mut parts: Vec<(String, Vec<u8>)> = zip::read(&out.docx)
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.data))
            .collect();
        let pasted = concat!(
            r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>A pasted heading</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>3</w:t></w:r>"#,
            r#"<w:r><w:t xml:space="preserve">Pasted </w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>verse</w:t></w:r><w:r><w:t>.</w:t></w:r></w:p>"#,
        );
        for (name, data) in &mut parts {
            if name == "word/document.xml" {
                let text = String::from_utf8(data.clone()).unwrap();
                *data = text
                    .replacen("<w:sectPr>", &format!("{pasted}<w:sectPr>"), 1)
                    .into_bytes();
            }
        }
        let refs: Vec<(&str, &[u8])> = parts
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect();
        let back = crate::import(&zip::write(&refs), &Settings::default()).unwrap();
        assert_eq!(
            back.books[0].usfm,
            "\\id TIT\n\\c 1\n\\p \\v 1 Paul, a servant \\v 2 in hope\n\\s1 A pasted heading\n\\p\n\\v 3 Pasted \\it verse\\it*.\n"
        );
        assert_eq!(back.report.guessed_paragraphs.get("Normal"), Some(&2));
        assert_eq!(back.report.guesses.len(), 1);
        assert_eq!(back.report.guesses[0].marker, "s1");
        assert_eq!(back.report.guesses[0].r#ref.as_deref(), Some("1:2"));
        assert!(!back.report.is_exact());
    }

    #[test]
    fn the_template_reads_back_as_its_starter_book() {
        let back = crate::import(&crate::template(), &Settings::default()).unwrap();
        assert!(back.report.is_exact(), "{:?}", back.report);
        assert_eq!(back.books[0].code.as_deref(), Some("XXX"));
        assert_eq!(back.books[0].source, None);
        assert!(
            back.books[0]
                .usfm
                .contains("\\q2 and the next one indented.")
        );
    }
}
