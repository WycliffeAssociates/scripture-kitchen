//! Harvests each gateway language's book names from its own texts into
//! `canon/data/names/<locale>.tsv`, for a person to review and commit.
//!
//! ```sh
//! canon/gen/fetch-gateways.sh            # GATEWAYS.tsv, and the sources cloned
//! cargo run -p canon-gen --bin harvest-names
//! ```
//!
//! A USFM book gives `\toc1` (long), `\toc2` (short) and `\toc3` (abbrev),
//! read through onion's lexer. A per-book Writer repo has no `\toc`, only
//! `front/title.txt`, which is the long name. Where two sources disagree, the
//! cell holds every value, ` | `-separated: `gen-canon` refuses such a cell,
//! so it cannot ship before someone picks one.
//!
//! Dropped on the way, because they are not names: an empty value, and an
//! abbreviation that is only the book code in one case (`\toc3 mat`, `MAT`),
//! which the code already matches. `Gen` is kept: it is how English abbreviates.
//!
//! A file already reviewed is not overwritten: the harvest goes to
//! `<locale>.tsv.harvested` beside it, to diff against.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use canon::BookKey;
use usfm_onion::{TokenKind, lex};

const FORMS: [&str; 3] = ["toc1", "toc2", "toc3"];

/// Per book code, per form, each distinct value and the sources that gave it.
type Harvest = BTreeMap<[u8; 3], [BTreeMap<String, Vec<String>>; 3]>;

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let names = root.join("canon/data/names");
    let cache = root.join("target/canon-harvest");
    let gateways = match std::fs::read_to_string(names.join("GATEWAYS.tsv")) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("harvest-names: GATEWAYS.tsv: {e} (run canon/gen/fetch-gateways.sh)");
            return ExitCode::FAILURE;
        }
    };
    for line in gateways.lines().filter(|l| !l.starts_with('#')) {
        let cols: Vec<&str> = line.split('\t').collect();
        let [locale, english, national, sources] = cols[..] else {
            eprintln!("harvest-names: bad GATEWAYS row: {line}");
            return ExitCode::FAILURE;
        };
        let mut harvest = Harvest::new();
        for source in sources.split(',').filter(|s| !s.is_empty()) {
            let dir = cache.join(source);
            if !dir.is_dir() {
                eprintln!("harvest-names: {source} is not cloned; skipped");
                continue;
            }
            let repo = source.rsplit('/').next().unwrap_or(source);
            harvest_dir(&dir, repo, &mut harvest);
        }
        if harvest.is_empty() {
            println!("{locale}: no names (falls back to English)");
            continue;
        }
        let text = render(locale, english, national, sources, &harvest);
        let target = names.join(format!("{locale}.tsv"));
        let target = if target.exists() {
            names.join(format!("{locale}.tsv.harvested"))
        } else {
            target
        };
        if let Err(e) = std::fs::write(&target, text) {
            eprintln!("harvest-names: writing {}: {e}", target.display());
            return ExitCode::FAILURE;
        }
        let conflicts = harvest
            .values()
            .flatten()
            .filter(|values| values.len() > 1)
            .count();
        println!(
            "{locale}: {} books, {conflicts} cells to pick from → {}",
            harvest.len(),
            target.display()
        );
    }
    ExitCode::SUCCESS
}

fn harvest_dir(dir: &Path, repo: &str, harvest: &mut Harvest) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if path.is_dir() {
                if name != ".git" {
                    stack.push(path);
                }
            } else if name.ends_with(".usfm") || name.ends_with(".sfm") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    harvest_usfm(&text, repo, harvest);
                }
            } else if name == "title.txt"
                && dir.file_name().is_some_and(|d| d == "front")
                && let Some(code) = writer_book(dir.parent().unwrap_or(&dir))
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                add(harvest, code, 0, text.trim(), repo);
            }
        }
    }
}

/// The book a Writer project is, from its `manifest.json`'s `project.id`.
fn writer_book(project: &Path) -> Option<[u8; 3]> {
    let manifest = std::fs::read_to_string(project.join("manifest.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let id = json.get("project")?.get("id")?.as_str()?;
    code_of(id)
}

fn code_of(text: &str) -> Option<[u8; 3]> {
    let bytes: [u8; 3] = text
        .trim()
        .to_ascii_uppercase()
        .as_bytes()
        .try_into()
        .ok()?;
    canon::book(BookKey::new(bytes)).map(|_| bytes)
}

fn harvest_usfm(text: &str, repo: &str, harvest: &mut Harvest) {
    let tokens = lex(text);
    let span = |at: usize| {
        let token = &tokens[at];
        let start = token.start as usize;
        &text[start..start + usize::from(token.len)]
    };
    let mut code = None;
    let mut at = 0;
    while at < tokens.len() {
        match tokens[at].kind() {
            TokenKind::BookCode => code = code_of(span(at)),
            TokenKind::Marker { .. } => {
                let marker = span(at).trim().trim_start_matches('\\');
                if let (Some(form), Some(code)) = (FORMS.iter().position(|f| *f == marker), code) {
                    let mut value = String::new();
                    while at + 1 < tokens.len() && tokens[at + 1].kind() == TokenKind::Text {
                        at += 1;
                        value.push_str(span(at));
                    }
                    add(harvest, code, form, value.trim(), repo);
                }
            }
            TokenKind::Designator => break, // past the headers
            _ => {}
        }
        at += 1;
    }
}

fn add(harvest: &mut Harvest, code: [u8; 3], form: usize, value: &str, repo: &str) {
    // Zero-width spaces (Lao marks word breaks with them) and a BOM are no part
    // of a name; a leading dash is a list bullet some Odia headers carry.
    let value: String = value
        .chars()
        .filter(|c| !matches!(c, '\u{200b}' | '\u{feff}'))
        .collect();
    let value = value.trim_start_matches(['-', ' ']);
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let bare_code = value == code_str(&code) || value == code_str(&code).to_ascii_lowercase();
    if value.is_empty() || (form == 2 && bare_code) {
        return;
    }
    let sources = harvest.entry(code).or_default()[form]
        .entry(value)
        .or_default();
    if !sources.iter().any(|s| s == repo) {
        sources.push(repo.to_owned());
    }
}

fn code_str(code: &[u8; 3]) -> &str {
    std::str::from_utf8(code).unwrap_or("")
}

fn render(locale: &str, english: &str, national: &str, sources: &str, harvest: &Harvest) -> String {
    let mut out = format!(
        "# {locale}: {english} ({national}). Book names harvested from \\toc1-3 (Writer\n\
         # projects: front/title.txt) of: {}.\n\
         # Reviewed: no. A cell with ` | ` lists what the sources disagree on; keep one.\n\
         code\tlong\tshort\tabbrev\n",
        sources.replace(',', ", ")
    );
    // Spec order, so a reviewer reads it like a table of contents.
    let order: BTreeMap<[u8; 3], usize> = canon::books(canon::Canon::All)
        .enumerate()
        .map(|(at, book)| (book.code.as_bytes(), at))
        .collect();
    let mut codes: Vec<&[u8; 3]> = harvest.keys().collect();
    codes.sort_by_key(|code| order.get(*code).copied().unwrap_or(usize::MAX));
    for code in codes {
        let cells: Vec<String> = harvest[code]
            .iter()
            .map(|values| {
                let mut ranked: Vec<(&String, &Vec<String>)> = values.iter().collect();
                ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
                ranked
                    .iter()
                    .map(|(value, _)| value.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect();
        out.push_str(&format!("{}\t{}\n", code_str(code), cells.join("\t")));
    }
    out
}
