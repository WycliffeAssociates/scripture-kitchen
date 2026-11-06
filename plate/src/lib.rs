//! USFM to Typst markup over onion's public CST (option A: onion writes markup,
//! a Typst template styles it, Typst never knows onion exists).
//!
//! The emitter does the structural work Typst is slow at — which paragraph a
//! chapter number lands in, which marker is a paragraph, a character style or
//! a note, whitespace folding — and leaves every visual decision to the
//! template `typst/usfm.typ`, which keys its styles off marker names:
//!
//! ```text
//! \c 1                  #u.c("1");
//! \p                    #u.para("p", 0, c: "1")[#u.v("1", "1");In the begin…
//! \v 1 In the begin…    …#u.n("f", "+")[#u.ch("ft")[Or…]];]
//! ```
//!
//! Every code expression ends in `;` so the text after it can never be read
//! as a method call or a trailing content argument.

pub mod recipe;
pub mod world;

use usfm_onion::cst::{Cst, build};
use usfm_onion::tables::generated;
use usfm_onion::tables::schema::{Category, MarkerKind};
use usfm_onion::{Token, TokenKind, lex};

/// `cst.child_ids` tags node ids with bit 31; the constant itself is
/// `pub(crate)` in onion, so a consumer restates it.
const NODE_BIT: u32 = 1 << 31;

/// Where a recipe plugs in. Every hook returns Typst markup (or nothing) that
/// the emitter places verbatim; the core never knows what a recipe prints.
#[derive(Default)]
pub struct Hooks<'h> {
    /// Resolves an `\xt` target's text ("Malachi 3:1") to an anchor name;
    /// the span is then emitted as `#u.xt("MAL-3-1")[…]`.
    pub xt: Option<&'h dyn Fn(&str) -> Option<String>>,
    /// After `\id` and identification: (book).
    pub book_start: Option<&'h dyn Fn(&str) -> Option<String>>,
    /// After `\c`: (book, chapter).
    pub chapter_start: Option<&'h dyn Fn(&str, &str) -> Option<String>>,
    /// After a paragraph that held verses: (book, [(chapter, verse)…]).
    pub after_paragraph: Option<&'h dyn Fn(&str, &[(String, String)]) -> Option<String>>,
}

/// One book's markup and what the template needs to know about it.
pub struct Book {
    pub code: String,
    pub name: String,
    pub markup: String,
}

/// Lexes, builds and folds one USFM book into Typst markup.
pub fn markup(usfm: &str) -> Book {
    markup_with(usfm, &Hooks::default())
}

/// [`markup`] with a recipe's hooks.
pub fn markup_with(usfm: &str, hooks: &Hooks) -> Book {
    let tokens = lex(usfm);
    let cst = build(&tokens);
    let mut e = Emitter {
        src: usfm,
        tokens: &tokens,
        cst: &cst,
        out: String::with_capacity(usfm.len() * 3 / 2),
        code: String::new(),
        name: String::new(),
        chapter: String::new(),
        pending_chapter: None,
        pending_inline: String::new(),
        space: false,
        hooks,
        para_verses: Vec::new(),
    };
    e.scan_identification(0);
    let mut out = format!(
        "#u.book(\"{}\", \"{}\");\n",
        str_lit(&e.code),
        str_lit(&e.name)
    );
    if let Some(hook) = hooks.book_start
        && let Some(extra) = hook(&e.code)
    {
        out.push_str(&extra);
        out.push('\n');
    }
    e.block_children(0);
    out.push_str(&e.out);
    Book {
        code: e.code,
        name: e.name,
        markup: out,
    }
}

struct Emitter<'a> {
    hooks: &'a Hooks<'a>,
    /// The verses the current paragraph has opened, for `after_paragraph`.
    para_verses: Vec<(String, String)>,
    src: &'a str,
    tokens: &'a [Token],
    cst: &'a Cst,
    out: String,
    code: String,
    name: String,
    /// The chapter verses are numbered in, for the running head.
    chapter: String,
    /// A `\c` seen but not yet handed to a paragraph.
    pending_chapter: Option<String>,
    /// Inline content (a verse number) met outside any paragraph, prepended to
    /// the next one.
    pending_inline: String,
    /// A space is owed before the next text: whitespace folds to one space and
    /// is dropped at a paragraph's edges.
    space: bool,
}

#[derive(Clone, Copy)]
enum Child {
    Token(u32),
    Node(u32),
}

impl<'a> Emitter<'a> {
    fn children(&self, node: u32) -> impl Iterator<Item = Child> + 'a {
        let cst = self.cst;
        cst.nodes[node as usize].children.clone().map(move |slot| {
            let id = cst.child_ids[slot as usize];
            if id & NODE_BIT != 0 {
                Child::Node(id & !NODE_BIT)
            } else {
                Child::Token(id)
            }
        })
    }

    fn span(&self, t: u32) -> &'a str {
        let t = self.tokens[t as usize];
        &self.src[t.start as usize..t.end() as usize]
    }

    /// `\+wj* ` → `wj`.
    fn marker_name(&self, t: u32) -> &'a str {
        self.span(t)
            .trim_end()
            .trim_start_matches('\\')
            .trim_start_matches('+')
            .trim_end_matches('*')
    }

    /// The name with its number stripped, so `q2` is `("q", 2)`.
    fn style(&self, t: u32) -> (&'a str, u8) {
        let name = self.marker_name(t);
        let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
        (base, self.tokens[t as usize].level)
    }

    fn node_token(&self, node: u32) -> u32 {
        self.cst.nodes[node as usize].token
    }

    fn text_of(&self, node: u32) -> String {
        let mut s = String::new();
        for child in self.children(node) {
            if let Child::Token(t) = child
                && self.tokens[t as usize].kind() == TokenKind::Text
            {
                s.push_str(self.span(t));
            }
        }
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// `\id`'s book code and the name the running head uses: `\h`, else
    /// `\toc2`, else `\toc1`, else the code.
    fn scan_identification(&mut self, root: u32) {
        let (mut h, mut toc2, mut toc1) = (None, None, None);
        for child in self.children(root) {
            let Child::Node(n) = child else { continue };
            let t = self.node_token(n);
            if t == u32::MAX {
                continue;
            }
            match self.marker_name(t) {
                "id" => {
                    for c in self.children(n) {
                        if let Child::Token(ct) = c
                            && self.tokens[ct as usize].kind() == TokenKind::BookCode
                        {
                            self.code = self.span(ct).trim().to_string();
                        }
                    }
                }
                "h" => h = Some(self.text_of(n)),
                "toc2" => toc2 = Some(self.text_of(n)),
                "toc1" => toc1 = Some(self.text_of(n)),
                _ => {}
            }
        }
        self.name = h
            .or(toc2)
            .or(toc1)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.code.clone());
    }

    /// Children of the root, a sidebar or a periph: paragraphs and the points
    /// between them.
    fn block_children(&mut self, node: u32) {
        let kids: Vec<Child> = self.children(node).collect();
        let mut i = 0;
        while i < kids.len() {
            match kids[i] {
                Child::Node(n) => self.block_node(n),
                Child::Token(t) => {
                    let tok = self.tokens[t as usize];
                    if tok.kind() == (TokenKind::Marker { nested: false }) {
                        match generated::kind(tok.marker_idx) {
                            MarkerKind::Chapter => {
                                let number = self.designator_after(&kids, i);
                                self.chapter = number.clone();
                                self.out
                                    .push_str(&format!("#u.c(\"{}\");\n", str_lit(&number)));
                                if let Some(hook) = self.hooks.chapter_start
                                    && let Some(extra) = hook(&self.code, &number)
                                {
                                    self.out.push_str(&extra);
                                    self.out.push('\n');
                                }
                                self.pending_chapter = Some(number);
                            }
                            MarkerKind::Verse => {
                                let number = self.designator_after(&kids, i);
                                let mut saved = std::mem::take(&mut self.out);
                                self.verse(&number);
                                std::mem::swap(&mut saved, &mut self.out);
                                self.pending_inline.push_str(&saved);
                            }
                            _ => {}
                        }
                    }
                }
            }
            i += 1;
        }
    }

    fn designator_after(&self, kids: &[Child], i: usize) -> String {
        match kids.get(i + 1) {
            Some(Child::Token(d)) if self.tokens[*d as usize].kind() == TokenKind::Designator => {
                self.span(*d).trim().to_string()
            }
            _ => String::new(),
        }
    }

    fn block_node(&mut self, n: u32) {
        let t = self.node_token(n);
        let idx = self.tokens[t as usize].marker_idx;
        let kind = generated::kind(idx);
        let category = generated::category(idx);
        match kind {
            MarkerKind::Header | MarkerKind::Meta => {}
            MarkerKind::Paragraph if category == Category::ParaIdentification => {}
            MarkerKind::Sidebar | MarkerKind::Periph => self.block_children(n),
            MarkerKind::Paragraph | MarkerKind::TableRow | MarkerKind::Unknown => {
                self.paragraph(n, t, category)
            }
            // A character or note node at block level: wrap it like `\p` would.
            _ => self.paragraph(n, t, category),
        }
    }

    fn paragraph(&mut self, n: u32, t: u32, category: Category) {
        let (style, level) = self.style(t);
        let prose = matches!(
            category,
            Category::ParaBody | Category::ParaPoetry | Category::ParaLists
        ) && style != "b";
        let chapter = if prose {
            self.pending_chapter.take()
        } else {
            None
        };
        self.out
            .push_str(&format!("#u.para(\"{}\", {}", str_lit(style), level));
        if let Some(c) = chapter {
            self.out.push_str(&format!(", c: \"{}\"", str_lit(&c)));
        }
        self.out.push_str(")[");
        let prefix = std::mem::take(&mut self.pending_inline);
        self.out.push_str(&prefix);
        self.space = false;
        self.inline_children(n);
        self.space = false;
        self.out.push_str("];\n");
        if !self.para_verses.is_empty() {
            let verses = std::mem::take(&mut self.para_verses);
            if let Some(hook) = self.hooks.after_paragraph
                && let Some(extra) = hook(&self.code, &verses)
            {
                self.out.push_str(&extra);
                self.out.push('\n');
            }
        }
    }

    fn verse(&mut self, number: &str) {
        self.flush_space();
        self.out.push_str(&format!(
            "#u.v(\"{}\", \"{}\", \"{}\");",
            str_lit(&self.code),
            str_lit(&self.chapter),
            str_lit(number)
        ));
        self.para_verses.push((self.chapter.clone(), number.to_string()));
    }

    fn flush_space(&mut self) {
        if self.space {
            self.out.push(' ');
            self.space = false;
        }
    }

    /// The inline content of a paragraph, character span or note. The node's
    /// own opening marker, caller, attributes and closer are skipped.
    fn inline_children(&mut self, node: u32) {
        let kids: Vec<Child> = self.children(node).collect();
        for (i, child) in kids.iter().enumerate() {
            match *child {
                Child::Node(n) => self.inline_node(n),
                Child::Token(t) => {
                    let tok = self.tokens[t as usize];
                    match tok.kind() {
                        TokenKind::Text => self.text(self.span(t)),
                        TokenKind::Newline | TokenKind::Pad | TokenKind::OptBreak => {
                            self.space = !self.out.ends_with('[');
                        }
                        TokenKind::Marker { .. }
                            if generated::kind(tok.marker_idx) == MarkerKind::Verse =>
                        {
                            let number = self.designator_after(&kids, i);
                            self.verse(&number);
                        }
                        _ => {}
                    }
                }
            }
        }
        // A space owed at a character span's end is owed to its parent:
        // `\ft see \+xt John` keeps "see John". `paragraph` drops it at the
        // paragraph's end.
    }

    fn inline_node(&mut self, n: u32) {
        let t = self.node_token(n);
        let tok = self.tokens[t as usize];
        let kind = generated::kind(tok.marker_idx);
        match kind {
            MarkerKind::Note => {
                let caller = self
                    .children(n)
                    .find_map(|c| match c {
                        Child::Token(ct)
                            if self.tokens[ct as usize].kind() == TokenKind::NoteCaller =>
                        {
                            Some(self.span(ct).trim().to_string())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| "+".into());
                let (style, _) = self.style(t);
                self.out.push_str(&format!(
                    "#u.n(\"{}\", \"{}\")[",
                    str_lit(style),
                    str_lit(&caller)
                ));
                // A note hangs on the word before it: a space owed before
                // the note is dropped.
                self.space = false;
                self.inline_children(n);
                self.space = false;
                self.out.push_str("];");
            }
            MarkerKind::Milestone | MarkerKind::Figure => {}
            // `\ca`/`\va`/`\cp`/`\vp` are alternates the print does not show.
            _ if generated::category(tok.marker_idx) == Category::ChapterVerse => {}
            // A paragraph node can only nest here by recovery; keep its text.
            _ => {
                let (style, level) = self.style(t);
                let name = if level > 0 && kind == MarkerKind::TableCell {
                    self.marker_name(t)
                } else {
                    style
                };
                self.flush_space();
                let target = match self.hooks.xt {
                    Some(resolve) if name == "xt" => resolve(&self.text_of(n)),
                    _ => None,
                };
                match target {
                    Some(t) => self.out.push_str(&format!("#u.xt(\"{}\")[", str_lit(&t))),
                    None => self
                        .out
                        .push_str(&format!("#u.ch(\"{}\")[", str_lit(name))),
                }
                self.inline_children(n);
                self.out.push_str("];");
            }
        }
    }

    fn text(&mut self, text: &str) {
        let mut words = text.split(char::is_whitespace).peekable();
        let leading = text.starts_with(char::is_whitespace);
        let trailing = text.ends_with(char::is_whitespace);
        if leading && !self.out.ends_with('[') {
            self.space = true;
        }
        let mut first = true;
        while let Some(word) = words.next() {
            if word.is_empty() {
                continue;
            }
            if !first {
                self.space = true;
            }
            first = false;
            self.flush_space();
            escape_into(&mut self.out, word);
        }
        if trailing {
            self.space = true;
        }
    }
}

/// [`escape_into`] into a new string.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    escape_into(&mut out, text);
    out
}

/// Escapes a run of text for Typst markup. Quotes stay bare so Typst's smart
/// quotes apply; everything with markup meaning is backslash-escaped.
pub fn escape_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '[' | ']' | '=' | '-' | '+'
                | '/' | '~' | '.' | ':'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
}

/// Escapes a value for a Typst string literal.
pub fn str_lit(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
