//! `data/schemes/*.json` → each scheme's chapter and verse counts and its
//! excluded verses.
//!
//! The files are the Copenhagen Alliance mapping format (nida-institute/
//! versification's `standard-mappings/`). This reads `maxVerses` and
//! `excludedVerses`; `mappedVerses` and `partialVerses` wait for `map` (plan
//! step 3). Anything else at the top level is refused, so a format change is
//! noticed here rather than silently dropped.

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
}

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

    Ok(Scheme {
        name,
        books,
        excluded,
    })
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
    out.push_str("use mise::books::BookKey;\n\nuse crate::scheme::Scheme;\n\n");
    out.push_str("#[rustfmt::skip]\npub(crate) static SCHEMES: &[Scheme] = &[\n");
    for scheme in schemes {
        let upper = scheme.name.to_ascii_uppercase();
        let _ = writeln!(
            out,
            "    Scheme::builtin({:?}, {upper}_BOOKS, {upper}_EXCLUDED),",
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
