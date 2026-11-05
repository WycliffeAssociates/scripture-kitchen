//! The exact path: a paragraph in one of our `USFM …` styles, read as the
//! markers its styles name, with no guessing.
//!
//! ```text
//! <w:p "USFM q1"><w:r "USFM v">3</w:r><w:r>He restores</w:r></w:p>
//!     ->  \q1 \v 3 He restores
//! <w:r "USFM wj">I am </w:r><w:r "USFM wj+nd">Lord</w:r>
//!     ->  \wj I am \+nd Lord\+nd*\wj*
//! <w:r "Emphasis">word</w:r> inside one   ->  word, unwrapped and reported
//! <w:r><w:i/>word</w:r> inside one        ->  \it word\it*, a guess in the report
//! ```

use crate::Guess;
use crate::import::Out;
use crate::reading::{Inline, NoteKind, Paragraph};
use crate::styles::{CALLER, LOOSE, NEST, NOTE_PEERS, PREFIX, RAW};

/// The marker a style name carries, when it is one of ours.
pub fn marker(style: Option<&str>) -> Option<&str> {
    style?.strip_prefix(PREFIX).filter(|m| !m.is_empty())
}

pub fn is_ours(p: &Paragraph) -> bool {
    marker(p.style.as_deref()).is_some()
}

/// Character styles that say nothing about the text: Word's and LibreOffice's
/// own note-mark styles and the default font.
fn benign(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("footnote") || n.contains("endnote") || n == "default paragraph font"
}

#[derive(Debug)]
enum Item {
    Text { style: Option<String>, text: String, italic: bool, bold: bool },
    Note(String),
    CommentStart(String),
    CommentEnd(String),
}

/// The paragraph's inlines as items, neighbouring text of one style joined:
/// Word splits runs freely. A line break is a space in the run before it.
fn items(inlines: &[Inline]) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for i in inlines {
        let (style, text, italic, bold) = match i {
            Inline::Text { text, style, direct, .. } => (style.clone(), text.clone(), direct.italic, direct.bold),
            Inline::Break => match out.last() {
                Some(Item::Text { style, italic, bold, .. }) => (style.clone(), " ".to_owned(), *italic, *bold),
                _ => (None, " ".to_owned(), false, false),
            },
            Inline::Note(id) => {
                out.push(Item::Note(id.clone()));
                continue;
            }
            Inline::CommentStart(id) => {
                out.push(Item::CommentStart(id.clone()));
                continue;
            }
            Inline::CommentEnd(id) => {
                out.push(Item::CommentEnd(id.clone()));
                continue;
            }
        };
        if let Some(Item::Text { style: s, text: t, italic: i, bold: b }) = out.last_mut()
            && *s == style
            && *i == italic
            && *b == bold
        {
            t.push_str(&text);
            continue;
        }
        out.push(Item::Text { style, text, italic, bold });
    }
    out
}

/// One paragraph in one of our styles.
pub fn paragraph(p: &Paragraph, out: &mut Out<'_>) {
    out.flush();
    let items = items(&p.inlines);
    let text: String = p.plain();
    match marker(p.style.as_deref()).unwrap_or("p") {
        "id" => {
            let code = text.split_whitespace().next().map(str::to_owned);
            out.new_book(code, format!("\\id {}", text.trim()));
            comment_marks(&items, out);
        }
        "c" => {
            out.lines().push(format!("\\c {}", text.trim()));
            out.strict_chapter(text.trim());
            comment_marks(&items, out);
        }
        LOOSE => {
            out.anchors().text(" ");
            let line = inline(&items, false, out);
            if !line.is_empty() {
                out.lines().push(line);
            }
        }
        m => {
            out.anchors().text(" ");
            let inline = inline(&items, false, out);
            let line = if inline.is_empty() { format!("\\{m}") } else { format!("\\{m} {inline}") };
            out.lines().push(line);
        }
    }
}

/// The comment anchors in a paragraph printed as one piece (`\id`, `\c`).
fn comment_marks(items: &[Item], out: &mut Out<'_>) {
    for i in items {
        match i {
            Item::CommentStart(id) => out.anchors().start(id),
            Item::CommentEnd(id) => out.anchors().end(id),
            _ => {}
        }
    }
}

fn inline(items: &[Item], in_note: bool, out: &mut Out<'_>) -> String {
    let mut spans = Spans::new(in_note);
    for (i, item) in items.iter().enumerate() {
        match item {
            Item::Text { style, text, italic, bold } => match marker(style.as_deref()) {
                Some("v") => {
                    spans.close_all();
                    spans.push(&format!("\\v {} ", text.trim()));
                    if !in_note {
                        out.strict_verse(text.trim());
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
                        out.anchors().text(text);
                    }
                }
                None => {
                    if let Some(name) = style
                        && !benign(name)
                    {
                        *out.report.unknown_character_styles.entry(name.clone()).or_default() += 1;
                    }
                    spans.close_all();
                    spans.push(&emphasis(text, *italic, *bold, out));
                    if !in_note {
                        out.anchors().text(text);
                    }
                }
            },
            Item::Note(id) => {
                // A note inside a span only when the span goes on after it.
                let goes_on = items[i + 1..].iter().find_map(|n| match n {
                    Item::Text { style, .. } => Some(style),
                    _ => None,
                });
                let inside =
                    goes_on.and_then(|s| marker(s.as_deref())).is_some_and(|m| m.split(NEST).eq(spans.stack.iter().copied()));
                if !inside {
                    spans.close_all();
                }
                let note = note(id, out);
                spans.push_markup(&note);
            }
            Item::CommentStart(id) => out.anchors().start(id),
            Item::CommentEnd(id) => out.anchors().end(id),
        }
    }
    spans.close_all();
    spans.out
}

/// Italic or bold set by hand on text in no character style of ours.
fn emphasis(text: &str, italic: bool, bold: bool, out: &mut Out<'_>) -> String {
    let italic = italic && out.settings.italic == crate::Keep::Keep;
    let bold = bold && out.settings.bold == crate::Keep::Keep;
    let core = text.trim();
    if (!italic && !bold) || core.is_empty() {
        return text.to_owned();
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
    let marker = if bold { "bd" } else { "it" };
    let reason = if bold { "bold set by hand" } else { "italic set by hand" };
    let r = out.verse_ref();
    out.report.guesses.push(Guess { r#ref: r, marker: marker.to_owned(), reason: reason.to_owned(), excerpt: core.to_owned() });
    format!("{lead}{wrapped}{trail}")
}

fn note(id: &str, out: &mut Out<'_>) -> String {
    let Some(n) = out.reading.notes.get(id) else {
        out.report.missing_notes += 1;
        return String::new();
    };
    let paragraphs = n.paragraphs.clone();
    let endnote = n.kind == NoteKind::Endnote;
    let style = paragraphs.first().and_then(|p| p.style.clone());
    let m = match marker(style.as_deref()) {
        Some(m) => m.to_owned(),
        None => {
            let name = style.unwrap_or_else(|| "(no style)".to_owned());
            *out.report.unknown_note_styles.entry(name).or_default() += 1;
            if endnote { "fe".to_owned() } else { "f".to_owned() }
        }
    };
    let all: Vec<Inline> = paragraphs.iter().flat_map(|p| p.inlines.iter().cloned()).collect();
    let all = items(&all);
    let caller = all
        .iter()
        .find_map(|i| match i {
            Item::Text { style, text, .. } if marker(style.as_deref()) == Some(CALLER) => Some(text.trim().to_owned()),
            _ => None,
        })
        .unwrap_or_else(|| "+".to_owned());
    let text: Vec<Item> = all.into_iter().filter(|i| matches!(i, Item::Text { .. })).collect();
    let body = inline(&text, true, out);
    format!("\\{m} {caller} {}\\{m}*", body.trim_start())
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

    /// Markup that is neither text nor a span: a whole note, a raw run.
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
}
