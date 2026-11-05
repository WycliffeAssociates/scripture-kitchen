//! An OpenDocument text into the same [`Reading`] a .docx gives.
//!
//! ```text
//! content.xml   office:text: the body, notes and comments inline where they sit;
//!               office:automatic-styles: anonymous "P3", "T1"
//! styles.xml    the named styles ("USFM q1", "Quote")
//! ```
//!
//! An automatic style's name is never what a person chose, so a paragraph or
//! span takes the name of its nearest named ancestor.

use std::collections::BTreeMap;

use crate::comments::Comment;
use crate::dom::{Element, Node};
use crate::reading::{Align, Formatting, Inline, Note, NoteKind, Paragraph, Reading, push_text, role_of};

pub const MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

#[derive(Debug, Clone, Default)]
struct StyleProps {
    automatic: bool,
    name: Option<String>,
    parent: Option<String>,
    indent: Option<f64>,
    align: Option<Align>,
    outline: Option<u32>,
    italic: Option<bool>,
    bold: Option<bool>,
    superscript: Option<bool>,
}

struct Styles {
    by_name: BTreeMap<String, StyleProps>,
}

impl Styles {
    fn read(roots: &[Option<&Element>]) -> Self {
        let mut by_name = BTreeMap::new();
        for root in roots.iter().flatten() {
            collect(root, false, &mut by_name);
        }
        Self { by_name }
    }

    /// The style with its ancestors' properties under it; `name` is the
    /// nearest named one's.
    fn resolve(&self, name: Option<&str>) -> StyleProps {
        let mut chain = Vec::new();
        let mut at = name.map(str::to_owned);
        while let Some(n) = at.take() {
            let Some(own) = self.by_name.get(&n) else { break };
            if chain.len() > 20 {
                break;
            }
            chain.push(own);
            at = own.parent.clone();
        }
        let mut merged = StyleProps::default();
        for own in chain.iter().rev() {
            merged = StyleProps {
                automatic: own.automatic,
                name: if own.automatic { merged.name.clone() } else { own.name.clone() },
                parent: None,
                indent: own.indent.or(merged.indent),
                align: own.align.or(merged.align),
                outline: own.outline.or(merged.outline),
                italic: own.italic.or(merged.italic),
                bold: own.bold.or(merged.bold),
                superscript: own.superscript.or(merged.superscript),
            };
        }
        merged
    }
}

fn collect(e: &Element, automatic: bool, out: &mut BTreeMap<String, StyleProps>) {
    for c in e.elements() {
        if c.name == "style:style" {
            let Some(name) = c.attr("style:name") else { continue };
            let para = c.child("style:paragraph-properties");
            let text = c.child("style:text-properties");
            let mut p = StyleProps {
                automatic,
                name: Some(c.attr("style:display-name").unwrap_or(name).to_owned()),
                parent: c.attr("style:parent-style-name").map(str::to_owned),
                ..StyleProps::default()
            };
            p.indent = para.and_then(|x| x.attr("fo:margin-left")).and_then(points);
            p.align = para.and_then(|x| x.attr("fo:text-align")).map(|a| match a {
                "center" => Align::Center,
                "end" | "right" => Align::End,
                "justify" => Align::Justify,
                _ => Align::Start,
            });
            p.outline = c.attr("style:default-outline-level").and_then(|o| o.parse().ok());
            p.italic = text.and_then(|t| t.attr("fo:font-style")).map(|s| s == "italic" || s == "oblique");
            p.bold = text
                .and_then(|t| t.attr("fo:font-weight"))
                .map(|w| w == "bold" || w.parse::<f64>().is_ok_and(|n| n >= 600.0));
            // "super 58%" or "33% 58%"
            p.superscript = text.and_then(|t| t.attr("style:text-position")).map(|pos| {
                pos.starts_with("super") || leading_number(pos).is_some_and(|n| n > 0.0)
            });
            out.insert(name.to_owned(), p);
        } else {
            collect(c, automatic || c.name == "office:automatic-styles", out);
        }
    }
}

/// `parseFloat`: the number a string starts with.
fn leading_number(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+'))))
        .map_or(s.len(), |(i, _)| i);
    s[..end].parse().ok()
}

/// An ODF length such as "0.5in" or "1.27cm", in points.
pub fn points(length: &str) -> Option<f64> {
    let t = length.trim();
    let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
    let n: f64 = t[..split].trim().parse().ok()?;
    let unit = match &t[split..] {
        "" | "pt" => 1.0,
        "in" => 72.0,
        "cm" => 72.0 / 2.54,
        "mm" => 72.0 / 25.4,
        "pc" => 12.0,
        "px" => 0.75,
        _ => return None,
    };
    Some(n * unit)
}

struct Ctx {
    styles: Styles,
    paragraphs: Vec<Paragraph>,
    notes: BTreeMap<String, Note>,
    comments: Vec<Comment>,
}

#[derive(Clone, Copy)]
struct Where {
    list: bool,
    table: bool,
}

pub fn read(content: &Element, styles: Option<&Element>) -> Option<Reading> {
    let text = find(content, "office:text")?;
    let mut ctx = Ctx {
        styles: Styles::read(&[styles, Some(content)]),
        paragraphs: Vec::new(),
        notes: BTreeMap::new(),
        comments: Vec::new(),
    };
    let mut paragraphs = Vec::new();
    blocks(text, &mut ctx, Where { list: false, table: false }, &mut paragraphs);
    ctx.paragraphs = paragraphs;
    Some(Reading {
        format: crate::reading::Format::Odt,
        paragraphs: ctx.paragraphs,
        notes: ctx.notes,
        comments: ctx.comments,
        tracked_changes: count(content, "text:changed-region"),
        properties: Vec::new(),
    })
}

/// `meta.xml`'s user-defined properties: where Writer keeps a .docx's
/// custom document properties.
pub fn properties(meta: &Element) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_user_defined(meta, &mut out);
    out
}

fn collect_user_defined(e: &Element, out: &mut Vec<(String, String)>) {
    for c in e.elements() {
        if c.name == "meta:user-defined" {
            if let Some(name) = c.attr("meta:name") {
                out.push((name.to_owned(), c.text()));
            }
        } else {
            collect_user_defined(c, out);
        }
    }
}

fn find<'a>(e: &'a Element, name: &str) -> Option<&'a Element> {
    if e.name == name {
        return Some(e);
    }
    e.elements().find_map(|c| find(c, name))
}

fn count(e: &Element, name: &str) -> usize {
    usize::from(e.name == name) + e.elements().map(|c| count(c, name)).sum::<usize>()
}

fn blocks(e: &Element, ctx: &mut Ctx, at: Where, out: &mut Vec<Paragraph>) {
    for c in e.elements() {
        match c.name.as_str() {
            "text:p" | "text:h" => {
                let p = paragraph(c, ctx, at);
                out.push(p);
            }
            "text:list" => blocks(c, ctx, Where { list: true, ..at }, out),
            "table:table" => blocks(c, ctx, Where { table: true, ..at }, out),
            "text:tracked-changes" | "text:sequence-decls" | "text:variable-decls" | "office:forms" => {}
            _ => blocks(c, ctx, at, out),
        }
    }
}

fn paragraph(p: &Element, ctx: &mut Ctx, at: Where) -> Paragraph {
    let style = ctx.styles.resolve(p.attr("text:style-name"));
    let outline = (p.name == "text:h")
        .then(|| p.attr("text:outline-level").and_then(|o| o.parse().ok()).or(style.outline).unwrap_or(1));
    let (role, level) = role_of(style.name.as_deref(), outline);
    let format = Formatting {
        superscript: style.superscript.unwrap_or(false),
        italic: style.italic.unwrap_or(false),
        bold: style.bold.unwrap_or(false),
    };
    let mut inlines = Vec::new();
    inline(p, ctx, format, Formatting::default(), None, &mut inlines);
    Paragraph {
        style: style.name.clone(),
        role,
        level,
        indent: style.indent.unwrap_or(0.0),
        align: style.align,
        list: at.list,
        table: at.table,
        inlines,
    }
}

fn inline(
    e: &Element,
    ctx: &mut Ctx,
    format: Formatting,
    direct: Formatting,
    style: Option<&str>,
    out: &mut Vec<Inline>,
) {
    let text = |out: &mut Vec<Inline>, value: &str| push_text(out, value, style.map(str::to_owned), format, direct);
    for node in &e.children {
        let c = match node {
            // ODF collapses whitespace; explicit spaces are <text:s/>.
            Node::Text(t) => {
                let collapsed: String = collapse_ws(t);
                text(out, &collapsed);
                continue;
            }
            Node::Element(c) => c,
        };
        match c.name.as_str() {
            "text:s" => {
                let n = c.attr("text:c").and_then(|n| n.parse().ok()).unwrap_or(1);
                text(out, &" ".repeat(n));
            }
            "text:tab" => text(out, "\t"),
            "text:line-break" => out.push(Inline::Break),
            "text:span" => {
                let name = c.attr("text:style-name");
                let own = ctx.styles.resolve(name);
                let set = Formatting {
                    superscript: own.superscript.unwrap_or(format.superscript),
                    italic: own.italic.unwrap_or(format.italic),
                    bold: own.bold.unwrap_or(format.bold),
                };
                // A span's own formatting is direct only when its style is
                // automatic (Writer's "T1"): a named style is a choice of style.
                let raw = ctx.styles.by_name.get(name.unwrap_or(""));
                let automatic = raw.is_some_and(|r| r.automatic);
                let span_direct = if automatic {
                    Formatting {
                        superscript: own.superscript.unwrap_or(direct.superscript),
                        italic: own.italic.unwrap_or(direct.italic),
                        bold: own.bold.unwrap_or(direct.bold),
                    }
                } else {
                    direct
                };
                let named = own.name.clone();
                inline(c, ctx, set, span_direct, named.as_deref().or(style), out);
            }
            "text:note" => {
                let id = note(c, ctx);
                out.push(Inline::Note(id));
            }
            "office:annotation" => annotation(c, ctx, out),
            "office:annotation-end" => {
                if let Some(n) = c.attr("office:name") {
                    out.push(Inline::CommentEnd(n.to_owned()));
                }
            }
            "text:change-start" | "text:change-end" | "text:change" | "text:bookmark" | "text:bookmark-start"
            | "text:bookmark-end" | "text:soft-page-break" | "text:reference-mark" => {}
            _ => inline(c, ctx, format, direct, style, out),
        }
    }
}

fn collapse_ws(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut ws = false;
    for c in t.chars() {
        if c.is_whitespace() {
            if !ws {
                out.push(' ');
            }
            ws = true;
        } else {
            out.push(c);
            ws = false;
        }
    }
    out
}

fn note(e: &Element, ctx: &mut Ctx) -> String {
    let kind = if e.attr("text:note-class") == Some("endnote") { NoteKind::Endnote } else { NoteKind::Footnote };
    let prefix = if kind == NoteKind::Footnote { "fn" } else { "en" };
    let id = format!("{prefix}:{}", e.attr("text:id").map_or_else(|| ctx.notes.len().to_string(), str::to_owned));
    let mut paragraphs = Vec::new();
    if let Some(body) = e.child("text:note-body") {
        blocks(body, ctx, Where { list: false, table: false }, &mut paragraphs);
    }
    ctx.notes.insert(id.clone(), Note { kind, paragraphs });
    id
}

fn annotation(e: &Element, ctx: &mut Ctx, out: &mut Vec<Inline>) {
    let name = e.attr("office:name");
    let id = name.map_or_else(|| format!("odt-{}", ctx.comments.len()), str::to_owned);
    let text: Vec<String> = e.elements().filter(|c| c.name == "text:p").map(Element::text).collect();
    ctx.comments.push(Comment {
        id: id.clone(),
        author: find(e, "dc:creator").map(Element::text).unwrap_or_default(),
        date: find(e, "dc:date").map(Element::text),
        text: text.join("\n").trim().to_owned(),
        parent_id: None,
        resolved: e.attr("loext:resolved") == Some("true"),
    });
    out.push(Inline::CommentStart(id.clone()));
    // An unnamed annotation marks a point, not a range.
    if name.is_none() {
        out.push(Inline::CommentEnd(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_in_points() {
        assert_eq!(points("0.5in"), Some(36.0));
        assert_eq!(points("12pt"), Some(12.0));
        assert!((points("1.27cm").unwrap() - 36.0).abs() < 0.01);
        assert_eq!(points("wide"), None);
    }
}
