//! Writes canon's tables from `canon/data/`.
//!
//! ```sh
//! cargo run -p canon --bin gen-canon                     # rewrite the committed tables
//! gen-canon <tables.rs> <tables.ts>                      # write elsewhere (the freshness test)
//! ```
//!
//! Both outputs come from one read of `data/books.tsv`, so the Rust and the
//! TypeScript tables cannot disagree. Names are folded HERE, with the library's
//! own `canon::fold`, and checked for collisions: a name or alias that folds
//! to another book's name, or to another book's code, is refused, because
//! `parse_book` would have to pick one.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use canon::{Division, fold};

struct Row {
    code: String,
    usfm_number: String,
    division: Division,
    canonical_index: Option<u8>,
    english: String,
    aliases: Vec<String>,
}

fn main() -> ExitCode {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let rust_out = args
        .next()
        .unwrap_or_else(|| manifest.join("src/tables.rs"));
    let ts_out = args.next().unwrap_or_else(|| manifest.join("ts/tables.ts"));

    let source = manifest.join("data/books.tsv");
    let text = match std::fs::read_to_string(&source) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("gen-canon: reading {}: {e}", source.display());
            return ExitCode::FAILURE;
        }
    };
    let rows = match read_books(&text) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("gen-canon: data/books.tsv: {e}");
            return ExitCode::FAILURE;
        }
    };
    let names = match name_index(&rows) {
        Ok(names) => names,
        Err(e) => {
            eprintln!("gen-canon: data/books.tsv: {e}");
            return ExitCode::FAILURE;
        }
    };

    for (path, body) in [
        (&rust_out, rust(&rows, &names)),
        (&ts_out, typescript(&rows, &names)),
    ] {
        if let Err(e) = std::fs::write(path, body) {
            eprintln!("gen-canon: writing {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn read_books(text: &str) -> Result<Vec<Row>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('#') && !line.trim().is_empty());
    let header = lines.next().ok_or("no header row")?.1;
    let want = "code\tusfm_number\tdivision\tcanonical_index\tenglish\taliases";
    if header != want {
        return Err(format!("header is {header:?}, want {want:?}"));
    }
    let mut rows = Vec::new();
    for (at, line) in lines {
        let at = at + 1;
        let cells: Vec<&str> = line.split('\t').collect();
        let [code, usfm_number, division, index, english, aliases] = cells[..] else {
            return Err(format!("line {at}: {} cells, want 6", cells.len()));
        };
        if code.len() != 3
            || !code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return Err(format!(
                "line {at}: {code:?} is not a three-character uppercase code"
            ));
        }
        let division = Division::parse(division)
            .ok_or_else(|| format!("line {at}: unknown division {division:?}"))?;
        let canonical_index = match index {
            "" => None,
            n => Some(
                n.parse::<u8>()
                    .map_err(|e| format!("line {at}: canonical_index {n:?}: {e}"))?,
            ),
        };
        rows.push(Row {
            code: code.to_owned(),
            usfm_number: usfm_number.to_owned(),
            division,
            canonical_index,
            english: english.to_owned(),
            aliases: aliases
                .split('|')
                .filter(|a| !a.is_empty())
                .map(str::to_owned)
                .collect(),
        });
    }
    if rows.len() > usize::from(u8::MAX) {
        return Err(format!(
            "{} rows; the name index stores a row as u8",
            rows.len()
        ));
    }
    Ok(rows)
}

/// Every name and alias, folded, sorted, each naming one row.
fn name_index(rows: &[Row]) -> Result<Vec<(String, u8)>, String> {
    let codes: Vec<String> = rows
        .iter()
        .map(|row| row.code.to_ascii_lowercase())
        .collect();
    let mut names: Vec<(String, u8)> = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        for name in std::iter::once(&row.english).chain(&row.aliases) {
            let folded = fold(name);
            if let Some(other) = codes.iter().position(|code| *code == folded)
                && other != at
            {
                return Err(format!(
                    "{}'s name {name:?} is {}'s code",
                    row.code, rows[other].code
                ));
            }
            match names.iter().find(|(seen, _)| *seen == folded) {
                Some((_, owner)) if usize::from(*owner) == at => {}
                Some((_, owner)) => {
                    return Err(format!(
                        "{name:?} names both {} and {}",
                        rows[usize::from(*owner)].code,
                        row.code
                    ));
                }
                None => names.push((folded, at as u8)),
            }
        }
    }
    names.sort();
    Ok(names)
}

const HEADER: &str =
    "@generated by `cargo run -p canon --bin gen-canon` from canon/data/books.tsv. Do not edit.";

fn rust(rows: &[Row], names: &[(String, u8)]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// {HEADER}\n");
    out.push_str("use mise::books::BookKey;\n\nuse crate::books::{Book, Division};\n\n");
    out.push_str("#[rustfmt::skip]\npub(crate) static BOOKS: &[Book] = &[\n");
    for row in rows {
        let division = match row.division {
            Division::Ot => "Ot",
            Division::Nt => "Nt",
            Division::Deuterocanon => "Deuterocanon",
            Division::Additional => "Additional",
            Division::Peripheral => "Peripheral",
            Division::Extra => "Extra",
        };
        let index = row
            .canonical_index
            .map_or("None".to_owned(), |n| format!("Some({n})"));
        let aliases: Vec<String> = row.aliases.iter().map(|a| format!("{a:?}")).collect();
        let _ = writeln!(
            out,
            "    Book {{ code: BookKey::new(*b{:?}), usfm_number: {:?}, division: Division::{division}, canonical_index: {index}, english: {:?}, aliases: &[{}] }},",
            row.code,
            row.usfm_number,
            row.english,
            aliases.join(", ")
        );
    }
    out.push_str("];\n\n");
    out.push_str(
        "/// Every English name and alias, folded, sorted for binary search, with its row.\n",
    );
    out.push_str("#[rustfmt::skip]\npub(crate) static NAMES: &[(&str, u8)] = &[\n");
    for (name, at) in names {
        let _ = writeln!(out, "    ({name:?}, {at}),");
    }
    out.push_str("];\n");
    out
}

fn typescript(rows: &[Row], names: &[(String, u8)]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// {HEADER}\n");
    out.push_str(
        "export type Division = \"ot\" | \"nt\" | \"deuterocanon\" | \"additional\" | \"peripheral\" | \"extra\";\n\n",
    );
    out.push_str("export interface Book {\n");
    out.push_str("  readonly code: string;\n  readonly usfmNumber: string;\n  readonly division: Division;\n");
    out.push_str("  readonly canonicalIndex: number | null;\n  readonly english: string;\n");
    out.push_str("  readonly aliases: readonly string[];\n}\n\n");
    out.push_str("export const BOOKS: readonly Book[] = [\n");
    for row in rows {
        let index = row
            .canonical_index
            .map_or("null".to_owned(), |n| n.to_string());
        let aliases: Vec<String> = row.aliases.iter().map(|a| json(a)).collect();
        let _ = writeln!(
            out,
            "  {{ code: {}, usfmNumber: {}, division: {}, canonicalIndex: {index}, english: {}, aliases: [{}] }},",
            json(&row.code),
            json(&row.usfm_number),
            json(row.division.as_str()),
            json(&row.english),
            aliases.join(", ")
        );
    }
    out.push_str("];\n\n");
    out.push_str("/** Every English name and alias, folded, to the code it names. */\n");
    out.push_str("export const NAMES: ReadonlyMap<string, string> = new Map([\n");
    for (name, at) in names {
        let _ = writeln!(
            out,
            "  [{}, {}],",
            json(name),
            json(&rows[usize::from(*at)].code)
        );
    }
    out.push_str("]);\n");
    out
}

/// A JSON (and TS) string literal. The table's text is printable; escape what
/// JSON requires and nothing else.
fn json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
