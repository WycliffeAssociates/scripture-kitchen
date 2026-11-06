//! recipe-diff OLD.usfm NEW.usfm [side|redline] [OLD_LABEL NEW_LABEL] — a
//! printable comparison, verse by verse, word by word (`similar`), with runs
//! of unchanged verses collapsed to one line. `side` sets old and new in two
//! columns; `redline` reads straight through with deletions struck and
//! insertions underlined.

use std::collections::HashMap;

use similar::{ChangeTag, TextDiff};
use usfm_onion::cst::build;
use usfm_onion::{Filter, lex, mask, toc, vref};

fn verses(usfm: &str) -> Vec<(String, String)> {
    let tokens = lex(usfm);
    let cst = build(&tokens);
    let t = toc(usfm.as_bytes(), &tokens);
    let m = mask(usfm.as_bytes(), &tokens, &cst, &Filter::verse_text());
    vref::joined(&t, &m, usfm.as_bytes(), true)
        .lines()
        .filter_map(|line| {
            let (key, text) = line.split_once('\t')?;
            Some((key.split_once(' ')?.1.to_string(), text.to_string()))
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let old = verses(&std::fs::read_to_string(&args[0]).unwrap());
    let new_src = std::fs::read_to_string(&args[1]).unwrap();
    let new: HashMap<String, String> = verses(&new_src).into_iter().collect();
    let layout = args.get(2).cloned().unwrap_or("side".into());
    let labels = (args.get(3).cloned().unwrap_or("Old".into()), args.get(4).cloned().unwrap_or("New".into()));
    let fonts = plate::world::fonts(&[]);
    let t = web_time::Instant::now();
    let book = plate::markup(&new_src);

    let (mut changed, mut same, mut added, mut removed) = (0, 0, 0, 0);
    let mut body = String::new();
    let mut run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, body: &mut String| {
        if !run.is_empty() {
            body.push_str(&format!("#u.same(\"{}\", {});\n", plate::str_lit(&format!("{}–{}", run[0], run[run.len() - 1])), run.len()));
            run.clear();
        }
    };
    for (cv, a) in &old {
        let b = new.get(cv).cloned().unwrap_or_default();
        if *a == b {
            same += 1;
            run.push(cv.clone());
            continue;
        }
        flush(&mut run, &mut body);
        changed += 1;
        let diff = TextDiff::from_words(a.as_str(), b.as_str());
        let (mut left, mut right, mut inline) = (String::new(), String::new(), String::new());
        for change in diff.iter_all_changes() {
            let text = plate::escape(change.value());
            match change.tag() {
                ChangeTag::Equal => {
                    left.push_str(&text);
                    right.push_str(&text);
                    inline.push_str(&text);
                }
                ChangeTag::Delete => {
                    removed += 1;
                    left.push_str(&format!("#u.del[{text}];"));
                    inline.push_str(&format!("#u.del[{text}];"));
                }
                ChangeTag::Insert => {
                    added += 1;
                    right.push_str(&format!("#u.ins[{text}];"));
                    inline.push_str(&format!("#u.ins[{text}];"));
                }
            }
        }
        let cv = plate::str_lit(cv);
        if layout == "redline" {
            body.push_str(&format!("#u.redline(\"{cv}\")[{inline}];\n"));
        } else {
            body.push_str(&format!("#u.row(\"{cv}\")[{left}][{right}];\n"));
        }
    }
    flush(&mut run, &mut body);
    let markup = format!(
        "#u.book(\"{}\", \"{}\");\n#u.summary(\"{}\", \"{}\", {changed}, {same}, {removed}, {added}, \"{layout}\");\n{body}",
        plate::str_lit(&book.code), plate::str_lit(&book.name),
        plate::str_lit(&labels.0), plate::str_lit(&labels.1)
    );
    let prep = t.elapsed().as_secs_f64() * 1e3;
    eprintln!("{changed} verses changed, {same} unchanged · {removed} words out, {added} in");
    let doc = plate::Book { code: book.code, name: book.name, markup };
    plate::recipe::report(
        &format!("diff-{layout}"),
        plate::recipe::compile(&fonts, "diff", &[doc], "", "", vec![], r#"{"columns":1,"running_head":"book"}"#, prep),
    );
}
