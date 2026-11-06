//! recipe-notes BOOK.usfm NOTES.tsv — a study edition: unfoldingWord-style
//! translation notes (TSV: Reference, ID, Tags, SupportReference, Quote,
//! Occurrence, Note) set after the paragraph holding their first verse, with
//! the book and chapter introductions where they belong. Note text is
//! Markdown, turned into Typst here.

use std::collections::HashMap;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

struct Note {
    support: String,
    markup: String,
}

/// Markdown → Typst markup, calling the overlay's `u.md-*` functions for
/// structure. Links keep their text; tN's relative and rc:// targets mean
/// nothing on paper.
pub fn markdown(md: &str) -> String {
    let mut out = String::new();
    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let n = match level { HeadingLevel::H1 => 1, HeadingLevel::H2 => 2, _ => 3 };
                out.push_str(&format!("#u.md-h({n})["));
            }
            Event::End(TagEnd::Heading(_)) => out.push_str("];\n"),
            Event::Start(Tag::Paragraph) => out.push_str("#u.md-p["),
            Event::End(TagEnd::Paragraph) => out.push_str("];\n"),
            Event::Start(Tag::Item) => out.push_str("#u.md-li["),
            Event::End(TagEnd::Item) => out.push_str("];\n"),
            Event::Start(Tag::Emphasis) => out.push_str("#emph["),
            Event::Start(Tag::Strong) => out.push_str("#strong["),
            Event::End(TagEnd::Emphasis | TagEnd::Strong) => out.push_str("];"),
            Event::Start(Tag::BlockQuote(_)) => out.push_str("#u.md-quote["),
            Event::End(TagEnd::BlockQuote(_)) => out.push_str("];\n"),
            Event::Text(t) | Event::Code(t) => plate::escape_into(&mut out, &t),
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push_str("#linebreak();"),
            _ => {}
        }
    }
    out
}

/// One column reads best for a study edition.
pub const FORCED: &str = r#"{"columns":1}"#;

pub fn run(
    fonts: &std::sync::Arc<plate::world::FontStore>,
    usfm: &str,
    tsv: &str,
    settings: &str,
) -> Result<(Vec<u8>, plate::world::Timings), String> {
    let settings = plate::recipe::merge(settings, FORCED);
    let t = web_time::Instant::now();

    // Reference → notes, keyed by the first verse of a range ("1:1-4" → 1:1).
    let mut by_ref: HashMap<String, Vec<Note>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            continue;
        }
        let reference = cols[0];
        let key = match reference.split_once(':') {
            Some((c, v)) => format!("{c}:{}", v.split(['-', ',']).next().unwrap()),
            None => continue,
        };
        let support = cols[3].rsplit('/').next().unwrap_or("").to_string();
        let text = cols[6].replace("\\n", "\n").replace("<br>", "\n");
        if !by_ref.contains_key(&key) {
            order.push(key.clone());
        }
        by_ref.entry(key).or_default().push(Note { support, markup: markdown(&text) });
    }
    let total: usize = by_ref.values().map(Vec::len).sum();

    let notes_for = |key: &str, verse: &str| -> Option<String> {
        let notes = by_ref.get(key)?;
        let mut s = format!("#u.notes(\"{}\")[", plate::str_lit(verse));
        for n in notes {
            s.push_str(&format!("#u.note(\"{}\")[{}];", plate::str_lit(&n.support), n.markup));
        }
        s.push_str("];");
        Some(s)
    };
    let book_start = |_: &str| -> Option<String> {
        let notes = by_ref.get("front:intro")?;
        Some(notes.iter().map(|n| format!("#u.intro[{}];", n.markup)).collect())
    };
    let chapter_start = |_: &str, c: &str| -> Option<String> {
        let notes = by_ref.get(&format!("{c}:intro"))?;
        Some(notes.iter().map(|n| format!("#u.intro[{}];", n.markup)).collect())
    };
    let after = |_: &str, verses: &[(String, String)]| -> Option<String> {
        let mut s = String::new();
        for (c, v) in verses {
            if let Some(block) = notes_for(&format!("{c}:{v}"), &format!("{c}:{v}")) {
                s.push_str(&block);
            }
        }
        (!s.is_empty()).then_some(s)
    };
    let hooks = plate::Hooks {
        book_start: Some(&book_start),
        chapter_start: Some(&chapter_start),
        after_paragraph: Some(&after),
        ..Default::default()
    };
    let book = plate::markup_with(usfm, &hooks);
    let prep = t.elapsed().as_secs_f64() * 1e3;
    eprintln!("{total} notes over {} references", order.len());
    plate::recipe::compile(fonts, "notes", &[book], "", "", vec![], &settings, prep)
}
