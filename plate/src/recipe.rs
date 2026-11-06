//! What every recipe shares: its overlay template, read from
//! `recipes/<name>/<name>.typ` on each run, and one compile call.
//!
//! An overlay starts `#import "/usfm.typ": *` (so every base function and `s`
//! stay reachable as `u.…`) and redefines what it changes. Markup always calls
//! `u.…`, so a redefinition wins.

#![cfg(feature = "native")]

use std::path::Path;
use std::sync::Arc;

use crate::Book;
use crate::world::{Doc, FontStore, Timings, book_sources, compile_doc, template_path};

/// The [`Doc`] a recipe compiles: its overlay at `/recipe.typ`, the books
/// importing it, `before`/`after` around them.
pub fn doc(name: &str, books: &[Book], before: &str, after: &str, settings: &str, markup_ms: f64) -> Doc {
    let (includes, mut sources) = book_sources(books, "/recipe.typ");
    sources.push(("/recipe.typ".into(), overlay(name)));
    Doc {
        main: format!("#import \"/recipe.typ\" as u\n#show: u.setup\n{before}\n{includes}{after}\n"),
        sources,
        settings: settings.to_string(),
        markup_ms,
    }
}

/// The base template's text, read from disk.
pub fn template() -> String {
    std::fs::read_to_string(template_path()).unwrap()
}

pub fn overlay(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("recipes")
        .join(name)
        .join(format!("{name}.typ"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Compiles `books` under the recipe's overlay. `before` and `after` are
/// markup placed around the books in the main file (a contents page, say).
pub fn compile(
    fonts: &Arc<FontStore>,
    name: &str,
    books: &[Book],
    before: &str,
    after: &str,
    extra: Vec<(String, String)>,
    settings: &str,
    markup_ms: f64,
) -> Result<(Vec<u8>, Timings), String> {
    let (includes, mut sources) = book_sources(books, "/recipe.typ");
    sources.push(("/recipe.typ".into(), overlay(name)));
    sources.extend(extra);
    let doc = Doc {
        main: format!("#import \"/recipe.typ\" as u\n#show: u.setup\n{before}\n{includes}{after}\n"),
        sources,
        settings: settings.to_string(),
        markup_ms,
    };
    let template = std::fs::read_to_string(template_path()).map_err(|e| e.to_string())?;
    compile_doc(fonts, &doc, &template)
}

/// The usual end of a recipe binary: write the PDF and say where time went.
pub fn report(name: &str, result: Result<(Vec<u8>, Timings), String>) {
    match result {
        Ok((pdf, t)) => {
            let out = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("debug/recipes/{name}.pdf"));
            std::fs::create_dir_all(out.parent().unwrap()).unwrap();
            std::fs::write(&out, pdf).unwrap();
            println!(
                "{name}: {} pages · prep {:.0} ms · layout {:.0} ms · pdf {:.0} ms → {}",
                t.pages, t.markup_ms, t.layout_ms, t.pdf_ms, out.display()
            );
        }
        Err(e) => {
            eprintln!("{name} failed:\n{e}");
            std::process::exit(1);
        }
    }
}

/// `user` settings with `forced` laid over them: a recipe that needs one
/// column says so, whatever the form sent.
pub fn merge(user: &str, forced: &str) -> String {
    let mut base: serde_json::Value = serde_json::from_str(user).unwrap_or_else(|_| serde_json::json!({}));
    if let (Some(b), Ok(serde_json::Value::Object(f))) = (base.as_object_mut(), serde_json::from_str(forced)) {
        b.extend(f);
    }
    base.to_string()
}
