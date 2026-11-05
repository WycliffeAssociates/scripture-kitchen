//! A manifest as a tree of values, in the order the file wrote them.
//!
//! ```text
//! {"b": 1, "a": [true, null]}   →  Map [("b", Number "1"), ("a", List [Bool true, Null])]
//! sort: 01                      →  Map [("sort", Number "01")]      the number as written
//!                                   (to_json writes it as JSON can: `1`)
//! ```
//!
//! JSON and YAML both read into [`Node`], so one set of lenient readers serves
//! every format. A map keeps its keys in file order (a Burrito's ingredients
//! are its books, in the order it lists them), without turning on
//! `serde_json`'s `preserve_order`, which would change key order for every
//! crate in the build. A number keeps the text it was written as, so reading
//! `version: 1.10` as text gives `1.10`, not `1.1`.

use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Null,
    Bool(bool),
    /// As written: `12`, `1.10`, `01`.
    Number(String),
    String(String),
    List(Vec<Node>),
    /// Keys in file order. A key written twice keeps both; [`Node::get`]
    /// answers with the later, as `JSON.parse` does.
    Map(Vec<(String, Node)>),
}

impl Node {
    /// A map's value for `key`.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(entries) => entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// A map's entries, each key once (the later value), in the order the
    /// keys first appear.
    pub fn entries(&self) -> Vec<(&str, &Node)> {
        let Node::Map(entries) = self else {
            return Vec::new();
        };
        let mut out: Vec<(&str, &Node)> = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => out.push((key, value)),
            }
        }
        out
    }

    /// What kind of value this is, for a finding: "a number", "a list".
    pub fn kind(&self) -> &'static str {
        match self {
            Node::Null => "null",
            Node::Bool(_) => "true or false",
            Node::Number(_) => "a number",
            Node::String(_) => "text",
            Node::List(_) => "a list",
            Node::Map(_) => "an object",
        }
    }
}

/// JSON text → [`Node`].
pub fn from_json(bytes: &[u8]) -> Result<Node, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Node, D::Error> {
        d.deserialize_any(NodeVisitor)
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }
    fn visit_none<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }
    fn visit_bool<E>(self, v: bool) -> Result<Node, E> {
        Ok(Node::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Node, E> {
        Ok(Node::Number(v.to_string()))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Node, E> {
        Ok(Node::Number(v.to_string()))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Node, E> {
        Ok(Node::Number(
            serde_json::Number::from_f64(v).map_or_else(|| v.to_string(), |n| n.to_string()),
        ))
    }
    fn visit_str<E>(self, v: &str) -> Result<Node, E> {
        Ok(Node::String(v.to_owned()))
    }
    fn visit_string<E>(self, v: String) -> Result<Node, E> {
        Ok(Node::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Node::List(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut entries = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Node>()? {
            entries.push((key, value));
        }
        Ok(Node::Map(entries))
    }
}

/// What a YAML file holds: its first document that says anything, and how
/// many documents it had.
pub struct YamlRead {
    pub node: Node,
    /// Documents in the file, empty ones included.
    pub documents: usize,
}

/// YAML text → its first non-empty document as a [`Node`].
///
/// Read from yaml-edit's lossless syntax tree directly, so a mapping keeps
/// the file's key order. A scalar's type is YAML 1.2's: `01` is a number,
/// `2025-10-06` and `yes` are text. An alias reads as null (manifests don't
/// use anchors); a key written twice keeps both, and [`Node::get`] answers
/// with the later.
pub fn from_yaml(text: &str) -> Result<YamlRead, String> {
    use std::str::FromStr;
    let file = yaml_edit::YamlFile::from_str(text).map_err(|e| e.to_string())?;
    let mut documents = 0;
    let mut first = None;
    for doc in file.documents() {
        documents += 1;
        if first.is_some() {
            continue;
        }
        let node = if let Some(m) = doc.as_mapping() {
            from_yaml_node(&yaml_edit::YamlNode::Mapping(m))
        } else if let Some(s) = doc.as_sequence() {
            from_yaml_node(&yaml_edit::YamlNode::Sequence(s))
        } else if let Some(s) = doc.as_scalar() {
            from_yaml_node(&yaml_edit::YamlNode::Scalar(s))
        } else {
            Node::Null
        };
        if node != Node::Null {
            first = Some(node);
        }
    }
    Ok(YamlRead {
        node: first.unwrap_or(Node::Null),
        documents,
    })
}

fn from_yaml_node(node: &yaml_edit::YamlNode) -> Node {
    use yaml_edit::{ScalarType, ScalarValue, YamlNode};
    match node {
        YamlNode::Scalar(scalar) => {
            let value = ScalarValue::from_scalar(scalar);
            let text = value.value();
            match value.scalar_type() {
                ScalarType::Null => Node::Null,
                ScalarType::Boolean => value
                    .to_bool()
                    .map_or_else(|| Node::String(text.to_owned()), Node::Bool),
                ScalarType::Integer | ScalarType::Float => Node::Number(text.to_owned()),
                _ => Node::String(text.to_owned()),
            }
        }
        YamlNode::Mapping(m) => Node::Map(
            m.iter()
                .map(|(k, v)| {
                    let key = match &k {
                        YamlNode::Scalar(s) => s.as_string(),
                        other => other.to_string(),
                    };
                    (key, from_yaml_node(&v))
                })
                .collect(),
        ),
        YamlNode::Sequence(s) => Node::List(s.values().map(|v| from_yaml_node(&v)).collect()),
        YamlNode::Alias(_) => Node::Null,
        YamlNode::TaggedNode(t) => {
            if let Some(m) = t.as_mapping() {
                from_yaml_node(&YamlNode::Mapping(m))
            } else if let Some(s) = t.as_sequence() {
                from_yaml_node(&YamlNode::Sequence(s))
            } else if let Some(s) = t.value() {
                from_yaml_node(&YamlNode::Scalar(s))
            } else {
                Node::Null
            }
        }
    }
}

impl Node {
    /// `{"key": value, …}` from pairs.
    pub fn map<K: Into<String>>(entries: impl IntoIterator<Item = (K, Node)>) -> Node {
        Node::Map(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn text(text: impl Into<String>) -> Node {
        Node::String(text.into())
    }

    /// The value for `key` to change, if it's there (the later, if twice).
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Node> {
        let Node::Map(entries) = self else {
            return None;
        };
        entries
            .iter_mut()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// The value for `key` to change, adding it last when it's not there.
    pub fn entry(&mut self, key: &str) -> Option<&mut Node> {
        let Node::Map(entries) = self else {
            return None;
        };
        match entries.iter().rposition(|(k, _)| k == key) {
            Some(i) => Some(&mut entries[i].1),
            None => {
                entries.push((key.to_owned(), Node::Null));
                entries.last_mut().map(|(_, v)| v)
            }
        }
    }

    /// As `JSON.stringify(value, null, 2)` writes it, plus a final newline:
    /// how Sefer writes `metadata.json` today.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        write_json(self, 0, &mut out);
        out.push('\n');
        out
    }
}

fn write_json(node: &Node, depth: usize, out: &mut String) {
    let pad = |n: usize| "  ".repeat(n);
    match node {
        Node::Null => out.push_str("null"),
        Node::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Node::Number(n) => json_number(n, out),
        Node::String(s) => json_string(s, out),
        Node::List(items) if items.is_empty() => out.push_str("[]"),
        Node::List(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                write_json(item, depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push(']');
        }
        Node::Map(_) => {
            let entries = node.entries();
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (key, value)) in entries.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                json_string(key, out);
                out.push_str(": ");
                write_json(value, depth + 1, out);
                out.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push('}');
        }
    }
}

/// A JSON string as `JSON.stringify` escapes it: quotes, backslashes and
/// control characters; everything else as itself.
/// A number as JSON can hold it. One read from JSON already is; YAML writes
/// numbers JSON can't (`01`, `+5`, `0x1F`, `1.`, `.inf`), which are written
/// as their value, or as text when they have no finite one.
fn json_number(text: &str, out: &mut String) {
    if is_json_number(text) {
        out.push_str(text);
        return;
    }
    let unsigned = text.strip_prefix('+').unwrap_or(text);
    let (negative, digits) = match unsigned.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, unsigned),
    };
    let radix = |prefix: &str, radix: u32| {
        digits
            .strip_prefix(prefix)
            .and_then(|d| i128::from_str_radix(&d.replace('_', ""), radix).ok())
    };
    let integer = radix("0x", 16)
        .or_else(|| radix("0o", 8))
        .or_else(|| radix("", 10));
    if let Some(i) = integer {
        out.push_str(&if negative { -i } else { i }.to_string());
        return;
    }
    match unsigned.replace('_', "").parse::<f64>() {
        Ok(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 => {
            out.push_str(&(f as i64).to_string());
        }
        Ok(f) if f.is_finite() => match serde_json::Number::from_f64(f) {
            Some(n) => out.push_str(&n.to_string()),
            None => json_string(text, out),
        },
        _ => json_string(text, out),
    }
}

/// `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`
fn is_json_number(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - start
    };
    let whole = i;
    match digits(&mut i) {
        0 => return false,
        n if n > 1 && b[whole] == b'0' => return false,
        _ => {}
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

pub(crate) fn json_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `at` + `/key`, escaped as a JSON pointer (`~` → `~0`, `/` → `~1`).
pub fn pointer(at: &str, key: &str) -> String {
    format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"))
}

#[cfg(test)]
mod tests;
