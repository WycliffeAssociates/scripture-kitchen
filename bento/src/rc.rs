//! Resource Container 0.2: `manifest.yaml`.
//!
//! ```text
//! dublin_core:                              RcManifest {
//!   identifier: reg                           dublin_core: DublinCore { identifier: "reg",
//!   language: {identifier: llx, …}              language: RcLanguage { identifier: "llx", … },
//!   source: [{identifier: ulb, language: en}]   source: [RcSource { … }], … },
//! checking: {checking_level: "1"}             checking: Some(Checking { … }),
//! projects:                                   projects: [RcProject { identifier: "psa",
//!   - {identifier: psa, sort: 19, path: …}                 sort: Some(19), … }] }
//! ```
//!
//! Read leniently, as the catalog and WACS write it: a number for text, one
//! source or relation for a list, a source that's only a name, a missing
//! language. `.manifest()` gives the [`Manifest`] every format shares.

use crate::lenient::Reader;
use crate::tree::{Node, pointer};
use crate::{BookEntry, Declared, Direction, Format, Kind, Language, Manifest, kind, one, related};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RcManifest {
    pub dublin_core: DublinCore,
    pub checking: Option<Checking>,
    pub projects: Vec<RcProject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DublinCore {
    pub conformsto: Option<String>,
    pub contributor: Vec<String>,
    pub creator: Option<String>,
    pub description: Option<String>,
    /// The content's media type: `text/usfm`.
    pub format: Option<String>,
    /// The resource's code: `ulb`, `tn`.
    pub identifier: Option<String>,
    pub issued: Option<String>,
    pub language: Option<RcLanguage>,
    pub modified: Option<String>,
    pub publisher: Option<String>,
    pub relation: Vec<String>,
    pub rights: Option<String>,
    pub source: Vec<RcSource>,
    pub subject: Option<String>,
    pub title: Option<String>,
    /// `bundle`, `book`, `help`.
    pub r#type: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RcLanguage {
    pub identifier: Option<String>,
    pub title: Option<String>,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RcSource {
    pub identifier: Option<String>,
    pub language: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Checking {
    pub checking_entity: Vec<String>,
    pub checking_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RcProject {
    /// A book code (`psa`) for Scripture and helps by book; an article or
    /// `obs` for others.
    pub identifier: Option<String>,
    pub title: Option<String>,
    pub path: Option<String>,
    pub sort: Option<i64>,
    pub versification: Option<String>,
    pub categories: Vec<String>,
}

pub(crate) fn from_node(root: &Node, r: &mut Reader) -> RcManifest {
    let dc = match r.map(root.get("dublin_core"), "/dublin_core") {
        Some(dc) => dublin_core(dc, r),
        None => {
            r.required("/dublin_core", "dublin_core");
            DublinCore::default()
        }
    };
    let checking = r.map(root.get("checking"), "/checking").map(|c| Checking {
        checking_entity: r.strings(c.get("checking_entity"), "/checking/checking_entity"),
        checking_level: r.field(c, "checking_level", "/checking"),
    });
    let projects = r
        .list(root.get("projects"), "/projects")
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let at = format!("/projects/{i}");
            let p = r.map(Some(p), &at)?;
            Some(RcProject {
                identifier: r.field(p, "identifier", &at),
                title: r.field(p, "title", &at),
                path: r.field(p, "path", &at),
                sort: r.int(p.get("sort"), &pointer(&at, "sort")),
                versification: r.field(p, "versification", &at),
                categories: r.strings(p.get("categories"), &pointer(&at, "categories")),
            })
        })
        .collect();
    RcManifest {
        dublin_core: dc,
        checking,
        projects,
    }
}

fn dublin_core(dc: &Node, r: &mut Reader) -> DublinCore {
    let at = "/dublin_core";
    let language = r
        .map(dc.get("language"), "/dublin_core/language")
        .map(|l| RcLanguage {
            identifier: r.field(l, "identifier", "/dublin_core/language"),
            title: r.field(l, "title", "/dublin_core/language"),
            direction: r.field(l, "direction", "/dublin_core/language"),
        });
    let source = sources(dc.get("source"), r);
    DublinCore {
        conformsto: r.field(dc, "conformsto", at),
        contributor: r.strings(dc.get("contributor"), "/dublin_core/contributor"),
        creator: r.field(dc, "creator", at),
        description: r.field(dc, "description", at),
        format: r.field(dc, "format", at),
        identifier: r.field(dc, "identifier", at),
        issued: r.field(dc, "issued", at),
        language,
        modified: r.field(dc, "modified", at),
        publisher: r.field(dc, "publisher", at),
        relation: r.strings(dc.get("relation"), "/dublin_core/relation"),
        rights: r.field(dc, "rights", at),
        source,
        subject: r.field(dc, "subject", at),
        title: r.field(dc, "title", at),
        r#type: r.field(dc, "type", at),
        version: r.field(dc, "version", at),
    }
}

/// `source`: a list of sources, one source, or a source that's only its name.
fn sources(node: Option<&Node>, r: &mut Reader) -> Vec<RcSource> {
    let at = "/dublin_core/source";
    let one_source = |node: &Node, at: &str, r: &mut Reader| match node {
        Node::Map(_) => Some(RcSource {
            identifier: r.field(node, "identifier", at),
            language: r.field(node, "language", at),
            version: r.field(node, "version", at),
        }),
        Node::String(name) => {
            r.note(
                crate::Check::LenientValue,
                crate::Severity::Info,
                at,
                format!("{at} is the name \"{name}\", read as a source with no language."),
            );
            Some(RcSource {
                identifier: Some(name.clone()),
                ..RcSource::default()
            })
        }
        _ => {
            r.string(Some(node), at);
            None
        }
    };
    match node {
        None | Some(Node::Null) => Vec::new(),
        Some(Node::List(items)) => items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| one_source(item, &format!("{at}/{i}"), r))
            .collect(),
        Some(single) => {
            if matches!(single, Node::Map(_)) {
                r.note(
                    crate::Check::LenientValue,
                    crate::Severity::Info,
                    at,
                    format!("{at} is one source, read as a list of one."),
                );
            }
            one_source(single, at, r).into_iter().collect()
        }
    }
}

impl RcManifest {
    /// What every format says. Findings about the manifest's shape go to `r`.
    pub(crate) fn manifest(&self, r: &mut Reader) -> Manifest {
        let dc = &self.dublin_core;
        if dc.identifier.as_deref().is_none_or(str::is_empty) {
            r.required("/dublin_core/identifier", "dublin_core.identifier");
        }
        for (i, p) in self.projects.iter().enumerate() {
            if p.identifier.is_none() {
                r.required(
                    &format!("/projects/{i}/identifier"),
                    "A project's identifier",
                );
            }
            if p.path.is_none() {
                r.required(&format!("/projects/{i}/path"), "A project's path");
            }
        }
        let tag = dc
            .language
            .as_ref()
            .and_then(|l| l.identifier.clone())
            .unwrap_or_default();
        let language = Language {
            name: one(&tag, dc.language.as_ref().and_then(|l| l.title.as_deref())),
            direction: match dc.language.as_ref().and_then(|l| l.direction.as_deref()) {
                Some("rtl") => Direction::Rtl,
                _ => Direction::Ltr,
            },
            tag: tag.clone(),
        };
        let subject = kind::subject(dc.subject.as_deref(), dc.identifier.as_deref(), None);
        let books = self
            .projects
            .iter()
            .filter_map(|p| {
                Some(BookEntry {
                    book: crate::book_of(p.identifier.as_deref()?)?,
                    path: p.path.clone(),
                    title: one(&tag, p.title.as_deref()),
                    sort: p.sort,
                })
            })
            .collect();
        Manifest {
            format: Format::Rc,
            id: None,
            name: one(&tag, dc.title.as_deref()),
            abbreviation: one(&tag, dc.identifier.as_deref()),
            language,
            kind: Kind {
                domain: subject.as_deref().and_then(crate::Domain::of_subject),
                subject,
                flavor: None,
                declared: Declared {
                    subject: dc.subject.clone(),
                    r#type: dc.r#type.clone(),
                    format: dc.format.clone(),
                    identifier: dc.identifier.clone(),
                },
            },
            books,
            sources: dc
                .source
                .iter()
                .filter_map(|s| {
                    related::related(
                        s.language.as_deref()?,
                        s.identifier.as_deref()?,
                        s.version.as_deref(),
                    )
                })
                .collect(),
            version: dc.version.clone(),
            issued: dc.issued.clone(),
            modified: dc.modified.clone(),
            rights: dc.rights.clone(),
            publisher: dc.publisher.clone(),
        }
    }
}

