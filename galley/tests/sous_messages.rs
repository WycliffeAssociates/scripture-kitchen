//! `sous-messages.ts` and `sous-messages.en.json` name the same ids, every
//! entry is a headline and details, every argument either tier reads is a
//! parameter `describe` declares for that id in `ParamsById`, a `Glyph` or
//! `Glyphs` parameter is shown inside `<g>` and nowhere else, every `Glyph`
//! has a `…Kind`, `…Name` and `…Code` beside it, a `select` over a kind
//! names every kind, and every message is whole sentences: plain text, or one
//! choice with nothing beside it whose every branch is again whole.
//!
//! ```text
//! ts    "convention.sentenceStart": { glyph: Glyph; word: string; usualWord: string; upper: number } & Spread;
//! json  "convention.sentenceStart": { "headline": "“{word}” is lowercase after <g>{glyph}</g> here; …",
//!                                     "details": "{count, plural, =1 {After <g>{glyph}</g>, … times.} other {…}}" }
//!       arguments of each tier ⊆ declared                   → ok
//!       inside <g> {glyph} = the Glyph params read          → ok
//!       glyph: Glyph beside glyphKind glyphName glyphCode   → ok
//!       {pool, select, quote {…} … other {…}}               → every PoolName
//!       {count, plural, =1 {…} other {…}} and nothing beside → whole
//!       count =1 → "After [?], this project capitalizes the next word 2162 of 2165 times."
//!                                                           → ends a sentence
//! ```
//!
//! Both files are read as text: the TypeScript by its one-id-per-line blocks,
//! the catalog as a flat object of strings, each string by an ICU
//! MessageFormat parser into a tree, which renders each branch as
//! `intl-messageformat` does in English.

use std::collections::{BTreeMap, BTreeSet};

const TS: &str = include_str!("../sous-messages.ts");
const CATALOG: &str = include_str!("../sous-messages.en.json");
const UNICODE: &str = include_str!("../sous-unicode.ts");

/// The block from `start` to the first line that is exactly `end`.
fn block<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let at = source
        .find(start)
        .unwrap_or_else(|| panic!("no `{start}` in sous-messages.ts"));
    let rest = &source[at + start.len()..];
    let body = &rest[rest.find('\n').map_or(rest.len(), |n| n + 1)..];
    let stop = body
        .lines()
        .position(|line| line == end)
        .unwrap_or_else(|| panic!("`{start}` is never closed by `{end}`"));
    let len: usize = body.lines().take(stop).map(|line| line.len() + 1).sum();
    &body[..len]
}

fn quoted(text: &str) -> Option<&str> {
    let open = text.find('"')?;
    let rest = &text[open + 1..];
    Some(&rest[..rest.find('"')?])
}

fn message_ids() -> BTreeSet<String> {
    let union = block(TS, "export type MessageId =", "");
    union
        .lines()
        .filter_map(|line| line.trim().strip_prefix('|'))
        .map(|member| {
            quoted(member)
                .expect("a MessageId member is a string")
                .to_owned()
        })
        .collect()
}

/// `{ a: T; b: U }` → `{a: T, b: U}`.
fn fields(object: &str) -> BTreeMap<String, String> {
    let inner = object.trim().trim_start_matches('{').trim_end_matches('}');
    inner
        .split(';')
        .filter_map(|field| field.split_once(':'))
        .map(|(name, ty)| (name.trim().to_owned(), ty.trim().to_owned()))
        .collect()
}

/// The fields of `type Name = { … };`.
fn alias(name: &str) -> BTreeMap<String, String> {
    let start = format!("type {name} = ");
    let line = TS
        .lines()
        .find_map(|line| line.strip_prefix(&start))
        .unwrap_or_else(|| panic!("no `{start}` in sous-messages.ts"));
    fields(line.trim_end_matches(';'))
}

fn declared_params() -> BTreeMap<String, BTreeMap<String, String>> {
    let body = block(TS, "export interface ParamsById {", "}");
    body.lines()
        .filter(|line| line.trim_start().starts_with('"'))
        .map(|line| {
            let id = quoted(line).expect("an id").to_owned();
            let ty = line
                .split_once(": ")
                .expect("`id: type`")
                .1
                .trim_end_matches(';');
            let names = ty
                .split('&')
                .map(str::trim)
                .flat_map(|part| {
                    if part.starts_with('{') {
                        fields(part)
                    } else {
                        alias(part)
                    }
                })
                .collect();
            (id, names)
        })
        .collect()
}

/// A JSON value as the catalog uses them: a string, or an object of values.
enum Json {
    Str(String),
    Obj(BTreeMap<String, Json>),
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

fn skip(chars: &mut Chars<'_>) {
    while chars
        .next_if(|c| c.is_whitespace() || *c == ',' || *c == ':')
        .is_some()
    {}
}

fn string(chars: &mut Chars<'_>) -> String {
    assert_eq!(chars.next(), Some('"'), "a catalog key is a string");
    let mut s = String::new();
    loop {
        match chars.next().expect("an unterminated catalog string") {
            '"' => return s,
            '\\' => match chars.next().expect("an escape") {
                'n' => s.push('\n'),
                't' => s.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let unit = u32::from_str_radix(&hex, 16).expect("a \\u escape");
                    s.push(char::from_u32(unit).expect("a BMP scalar"));
                }
                other => s.push(other),
            },
            c => s.push(c),
        }
    }
}

fn value(chars: &mut Chars<'_>) -> Json {
    if chars.next_if_eq(&'{').is_none() {
        return Json::Str(string(chars));
    }
    let mut out = BTreeMap::new();
    loop {
        skip(chars);
        if chars.next_if_eq(&'}').is_some() {
            return Json::Obj(out);
        }
        let key = string(chars);
        skip(chars);
        let inner = value(chars);
        assert!(
            out.insert(key.clone(), inner).is_none(),
            "{key} appears twice"
        );
    }
}

/// The catalog: one `{ headline, details }` object per id, as `json.dumps` or
/// any formatter writes it. Anything else in an entry fails here.
fn catalog() -> BTreeMap<String, [String; 2]> {
    let mut chars = CATALOG.trim().chars().peekable();
    let Json::Obj(entries) = value(&mut chars) else {
        panic!("the catalog is one object")
    };
    entries
        .into_iter()
        .map(|(id, entry)| {
            let Json::Obj(mut tiers) = entry else {
                panic!("{id} is not a {{ headline, details }} object")
            };
            let mut take = |tier: &str| match tiers.remove(tier) {
                Some(Json::Str(text)) => text,
                _ => panic!("{id} has no {tier} string"),
            };
            let pair = [take("headline"), take("details")];
            assert!(
                tiers.is_empty(),
                "{id} has keys beside headline and details"
            );
            (id, pair)
        })
        .collect()
}

/// Every tier of every entry: `(id, "headline" | "details", text)`.
fn tiers() -> Vec<(String, &'static str, String)> {
    catalog()
        .into_iter()
        .flat_map(|(id, [headline, details])| {
            [(id.clone(), "headline", headline), (id, "details", details)]
        })
        .collect()
}

/// Every quoted string in `text`.
fn strings(text: &str) -> BTreeSet<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// The values of every enum a parameter may be typed by: each
/// `export type Name = "a" | "b";` in sous-messages.ts, `MarkKind` from
/// sous-unicode.ts, and `Exclude<MarkKind, …>`.
fn enums() -> BTreeMap<String, BTreeSet<String>> {
    let kinds_line = UNICODE
        .lines()
        .find_map(|line| line.strip_prefix("export const MARK_KINDS = "))
        .expect("sous-unicode.ts lists MARK_KINDS");
    let kinds = strings(kinds_line);
    let mut out = BTreeMap::from([("MarkKind".to_owned(), kinds.clone())]);
    let mut rest = TS;
    while let Some(at) = rest.find("export type ") {
        rest = &rest[at + "export type ".len()..];
        let Some((name, body)) = rest.split_once(" =") else {
            break;
        };
        let body = &body[..body.find(';').expect("a type alias ends")];
        let values = if let Some(excluded) = body.trim().strip_prefix("Exclude<MarkKind,") {
            kinds.difference(&strings(excluded)).cloned().collect()
        } else if body.trim_start().starts_with(['"', '|']) {
            strings(body)
        } else {
            continue;
        };
        out.insert(name.trim().to_owned(), values);
    }
    out
}

/// One piece of a parsed ICU message.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Text(String),
    /// `{name}`, or `{name, number}` with any style.
    Arg(String),
    /// `#`, the number of the plural it stands in.
    Pound,
    /// `<g>…</g>`.
    Glyph(Vec<Node>),
    Choice {
        name: String,
        plural: bool,
        branches: Vec<(String, Vec<Node>)>,
    },
}

/// The argument names an ICU message reads, split by whether a `<g>` tag
/// holds them, and the selectors of every `select`.
#[derive(Debug, Default, PartialEq, Eq)]
struct Read {
    names: BTreeSet<String>,
    in_glyph: BTreeSet<String>,
    outside: BTreeSet<String>,
    selects: Vec<(String, BTreeSet<String>)>,
}

fn read(nodes: &[Node]) -> Read {
    fn walk(nodes: &[Node], in_glyph: bool, out: &mut Read) {
        for node in nodes {
            let name = match node {
                Node::Text(_) | Node::Pound => continue,
                Node::Glyph(inner) => {
                    walk(inner, true, out);
                    continue;
                }
                Node::Arg(name) => name,
                Node::Choice {
                    name,
                    plural,
                    branches,
                } => {
                    if !plural {
                        let selectors = branches.iter().map(|(s, _)| s.clone()).collect();
                        out.selects.push((name.clone(), selectors));
                    }
                    for (_, branch) in branches {
                        walk(branch, in_glyph, out);
                    }
                    name
                }
            };
            out.names.insert(name.clone());
            if in_glyph {
                out.in_glyph.insert(name.clone());
            } else {
                out.outside.insert(name.clone());
            }
        }
    }
    let mut out = Read::default();
    walk(nodes, false, &mut out);
    out
}

/// An ICU message as a tree, refusing unbalanced braces or tags, a tag other
/// than `<g>`, and a `select` or `plural` with no `other` or a selector twice.
struct Icu<'a> {
    text: &'a [u8],
    at: usize,
}

impl Icu<'_> {
    fn parse(text: &str) -> Result<Vec<Node>, String> {
        let mut icu = Icu {
            text: text.as_bytes(),
            at: 0,
        };
        let nodes = icu.message(false)?;
        if icu.at != icu.text.len() {
            return Err(format!("a stray `}}` at byte {}", icu.at));
        }
        Ok(nodes)
    }

    /// Text, tags and arguments up to an unmatched `}`, the `</g>` that
    /// closes `in_tag`, or the end. A tag opened in a branch closes in it, as
    /// `intl-messageformat` requires.
    fn message(&mut self, in_tag: bool) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        let mut text = Vec::new();
        let flush = |text: &mut Vec<u8>, nodes: &mut Vec<Node>| {
            if !text.is_empty() {
                let run = String::from_utf8(std::mem::take(text)).expect("split at ASCII");
                nodes.push(Node::Text(run));
            }
        };
        while let Some(&byte) = self.text.get(self.at) {
            let rest = &self.text[self.at..];
            match byte {
                b'}' => break,
                b'{' => {
                    flush(&mut text, &mut nodes);
                    self.at += 1;
                    nodes.push(self.argument()?);
                }
                b'#' => {
                    flush(&mut text, &mut nodes);
                    self.at += 1;
                    nodes.push(Node::Pound);
                }
                b'<' if rest.starts_with(b"</g>") => {
                    if !in_tag {
                        return Err(format!("a `</g>` with no `<g>` at byte {}", self.at));
                    }
                    flush(&mut text, &mut nodes);
                    self.at += 4;
                    return Ok(nodes);
                }
                b'<' if rest.starts_with(b"<g>") => {
                    flush(&mut text, &mut nodes);
                    self.at += 3;
                    nodes.push(Node::Glyph(self.message(true)?));
                }
                b'<' => return Err(format!("a tag other than `<g>` at byte {}", self.at)),
                // `'{` quotes a brace and `'<` a tag; the catalog needs neither.
                b'\'' if matches!(rest.get(1), Some(b'{' | b'}' | b'<')) => {
                    return Err("a quoted brace or tag".into());
                }
                _ => {
                    text.push(byte);
                    self.at += 1;
                }
            }
        }
        if in_tag {
            return Err(format!("a `<g>` never closed before byte {}", self.at));
        }
        flush(&mut text, &mut nodes);
        Ok(nodes)
    }

    fn word(&mut self) -> String {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
        let start = self.at;
        while self
            .text
            .get(self.at)
            .is_some_and(|b| !b",{}".contains(b) && !b.is_ascii_whitespace())
        {
            self.at += 1;
        }
        let word = String::from_utf8_lossy(&self.text[start..self.at]).into_owned();
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
        word
    }

    fn eat(&mut self, byte: u8) -> Result<(), String> {
        if self.text.get(self.at) == Some(&byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("expected `{}` at byte {}", byte as char, self.at))
        }
    }

    /// After a `{`: `name`, `name, type`, `name, type, style`, or a
    /// `select`/`plural` with its branches, through the closing `}`.
    fn argument(&mut self) -> Result<Node, String> {
        let name = self.word();
        if name.is_empty() {
            return Err(format!("an argument with no name at byte {}", self.at));
        }
        if self.text.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(Node::Arg(name));
        }
        self.eat(b',')?;
        let kind = self.word();
        match kind.as_str() {
            "select" | "plural" | "selectordinal" => {
                self.eat(b',')?;
                let mut branches: Vec<(String, Vec<Node>)> = Vec::new();
                loop {
                    let selector = self.word();
                    if selector.is_empty() {
                        break;
                    }
                    if selector.starts_with("offset:") {
                        continue;
                    }
                    if branches.iter().any(|(s, _)| *s == selector) {
                        return Err(format!("`{name}` names `{selector}` twice"));
                    }
                    self.eat(b'{')?;
                    let branch = self.message(false)?;
                    self.eat(b'}')?;
                    branches.push((selector, branch));
                }
                if !branches.iter().any(|(s, _)| s == "other") {
                    return Err(format!("`{name}` has no `other` branch"));
                }
                self.eat(b'}')?;
                Ok(Node::Choice {
                    name,
                    plural: kind != "select",
                    branches,
                })
            }
            "number" | "date" | "time" => {
                // A style is free text up to the closing brace.
                while self.text.get(self.at).is_some_and(|b| *b != b'}') {
                    self.at += 1;
                }
                self.eat(b'}')?;
                Ok(Node::Arg(name))
            }
            other => Err(format!("`{name}` has an unknown type `{other}`")),
        }
    }
}

fn parsed(id: &str, tier: &str, text: &str) -> Vec<Node> {
    Icu::parse(text).unwrap_or_else(|why| panic!("{id} {tier}: {why}"))
}

/// Whole sentences only: plain text with simple arguments, or exactly one
/// `select` or `plural` with nothing beside it whose every branch is again a
/// whole message. `#` stands only in a plural's own branch; anywhere else
/// `intl-messageformat` prints it as `#`.
fn whole(nodes: &[Node], in_plural: bool) -> Result<(), String> {
    if let [
        Node::Choice {
            name,
            plural,
            branches,
        },
    ] = nodes
    {
        for (selector, branch) in branches {
            whole(branch, *plural).map_err(|why| format!("{name} {selector} → {why}"))?;
        }
        return Ok(());
    }
    plain(nodes, in_plural)
}

fn plain(nodes: &[Node], in_plural: bool) -> Result<(), String> {
    for node in nodes {
        match node {
            Node::Choice { name, .. } => {
                return Err(format!("`{name}` stands beside text, a fragment"));
            }
            Node::Pound if !in_plural => return Err("`#` outside a plural's own branch".into()),
            Node::Glyph(inner) => plain(inner, in_plural)?,
            _ => {}
        }
    }
    Ok(())
}

/// One choice on the way to a leaf: the argument, whether it is a plural,
/// the branch taken, and every branch it offers.
#[derive(Debug, Clone)]
struct Step {
    name: String,
    plural: bool,
    selector: String,
    selectors: Vec<String>,
}

/// Every branch of a whole message that holds no further choice, with the
/// choices that reach it.
fn leaves(nodes: &[Node]) -> Vec<(Vec<Step>, Vec<Node>)> {
    let [
        Node::Choice {
            name,
            plural,
            branches,
        },
    ] = nodes
    else {
        return vec![(Vec::new(), nodes.to_vec())];
    };
    let selectors: Vec<String> = branches.iter().map(|(s, _)| s.clone()).collect();
    branches
        .iter()
        .flat_map(|(selector, branch)| {
            let step = Step {
                name: name.clone(),
                plural: *plural,
                selector: selector.clone(),
                selectors: selectors.clone(),
            };
            leaves(branch).into_iter().map(move |(mut path, leaf)| {
                path.insert(0, step.clone());
                (path, leaf)
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Num(f64),
    Str(String),
}

impl Value {
    fn text(&self) -> String {
        match self {
            Value::Num(n) => n.to_string(),
            Value::Str(s) => s.clone(),
        }
    }
}

/// Parameters that take every step of `path`: a select's own key, a value no
/// select names for `other`, `=N` as N, `one` as 1, and a plural's `other` a
/// number no `=N` and no English `one` takes. An argument the path already
/// set keeps its first value, so a branch two steps contradict is dead.
fn choose(path: &[Step]) -> BTreeMap<String, Value> {
    let mut params = BTreeMap::new();
    for step in path {
        let value = match (step.plural, step.selector.as_str()) {
            (false, "other") => Value::Str("(none of these)".into()),
            (false, key) => Value::Str(key.into()),
            (true, "one") => Value::Num(1.0),
            (true, "other") => {
                let exact: Vec<f64> = step
                    .selectors
                    .iter()
                    .filter_map(|s| s.strip_prefix('=')?.parse().ok())
                    .collect();
                let n = [7.0, 12.0, 100.0, 3.0]
                    .into_iter()
                    .find(|n| !exact.contains(n))
                    .expect("a free number");
                Value::Num(n)
            }
            (true, key) => Value::Num(
                key.strip_prefix('=')
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(f64::NAN),
            ),
        };
        params.entry(step.name.clone()).or_insert(value);
    }
    params
}

/// A message as `intl-messageformat` renders it in English, `<g>` as `[…]`.
fn render(
    nodes: &[Node],
    params: &BTreeMap<String, Value>,
    pound: Option<f64>,
) -> Result<String, String> {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Pound => out.push_str(&pound.ok_or("`#` outside a plural")?.to_string()),
            Node::Arg(name) => {
                out.push_str(&params.get(name).ok_or(format!("no `{name}`"))?.text())
            }
            Node::Glyph(inner) => out.push_str(&format!("[{}]", render(inner, params, pound)?)),
            Node::Choice {
                name,
                plural,
                branches,
            } => {
                let value = params.get(name).ok_or(format!("no `{name}`"))?;
                let branch = |key: &str| branches.iter().find(|(s, _)| s == key).map(|(_, b)| b);
                let other = branch("other").expect("parsed with an other");
                if !plural {
                    out.push_str(&render(
                        branch(&value.text()).unwrap_or(other),
                        params,
                        pound,
                    )?);
                    continue;
                }
                let n = match value {
                    Value::Num(n) => *n,
                    Value::Str(s) => s.parse().map_err(|_| format!("`{name}` is no number"))?,
                };
                let exact = branch(&format!("={n}"));
                let category = if n == 1.0 { branch("one") } else { None };
                out.push_str(&render(
                    exact.or(category).unwrap_or(other),
                    params,
                    Some(n),
                )?);
            }
        }
    }
    Ok(out)
}

/// A stand-in for every argument the path leaves unset, by its declared type.
fn sample(read: &Read, declared: &BTreeMap<String, String>, params: &mut BTreeMap<String, Value>) {
    for name in &read.names {
        let value = match declared.get(name).map(String::as_str) {
            Some("number") => Value::Num(3.0),
            _ => Value::Str(format!("‹{name}›")),
        };
        params.entry(name.clone()).or_insert(value);
    }
}

/// The last sentence of a leaf ends at `.`, `?` or `!`, with only closing
/// quotes or a closing `<g>` after it.
fn ends_a_sentence(text: &str) -> bool {
    text.trim_end_matches(['”', '’', '"', '»', ']'])
        .ends_with(['.', '?', '!'])
}

#[test]
fn every_message_id_has_a_catalog_entry_and_nothing_else_does() {
    let ids = message_ids();
    let catalog: BTreeSet<String> = catalog().into_keys().collect();
    let declared: BTreeSet<String> = declared_params().into_keys().collect();
    assert!(!ids.is_empty(), "no MessageId members were read");
    assert_eq!(
        ids.difference(&catalog).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "ids with no catalog entry"
    );
    assert_eq!(
        catalog.difference(&ids).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "catalog entries with no id"
    );
    assert_eq!(ids, declared, "MessageId and ParamsById disagree");
}

#[test]
fn every_catalog_argument_is_a_declared_parameter() {
    let declared = declared_params();
    for (id, tier, text) in tiers() {
        let used = read(&parsed(&id, tier, &text));
        let params = declared
            .get(&id)
            .unwrap_or_else(|| panic!("{id} declares no parameters"));
        let unknown: Vec<_> = used
            .names
            .iter()
            .filter(|name| !params.contains_key(*name))
            .collect();
        assert!(
            unknown.is_empty(),
            "{id} {tier} reads {unknown:?}, which describe never provides"
        );
    }
}

/// A mark in quotation marks cannot be read when the mark is one (`“"”`), so
/// every `Glyph` or `Glyphs` is shown inside `<g>`, and `<g>` holds nothing
/// else.
#[test]
fn every_glyph_is_shown_inside_a_glyph_tag_and_nothing_else_is() {
    let declared = declared_params();
    for (id, tier, text) in tiers() {
        let used = read(&parsed(&id, tier, &text));
        let glyph = |name: &String| {
            declared[&id]
                .get(name)
                .is_some_and(|ty| ty == "Glyph" || ty == "Glyphs")
        };
        let bare: Vec<_> = used.outside.iter().filter(|name| glyph(name)).collect();
        assert!(bare.is_empty(), "{id} {tier} shows {bare:?} outside <g>");
        let tagged: Vec<_> = used.in_glyph.iter().filter(|name| !glyph(name)).collect();
        assert!(
            tagged.is_empty(),
            "{id} {tier} tags {tagged:?}, which is no Glyph"
        );
    }
}

/// A reader asks "what is a mark?", so every single mark carries its kind,
/// its Unicode name and its code point.
#[test]
fn every_glyph_has_a_kind_a_name_and_a_code_point_beside_it() {
    for (id, params) in declared_params() {
        for (name, ty) in &params {
            if ty != "Glyph" {
                continue;
            }
            for (suffix, want) in [("Kind", "MarkKind"), ("Name", "string"), ("Code", "string")] {
                assert_eq!(
                    params.get(&format!("{name}{suffix}")).map(String::as_str),
                    Some(want),
                    "{id}'s {name} has no {name}{suffix}: {want}"
                );
            }
        }
    }
}

/// A `select` names only values of its parameter's enum, and one over a kind
/// names every kind, so no kind reads as "a punctuation mark" by omission;
/// `other` there stands for the kind `other`.
#[test]
fn every_select_over_an_enum_names_only_its_values_and_every_kind() {
    let declared = declared_params();
    let enums = enums();
    assert!(
        enums["MarkKind"].contains("sentenceEnd"),
        "MarkKind was read"
    );
    assert!(
        !enums["PoolName"].contains("letter"),
        "PoolName excludes letter"
    );
    assert!(
        enums["HygieneName"].contains("noncharacter"),
        "a multi-line enum was read"
    );
    for (id, tier, text) in tiers() {
        let used = read(&parsed(&id, tier, &text));
        for (name, selectors) in &used.selects {
            let Some(ty) = declared[&id].get(name) else {
                continue;
            };
            let Some(values) = enums.get(ty) else {
                continue;
            };
            let stray: Vec<_> = selectors
                .iter()
                .filter(|selector| *selector != "other" && !values.contains(*selector))
                .collect();
            assert!(
                stray.is_empty(),
                "{id} {tier}: {{{name}, select}} names {stray:?}, which is no {ty}"
            );
            if ty == "MarkKind" || ty == "PoolName" {
                let missing: Vec<_> = values.difference(selectors).collect();
                assert!(
                    missing.is_empty(),
                    "{id} {tier}: {{{name}, select}} never names {missing:?}"
                );
            }
        }
    }
}

#[test]
fn the_catalog_never_says_what_the_guide_forbids() {
    const FORBIDDEN: [&str; 10] = [
        "error",
        "wrong",
        "unconventional",
        "channel",
        "run shape",
        "placement",
        "nonletter",
        "band",
        "basis point",
        "site",
    ];
    for (id, tier, text) in tiers() {
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower.split(|c: char| !c.is_alphabetic()).collect();
        for word in FORBIDDEN {
            let said = if word.contains(' ') {
                lower.contains(word)
            } else {
                words
                    .iter()
                    .any(|w| w.strip_suffix('s').unwrap_or(w) == word || *w == word)
            };
            assert!(!said, "{id} {tier} says “{word}”");
        }
    }
}

/// A headline says one fact and at most one alternative; how the check
/// reached it belongs in the details.
#[test]
fn no_headline_explains_the_rule() {
    const INTERNALS: [&str; 5] = [
        "group of this size",
        "stands alone",
        "when another mark follows",
        "of the same kind",
        "usually followed",
    ];
    for (id, [headline, _]) in catalog() {
        for phrase in INTERNALS {
            assert!(!headline.contains(phrase), "{id} headline says “{phrase}”");
        }
    }
}

/// A SentenceStart row counts the mark's handoffs, never the word, so its
/// headline may not say what the project writes instead of the word.
#[test]
fn a_sentence_start_headline_claims_nothing_about_the_word() {
    let [headline, _] = &catalog()["convention.sentenceStart"];
    let used = read(&parsed("convention.sentenceStart", "headline", headline));
    assert!(
        !used.names.contains("usualWord"),
        "the headline reads usualWord: {headline}"
    );
}

/// A straight quote's pair reversed is a swap only under a known facing, so
/// the swapped headline says which way the quote faces; a mark inside a word
/// names no usual class, since the row has none.
#[test]
fn a_straight_quote_swap_names_its_facing_and_inside_names_no_usual() {
    let catalog = catalog();
    let declared = declared_params();
    let [swapped, _] = &catalog["convention.exactNeighbor.swapped"];
    for (path, leaf) in leaves(&parsed("swapped", "headline", swapped)) {
        let facing = &path[0];
        assert_eq!(
            facing.name, "facing",
            "the swapped headline chooses by facing first"
        );
        let text = render(&leaf, &BTreeMap::new(), None).unwrap_or_default();
        let named = match facing.selector.as_str() {
            "opening" => "an opening quote",
            "closing" => "a closing quote",
            _ => continue,
        };
        assert!(swapped.contains(named), "the {named} branch: {text}");
    }
    for id in [
        "convention.exactNeighbor",
        "convention.exactNeighbor.swapped",
        "convention.runShape.swapped",
    ] {
        assert_eq!(
            declared[id].get("facing").map(String::as_str),
            Some("QuoteFacing"),
            "{id}"
        );
    }
    let inside = &declared["convention.placement.inside"];
    assert!(
        inside.contains_key("glyph")
            && inside.contains_key("count")
            && inside.contains_key("book1")
    );
    assert!(
        !inside.contains_key("usual"),
        "a mark inside a word has no usual class"
    );
    for tier in &catalog["convention.placement.inside"] {
        assert!(tier.contains("letter"), "{tier}");
    }
}

/// Rule 1: a translator gets whole sentences, never a fragment glued beside a
/// choice.
#[test]
fn every_message_is_whole_sentences() {
    let broken: Vec<String> = tiers()
        .into_iter()
        .filter_map(|(id, tier, text)| {
            let why = whole(&parsed(&id, tier, &text), false).err()?;
            Some(format!("{id} {tier}: {why}"))
        })
        .collect();
    assert!(
        broken.is_empty(),
        "{} tiers are not whole sentences:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

/// Every branch is reached by some parameters, renders with no argument
/// missing, and ends a sentence, so no branch is dead or a fragment.
#[test]
fn every_branch_renders_a_sentence_from_some_parameters() {
    let declared = declared_params();
    let mut broken = Vec::new();
    let mut branches = 0;
    for (id, tier, text) in tiers() {
        let nodes = parsed(&id, tier, &text);
        let all = read(&nodes);
        for (path, leaf) in leaves(&nodes) {
            branches += 1;
            let mut params = choose(&path);
            sample(&all, &declared[&id], &mut params);
            let pound =
                path.last()
                    .filter(|step| step.plural)
                    .and_then(|step| match params[&step.name] {
                        Value::Num(n) => Some(n),
                        Value::Str(_) => None,
                    });
            let route: Vec<String> = path
                .iter()
                .map(|step| format!("{} {}", step.name, step.selector))
                .collect();
            let at = format!("{id} {tier} [{}]", route.join(" › "));
            match (render(&nodes, &params, None), render(&leaf, &params, pound)) {
                (Err(why), _) | (_, Err(why)) => broken.push(format!("{at}: {why}")),
                (Ok(all), Ok(own)) if all != own => {
                    broken.push(format!("{at}: no parameters reach it"));
                }
                (Ok(all), _) if all.trim() != all || !ends_a_sentence(&all) => {
                    broken.push(format!("{at}: “{all}” is no whole sentence"));
                }
                _ => {}
            }
        }
    }
    assert!(branches > 0, "no branch was read");
    assert!(
        broken.is_empty(),
        "{} of {branches} branches:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

/// Rule 2: where branches would multiply, `describe` picks a more specific
/// id, each a few whole sentences.
#[test]
fn no_message_has_more_than_sixteen_branches() {
    for (id, tier, text) in tiers() {
        let n = leaves(&parsed(&id, tier, &text)).len();
        assert!(
            n <= 16,
            "{id} {tier} has {n} branches; split it into ids describe chooses among"
        );
    }
}

/// The parser itself: a nested branch, a styled number, tags, and the
/// refusals.
#[test]
fn the_icu_reader_finds_names_and_refuses_bad_shapes() {
    let read = read(
        &Icu::parse(
            "<g>{a}</g> {n, plural, one {only <g>{b}</g>} other {# of {c, number, ::percent .#}}}",
        )
        .unwrap(),
    );
    let set = |names: &[&str]| names.iter().copied().map(String::from).collect();
    assert_eq!(read.names, set(&["a", "b", "c", "n"]));
    assert_eq!(read.in_glyph, set(&["a", "b"]));
    assert_eq!(read.outside, set(&["c", "n"]));
    assert!(Icu::parse("{s, select, x {y}}").is_err(), "no other");
    assert!(
        Icu::parse("{s, select, x {y} x {z} other {w}}").is_err(),
        "twice"
    );
    assert!(Icu::parse("{a").is_err(), "unclosed");
    assert!(Icu::parse("a}").is_err(), "stray");
    assert!(Icu::parse("<g>{a}").is_err(), "unclosed tag");
    assert!(Icu::parse("{a}</g>").is_err(), "stray tag");
    assert!(
        Icu::parse("<g>{n, plural, other {x</g>}}").is_err(),
        "a tag across a branch"
    );
    assert!(Icu::parse("<b>{a}</b>").is_err(), "a tag other than g");
}

/// Whole sentences: a choice with text beside it is a fragment, a choice
/// nested as a whole branch is not, and `#` belongs to its own plural.
#[test]
fn whole_sentences_refuse_a_fragment_and_take_a_nested_choice() {
    let check = |text: &str| whole(&Icu::parse(text).unwrap(), false);
    assert!(check("Plain <g>{a}</g> and {n, number}.").is_ok());
    assert!(check("{n, plural, =1 {One.} other {# of them.}}").is_ok());
    assert!(check("{s, select, x {{n, plural, other {# here.}}} other {None.}}").is_ok());
    assert!(
        check("{n, plural, other {# here}} today.").is_err(),
        "text beside"
    );
    assert!(check(" {s, select, other {A.}}").is_err(), "a space beside");
    assert!(
        check("{n, plural, other {{s, select, other {# here.}}}}").is_err(),
        "# in a select"
    );
    assert!(check("# here.").is_err(), "# outside a plural");
    assert!(
        check("<g>{s, select, other {x}}</g>.").is_err(),
        "a choice in a tag"
    );

    let nodes =
        Icu::parse("{s, select, x {{n, plural, =1 {One.} other {# x.}}} other {None.}}").unwrap();
    let found = leaves(&nodes);
    let routes: Vec<String> = found
        .iter()
        .map(|(path, _)| {
            let steps: Vec<String> = path
                .iter()
                .map(|s| format!("{} {}", s.name, s.selector))
                .collect();
            steps.join(" › ")
        })
        .collect();
    assert_eq!(routes, ["s x › n =1", "s x › n other", "s other"]);
    assert_eq!(render(&nodes, &choose(&found[1].0), None).unwrap(), "7 x.");
    let dead =
        Icu::parse("{n, plural, =1 {{n, plural, =1 {A.} other {Never.}}} other {B.}}").unwrap();
    let [_, (path, _), _] = &leaves(&dead)[..] else {
        panic!("three leaves")
    };
    assert_ne!(
        render(&dead, &choose(path), None).unwrap(),
        "Never.",
        "a contradicted branch is dead"
    );

    assert!(ends_a_sentence("He said “Go.”"));
    assert!(ends_a_sentence("[x] is here."));
    assert!(!ends_a_sentence("[x] is here"));
    assert!(!ends_a_sentence("all in [ISA]"));
}
