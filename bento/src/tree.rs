//! A manifest as a tree of values, in the order the file wrote them.
//!
//! ```text
//! {"b": 1, "a": [true, null]}   →  Map [("b", Number "1"), ("a", List [Bool true, Null])]
//! sort: 01                      →  Map [("sort", Number "01")]      the number as written
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
/// Mappings come back in key order (YAML's own reading sorts them, which
/// doesn't matter for a manifest's fields; lists keep their order). A
/// scalar's type is YAML 1.2's: `01` is a number, `2025-10-06` and `yes`
/// are text.
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
        let value = yaml_edit::YamlValue::from_document(&doc).map_err(|e| e.to_string())?;
        let node = from_yaml_value(&value);
        if node != Node::Null {
            first = Some(node);
        }
    }
    Ok(YamlRead {
        node: first.unwrap_or(Node::Null),
        documents,
    })
}

fn from_yaml_value(value: &yaml_edit::YamlValue) -> Node {
    use yaml_edit::{ScalarType, YamlValue};
    match value {
        YamlValue::Scalar(scalar) => {
            let text = scalar.value();
            match scalar.scalar_type() {
                ScalarType::Null => Node::Null,
                ScalarType::Boolean => scalar
                    .to_bool()
                    .map_or_else(|| Node::String(text.to_owned()), Node::Bool),
                ScalarType::Integer | ScalarType::Float => Node::Number(text.to_owned()),
                _ => Node::String(text.to_owned()),
            }
        }
        YamlValue::Sequence(items) => Node::List(items.iter().map(from_yaml_value).collect()),
        YamlValue::Mapping(map) => Node::Map(
            map.iter()
                .map(|(k, v)| (k.clone(), from_yaml_value(v)))
                .collect(),
        ),
        YamlValue::OrderedMapping(pairs) | YamlValue::Pairs(pairs) => Node::Map(
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), from_yaml_value(v)))
                .collect(),
        ),
        YamlValue::Set(items) => Node::List(items.iter().cloned().map(Node::String).collect()),
        YamlValue::Tagged(_, inner) => from_yaml_value(inner),
    }
}

/// `at` + `/key`, escaped as a JSON pointer (`~` → `~0`, `/` → `~1`).
pub fn pointer(at: &str, key: &str) -> String {
    format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"))
}

#[cfg(test)]
mod tests;
