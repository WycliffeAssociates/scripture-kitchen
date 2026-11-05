//! One Writer book's files → its USFM.
//!
//! ```text
//! is_text_path("05/03.txt")          true      a chunk
//! is_text_path("05/title.txt")       true      a chapter's label
//! is_text_path("front/title.txt")    true      the book's title, one line per \mt
//! is_text_path("manifest.json")      false
//!
//! decode_text(b"\xEF\xBB\xBFa\r\nb")  Some("a\nb")   BOM dropped, line ends to \n
//! decode_text(b"\xFF")                None           not UTF-8: the file reads as absent
//!
//! tree_from_files([(path, text), …]) → WriterTree { title, chapters: [{ number, label, chunks }] }
//! book_to_usfm(&tree, ROM, "Romans") → "\id ROM\n\ide UTF-8\n…"
//! ```
//!
//! One template, and it copies structure only: the header usfm-tools'
//! `txt2USFM.py` writes, then the chapters and their chunks in numeric order.
//! It never repairs the translator's text: no verse-marker fixes, no `\p`
//! added or removed, no guessed headings.
//!
//! Sefer's `writerBook.ts` must write the same bytes, so where JavaScript and
//! Rust disagree on a primitive the JavaScript meaning is kept: `trim` and
//! `\s` are ECMAScript's whitespace (U+FEFF counts, U+0085 does not), `\d` is
//! ASCII digits only, and a list of files reads as a JS `Map` would.

use std::collections::BTreeMap;

use mise::books::BookKey;
use rustc_hash::FxHashMap;

/// A book's parts, in numeric order: what the template reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterTree {
    /// `front/title.txt`: one entry per non-empty line, each trimmed.
    pub title: Vec<String>,
    pub chapters: Vec<WriterChapter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterChapter {
    pub number: u32,
    /// `NN/title.txt`, when it says something other than the number.
    pub label: Option<String>,
    /// The chunk files' text as written, in chunk-number order.
    pub chunks: Vec<String>,
}

/// One book's files as `(path, bytes)` → its USFM. Files that aren't the
/// book's text, or aren't UTF-8, are left out. `fallback_title` stands in when
/// `front/title.txt` is missing or empty.
pub fn stitch<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    book: BookKey,
    fallback_title: &str,
) -> String {
    let decoded: Vec<(&str, String)> = files
        .into_iter()
        .filter(|(path, _)| is_text_path(path))
        .filter_map(|(path, bytes)| decode_text(bytes).map(|text| (path, text)))
        .collect();
    let tree = tree_from_files(decoded.iter().map(|(path, text)| (*path, text.as_str())));
    book_to_usfm(&tree, book, fallback_title)
}

/// ECMAScript's whitespace, as `String.prototype.trim` and a regex `\s` mean
/// it. Rust's `char::is_whitespace` differs on two code points: it counts
/// U+0085, which JavaScript does not, and not U+FEFF, which JavaScript does.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// `/^\d{2,3}$/`.
fn is_two_or_three_digits(name: &str) -> bool {
    (2..=3).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_digit())
}

/// A file's bytes as the text the template reads, or `None` when they aren't
/// UTF-8 (no repair).
///
/// - A leading byte order mark is dropped, and then a second one: Sefer strips
///   one itself and its `TextDecoder` drops another.
/// - Every `\r\n`, and every lone `\r`, becomes `\n`. A chunk is trimmed only
///   at its ends, so a CRLF inside it would otherwise reach the book.
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    const BOM: &[u8] = b"\xEF\xBB\xBF";
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let body = body.strip_prefix(BOM).unwrap_or(body);
    let text = std::str::from_utf8(body).ok()?;
    Some(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `01`, `150`; not `00`, which Writer uses for front matter.
fn numbered(name: &str) -> Option<u32> {
    if !is_two_or_three_digits(name) {
        return None;
    }
    let number: u32 = name.parse().ok()?;
    (number > 0).then_some(number)
}

/// `NN/title.txt` or `NN/NN.txt`, in any case, split into the chapter folder
/// and the file's stem: `^(\d{2,3})\/(title|\d{2,3})\.txt$` with `/i`.
fn split_chapter_path(path: &str) -> Option<(&str, &str)> {
    let (folder, file) = path.split_once('/')?;
    if !is_two_or_three_digits(folder) {
        return None;
    }
    if file.len() < 4 || !file.is_char_boundary(file.len() - 4) {
        return None;
    }
    let (stem, extension) = file.split_at(file.len() - 4);
    if !extension.eq_ignore_ascii_case(".txt") {
        return None;
    }
    (stem.eq_ignore_ascii_case("title") || is_two_or_three_digits(stem)).then_some((folder, stem))
}

/// The files the template reads: `front/title.txt`, and each chapter's
/// `NN/title.txt` and `NN/NN.txt`. Everything else in a project (the
/// manifest, `LICENSE.md`) is not the book.
///
/// `00/…` passes here and is dropped by [`tree_from_files`], which takes only
/// chapters above zero.
pub fn is_text_path(path: &str) -> bool {
    path == "front/title.txt" || split_chapter_path(path).is_some()
}

/// The book's parts, from its files as `(path, text)`.
///
/// Read as a JS `Map`: a path given twice keeps its first position and its
/// last text. Order otherwise matters only where two files land on one number
/// (`01/01.txt` and `01/001.txt`): chunks keep their given order (a stable
/// sort), and of two title files the later wins.
pub fn tree_from_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> WriterTree {
    let mut order: Vec<(&str, &str)> = Vec::new();
    let mut seen: FxHashMap<&str, usize> = FxHashMap::default();
    for (path, text) in files {
        match seen.get(path) {
            Some(&at) => order[at].1 = text,
            None => {
                seen.insert(path, order.len());
                order.push((path, text));
            }
        }
    }

    let front = order
        .iter()
        .find(|(path, _)| *path == "front/title.txt")
        .map_or("", |(_, text)| *text);
    // `split(/\r?\n/)` then `trim`: the trim takes any `\r` with it.
    let title = front
        .split('\n')
        .map(js_trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    struct Held<'a> {
        label: Option<&'a str>,
        chunks: Vec<(u32, &'a str)>,
    }
    let mut by_chapter: BTreeMap<u32, Held> = BTreeMap::new();
    for (path, text) in order {
        let Some((folder, stem)) = split_chapter_path(path) else {
            continue;
        };
        let Some(chapter) = numbered(folder) else {
            continue;
        };
        let held = by_chapter.entry(chapter).or_insert(Held {
            label: None,
            chunks: Vec::new(),
        });
        if stem.eq_ignore_ascii_case("title") {
            held.label = Some(js_trim(text));
        } else if let Some(chunk) = numbered(stem) {
            held.chunks.push((chunk, text));
        }
    }

    let chapters = by_chapter
        .into_iter()
        .filter(|(_, held)| !held.chunks.is_empty())
        .map(|(number, mut held)| {
            held.chunks.sort_by_key(|(chunk, _)| *chunk);
            let label = held
                .label
                .filter(|label| !label.is_empty() && *label != number.to_string())
                .map(str::to_string);
            WriterChapter {
                number,
                label,
                chunks: held
                    .chunks
                    .into_iter()
                    .map(|(_, text)| text.to_string())
                    .collect(),
            }
        })
        .collect();

    WriterTree { title, chapters }
}

/// A chunk's own leading `\c N`, every one of them: the template writes the
/// chapter marker once. Writer sometimes saved it twice (`\c 3 \c 3 \v 1`),
/// and the second made the book unreadable. It's Writer's chunk framing, not
/// the translator's text.
///
/// The regex `^(?:\s*\\c\s+\d+\s*)+`, by hand. Taking each repetition
/// greedily is the match the regex finds: backtracking only gives a shorter one.
fn without_chapter_marker(chunk: &str) -> &str {
    let mut rest = chunk;
    loop {
        let candidate = rest.trim_start_matches(is_js_space);
        let Some(after_c) = candidate.strip_prefix("\\c") else {
            return rest;
        };
        let after_space = after_c.trim_start_matches(is_js_space);
        if after_space.len() == after_c.len() {
            return rest;
        }
        let after_digits = after_space.trim_start_matches(|c: char| c.is_ascii_digit());
        if after_digits.len() == after_space.len() {
            return rest;
        }
        rest = after_digits.trim_start_matches(is_js_space);
    }
}

/// The template: the header as `txt2USFM.py`'s `writeHeader` writes it
/// (`\toc3` is the code in lower case; one `\mt` per title line), then each
/// chapter's `\c`, its `\cl` when it has its own label, and its chunks, each
/// trimmed and the empty ones left out. Writer's chunks carry no `\id`, so
/// `book` supplies it.
pub fn book_to_usfm(tree: &WriterTree, book: BookKey, fallback_title: &str) -> String {
    let fallback = [fallback_title.to_string()];
    let title: &[String] = if tree.title.is_empty() {
        &fallback
    } else {
        &tree.title
    };
    let head = title[0].as_str();
    let code = book.to_string();
    let mut lines: Vec<String> = vec![
        format!("\\id {code}"),
        "\\ide UTF-8".to_string(),
        format!("\\h {head}"),
        format!("\\toc1 {head}"),
        format!("\\toc2 {head}"),
        format!("\\toc3 {}", code.to_lowercase()),
    ];
    for (index, line) in title.iter().enumerate() {
        lines.push(format!("\\mt{} {line}", index + 1));
    }
    for chapter in &tree.chapters {
        lines.push(format!("\\c {}", chapter.number));
        if let Some(label) = &chapter.label {
            lines.push(format!("\\cl {label}"));
        }
        for (index, chunk) in chapter.chunks.iter().enumerate() {
            let text = js_trim(if index == 0 {
                without_chapter_marker(chunk)
            } else {
                chunk
            });
            if !text.is_empty() {
                lines.push(text.to_string());
            }
        }
    }
    let mut usfm = lines.join("\n");
    usfm.push('\n');
    usfm
}

#[cfg(test)]
mod tests;
