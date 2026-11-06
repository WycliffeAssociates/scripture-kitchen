//! recipe-links BOOK.usfm… — a contents page, PDF bookmarks for books and
//! section headings, and every `\xt` ("Malachi 3:1") a link to that verse
//! when the verse is in the document.

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

pub fn run(
    fonts: &std::sync::Arc<plate::world::FontStore>,
    usfm: &[String],
    settings: &str,
) -> Result<(Vec<u8>, plate::world::Timings), String> {
    let t = web_time::Instant::now();
    let hooks = plate::Hooks { xt: Some(&resolve), ..Default::default() };
    let books: Vec<plate::Book> = usfm.iter().map(|u| plate::markup_with(u, &hooks)).collect();
    let prep = t.elapsed().as_secs_f64() * 1e3;
    plate::recipe::compile(fonts, "links", &books, "#u.contents()", "", vec![], settings, prep)
}
