//! `markers.ext` — the spec's definition file for user `\z` markers, read into
//! the list the marker registry takes.
//!
//! ```text
//! parse_markers_ext("\\marker zmyp\n\\category versepara\n\\description A paragraph.\n")
//!     -> markers:   [CustomMarker { name: "zmyp", category: VersePara, description: "A paragraph.", attributes: [] }]
//!        malformed: []
//!
//! parse_markers_ext("\\marker foo\n\\category char\n")
//!     -> markers:   []
//!        malformed: [Malformed { line: 1, name: Some("foo"), reason: "name does not start with z" }]
//! ```
//!
//! One field per line: `\marker`, `\category`, `\description`, `\attribute`
//! (repeatable), in the spec's order or any other. `\marker` opens an entry;
//! the rest attach to the open one. An entry is kept when it has a legal name
//! and a known category; `\description` is optional and `\attribute` is "as
//! needed". Everything that cannot be kept becomes one flat [`Malformed`] and
//! costs only itself — never the file.
//!
//! Source: docs.usfm.bible/usfm/3.2/extensions.html, "Defining Extensions".
//! The category words are the spec's; what each one MEANS to a parser is the
//! engine's business, which is why this module knows only the names.

use core::fmt;

/// The spec's `\category` words. `Attribute` and `Internal` describe USX
/// internals and have no USFM behaviour; they parse so a valid file never
/// errors, and the engine decides what to do with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtensionCategory {
    // Paragraph
    Header,
    Title,
    Introduction,
    SectionPara,
    VersePara,
    List,
    OtherPara,
    // Note
    CrossReference,
    Footnote,
    // Character
    Char,
    IntroChar,
    ListChar,
    FootnoteChar,
    CrossReferenceChar,
    // Milestone
    Milestone,
    // Other
    Attribute,
    Cell,
    Standalone,
    Internal,
}

impl ExtensionCategory {
    /// Every category, in the spec page's order.
    pub const ALL: [Self; 19] = [
        Self::Header,
        Self::Title,
        Self::Introduction,
        Self::SectionPara,
        Self::VersePara,
        Self::List,
        Self::OtherPara,
        Self::CrossReference,
        Self::Footnote,
        Self::Char,
        Self::IntroChar,
        Self::ListChar,
        Self::FootnoteChar,
        Self::CrossReferenceChar,
        Self::Milestone,
        Self::Attribute,
        Self::Cell,
        Self::Standalone,
        Self::Internal,
    ];

    /// The word as the spec spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::Title => "title",
            Self::Introduction => "introduction",
            Self::SectionPara => "sectionpara",
            Self::VersePara => "versepara",
            Self::List => "list",
            Self::OtherPara => "otherpara",
            Self::CrossReference => "crossreference",
            Self::Footnote => "footnote",
            Self::Char => "char",
            Self::IntroChar => "introchar",
            Self::ListChar => "listchar",
            Self::FootnoteChar => "footnotechar",
            Self::CrossReferenceChar => "crossreferencechar",
            Self::Milestone => "milestone",
            Self::Attribute => "attribute",
            Self::Cell => "cell",
            Self::Standalone => "standalone",
            Self::Internal => "internal",
        }
    }

    /// The spec word, byte-exact and lowercase, to its category.
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == word)
    }
}

impl fmt::Display for ExtensionCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a marker NAME is not usable, or `None` when it is.
///
/// The spec's rule, in one place: non-empty, `z`-initial, ASCII alphanumeric
/// throughout. Both this reader and the engine's registry ask it — the file is
/// one way a name arrives and a host's own list is another, and a name legal
/// in one has to be legal in the other.
///
/// The reasons a name can fail are these three; duplicate-name and
/// missing-category rules belong to whoever is assembling a list and stay
/// there.
pub fn check_name(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("\\marker with no name")
    } else if !name.starts_with('z') {
        Some("name does not start with z")
    } else if !name.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Some("name is not ASCII alphanumeric")
    } else {
        None
    }
}

/// One `\marker` entry the file defines and this reader accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomMarker {
    /// As spelled after `\marker`, leading `\` dropped: `zmyp`, never `\zmyp`.
    /// Starts with `z`, ASCII alphanumeric throughout.
    pub name: String,
    pub category: ExtensionCategory,
    /// Empty when the file gave none.
    pub description: String,
    /// `\attribute` values in file order, verbatim.
    pub attributes: Vec<String>,
}

/// One thing the file said that could not be kept. Flat on purpose: a host
/// shows `reason` next to `name` and that is the whole UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed {
    /// 1-based line of the offending field, or of the `\marker` line for an
    /// entry dropped as a whole.
    pub line: u32,
    /// The entry's name when one was read.
    pub name: Option<String>,
    pub reason: &'static str,
}

/// The whole file, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkersExt {
    pub markers: Vec<CustomMarker>,
    pub malformed: Vec<Malformed>,
}

/// An entry under construction: the `\marker` line has been seen, the rest
/// may still arrive.
struct Open {
    line: u32,
    name: String,
    category: Option<ExtensionCategory>,
    description: String,
    attributes: Vec<String>,
}

/// Reads a whole `markers.ext`. Line endings may be LF or CRLF; blank lines
/// are skipped; a UTF-8 BOM on the first line is ignored.
pub fn parse_markers_ext(text: &str) -> MarkersExt {
    let mut out = MarkersExt::default();
    let mut open: Option<Open> = None;

    for (index, raw) in text.split('\n').enumerate() {
        let line = index as u32 + 1;
        let mut trimmed = raw.trim_end_matches('\r').trim();
        if index == 0 {
            trimmed = trimmed.trim_start_matches('\u{feff}');
        }
        if trimmed.is_empty() {
            continue;
        }

        let Some(field) = trimmed.strip_prefix('\\') else {
            out.malformed.push(Malformed {
                line,
                name: None,
                reason: "line does not start with a \\field",
            });
            continue;
        };
        let (key, value) = match field.find(char::is_whitespace) {
            Some(at) => (&field[..at], field[at..].trim()),
            None => (field, ""),
        };

        match key {
            "marker" => {
                if let Some(done) = open.take() {
                    close(done, &mut out);
                }
                let name = value.strip_prefix('\\').unwrap_or(value);
                open = Some(Open {
                    line,
                    name: name.to_owned(),
                    category: None,
                    description: String::new(),
                    attributes: Vec::new(),
                });
            }
            "category" | "description" | "attribute" => {
                let Some(entry) = open.as_mut() else {
                    out.malformed.push(Malformed {
                        line,
                        name: None,
                        reason: "field before any \\marker",
                    });
                    continue;
                };
                match key {
                    "category" => match ExtensionCategory::parse(value) {
                        Some(category) if entry.category.is_none() => {
                            entry.category = Some(category)
                        }
                        Some(_) => out.malformed.push(Malformed {
                            line,
                            name: Some(entry.name.clone()),
                            reason: "second \\category for one marker",
                        }),
                        None => out.malformed.push(Malformed {
                            line,
                            name: Some(entry.name.clone()),
                            reason: "unknown category word",
                        }),
                    },
                    "description" => entry.description = value.to_owned(),
                    _ => {
                        if value.is_empty() {
                            out.malformed.push(Malformed {
                                line,
                                name: Some(entry.name.clone()),
                                reason: "\\attribute with no name",
                            });
                        } else {
                            entry.attributes.push(value.to_owned());
                        }
                    }
                }
            }
            _ => {
                let name = open.as_ref().map(|e| e.name.clone());
                out.malformed.push(Malformed {
                    line,
                    name,
                    reason: "unknown field",
                });
            }
        }
    }
    if let Some(done) = open.take() {
        close(done, &mut out);
    }
    out
}

/// Keeps or drops one finished entry. Name legality first, then the
/// category, then uniqueness — one reason per dropped entry, the first that
/// applies.
fn close(entry: Open, out: &mut MarkersExt) {
    let reason = if let Some(reason) = check_name(&entry.name) {
        Some(reason)
    } else if entry.category.is_none() {
        Some("no \\category")
    } else if out.markers.iter().any(|m| m.name == entry.name) {
        Some("duplicate name; the first definition stands")
    } else {
        None
    };
    match reason {
        Some(reason) => out.malformed.push(Malformed {
            line: entry.line,
            name: Some(entry.name),
            reason,
        }),
        None => out.markers.push(CustomMarker {
            name: entry.name,
            // SAFETY of the unwrap: `None` was handled as a reason above.
            category: entry.category.unwrap_or(ExtensionCategory::Internal),
            description: entry.description,
            attributes: entry.attributes,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> CustomMarker {
        let read = parse_markers_ext(text);
        assert!(read.malformed.is_empty(), "{:?}", read.malformed);
        assert_eq!(read.markers.len(), 1, "{:?}", read.markers);
        read.markers.into_iter().next().unwrap()
    }

    fn reasons(text: &str) -> Vec<&'static str> {
        parse_markers_ext(text)
            .malformed
            .into_iter()
            .map(|m| m.reason)
            .collect()
    }

    #[test]
    fn the_spec_paragraph_example_reads() {
        let m = one(
            "\\marker zmyp\n\\category versepara\n\\description An paragraph marker extension.\n",
        );
        assert_eq!(m.name, "zmyp");
        assert_eq!(m.category, ExtensionCategory::VersePara);
        assert_eq!(m.description, "An paragraph marker extension.");
        assert!(m.attributes.is_empty());
    }

    #[test]
    fn the_spec_character_example_keeps_its_attribute() {
        let m = one(
            "\\marker zmyc\n\\category char\n\\description A character marker extension.\n\\attribute x-myattr1\n",
        );
        assert_eq!(m.category, ExtensionCategory::Char);
        assert_eq!(m.attributes, vec!["x-myattr1".to_owned()]);
    }

    #[test]
    fn every_category_word_round_trips() {
        for category in ExtensionCategory::ALL {
            assert_eq!(ExtensionCategory::parse(category.as_str()), Some(category));
            let text = format!("\\marker zx\n\\category {category}\n");
            assert_eq!(one(&text).category, category);
        }
        assert_eq!(ExtensionCategory::parse("Header"), None);
        assert_eq!(ExtensionCategory::parse("para"), None);
    }

    #[test]
    fn description_is_optional_and_attributes_repeat() {
        let m = one("\\marker zw\n\\category char\n\\attribute x-a\n\\attribute z-b\n");
        assert_eq!(m.description, "");
        assert_eq!(m.attributes, vec!["x-a".to_owned(), "z-b".to_owned()]);
    }

    #[test]
    fn several_entries_crlf_bom_blank_lines_and_a_stray_backslash() {
        let text = "\u{feff}\\marker \\zone\r\n\\category footnote\r\n\r\n\\marker ztwo\r\n\\category milestone\r\n";
        let read = parse_markers_ext(text);
        assert!(read.malformed.is_empty(), "{:?}", read.malformed);
        let names: Vec<&str> = read.markers.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["zone", "ztwo"]);
        assert_eq!(read.markers[0].category, ExtensionCategory::Footnote);
    }

    #[test]
    fn a_bad_entry_costs_only_itself() {
        let text = "\\marker foo\n\\category char\n\\marker zok\n\\category char\n\\marker znocat\n\\description x\n";
        let read = parse_markers_ext(text);
        assert_eq!(read.markers.len(), 1);
        assert_eq!(read.markers[0].name, "zok");
        assert_eq!(
            read.malformed,
            vec![
                Malformed {
                    line: 1,
                    name: Some("foo".into()),
                    reason: "name does not start with z"
                },
                Malformed {
                    line: 5,
                    name: Some("znocat".into()),
                    reason: "no \\category"
                },
            ]
        );
    }

    #[test]
    fn the_first_definition_of_a_name_stands() {
        let read =
            parse_markers_ext("\\marker za\n\\category char\n\\marker za\n\\category footnote\n");
        assert_eq!(read.markers.len(), 1);
        assert_eq!(read.markers[0].category, ExtensionCategory::Char);
        assert_eq!(
            read.malformed[0].reason,
            "duplicate name; the first definition stands"
        );
        assert_eq!(read.malformed[0].line, 3);
    }

    #[test]
    fn field_level_faults_are_reported_and_the_entry_survives_when_it_can() {
        assert_eq!(
            reasons(
                "\\category char\nhello\n\\marker zq\n\\category nope\n\\category char\n\\category footnote\n\\attribute\n\\bogus 1\n"
            ),
            [
                "field before any \\marker",
                "line does not start with a \\field",
                "unknown category word",
                "second \\category for one marker",
                "\\attribute with no name",
                "unknown field",
            ]
        );
        let read = parse_markers_ext("\\marker zq\n\\category nope\n\\category char\n");
        assert_eq!(read.markers[0].category, ExtensionCategory::Char);
    }

    #[test]
    fn name_legality() {
        assert_eq!(
            reasons("\\marker\n\\category char\n"),
            ["\\marker with no name"]
        );
        assert_eq!(
            reasons("\\marker z-a\n\\category char\n"),
            ["name is not ASCII alphanumeric"]
        );
        assert_eq!(
            reasons("\\marker zé\n\\category char\n"),
            ["name is not ASCII alphanumeric"]
        );
        assert!(reasons("\\marker z9\n\\category cell\n").is_empty());
    }

    #[test]
    fn an_empty_file_is_empty() {
        assert_eq!(parse_markers_ext(""), MarkersExt::default());
        assert_eq!(parse_markers_ext("\n\n"), MarkersExt::default());
    }
}
