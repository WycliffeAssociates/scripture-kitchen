//! `data/sniff/rules.json` → the sniffing rules, each with the columns every
//! built-in scheme fits.
//!
//! The file is nida-institute/versification's `merged_rules.json`: TVTMS
//! rules, each naming a passage, its columns (numbering traditions: "English",
//! "Hebrew", "Greek2 (eg NETS)", …) and, per column, the tests a text passes
//! when it follows that column:
//!
//! ```text
//! Gen.2:25=Last           the verse is its chapter's highest
//! Exo.8:28=Exist          the verse is there      (=NotExist: it isn't)
//! Gen.5:31.2=Exist        the verse's second part (31b) is there
//! Neh.4:6*2>Neh.4:3       twice 4:6's length beats 4:3's
//! ```
//!
//! The labels are not our schemes, so this works out which columns each
//! scheme fits by running the rule's structural tests (Last, Exist, NotExist,
//! whole verses) against the scheme's own counts. Length and part tests say
//! nothing about a scheme and are skipped there. A scheme that fits no column
//! (it lacks the book) sits the rule out.
//!
//! Only `ranges` (what each column's verses correspond to) is not read: canon
//! votes for a named scheme and takes its mapping from the scheme files.

use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

use crate::schemes::Scheme;

pub struct Rule {
    pub name: String,
    pub columns: Vec<String>,
    pub tests: Vec<Vec<Test>>,
    /// Per scheme, in `schemes::ORDER`: a bit per column the scheme fits.
    pub fits: Vec<u8>,
}

#[derive(Clone)]
pub struct Place {
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    /// `b` for `.2`.
    pub part: Option<char>,
}

pub enum Test {
    Last(Place),
    Exist(Place),
    NotExist(Place),
    /// `left * left_factor` is longer (`>`) or shorter (`<`) than
    /// `right * right_factor`.
    Compare {
        left: Place,
        left_factor: u8,
        longer: bool,
        right: Place,
        right_factor: u8,
    },
    /// A form the sniffer can't evaluate (a range, `Pro.20:20-22`; a part 0;
    /// `Last` of a part). The rule abstains wherever it is met.
    Unsupported(String),
}

pub fn read(path: &Path, schemes: &[Scheme], book_order: &[String]) -> Result<Vec<Rule>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let list = doc.as_array().ok_or("not a JSON array")?;
    let mut rules = Vec::new();
    for (at, rule) in list.iter().enumerate() {
        let read = || -> Result<Rule, String> {
            let name = rule["name"].as_str().ok_or("no name")?.to_owned();
            let columns = rule["columns"]
                .as_array()
                .ok_or("no columns")?
                .iter()
                .map(|labels| {
                    let labels = labels.as_array().ok_or("a column is not a list")?;
                    let labels: Vec<&str> = labels
                        .iter()
                        .map(|l| l.as_str().map(str::trim).ok_or("a label is not a string"))
                        .collect::<Result<_, _>>()?;
                    Ok(labels.join(" / "))
                })
                .collect::<Result<Vec<String>, &str>>()?;
            let tests = rule["tests"]
                .as_array()
                .ok_or("no tests")?
                .iter()
                .map(|column| {
                    column
                        .as_array()
                        .ok_or_else(|| "a test column is not a list".to_owned())?
                        .iter()
                        .map(|t| {
                            let t = t.as_str().ok_or("a test is not a string")?;
                            test(t, book_order).map_err(|e| format!("test {t:?}: {e}"))
                        })
                        .collect::<Result<Vec<Test>, String>>()
                })
                .collect::<Result<Vec<Vec<Test>>, String>>()?;
            if tests.len() != columns.len() {
                return Err(format!(
                    "{} columns, {} test lists",
                    columns.len(),
                    tests.len()
                ));
            }
            if columns.len() > 8 {
                return Err(format!("{} columns; the fit mask is a u8", columns.len()));
            }
            let fits = schemes.iter().map(|scheme| fit(scheme, &tests)).collect();
            Ok(Rule {
                name,
                columns,
                tests,
                fits,
            })
        };
        rules.push(read().map_err(|e| format!("rule {at}: {e}"))?);
    }
    Ok(rules)
}

/// Which columns a scheme's counts pass, structural tests only.
fn fit(scheme: &Scheme, tests: &[Vec<Test>]) -> u8 {
    let last = |p: &Place| -> Option<u16> {
        let (_, counts) = scheme.books.iter().find(|(book, _)| *book == p.book)?;
        counts.get(usize::from(p.chapter).checked_sub(1)?).copied()
    };
    let mut mask = 0;
    for (column, tests) in tests.iter().enumerate() {
        let passes = tests.iter().all(|t| match t {
            Test::Last(p) if p.part.is_none() => last(p) == Some(p.verse),
            Test::Exist(p) if p.part.is_none() => {
                last(p).is_some_and(|n| (1..=n).contains(&p.verse))
            }
            Test::NotExist(p) if p.part.is_none() => {
                // A scheme without the chapter fails, as a text without it
                // would: it says nothing about that book.
                last(p).is_some_and(|n| !(1..=n).contains(&p.verse))
            }
            _ => true,
        });
        if passes {
            mask |= 1 << column;
        }
    }
    mask
}

/// `Gen.2:25=Last`, `Gen.5:31.2=Exist`, `Neh.4:6*2>Neh.4:3`.
fn test(text: &str, book_order: &[String]) -> Result<Test, String> {
    let text = text.trim();
    let at = text.find(['=', '<', '>']).ok_or("no operator")?;
    let (left, op, right) = (text[..at].trim(), &text[at..=at], text[at + 1..].trim());
    let (left, left_factor) = match place(left, book_order)? {
        Some(found) => found,
        None => return Ok(Test::Unsupported(text.to_owned())),
    };
    if op == "=" {
        if left_factor != 1 {
            return Err("a factor on an existence test".to_owned());
        }
        return Ok(match (right, left.part) {
            ("Last", None) => Test::Last(left),
            ("Exist", _) => Test::Exist(left),
            ("NotExist", _) => Test::NotExist(left),
            ("Last", Some(_)) => Test::Unsupported(text.to_owned()),
            (other, _) => return Err(format!("unknown keyword {other:?}")),
        });
    }
    let (right, right_factor) = match place(right, book_order)? {
        Some(found) => found,
        None => return Ok(Test::Unsupported(text.to_owned())),
    };
    if left.part.is_some() || right.part.is_some() {
        return Ok(Test::Unsupported(text.to_owned()));
    }
    Ok(Test::Compare {
        left,
        left_factor,
        longer: op == ">",
        right,
        right_factor,
    })
}

/// `Neh.4:6*2` → the place and its factor. `None` for a form the sniffer
/// can't evaluate: a range (`Pro.20:20-22`) or part 0.
fn place(text: &str, book_order: &[String]) -> Result<Option<(Place, u8)>, String> {
    let (text, factor) = match text.split_once('*') {
        Some((text, factor)) => (
            text,
            factor
                .parse::<u8>()
                .map_err(|e| format!("factor {factor:?}: {e}"))?,
        ),
        None => (text, 1),
    };
    let (book, rest) = text.split_once('.').ok_or("no book")?;
    let book = book.to_ascii_uppercase();
    if !book_order.contains(&book) {
        return Err(format!("{book} is not in books.tsv"));
    }
    let (chapter, rest) = rest.split_once(':').ok_or("no chapter")?;
    if rest.contains('-') {
        return Ok(None);
    }
    let (verse, part) = match rest.split_once('.') {
        Some((verse, part)) => (verse, Some(part)),
        None => (rest, None),
    };
    let part = match part {
        None => None,
        Some(n) => {
            let n: u8 = n.parse().map_err(|e| format!("part {n:?}: {e}"))?;
            if n == 0 || n > 26 {
                return Ok(None);
            }
            Some(char::from(b'a' + n - 1))
        }
    };
    Ok(Some((
        Place {
            book,
            chapter: chapter
                .parse()
                .map_err(|e| format!("chapter {chapter:?}: {e}"))?,
            verse: verse.parse().map_err(|e| format!("verse {verse:?}: {e}"))?,
            part,
        },
        factor,
    )))
}

pub fn rust(rules: &[Rule], header: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// {header}\n");
    out.push_str("use mise::books::BookKey;\n\nuse crate::sniff::{Place, Rule, Test};\n\n");
    let _ = writeln!(
        out,
        "/// {} rules. `fits[s]` has a bit per column built-in scheme `s` fits.",
        rules.len()
    );
    out.push_str("#[rustfmt::skip]\npub(crate) static RULES: &[Rule] = &[\n");
    for rule in rules {
        let columns: Vec<String> = rule.columns.iter().map(|c| format!("{c:?}")).collect();
        let tests: Vec<String> = rule
            .tests
            .iter()
            .map(|column| {
                let tests: Vec<String> = column.iter().map(test_literal).collect();
                format!("&[{}]", tests.join(", "))
            })
            .collect();
        let fits: Vec<String> = rule.fits.iter().map(|m| format!("{m:#04x}")).collect();
        let _ = writeln!(
            out,
            "    Rule {{ name: {:?}, columns: &[{}], tests: &[{}], fits: &[{}] }},",
            rule.name,
            columns.join(", "),
            tests.join(", "),
            fits.join(", ")
        );
    }
    out.push_str("];\n");
    out
}

fn place_literal(p: &Place) -> String {
    let part = p.part.map_or("None".to_owned(), |c| format!("Some({c:?})"));
    format!(
        "Place {{ book: BookKey::new(*b{:?}), chapter: {}, verse: {}, part: {part} }}",
        p.book, p.chapter, p.verse
    )
}

fn test_literal(t: &Test) -> String {
    match t {
        Test::Last(p) => format!("Test::Last({})", place_literal(p)),
        Test::Exist(p) => format!("Test::Exist({})", place_literal(p)),
        Test::NotExist(p) => format!("Test::NotExist({})", place_literal(p)),
        Test::Compare {
            left,
            left_factor,
            longer,
            right,
            right_factor,
        } => format!(
            "Test::Compare {{ left: {}, left_factor: {left_factor}, longer: {longer}, right: {}, right_factor: {right_factor} }}",
            place_literal(left),
            place_literal(right)
        ),
        Test::Unsupported(text) => format!("Test::Unsupported({text:?})"),
    }
}
