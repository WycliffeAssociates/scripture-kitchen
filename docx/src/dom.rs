//! One XML part read into a small tree, for import to walk.
//!
//! ```text
//! <w:p><w:r><w:t>a &amp; b</w:t></w:r></w:p>
//!   ->  Element { name: "w:p", children: [Element { name: "w:r", children:
//!         [Element { name: "w:t", children: [Text("a & b")] }] }] }
//! ```
//!
//! Names stay qualified as written (`w:p`). Word, LibreOffice and Google Docs
//! all write WordprocessingML with the `w` prefix.

use quick_xml::{Reader, XmlVersion};
use quick_xml::events::{BytesStart, Event};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// All text below this element, in document order.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.push_text(&mut out);
        out
    }

    fn push_text(&self, out: &mut String) {
        for n in &self.children {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => e.push_text(out),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError {
    pub part: String,
    pub message: String,
}

/// Parses one part; the root element is returned.
pub fn parse(part: &str, xml: &[u8]) -> Result<Element, XmlError> {
    let fail = |message: String| XmlError { part: part.to_owned(), message };
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = vec![Element::default()];
    loop {
        let event = reader.read_event().map_err(|e| fail(e.to_string()))?;
        match event {
            Event::Start(start) => stack.push(open(&start).map_err(fail)?),
            Event::Empty(start) => {
                let e = open(&start).map_err(fail)?;
                push(&mut stack, Node::Element(e));
            }
            Event::End(_) => {
                let done = stack.pop().filter(|_| !stack.is_empty());
                let done = done.ok_or_else(|| fail("unbalanced end tag".to_owned()))?;
                push(&mut stack, Node::Element(done));
            }
            Event::Text(t) => {
                let s = t.xml10_content();
                push_str(&mut stack, &s);
            }
            Event::CData(c) => {
                let s = c.into_inner().into_owned();
                push_str(&mut stack, &s);
            }
            Event::GeneralRef(r) => {
                let c = match r.resolve_char_ref().map_err(|e| fail(e.to_string()))? {
                    Some(c) => c,
                    None => match r.as_ref() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" => '\'',
                        other => {
                            return Err(fail(format!("unknown entity &{other};")));
                        }
                    },
                };
                push_str(&mut stack, c.encode_utf8(&mut [0; 4]));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop().filter(|_| stack.is_empty());
    let root = root.take().ok_or_else(|| fail("unclosed element".to_owned()))?;
    root.elements()
        .next()
        .cloned()
        .ok_or_else(|| fail("no root element".to_owned()))
}

fn open(start: &BytesStart<'_>) -> Result<Element, String> {
    let name = start.name().as_ref().to_owned();
    let mut attrs = Vec::new();
    for a in start.attributes().with_checks(false) {
        let a = a.map_err(|e| e.to_string())?;
        let key = a.key.as_ref().to_owned();
        let value = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?
            .into_owned();
        attrs.push((key, value));
    }
    Ok(Element { name, attrs, children: Vec::new() })
}

fn push(stack: &mut [Element], node: Node) {
    if let Some(top) = stack.last_mut() {
        top.children.push(node);
    }
}

fn push_str(stack: &mut [Element], s: &str) {
    if let Some(top) = stack.last_mut() {
        if let Some(Node::Text(t)) = top.children.last_mut() {
            t.push_str(s);
        } else {
            top.children.push(Node::Text(s.to_owned()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_entities_join() {
        let root = parse("t", br#"<?xml version="1.0"?><w:t a="x &amp; y">a &amp; b&#x41;</w:t>"#)
            .unwrap();
        assert_eq!(root.name, "w:t");
        assert_eq!(root.attr("a"), Some("x & y"));
        assert_eq!(root.children, vec![Node::Text("a & bA".to_owned())]);
    }
}
