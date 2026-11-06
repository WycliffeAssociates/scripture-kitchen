//! recipe-links BOOK.usfm… — a contents page, PDF bookmarks for books and
//! section headings, and every `\xt` ("Malachi 3:1") a link to that verse
//! when the verse is in the document.

use std::path::PathBuf;

/// "Malachi 3:1", "1 Kings 2:3–4", "Psalm 2:7; 110:1" → "MAL-3-1". The first
/// chapter:verse wins; the book is everything before it, parsed by canon.
fn resolve(text: &str) -> Option<String> {
    let first = text.split(';').next()?.trim();
    let (book, cv) = first.rsplit_once(' ')?;
    let (c, v) = cv.split_once(':')?;
    let v: String = v.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if c.parse::<u32>().is_err() || v.is_empty() {
        return None;
    }
    let book = book.trim();
    let key = canon::parse_book(book)
        .or_else(|| canon::parse_book(book.trim_end_matches('s')))?;
    let code = String::from_utf8_lossy(&key.as_bytes()).into_owned();
    Some(format!("{code}-{c}-{v}"))
}

fn main() {
    let paths: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let fonts = plate::world::fonts(&[]);
    let t = web_time::Instant::now();
    let hooks = plate::Hooks { xt: Some(&resolve), ..Default::default() };
    let books: Vec<plate::Book> = paths
        .iter()
        .map(|p| plate::markup_with(&std::fs::read_to_string(p).unwrap(), &hooks))
        .collect();
    let prep = t.elapsed().as_secs_f64() * 1e3;
    let links = books.iter().map(|b| b.markup.matches("#u.xt(").count()).sum::<usize>();
    eprintln!("{links} cross references resolved");
    plate::recipe::report(
        "links",
        plate::recipe::compile(&fonts, "links", &books, "#u.contents()", "", vec![], "{}", prep),
    );
}
