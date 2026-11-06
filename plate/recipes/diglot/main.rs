//! recipe-diglot LEFT.usfm RIGHT.usfm [LEFT_LABEL RIGHT_LABEL] — two texts
//! side by side, one row per paragraph of the left text, verses matched by
//! number. Verse text comes from onion's verse-text mask (`vref`), so notes
//! and headings drop out; a real diglot would keep them and map
//! versifications through canon.

use std::collections::HashMap;

use usfm_onion::cst::build;
use usfm_onion::{Filter, lex, mask, toc, vref};

/// "C:V" → verse text.
fn verse_text(usfm: &str) -> HashMap<String, String> {
    let tokens = lex(usfm);
    let cst = build(&tokens);
    let t = toc(usfm.as_bytes(), &tokens);
    let m = mask(usfm.as_bytes(), &tokens, &cst, &Filter::verse_text());
    vref::joined(&t, &m, usfm.as_bytes(), true)
        .lines()
        .filter_map(|line| {
            let (key, text) = line.split_once('\t')?;
            let cv = key.split_once(' ')?.1;
            Some((cv.to_string(), text.to_string()))
        })
        .collect()
}

fn side(texts: &HashMap<String, String>, verses: &[(String, String)]) -> String {
    let mut s = String::new();
    for (c, v) in verses {
        let Some(text) = texts.get(&format!("{c}:{v}")) else { continue };
        if text == "<range>" {
            continue;
        }
        s.push_str(&format!("#u.vn(\"{}\", \"{}\");", plate::str_lit(c), plate::str_lit(v)));
        plate::escape_into(&mut s, text);
        s.push(' ');
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (left, right) = (std::fs::read_to_string(&args[0]).unwrap(), std::fs::read_to_string(&args[1]).unwrap());
    let labels = (args.get(2).cloned().unwrap_or("Left".into()), args.get(3).cloned().unwrap_or("Right".into()));
    let fonts = plate::world::fonts(&[]);
    let t = web_time::Instant::now();
    let (lt, rt) = (verse_text(&left), verse_text(&right));

    // The left text's paragraphs decide the rows.
    let groups = std::cell::RefCell::new(Vec::<Vec<(String, String)>>::new());
    let collect = |_: &str, verses: &[(String, String)]| -> Option<String> {
        groups.borrow_mut().push(verses.to_vec());
        None
    };
    let hooks = plate::Hooks { after_paragraph: Some(&collect), ..Default::default() };
    let book = plate::markup_with(&left, &hooks);

    let mut markup = format!(
        "#u.book(\"{}\", \"{}\");\n#u.labels(\"{}\", \"{}\");\n",
        plate::str_lit(&book.code), plate::str_lit(&book.name),
        plate::str_lit(&labels.0), plate::str_lit(&labels.1)
    );
    let mut chapter = String::new();
    for verses in groups.borrow().iter() {
        let c = &verses[0].0;
        if *c != chapter {
            markup.push_str(&format!("#u.chapter-row(\"{}\");\n", plate::str_lit(c)));
            chapter = c.clone();
        }
        markup.push_str(&format!("#u.pair[{}][{}];\n", side(&lt, verses), side(&rt, verses)));
    }
    let rows = groups.borrow().len();
    let doc = plate::Book { code: book.code, name: book.name, markup };
    let prep = t.elapsed().as_secs_f64() * 1e3;
    eprintln!("{rows} paired rows");
    plate::recipe::report(
        "diglot",
        plate::recipe::compile(&fonts, "diglot", &[doc], "", "", vec![], r#"{"columns":1}"#, prep),
    );
}
