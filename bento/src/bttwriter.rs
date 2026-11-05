//! BTT Writer's `manifest.json`: one book project.
//!
//! ```text
//! {"target_language": {"id": "tmf", "direction": "ltr"},
//!  "project": {"id": "3jn", "name": "3 John"},
//!  "resource": {"id": "reg"},
//!  "source_translations": [{"language_id": "en", "resource_id": "ulb", "version": "12"}],
//!  "translators": ["kpass"], "finished_chunks": ["01-01", "01-05"]}
//!
//! read(bytes)                              → BttWriterManifest, every field as written
//! .project(Some("tmf_3jn_text_reg"))       → WriterProject { book: 3JN, language: tmf, resource: reg,
//!                                                           sources: [en/ulb 12], translators: [kpass] }
//! ```
//!
//! [`read`] keeps what the file says, leniently: translationCore writes the same
//! file name with the same core fields and no `finished_chunks`, and real files
//! write a version as a number, `source_translations` as a map, or `resource`
//! as a bare string. [`BttWriterManifest::project`] is the stricter question a
//! book project must answer (which book, which language, which resource),
//! as Sefer's migration asks it.

use crate::lenient::Reader;
use crate::tree::{Node, pointer};
use crate::{
    BookEntry, Declared, Direction, Format, Kind, Manifest, ReadError, Related, Severity, kind,
    one, related, without_bom,
};
use mise::books::BookKey;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BttWriterManifest {
    pub format: Option<String>,
    pub target_language: Option<TargetLanguage>,
    pub project: Option<Project>,
    /// `type`: `{"id": "text"}`, or a bare string.
    pub kind: Option<NameBlock>,
    pub resource: Option<NameBlock>,
    pub source_translations: Vec<SourceTranslation>,
    pub translators: Vec<String>,
    /// Chunks marked done: `front-title`, `01-title`, `01-05`. `None` when the
    /// file has no list (translationCore never writes one).
    pub finished_chunks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetLanguage {
    pub id: Option<String>,
    pub name: Option<String>,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Project {
    pub id: Option<String>,
    pub name: Option<String>,
}

/// `type` and `resource`: normally `{"id", "name"}`, sometimes a bare string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameBlock {
    Object {
        id: Option<String>,
        name: Option<String>,
    },
    Bare(String),
}

impl NameBlock {
    pub fn id(&self) -> Option<&str> {
        match self {
            NameBlock::Object { id, .. } => id.as_deref(),
            NameBlock::Bare(id) => Some(id),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            NameBlock::Object { name, .. } => name.as_deref(),
            NameBlock::Bare(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceTranslation {
    pub language_id: Option<String>,
    pub resource_id: Option<String>,
    pub version: Option<String>,
    pub checking_level: Option<String>,
    pub date_modified: Option<String>,
}

/// What a BTT Writer book project is, once its manifest answers the three
/// questions a book needs: which book, which language, which resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterProject {
    pub book: BookKey,
    pub language: Language,
    /// Lowercase: `reg`, `ulb`.
    pub resource: String,
    pub sources: Vec<Related>,
    pub translators: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub id: String,
    pub name: Option<String>,
    pub direction: Direction,
}

/// A `manifest.json`'s bytes, as written. A leading byte order mark is
/// skipped. An error only when it isn't a JSON object; what reading forgave
/// is in [`crate::read`]'s findings.
pub fn read(bytes: &[u8]) -> Result<BttWriterManifest, ReadError> {
    let node = crate::tree::from_json(without_bom(bytes)).map_err(ReadError::Json)?;
    if !matches!(node, Node::Map(_)) {
        return Err(ReadError::Json(format!(
            "the manifest is {}, not an object",
            node.kind()
        )));
    }
    Ok(from_node(&node, &mut Reader::new(Vec::new())))
}

pub(crate) fn from_node(root: &Node, r: &mut Reader) -> BttWriterManifest {
    let target_language = r
        .map(root.get("target_language"), "/target_language")
        .map(|t| TargetLanguage {
            id: r.field(t, "id", "/target_language"),
            name: r.field(t, "name", "/target_language"),
            direction: r.field(t, "direction", "/target_language"),
        });
    let project = r.map(root.get("project"), "/project").map(|p| Project {
        id: r.field(p, "id", "/project"),
        name: r.field(p, "name", "/project"),
    });
    let finished_chunks = match root.get("finished_chunks") {
        None | Some(Node::Null) => None,
        Some(list @ Node::List(_)) => Some(r.strings(Some(list), "/finished_chunks")),
        Some(other) => {
            r.note(
                crate::Check::ManifestValid,
                Severity::Warning,
                "/finished_chunks",
                format!(
                    "Expected a list at /finished_chunks, found {}; it was left unread.",
                    other.kind()
                ),
            );
            None
        }
    };
    BttWriterManifest {
        format: r.field(root, "format", ""),
        target_language,
        project,
        kind: name_block(root.get("type"), "/type", r),
        resource: name_block(root.get("resource"), "/resource", r),
        source_translations: sources(root.get("source_translations"), r),
        translators: r.strings(root.get("translators"), "/translators"),
        finished_chunks,
    }
}

fn name_block(node: Option<&Node>, at: &str, r: &mut Reader) -> Option<NameBlock> {
    match node? {
        Node::String(s) => Some(NameBlock::Bare(s.clone())),
        other => r.map(Some(other), at).map(|b| NameBlock::Object {
            id: r.field(b, "id", at),
            name: r.field(b, "name", at),
        }),
    }
}

/// A list of sources, or an object whose values are the sources (`{}` is
/// seen in real files). An entry that isn't an object is left out.
fn sources(node: Option<&Node>, r: &mut Reader) -> Vec<SourceTranslation> {
    let at = "/source_translations";
    let items: Vec<(String, &Node)> = match node {
        None | Some(Node::Null) => Vec::new(),
        Some(Node::List(items)) => items
            .iter()
            .enumerate()
            .map(|(i, n)| (format!("{at}/{i}"), n))
            .collect(),
        Some(map @ Node::Map(_)) => {
            r.note(
                crate::Check::LenientValue,
                Severity::Info,
                at,
                format!("{at} is an object, read as the list of its values."),
            );
            map.entries()
                .into_iter()
                .map(|(k, n)| (pointer(at, k), n))
                .collect()
        }
        Some(other) => {
            r.list(Some(other), at);
            Vec::new()
        }
    };
    items
        .into_iter()
        .filter_map(|(at, n)| {
            let s = r.map(Some(n), &at)?;
            Some(SourceTranslation {
                language_id: r.field(s, "language_id", &at),
                resource_id: r.field(s, "resource_id", &at),
                version: r.field(s, "version", &at),
                checking_level: r.field(s, "checking_level", &at),
                date_modified: r.field(s, "date_modified", &at),
            })
        })
        .collect()
}

impl BttWriterManifest {
    /// What every format says. A Writer project is one book; `books` is
    /// empty when the manifest names none of canon's table.
    pub(crate) fn manifest(&self) -> Manifest {
        let tag = self
            .target_language
            .as_ref()
            .and_then(|t| t.id.clone())
            .unwrap_or_default();
        let project_id = self.project.as_ref().and_then(|p| p.id.as_deref());
        let resource_id = self.resource.as_ref().and_then(NameBlock::id);
        let subject = kind::subject(None, resource_id, project_id);
        Manifest {
            format: Format::BttWriter,
            id: None,
            name: one(&tag, self.resource.as_ref().and_then(NameBlock::name)),
            abbreviation: one(&tag, resource_id),
            language: crate::Language {
                name: one(
                    &tag,
                    self.target_language
                        .as_ref()
                        .and_then(|t| t.name.as_deref()),
                ),
                direction: match self
                    .target_language
                    .as_ref()
                    .and_then(|t| t.direction.as_deref())
                {
                    Some("rtl") => Direction::Rtl,
                    _ => Direction::Ltr,
                },
                tag: tag.clone(),
            },
            kind: Kind {
                domain: subject.as_deref().and_then(crate::Domain::of_subject),
                subject,
                flavor: None,
                declared: Declared {
                    subject: None,
                    r#type: self
                        .kind
                        .as_ref()
                        .and_then(NameBlock::id)
                        .map(str::to_owned),
                    format: self.format.clone(),
                    identifier: resource_id.map(str::to_owned),
                },
            },
            books: project_id
                .and_then(crate::book_of)
                .map(|book| BookEntry {
                    book,
                    path: None,
                    title: one(&tag, self.project.as_ref().and_then(|p| p.name.as_deref())),
                    sort: None,
                })
                .into_iter()
                .collect(),
            sources: self
                .source_translations
                .iter()
                .filter_map(|s| {
                    related::related(
                        s.language_id.as_deref()?,
                        s.resource_id.as_deref()?,
                        s.version.as_deref(),
                    )
                })
                .collect(),
            version: None,
            issued: None,
            modified: None,
            rights: None,
            publisher: None,
        }
    }

    /// The book project, or `None` when the manifest doesn't name a book of the
    /// table, a language, and a resource.
    ///
    /// `repo` is the project's repo or folder name (`tmf_3jn_text_reg`): when
    /// the manifest names no resource, Writer's naming
    /// `{language}_{book}_text_{resource}` supplies it.
    pub fn project(&self, repo: Option<&str>) -> Option<WriterProject> {
        let code = self
            .project
            .as_ref()?
            .id
            .as_deref()?
            .trim()
            .to_ascii_uppercase();
        let book = BookKey::new(code.as_bytes().try_into().ok()?);
        canon::book(book)?;

        let target = self.target_language.as_ref()?;
        let id = target.id.as_deref()?.trim();
        if id.is_empty() {
            return None;
        }
        let direction = match target.direction.as_deref().map(str::trim) {
            Some("rtl") => Direction::Rtl,
            _ => Direction::Ltr,
        };

        let resource = self
            .resource
            .as_ref()
            .and_then(NameBlock::id)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .or_else(|| repo.and_then(|repo| repo.rsplit('_').next()).map(str::trim))
            .filter(|id| !id.is_empty())?
            .to_lowercase();

        let sources = self
            .source_translations
            .iter()
            .filter_map(|source| {
                related::related(
                    source.language_id.as_deref()?,
                    source.resource_id.as_deref()?,
                    source.version.as_deref(),
                )
            })
            .collect();

        Some(WriterProject {
            book,
            language: Language {
                id: id.to_string(),
                name: target.name.clone(),
                direction,
            },
            resource,
            sources,
            translators: self.translators.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
