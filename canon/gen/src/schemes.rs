//! `data/schemes/*.json` → each scheme's chapter and verse counts, its
//! excluded verses, and its rows toward `org`.
//!
//! The files are the Copenhagen Alliance mapping format (nida-institute/
//! versification's `standard-mappings/`). This reads `maxVerses`,
//! `excludedVerses` and `mappedVerses`. `partialVerses` (which parts a verse
//! is cut into, `lxx` only) has no reader yet: a row naming a part carries
//! the letter itself. Anything else at the top level is refused, so a format
//! change is noticed here rather than silently dropped.
//!
//! `org`'s own `mappedVerses` (eight rows sending S3Y into DAG) are not read:
//! `org` is the pivot, and every `org` verse is itself.

use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

/// The built-in schemes, in the order `canon::schemes()` returns them: `org`,
/// the pivot every mapping passes through, first. A file in `data/schemes/`
/// that is not listed here is an error, and so is a listed one that is missing.
pub const ORDER: [&str; 7] = ["org", "eng", "lxx", "vul", "rsc", "rso", "ulb"];

pub struct Scheme {
    pub name: &'static str,
    /// In book-table order.
    pub books: Vec<(String, Vec<u16>)>,
    /// Sorted.
    pub excluded: Vec<(String, u16, u16)>,
    /// `mappedVerses`, toward `org`, sorted by where each starts here.
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    Parallel,
    Merge,
    Split,
}

/// One `mappedVerses` row: `len` verses from `from` (Parallel, Merge) or
/// one (Split), to `len` verses from `to` (Parallel, Split) or one (Merge).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Row {
    pub from: (String, u16, u16),
    pub to: (String, u16, u16, Option<char>),
    pub len: u16,
    pub shape: Shape,
}

/// Rows the files get wrong, and what they evidently mean, keyed by scheme
/// and the row's key as written. Each is an unequal range pair that can only
/// be a typo. Any other unequal pair is an error.
///
/// - `vul` `DAG 3:52-23`: a backwards range; `org.json` itself pairs
///   S3Y 1:30-31 with DAG 3:52-53.
/// - `vul` `DAG 13:1-63` and `DAG 14:1-42`: whole chapters sent to their last
///   verse alone; they are Susanna and Bel, verse for verse.
/// - `rso` `PSA 89:2-6` to `PSA 90:1-6`: five verses onto six. The row before
///   already sends 89:0-1 to 90:0-1, so 89:2-6 is 90:2-6.
const ERRATA: [(&str, &str, &str, &str); 4] = [
    ("vul", "DAG 3:52-23", "DAG 3:52-53", "S3Y 1:30-31"),
    ("vul", "DAG 13:1-63", "DAG 13:1-63", "SUS 1:1-63"),
    ("vul", "DAG 14:1-42", "DAG 14:1-42", "BEL 1:1-42"),
    ("rso", "PSA 89:2-6", "PSA 89:2-6", "PSA 90:2-6"),
];

/// Paratext's codes for alternate Greek texts (Theodotion's Bel, Daniel and
/// Susanna; Judges B; Joshua A; Sinaiticus Tobit). `org`, `eng` and `lxx`
/// count them, but they are not USFM book identifiers, no `\id` can name
/// them and no mapping refers to them, so they are dropped. Any other code
/// outside books.tsv is an error.
const PARATEXT_ONLY: [&str; 6] = ["BLT", "DNT", "JDB", "JSA", "SST", "TBS"];

const KNOWN_KEYS: [&str; 6] = [
    "maxVerses",
    "excludedVerses",
    "mappedVerses",
    "partialVerses",
    "mergedVerses",
    "basedOn",
];

/// Reads every scheme. `book_order` is the book table's codes, in order: a
/// scheme's books are emitted in that order, and a code outside it is refused.
pub fn read(dir: &Path, book_order: &[String]) -> Result<Vec<Scheme>, String> {
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("reading {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.strip_suffix(".json").map(str::to_owned)
        })
        .collect();
    on_disk.sort();
    for name in &on_disk {
        if !ORDER.contains(&name.as_str()) {
            return Err(format!(
                "data/schemes/{name}.json is not in gen-canon's ORDER; add it there"
            ));
        }
    }
    ORDER
        .iter()
        .map(|&name| {
            let path = dir.join(format!("{name}.json"));
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            read_one(name, &text, book_order).map_err(|e| format!("data/schemes/{name}.json: {e}"))
        })
        .collect()
}

fn read_one(name: &'static str, text: &str, book_order: &[String]) -> Result<Scheme, String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let doc = doc.as_object().ok_or("not a JSON object")?;
    if let Some(key) = doc.keys().find(|key| !KNOWN_KEYS.contains(&key.as_str())) {
        return Err(format!("unknown top-level key {key:?}"));
    }
    if doc.contains_key("basedOn") {
        return Err("basedOn is not supported; write the scheme out in full".to_owned());
    }

    let max = doc
        .get("maxVerses")
        .and_then(Value::as_object)
        .ok_or("no maxVerses object")?;
    let mut books = Vec::new();
    for code in max.keys() {
        if !book_order.contains(code) && !PARATEXT_ONLY.contains(&code.as_str()) {
            return Err(format!(
                "maxVerses names {code:?}, which is not in books.tsv"
            ));
        }
    }
    for code in book_order {
        let Some(chapters) = max.get(code) else {
            continue;
        };
        let chapters = chapters
            .as_array()
            .ok_or_else(|| format!("maxVerses.{code} is not an array"))?;
        let counts = chapters
            .iter()
            .enumerate()
            .map(|(at, n)| count(n).ok_or_else(|| format!("maxVerses.{code}[{at}] is {n}")))
            .collect::<Result<Vec<u16>, String>>()?;
        if counts.is_empty() {
            return Err(format!("maxVerses.{code} has no chapters"));
        }
        books.push((code.clone(), counts));
    }

    let mut excluded = Vec::new();
    if let Some(list) = doc.get("excludedVerses") {
        let list = list.as_array().ok_or("excludedVerses is not an array")?;
        for reference in list {
            let reference = reference
                .as_str()
                .ok_or_else(|| format!("excludedVerses holds {reference}"))?;
            let (code, chapter, verse) = bcv(reference)
                .ok_or_else(|| format!("excludedVerses holds {reference:?}, not \"BBB c:v\""))?;
            let last = books
                .iter()
                .find(|(book, _)| *book == code)
                .and_then(|(_, counts)| counts.get(usize::from(chapter).checked_sub(1)?));
            if last.is_none_or(|&last| verse > last) {
                return Err(format!(
                    "excludedVerses holds {reference:?}, outside maxVerses"
                ));
            }
            excluded.push((code, chapter, verse));
        }
    }
    excluded.sort();
    excluded.dedup();

    let mut rows = Vec::new();
    if name != "org"
        && let Some(mapped) = doc.get("mappedVerses")
    {
        let mapped = mapped.as_object().ok_or("mappedVerses is not an object")?;
        let mut used = 0;
        for (key, value) in mapped {
            let value = value
                .as_str()
                .ok_or_else(|| format!("mappedVerses.{key} is {value}"))?;
            let (key, value) = match ERRATA
                .iter()
                .find(|(scheme, wrong, _, _)| *scheme == name && wrong == key)
            {
                Some((_, _, key, value)) => {
                    used += 1;
                    (*key, *value)
                }
                None => (key.as_str(), value),
            };
            let row = mapped_row(key, value, book_order)
                .map_err(|e| format!("mappedVerses {key:?}: {value:?}: {e}"))?;
            rows.push(row);
        }
        let listed = ERRATA.iter().filter(|(scheme, ..)| *scheme == name).count();
        if used != listed {
            return Err(format!(
                "{listed} errata listed but {used} found; one has been fixed upstream?"
            ));
        }
    }
    rows.sort();

    Ok(Scheme {
        name,
        books,
        excluded,
        rows,
    })
}

/// One row, as `"BBB c:v[-v]"` or `"BBB c:v<part>"` on each side.
fn mapped_row(key: &str, value: &str, book_order: &[String]) -> Result<Row, String> {
    let (from_book, from_chapter, from_first, from_last, from_part) =
        range(key).ok_or("the key is not \"BBB c:v[-v]\"")?;
    let (to_book, to_chapter, to_first, to_last, to_part) =
        range(value).ok_or("the value is not \"BBB c:v[-v]\"")?;
    for book in [&from_book, &to_book] {
        if !book_order.contains(book) {
            return Err(format!("{book} is not in books.tsv"));
        }
    }
    if from_part.is_some() {
        return Err("a part on the scheme's side is not supported".to_owned());
    }
    let from_len = from_last + 1 - from_first;
    let to_len = to_last + 1 - to_first;
    let (len, shape) = match (from_len, to_len) {
        (a, b) if a == b => (a, Shape::Parallel),
        (a, 1) => (a, Shape::Merge),
        (1, b) => (b, Shape::Split),
        (a, b) => {
            return Err(format!(
                "{a} verses onto {b}; add it to ERRATA if it is a typo"
            ));
        }
    };
    if to_part.is_some() && to_len != 1 {
        return Err("a part on a range".to_owned());
    }
    Ok(Row {
        from: (from_book, from_chapter, from_first),
        to: (to_book, to_chapter, to_first, to_part),
        len,
        shape,
    })
}

/// `"PSA 3:0-8"` → `("PSA", 3, 0, 8, None)`; `"ESG 4:17c"` → `(.., 17, 17, Some('c'))`.
/// A backwards range is refused.
fn range(text: &str) -> Option<(String, u16, u16, u16, Option<char>)> {
    let (book, rest) = text.split_once(' ')?;
    if book.len() != 3 {
        return None;
    }
    let (chapter, verses) = rest.split_once(':')?;
    let chapter = chapter.parse().ok()?;
    let (first, last) = match verses.split_once('-') {
        Some((first, last)) => (first, Some(last)),
        None => (verses, None),
    };
    let (first, part) = match first.strip_suffix(|c: char| c.is_ascii_lowercase()) {
        Some(digits) => (digits, first.chars().last()),
        None => (first, None),
    };
    let first: u16 = first.parse().ok()?;
    let last: u16 = match last {
        Some(last) if part.is_none() => last.parse().ok()?,
        Some(_) => return None,
        None => first,
    };
    (last >= first).then(|| (book.to_owned(), chapter, first, last, part))
}

/// A count, written as a number or (as nida writes them) a decimal string.
/// Zero is refused: no scheme has an empty chapter.
fn count(value: &Value) -> Option<u16> {
    let n = match value {
        Value::Number(n) => u16::try_from(n.as_u64()?).ok()?,
        Value::String(s) => s.parse().ok()?,
        _ => return None,
    };
    (n > 0).then_some(n)
}

/// `"MAT 17:21"` → `("MAT", 17, 21)`.
fn bcv(reference: &str) -> Option<(String, u16, u16)> {
    let (code, rest) = reference.split_once(' ')?;
    let (chapter, verse) = rest.split_once(':')?;
    (code.len() == 3).then_some(())?;
    Some((code.to_owned(), chapter.parse().ok()?, verse.parse().ok()?))
}

pub fn rust(schemes: &[Scheme], header: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// {header}\n");
    out.push_str("use mise::books::BookKey;\n\nuse crate::map::{Row, Shape};\nuse crate::scheme::Scheme;\n\n");
    out.push_str("#[rustfmt::skip]\npub(crate) static SCHEMES: &[Scheme] = &[\n");
    for scheme in schemes {
        let upper = scheme.name.to_ascii_uppercase();
        let _ = writeln!(
            out,
            "    Scheme::builtin({:?}, {upper}_BOOKS, {upper}_EXCLUDED, {upper}_ROWS, {upper}_BY_TARGET),",
            scheme.name
        );
    }
    out.push_str("];\n");
    for scheme in schemes {
        let upper = scheme.name.to_ascii_uppercase();
        let _ = writeln!(
            out,
            "\n#[rustfmt::skip]\nstatic {upper}_BOOKS: &[(BookKey, &[u16])] = &["
        );
        for (code, counts) in &scheme.books {
            let _ = writeln!(out, "    (BookKey::new(*b{code:?}), &[{}]),", join(counts));
        }
        out.push_str("];\n");
        let _ = write!(
            out,
            "\n#[rustfmt::skip]\nstatic {upper}_EXCLUDED: &[([u8; 3], u16, u16)] = &["
        );
        if scheme.excluded.is_empty() {
            out.push_str("];\n");
        } else {
            out.push('\n');
            for (code, chapter, verse) in &scheme.excluded {
                let _ = writeln!(out, "    (*b{code:?}, {chapter}, {verse}),");
            }
            out.push_str("];\n");
        }
        let _ = write!(out, "\n#[rustfmt::skip]\nstatic {upper}_ROWS: &[Row] = &[");
        if !scheme.rows.is_empty() {
            out.push('\n');
        }
        for row in &scheme.rows {
            let (fb, fc, fv) = &row.from;
            let (tb, tc, tv, part) = &row.to;
            let part = part.map_or("None".to_owned(), |p| format!("Some({p:?})"));
            let _ = writeln!(
                out,
                "    Row::new(*b{fb:?}, {fc}, {fv}, *b{tb:?}, {tc}, {tv}, {part}, {}, Shape::{:?}),",
                row.len, row.shape
            );
        }
        out.push_str("];\n");
        let mut by_target: Vec<usize> = (0..scheme.rows.len()).collect();
        by_target.sort_by_key(|&at| &scheme.rows[at].to);
        let index: Vec<String> = by_target.iter().map(usize::to_string).collect();
        let _ = writeln!(
            out,
            "\n#[rustfmt::skip]\nstatic {upper}_BY_TARGET: &[u16] = &[{}];",
            index.join(", ")
        );
    }
    out
}

pub fn typescript(schemes: &[Scheme], header: &str, json: fn(&str) -> String) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// {header}\n");
    let names: Vec<String> = schemes.iter().map(|s| json(s.name)).collect();
    let _ = writeln!(out, "export type SchemeName = {};\n", names.join(" | "));
    let _ = writeln!(
        out,
        "export const SCHEME_NAMES: readonly SchemeName[] = [{}];\n",
        names.join(", ")
    );
    out.push_str("/** Per scheme, each book's last verse per chapter, chapter 1 first. */\n");
    out.push_str(
        "export const VERSE_COUNTS: Readonly<Record<SchemeName, ReadonlyMap<string, readonly number[]>>> = {\n",
    );
    for scheme in schemes {
        let _ = writeln!(out, "  {}: new Map([", scheme.name);
        for (code, counts) in &scheme.books {
            let _ = writeln!(out, "    [{}, [{}]],", json(code), join(counts));
        }
        out.push_str("  ]),\n");
    }
    out.push_str("};\n\n");
    out.push_str("/** Per scheme, the verses it omits on purpose, as `\"MAT 17:21\"`. */\n");
    out.push_str("export const EXCLUDED: Readonly<Record<SchemeName, ReadonlySet<string>>> = {\n");
    for scheme in schemes {
        let refs: Vec<String> = scheme
            .excluded
            .iter()
            .map(|(code, chapter, verse)| json(&format!("{code} {chapter}:{verse}")))
            .collect();
        let _ = writeln!(out, "  {}: new Set([{}]),", scheme.name, refs.join(", "));
    }
    out.push_str("};\n");
    out
}

fn join(counts: &[u16]) -> String {
    counts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
