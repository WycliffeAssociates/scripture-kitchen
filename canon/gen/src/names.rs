//! `data/names/<locale>.tsv` → `src/name_tables.rs` and `ts/names/<locale>.ts`.
//!
//! Each file is a gateway language's own book names, harvested by
//! `harvest-names` and reviewed. Refused here, so `parse_book_in` never has to
//! pick: a cell still holding the harvest's ` | ` choices, an unknown or
//! repeated code, and a name that folds to another book's name in the same
//! locale.
//!
//! A name spelled as another book's code (Tok Pisin's John is "Jon") is
//! allowed: `parse_book_in` tries codes first, as Sefer does, so "Jon" is
//! Jonah there, while "Jón" or a longer name still reaches the index.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use canon::fold;

pub struct Locale {
    pub tag: String,
    /// Per book (its row in the book table): long, short, abbrev; "" is none.
    pub books: Vec<(u8, [String; 3])>,
    /// Every name, folded, sorted, with its book's row.
    pub index: Vec<(String, u8)>,
}

pub fn read(dir: &Path, codes: &[String]) -> Result<Vec<Locale>, String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "tsv")
                && path.file_stem().is_some_and(|stem| stem != "GATEWAYS")
        })
        .collect();
    files.sort();
    files
        .iter()
        .map(|path| {
            let tag = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("{}: not a UTF-8 name", path.display()))?;
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            locale(tag, &text, codes).map_err(|e| format!("data/names/{tag}.tsv: {e}"))
        })
        .collect()
}

fn locale(tag: &str, text: &str, codes: &[String]) -> Result<Locale, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('#') && !line.trim().is_empty());
    let header = lines.next().ok_or("no header row")?.1;
    if header != "code\tlong\tshort\tabbrev" {
        return Err(format!("header is {header:?}"));
    }
    let mut books: Vec<(u8, [String; 3])> = Vec::new();
    let mut seen: BTreeMap<String, u8> = BTreeMap::new();
    for (at, line) in lines {
        let at = at + 1;
        let cells: Vec<&str> = line.split('\t').collect();
        let [code, long, short, abbrev] = cells[..] else {
            return Err(format!("line {at}: {} cells, want 4", cells.len()));
        };
        let row = codes
            .iter()
            .position(|c| c == code)
            .ok_or_else(|| format!("line {at}: unknown code {code:?}"))?;
        let row = u8::try_from(row).map_err(|_| "more than 255 books")?;
        if books.iter().any(|(r, _)| *r == row) {
            return Err(format!("line {at}: {code} twice"));
        }
        let names = [long, short, abbrev].map(str::to_owned);
        for name in names.iter().filter(|n| !n.is_empty()) {
            if name.contains(" | ") {
                return Err(format!(
                    "line {at}: {code} {name:?} is the harvest's choices; keep one"
                ));
            }
            if name.trim() != name {
                return Err(format!("line {at}: {code} {name:?} has outer spaces"));
            }
            let folded = fold(name);
            match seen.get(&folded) {
                Some(owner) if *owner != row => {
                    return Err(format!(
                        "line {at}: {name:?} names both {} and {code}",
                        codes[usize::from(*owner)]
                    ));
                }
                _ => {
                    seen.insert(folded, row);
                }
            }
        }
        books.push((row, names));
    }
    books.sort_by_key(|(row, _)| *row);
    Ok(Locale {
        tag: tag.to_owned(),
        books,
        index: seen.into_iter().collect(),
    })
}

pub fn rust(locales: &[Locale], header: &str) -> String {
    let mut out = format!("// {header}\n\n");
    out.push_str("use crate::names::Locale;\n\n");
    out.push_str(
        "/// Each gateway language's reviewed names, by tag. `books` is by book-table\n\
         /// row (long, short, abbrev; \"\" is none); `index` is every name folded,\n\
         /// sorted for binary search.\n",
    );
    out.push_str("#[rustfmt::skip]\npub(crate) static LOCALES: &[Locale] = &[\n");
    for locale in locales {
        let _ = writeln!(out, "    Locale {{ tag: {:?}, books: &[", locale.tag);
        for (row, [long, short, abbrev]) in &locale.books {
            let _ = writeln!(out, "        ({row}, [{long:?}, {short:?}, {abbrev:?}]),");
        }
        out.push_str("    ], index: &[\n");
        for (name, row) in &locale.index {
            let _ = writeln!(out, "        ({name:?}, {row}),");
        }
        out.push_str("    ] },\n");
    }
    out.push_str("];\n");
    out
}

/// One module per locale, so a bundle carries only the languages it imports.
pub fn typescript(
    locale: &Locale,
    codes: &[String],
    header: &str,
    json: fn(&str) -> String,
) -> String {
    let mut out = format!("// {header}\n\n");
    out.push_str("import type { NameTable } from \"../canon.ts\";\n\n");
    let _ = writeln!(
        out,
        "const names: NameTable = {{\n  locale: {},",
        json(&locale.tag)
    );
    out.push_str("  books: {\n");
    for (row, [long, short, abbrev]) in &locale.books {
        let code = &codes[usize::from(*row)];
        let key = if code.starts_with(|c: char| c.is_ascii_digit()) {
            json(code)
        } else {
            code.clone()
        };
        let _ = writeln!(
            out,
            "    {key}: [{}, {}, {}],",
            json(long),
            json(short),
            json(abbrev)
        );
    }
    out.push_str("  },\n};\n\nexport default names;\n");
    out
}
