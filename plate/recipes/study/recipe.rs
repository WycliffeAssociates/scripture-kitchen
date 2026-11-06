//! A classic study Bible page: the text above, the notes for the verses on
//! that page below a rule in small type. Each note is a Typst footnote with no
//! caller, anchored at its verse, so Typst keeps it on its verse's page,
//! shortens the text to make room, and carries an overflow to the next page.
//!
//! Notes come from an unfoldingWord-style TSV. At `sample` density (the
//! default) each verse gets its first note's first paragraph, about the
//! density of a printed study Bible; `all` takes every note.

use std::collections::HashMap;

#[path = "../notes/recipe.rs"]
#[allow(dead_code)]
mod notes;

pub fn run(
    fonts: &std::sync::Arc<plate::world::FontStore>,
    usfm: &str,
    tsv: &str,
    density: &str,
    settings: &str,
) -> Result<(Vec<u8>, plate::world::Timings), String> {
    let t = web_time::Instant::now();
    let mut by_verse: HashMap<String, Vec<String>> = HashMap::new();
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            continue;
        }
        let Some((c, v)) = cols[0].split_once(':') else { continue };
        if c == "front" || v == "intro" {
            continue;
        }
        let key = format!("{c}:{}", v.split(['-', ',']).next().unwrap());
        let text = cols[6].replace("\\n", "\n").replace("<br>", "\n");
        let entry = by_verse.entry(key).or_default();
        if density == "all" {
            entry.push(text);
        } else if entry.is_empty() {
            entry.push(text.split("\n\n").next().unwrap_or("").to_string());
        }
    }
    let at_verse = |_: &str, c: &str, v: &str| -> Option<String> {
        let key = format!("{c}:{v}");
        let list = by_verse.get(&key)?;
        let body: String = list.iter().map(|md| notes::markdown(md)).collect::<Vec<_>>().join(" ");
        Some(format!("#u.sn(\"{}\")[{body}];", plate::str_lit(&key)))
    };
    let hooks = plate::Hooks { at_verse: Some(&at_verse), ..Default::default() };
    let book = plate::markup_with(usfm, &hooks);
    let prep = t.elapsed().as_secs_f64() * 1e3;
    plate::recipe::compile(fonts, "study", &[book], "", "", vec![], settings, prep)
}
