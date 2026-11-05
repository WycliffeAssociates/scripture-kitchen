//! What a save changed, said in the book's terms: a replayed commit's subject.
//!
//! ```text
//! changes: 05/01.txt "\v 1 … \v 2-3 … \v 11 …", 05/title.txt     → "ROM 5:1–11; titles"
//! changes: 05/01.txt "no verse marker"                           → "ROM 5 (no verse number)"
//! changes: eight chunks of chapter 1                             → "TIT 1:1, 1:2, 1:3, 1:4, 1:5, 1:6, +2 more"
//! ```
//!
//! Ported from Sefer's `versesOf`/`summaryOf` (`writerHistory.ts` at
//! `326f02c8`), keeping its regex's semantics: a verse is `\v`, JavaScript
//! whitespace, ASCII digits, and an optional `-digits` bridge.

use crate::stitch::is_js_space;

/// Each `/\\v\s+(\d+)(?:-(\d+))?/g` match in `chunk`, as `(from, to)`: `to`
/// is `from` when there's no bridge. A number past `u64` saturates.
pub(crate) fn verse_markers(chunk: &str) -> impl Iterator<Item = (u64, u64)> + '_ {
    let mut rest = chunk;
    std::iter::from_fn(move || {
        loop {
            let at = rest.find("\\v")?;
            rest = &rest[at + 2..];
            let after_space = rest.trim_start_matches(is_js_space);
            if after_space.len() == rest.len() {
                continue; // `\va`, `\vp`, or `\v` with nothing after it
            }
            let (from, after_from) = digits(after_space);
            let Some(from) = from else { continue };
            let (to, after_to) = match after_from.strip_prefix('-').map(digits) {
                Some((Some(to), after_to)) => (to, after_to),
                _ => (from, after_from),
            };
            rest = after_to;
            return Some((from, to));
        }
    })
}

/// The ASCII digits `text` starts with, as a number, and what follows them.
fn digits(text: &str) -> (Option<u64>, &str) {
    let end = text.bytes().take_while(u8::is_ascii_digit).count();
    if end == 0 {
        return (None, text);
    }
    let value = text.as_bytes()[..end].iter().fold(0u64, |n, b| {
        n.saturating_mul(10).saturating_add(u64::from(b - b'0'))
    });
    (Some(value), &text[end..])
}

/// `5:1–11`: the chapter and the verses a chunk holds, lowest start to
/// highest end. A chunk with no verse marker says so, rather than a bare `5`
/// that reads as a verse.
pub fn verses_of(chapter: u64, chunk: &str) -> String {
    let (low, high) = verse_markers(chunk).fold((None, None), |(low, high), (from, to)| {
        (
            Some(low.map_or(from, |l: u64| l.min(from))),
            Some(high.map_or(to, |h: u64| h.max(to))),
        )
    });
    match (low, high) {
        (Some(low), Some(high)) if low == high => format!("{chapter}:{low}"),
        (Some(low), Some(high)) => format!("{chapter}:{low}–{high}"),
        _ => format!("{chapter} (no verse number)"),
    }
}

/// Verses a chunk holds, a bridge `1-3` counted as three.
pub(crate) fn verse_count(chunk: &str) -> u64 {
    verse_markers(chunk)
        .map(|(from, to)| to.saturating_sub(from).saturating_add(1))
        .sum()
}

/// `/^\d{2,3}\/\d{2,3}\.txt$/`: a chunk, not a title.
fn is_chunk_path(path: &str) -> bool {
    let two_or_three =
        |s: &str| (2..=3).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    path.split_once('/').is_some_and(|(chapter, file)| {
        two_or_three(chapter) && file.strip_suffix(".txt").is_some_and(two_or_three)
    })
}

/// What a save did to its book: `ROM 5:1–11, 6:4–6; titles`.
///
/// `changes` is the save's files and their new text; `before` gives a
/// deleted chunk's text as it was, so its verses can still be named. Chunks
/// are listed in path order, six at most, then `+N more`; any changed
/// `title.txt` adds `titles`.
pub fn summary_of<'a>(
    book: &str,
    changes: &[(String, Option<String>)],
    before: impl Fn(&str) -> Option<&'a str>,
) -> String {
    let mut chunks: Vec<&(String, Option<String>)> = changes
        .iter()
        .filter(|(path, _)| is_chunk_path(path))
        .collect();
    chunks.sort_by(|a, b| a.0.cmp(&b.0)); // ASCII paths: JavaScript's sort() order
    let where_: Vec<String> = chunks
        .iter()
        .map(|(path, text)| {
            let chapter = path
                .split('/')
                .next()
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            let text = text.as_deref().or_else(|| before(path)).unwrap_or("");
            verses_of(chapter, text)
        })
        .collect();
    let shown = if where_.len() > 6 {
        format!("{}, +{} more", where_[..6].join(", "), where_.len() - 6)
    } else {
        where_.join(", ")
    };
    let titles = changes.iter().any(|(path, _)| path.ends_with("title.txt"));
    let said: Vec<&str> = [shown.as_str(), if titles { "titles" } else { "" }]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    if said.is_empty() {
        book.to_owned()
    } else {
        format!("{book} {}", said.join("; "))
    }
}

#[cfg(test)]
mod tests;
