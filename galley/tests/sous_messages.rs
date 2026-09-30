//! `sous-messages.ts` and `sous-messages.en.json` name the same ids, every
//! argument a catalog string reads is a parameter `describe` declares for
//! that id in `ParamsById`, and a `Glyph` parameter is shown inside `<g>` and
//! nowhere else.
//!
//! ```text
//! ts    "convention.sentenceStart": { glyph: Glyph; word: string; upper: number } & Spread;
//! json  "convention.sentenceStart": "After <g>{glyph}</g>, … Here “{word}” is lowercase."
//!       arguments {glyph, word, upper, total} ⊆ declared   → ok
//!       inside <g> {glyph} = the Glyph params read         → ok
//! ```
//!
//! Both files are read as text: the TypeScript by its one-id-per-line blocks,
//! the catalog as a flat object of strings, each string by an ICU
//! MessageFormat parser just deep enough to find argument names.

use std::collections::{BTreeMap, BTreeSet};

const TS: &str = include_str!("../sous-messages.ts");
const CATALOG: &str = include_str!("../sous-messages.en.json");

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
            let close = ty.rfind('}').expect("an object type");
            let mut names = fields(&ty[..=close]);
            for extra in ty[close + 1..]
                .split('&')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                names.extend(alias(extra));
            }
            (id, names)
        })
        .collect()
}

/// A flat JSON object of strings, as `json.dumps` or any formatter writes one.
fn catalog() -> BTreeMap<String, String> {
    let mut chars = CATALOG.trim().chars().peekable();
    let mut out = BTreeMap::new();
    let string = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| -> String {
        assert_eq!(
            chars.next(),
            Some('"'),
            "a catalog key or value is a string"
        );
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
    };
    let skip = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
        while chars
            .next_if(|c| c.is_whitespace() || *c == ',' || *c == ':')
            .is_some()
        {}
    };
    assert_eq!(chars.next(), Some('{'), "the catalog is one object");
    loop {
        skip(&mut chars);
        if chars.next_if_eq(&'}').is_some() {
            return out;
        }
        let key = string(&mut chars);
        skip(&mut chars);
        let value = string(&mut chars);
        assert!(
            out.insert(key.clone(), value).is_none(),
            "{key} is in the catalog twice"
        );
    }
}

/// The argument names an ICU message reads, split by whether a `<g>` tag
/// holds them.
#[derive(Debug, Default, PartialEq, Eq)]
struct Read {
    names: BTreeSet<String>,
    in_glyph: BTreeSet<String>,
    outside: BTreeSet<String>,
}

/// Every argument name an ICU message reads, refusing unbalanced braces or
/// tags, a tag other than `<g>`, and a `select` or `plural` with no `other`.
struct Icu<'a> {
    text: &'a [u8],
    at: usize,
    tags: usize,
    read: Read,
}

impl Icu<'_> {
    fn parse(text: &str) -> Result<Read, String> {
        let mut icu = Icu {
            text: text.as_bytes(),
            at: 0,
            tags: 0,
            read: Read::default(),
        };
        icu.message()?;
        if icu.at != icu.text.len() {
            return Err(format!("a stray `}}` at byte {}", icu.at));
        }
        Ok(icu.read)
    }

    /// Text, tags and arguments up to an unmatched `}` or the end. A tag
    /// opened here closes here, as `intl-messageformat` requires.
    fn message(&mut self) -> Result<(), String> {
        let open = self.tags;
        while let Some(&byte) = self.text.get(self.at) {
            match byte {
                b'{' => {
                    self.at += 1;
                    self.argument()?;
                }
                b'}' => break,
                b'<' => self.tag(open)?,
                // `'{` quotes a brace and `'<` a tag; the catalog needs neither.
                b'\'' if matches!(self.text.get(self.at + 1), Some(b'{' | b'}' | b'<')) => {
                    return Err("a quoted brace or tag".into());
                }
                _ => self.at += 1,
            }
        }
        if self.tags != open {
            return Err(format!("a `<g>` never closed before byte {}", self.at));
        }
        Ok(())
    }

    /// `<g>` or `</g>`; `open` is the depth the enclosing message began at.
    fn tag(&mut self, open: usize) -> Result<(), String> {
        let rest = &self.text[self.at..];
        if rest.starts_with(b"<g>") {
            self.tags += 1;
            self.at += 3;
        } else if rest.starts_with(b"</g>") {
            if self.tags == open {
                return Err(format!("a `</g>` with no `<g>` at byte {}", self.at));
            }
            self.tags -= 1;
            self.at += 4;
        } else {
            return Err(format!("a tag other than `<g>` at byte {}", self.at));
        }
        Ok(())
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
    fn argument(&mut self) -> Result<(), String> {
        let name = self.word();
        if name.is_empty() {
            return Err(format!("an argument with no name at byte {}", self.at));
        }
        self.read.names.insert(name.clone());
        if self.tags > 0 {
            self.read.in_glyph.insert(name.clone());
        } else {
            self.read.outside.insert(name.clone());
        }
        if self.text.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(());
        }
        self.eat(b',')?;
        let kind = self.word();
        match kind.as_str() {
            "select" | "plural" | "selectordinal" => {
                self.eat(b',')?;
                let mut other = false;
                loop {
                    let selector = self.word();
                    if selector.is_empty() {
                        break;
                    }
                    if selector.starts_with("offset:") {
                        continue;
                    }
                    other |= selector == "other";
                    self.eat(b'{')?;
                    self.message()?;
                    self.eat(b'}')?;
                }
                if !other {
                    return Err(format!("`{name}` has no `other` branch"));
                }
                self.eat(b'}')
            }
            "number" | "date" | "time" => {
                // A style is free text up to the closing brace.
                while self.text.get(self.at).is_some_and(|b| *b != b'}') {
                    self.at += 1;
                }
                self.eat(b'}')
            }
            other => Err(format!("`{name}` has an unknown type `{other}`")),
        }
    }
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
    for (id, text) in catalog() {
        let used = Icu::parse(&text).unwrap_or_else(|why| panic!("{id}: {why}"));
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
            "{id} reads {unknown:?}, which describe never provides"
        );
    }
}

/// A mark in quotation marks cannot be read when the mark is one (`“"”`), so
/// every `Glyph` is shown inside `<g>`, and `<g>` holds nothing else.
#[test]
fn every_glyph_is_shown_inside_a_glyph_tag_and_nothing_else_is() {
    let declared = declared_params();
    for (id, text) in catalog() {
        let used = Icu::parse(&text).unwrap_or_else(|why| panic!("{id}: {why}"));
        let glyph = |name: &String| declared[&id].get(name).is_some_and(|ty| ty == "Glyph");
        let bare: Vec<_> = used.outside.iter().filter(|name| glyph(name)).collect();
        assert!(bare.is_empty(), "{id} shows {bare:?} outside <g>");
        let tagged: Vec<_> = used.in_glyph.iter().filter(|name| !glyph(name)).collect();
        assert!(tagged.is_empty(), "{id} tags {tagged:?}, which is no Glyph");
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
    for (id, text) in catalog() {
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
            assert!(!said, "{id} says “{word}”");
        }
    }
}

/// The parser itself: a nested branch, a styled number, tags, and the
/// refusals.
#[test]
fn the_icu_reader_finds_names_and_refuses_bad_shapes() {
    let read = Icu::parse(
        "<g>{a}</g> {n, plural, one {only <g>{b}</g>} other {# of {c, number, ::percent .#}}}",
    )
    .unwrap();
    let set = |names: &[&str]| names.iter().copied().map(String::from).collect();
    assert_eq!(read.names, set(&["a", "b", "c", "n"]));
    assert_eq!(read.in_glyph, set(&["a", "b"]));
    assert_eq!(read.outside, set(&["c", "n"]));
    assert!(Icu::parse("{s, select, x {y}}").is_err(), "no other");
    assert!(Icu::parse("{a").is_err(), "unclosed");
    assert!(Icu::parse("a}").is_err(), "stray");
    assert!(Icu::parse("<g>{a}").is_err(), "unclosed tag");
    assert!(Icu::parse("{a}</g>").is_err(), "stray tag");
    assert!(Icu::parse("<g>{n, plural, other {x</g>}}").is_err(), "a tag across a branch");
    assert!(Icu::parse("<b>{a}</b>").is_err(), "a tag other than g");
}
