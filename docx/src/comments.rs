//! Word comments, in the shape `@sefer/document-import` returns them: one
//! thread per root comment, anchored by verse and an offset into that verse's
//! plain text.
//!
//! ```text
//! comments.xml          w:comment id author date, its paragraphs' text
//! commentsExtended.xml  w15:commentEx paraId → paraIdParent (a reply), done (resolved)
//! document.xml          w:commentRangeStart/End id, where the anchor sits
//!
//! -> CommentThread { ref: "3:16", anchor_text, start: (3:16, 4), end: (3:16, 9),
//!                    comments: [root, reply, …] }
//! ```
//!
//! A verse's plain text is its words with every run of whitespace as one
//! space: no markers, no verse number, no notes. Offsets count UTF-16 code
//! units, as JavaScript does. The ref is `chapter:verse`, the verse as the
//! `\v` spells it (`3-4` for a bridge).

use std::collections::BTreeMap;

use crate::dom::Element;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub author: String,
    /// `None` when Word stripped personal information (it writes 1900-01-01).
    pub date: Option<String>,
    pub text: String,
    pub parent_id: Option<String>,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersePoint {
    /// `chapter:verse`.
    pub r#ref: String,
    /// UTF-16 code units into the verse's plain text.
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentThread {
    pub r#ref: Option<String>,
    pub anchor_text: String,
    pub start: Option<VersePoint>,
    /// The start again for a comment on a point.
    pub end: Option<VersePoint>,
    /// The root first, then replies in document order.
    pub comments: Vec<Comment>,
}

/// Every comment in comments.xml, in document order, with replies and
/// resolved state from commentsExtended.xml.
pub fn read(comments: Option<&Element>, extended: Option<&Element>) -> Vec<Comment> {
    let Some(root) = comments else { return Vec::new() };
    let mut out = Vec::new();
    let mut by_para: BTreeMap<String, usize> = BTreeMap::new();
    for c in root.elements().filter(|e| e.name == "w:comment") {
        let Some(id) = c.attr("w:id") else { continue };
        let paragraphs: Vec<&Element> = c.elements().filter(|e| e.name == "w:p").collect();
        let text = paragraphs.iter().map(|p| texts(p)).collect::<Vec<_>>().join("\n").trim().to_owned();
        if let Some(para) = paragraphs.last().and_then(|p| p.attr("w14:paraId")) {
            by_para.insert(para.to_owned(), out.len());
        }
        let date = c.attr("w:date").filter(|d| !d.starts_with("1900")).map(str::to_owned);
        out.push(Comment {
            id: id.to_owned(),
            author: c.attr("w:author").unwrap_or("").to_owned(),
            date,
            text,
            parent_id: None,
            resolved: false,
        });
    }
    for ex in extended.iter().flat_map(|e| e.elements()).filter(|e| e.name == "w15:commentEx") {
        let Some(&i) = ex.attr("w15:paraId").and_then(|p| by_para.get(p)) else { continue };
        let parent = ex.attr("w15:paraIdParent").and_then(|p| by_para.get(p)).map(|&j| out[j].id.clone());
        out[i].parent_id = parent;
        out[i].resolved = ex.attr("w15:done") == Some("1");
    }
    out
}

/// The `w:t` text below an element: a comment's own paragraphs.
fn texts(e: &Element) -> String {
    let mut out = String::new();
    for c in e.elements() {
        if c.name == "w:t" {
            out.push_str(&c.text());
        } else {
            out.push_str(&texts(c));
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
struct Anchor {
    start: Option<VersePoint>,
    end: Option<VersePoint>,
    text: String,
}

/// Where comment anchors fall in the book being printed.
#[derive(Debug, Default)]
pub struct Anchors {
    chapter: String,
    verse: Option<String>,
    /// Each verse's plain text, by ref.
    verses: BTreeMap<String, String>,
    anchors: BTreeMap<String, Anchor>,
    open: Vec<String>,
}

impl Anchors {
    /// `chapter:verse` where the next text lands.
    pub fn current(&self) -> Option<String> {
        self.verse.as_ref().map(|v| format!("{}:{v}", self.chapter))
    }

    /// A verse starts: its plain text starts empty.
    pub fn at(&mut self, chapter: &str, verse: &str) {
        self.chapter = chapter.trim().to_owned();
        self.verse = Some(verse.trim().to_owned());
        if let Some(r) = self.current() {
            self.verses.insert(r, String::new());
        }
    }

    fn here(&self) -> Option<VersePoint> {
        let r = self.current()?;
        let offset = self.verses.get(&r).map_or(0, |t| t.encode_utf16().count());
        Some(VersePoint { r#ref: r, offset })
    }

    pub fn chapter(&mut self, number: &str) {
        self.chapter = number.trim().to_owned();
        self.verse = None;
    }

    /// Words of the verse being printed (not a note's).
    pub fn text(&mut self, text: &str) {
        for id in &self.open {
            if let Some(a) = self.anchors.get_mut(id) {
                a.text.push_str(text);
            }
        }
        if let Some(r) = self.current() {
            let verse = self.verses.entry(r).or_default();
            for c in text.chars() {
                if !c.is_whitespace() {
                    verse.push(c);
                } else if !verse.is_empty() && !verse.ends_with(' ') {
                    verse.push(' ');
                }
            }
        }
    }

    pub fn start(&mut self, id: &str) {
        let start = self.here();
        self.anchors.insert(id.to_owned(), Anchor { start, ..Anchor::default() });
        self.open.push(id.to_owned());
    }

    pub fn end(&mut self, id: &str) {
        let end = self.here();
        if let Some(a) = self.anchors.get_mut(id) {
            a.end = end;
        }
        self.open.retain(|o| o != id);
    }

    pub fn has(&self, id: &str) -> bool {
        self.anchors.contains_key(id)
    }

    pub fn ids(&self) -> Vec<String> {
        self.anchors.keys().cloned().collect()
    }

    /// The threads whose root comment is anchored in this book. Ranges are
    /// pulled in off the collapsed space at either end.
    pub fn threads(mut self, comments: &[Comment], take: impl Fn(&str) -> bool) -> Vec<CommentThread> {
        for text in self.verses.values_mut() {
            let trimmed = text.trim_end().len();
            text.truncate(trimmed);
        }
        let units = |r: &str| -> Vec<u16> { self.verses.get(r).map_or(Vec::new(), |t| t.encode_utf16().collect()) };
        for a in self.anchors.values_mut() {
            if let Some(s) = &mut a.start {
                let v = units(&s.r#ref);
                if v.get(s.offset) == Some(&(b' ' as u16)) {
                    s.offset += 1;
                }
            }
            if let Some(e) = &mut a.end {
                let v = units(&e.r#ref);
                if e.offset > 0 && v.get(e.offset - 1) == Some(&(b' ' as u16)) {
                    e.offset -= 1;
                }
                e.offset = e.offset.min(v.len());
            }
        }
        let root_of = |id: &str| -> String {
            let mut id = id.to_owned();
            for _ in 0..50 {
                match comments.iter().find(|c| c.id == id).and_then(|c| c.parent_id.clone()) {
                    Some(p) if p != id => id = p,
                    _ => break,
                }
            }
            id
        };
        let mut threads: Vec<(String, CommentThread)> = Vec::new();
        for c in comments {
            let root = root_of(&c.id);
            if !take(&root) {
                continue;
            }
            let i = match threads.iter().position(|(r, _)| *r == root) {
                Some(i) => i,
                None => {
                    let a = self.anchors.get(&root).or_else(|| self.anchors.get(&c.id)).cloned().unwrap_or_default();
                    threads.push((
                        root.clone(),
                        CommentThread {
                            r#ref: a.start.as_ref().map(|s| s.r#ref.clone()),
                            anchor_text: a.text.split_whitespace().collect::<Vec<_>>().join(" "),
                            end: a.end.or_else(|| a.start.clone()),
                            start: a.start,
                            comments: Vec::new(),
                        },
                    ));
                    threads.len() - 1
                }
            };
            let t = &mut threads[i].1;
            if c.id == root {
                t.comments.insert(0, c.clone());
            } else {
                t.comments.push(c.clone());
            }
        }
        threads.into_iter().map(|(_, t)| t).collect()
    }
}
