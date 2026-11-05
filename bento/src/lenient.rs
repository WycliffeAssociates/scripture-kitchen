//! Reading values the way real manifests write them, and saying so.
//!
//! ```text
//! "version": 12            string(…)   → Some("12")        + info: a number where text belongs
//! "relation": "en/tn"      strings(…)  → ["en/tn"]         + info: one value where a list belongs
//! sort: "01"               int(…)      → Some(1)           + info: text where a number belongs
//! "title": {"a": 1}        string(…)   → None              + warning: an object where text belongs
//! ```
//!
//! Every value a format reads goes through a [`Reader`], which keeps what it
//! had to forgive as [`ManifestFinding`]s with a JSON pointer to the value.
//! Nothing is changed in the file: the finding says what was there.

use crate::tree::{Node, pointer};
use crate::{Check, LocalizedText, ManifestFinding, Severity};

pub(crate) fn finding(
    check: Check,
    severity: Severity,
    at: Option<String>,
    message: String,
) -> ManifestFinding {
    ManifestFinding {
        check,
        severity,
        at,
        book: None,
        message,
    }
}

pub(crate) struct Reader {
    findings: Vec<ManifestFinding>,
}

impl Reader {
    pub fn new(findings: Vec<ManifestFinding>) -> Reader {
        Reader { findings }
    }

    pub fn into_findings(self) -> Vec<ManifestFinding> {
        self.findings
    }

    /// A finding at `at`.
    pub fn note(&mut self, check: Check, severity: Severity, at: &str, message: String) {
        self.findings
            .push(finding(check, severity, Some(at.to_owned()), message));
    }

    /// A field its format requires is missing: a [`Check::ManifestValid`] warning.
    pub fn required(&mut self, at: &str, what: &str) {
        self.note(
            Check::ManifestValid,
            Severity::Warning,
            at,
            format!("{what} is required and missing."),
        );
    }

    fn wrong(&mut self, at: &str, node: &Node, wanted: &str) {
        self.note(
            Check::ManifestValid,
            Severity::Warning,
            at,
            format!(
                "Expected {wanted} at {at}, found {}; it was left unread.",
                node.kind()
            ),
        );
    }

    fn forgiven(&mut self, at: &str, message: String) {
        self.note(Check::LenientValue, Severity::Info, at, message);
    }

    /// Text. A number or `true`/`false` reads as its text (a finding says
    /// so); null or absent is `None`; a list or an object is `None` and a
    /// warning.
    pub fn string(&mut self, node: Option<&Node>, at: &str) -> Option<String> {
        match node? {
            Node::Null => None,
            Node::String(s) => Some(s.clone()),
            Node::Number(n) => {
                self.forgiven(
                    at,
                    format!("{at} is the number {n}, read as the text \"{n}\"."),
                );
                Some(n.clone())
            }
            Node::Bool(b) => {
                self.forgiven(at, format!("{at} is {b}, read as the text \"{b}\"."));
                Some(b.to_string())
            }
            other => {
                self.wrong(at, other, "text");
                None
            }
        }
    }

    /// `parent[key]` as text.
    pub fn field(&mut self, parent: &Node, key: &str, at: &str) -> Option<String> {
        self.string(parent.get(key), &pointer(at, key))
    }

    /// A list of text. One value reads as a list of one (a finding says so);
    /// entries that aren't text are left out, with a warning each.
    pub fn strings(&mut self, node: Option<&Node>, at: &str) -> Vec<String> {
        match node {
            None | Some(Node::Null) => Vec::new(),
            Some(Node::List(items)) => items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| self.string(Some(item), &format!("{at}/{i}")))
                .collect(),
            Some(single @ (Node::String(_) | Node::Number(_) | Node::Bool(_))) => {
                self.forgiven(at, format!("{at} is one value, read as a list of one."));
                self.string(Some(single), at).into_iter().collect()
            }
            Some(other) => {
                self.wrong(at, other, "a list");
                Vec::new()
            }
        }
    }

    /// A whole number. Text of digits reads as its number (a finding says so).
    pub fn int(&mut self, node: Option<&Node>, at: &str) -> Option<i64> {
        match node? {
            Node::Null => None,
            Node::Number(n) => match n.parse() {
                Ok(n) => Some(n),
                Err(_) => {
                    self.note(
                        Check::ManifestValid,
                        Severity::Warning,
                        at,
                        format!("{at} is {n}, not a whole number; it was left unread."),
                    );
                    None
                }
            },
            Node::String(s) => match s.trim().parse() {
                Ok(n) => {
                    self.forgiven(
                        at,
                        format!("{at} is the text \"{s}\", read as the number {n}."),
                    );
                    Some(n)
                }
                Err(_) => {
                    self.note(
                        Check::ManifestValid,
                        Severity::Warning,
                        at,
                        format!("{at} is \"{s}\", not a number; it was left unread."),
                    );
                    None
                }
            },
            other => {
                self.wrong(at, other, "a number");
                None
            }
        }
    }

    /// An object, or `None` (with a warning when something else is there).
    pub fn map<'n>(&mut self, node: Option<&'n Node>, at: &str) -> Option<&'n Node> {
        match node? {
            Node::Null => None,
            map @ Node::Map(_) => Some(map),
            other => {
                self.wrong(at, other, "an object");
                None
            }
        }
    }

    /// A list's items, or none (with a warning when something else is there).
    pub fn list<'n>(&mut self, node: Option<&'n Node>, at: &str) -> &'n [Node] {
        match node {
            None | Some(Node::Null) => &[],
            Some(Node::List(items)) => items,
            Some(other) => {
                self.wrong(at, other, "a list");
                &[]
            }
        }
    }

    /// `{"en": "Romans", …}`: an object of text by language tag.
    pub fn localized(&mut self, node: Option<&Node>, at: &str) -> LocalizedText {
        let Some(map) = self.map(node, at) else {
            return LocalizedText::new();
        };
        map.entries()
            .into_iter()
            .filter_map(|(tag, text)| {
                Some((tag.to_owned(), self.string(Some(text), &pointer(at, tag))?))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
