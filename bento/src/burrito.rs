//! Scripture Burrito 1.0, as WA's fork pins it: `metadata.json`.
//!
//! ```text
//! {"format": "scripture burrito",                BurritoMetadata {
//!  "meta": {"version": "1.0.0", …},                meta: Meta { … },
//!  "identification": {"name": {"en": "ULB"},       identification: Identification { … },
//!    "primary": {"wacs": {"o/en_ulb": {…}}}},
//!  "type": {"flavorType": {"name": "scripture",    flavor_type: Some(FlavorType { … }),
//!    "flavor": {"name": "textTranslation"}}},
//!  "ingredients": {"./41-MAT.usfm":                ingredients: [("./41-MAT.usfm", Ingredient {
//!    {"scope": {"MAT": []}, "size": 9, …}}}          scope: [("MAT", [])], size: Some(9), … })] }
//! ```
//!
//! The fork is <https://github.com/Bible-Translation-Tools/scripture-burrito>,
//! branch `wycliffe-associates`, at `b8872ee`. Reading checks the fields it
//! requires that apps get wrong (`copyright`, `meta.dateCreated` as a
//! date-time, `meta.generator.softwareVersion`, `identification.primary`), as
//! [`Check::ManifestValid`](crate::Check::ManifestValid) warnings. Maps keep
//! the file's order: ingredients are the books, in the order listed.

use crate::lenient::Reader;
use crate::tree::{Node, pointer};
use crate::{
    BookEntry, Declared, Direction, Domain, Format, Kind, Language, LocalizedText, Manifest, kind,
    related,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BurritoMetadata {
    /// `scripture burrito`
    pub format: Option<String>,
    pub meta: Meta,
    /// `wacs` → its id and name.
    pub id_authorities: Vec<(String, IdAuthority)>,
    pub identification: Identification,
    pub confidential: Option<bool>,
    pub flavor_type: Option<FlavorType>,
    pub copyright: Option<Copyright>,
    pub languages: Vec<BurritoLanguage>,
    /// Book code (`MAT`) → its names.
    pub localized_names: Vec<(String, LocalizedName)>,
    /// Path → ingredient, in file order.
    pub ingredients: Vec<(String, Ingredient)>,
    pub relationships: Vec<Relationship>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    /// The Burrito spec's version: `1.0.0`.
    pub version: Option<String>,
    /// `source`, `derived`, `template`.
    pub category: Option<String>,
    pub generator: Option<Generator>,
    pub default_locale: Option<String>,
    pub date_created: Option<String>,
    pub normalization: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Generator {
    pub software_name: Option<String>,
    pub software_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdAuthority {
    pub id: Option<String>,
    pub name: LocalizedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Identification {
    pub name: LocalizedText,
    pub abbreviation: LocalizedText,
    pub description: LocalizedText,
    /// Authority (`wacs`) → repo (`Tech_Advance/cdi_reg`) → revision and timestamp.
    pub primary: Vec<(String, Vec<(String, PrimaryId)>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrimaryId {
    pub revision: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlavorType {
    /// `scripture`, `gloss`, `parascriptural`, `peripheral`.
    pub name: Option<String>,
    /// `textTranslation`
    pub flavor: Option<String>,
    pub usfm_version: Option<String>,
    /// Book code → its chapter ranges (empty: the whole book).
    pub current_scope: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Copyright {
    pub public_domain: Option<bool>,
    /// Each license's `url` or `ingredient`.
    pub licenses: Vec<String>,
    pub short_statements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BurritoLanguage {
    pub tag: Option<String>,
    pub name: LocalizedText,
    pub script_direction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalizedName {
    pub short: LocalizedText,
    pub long: LocalizedText,
    pub abbr: LocalizedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ingredient {
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub md5: Option<String>,
    /// Book code → chapter ranges.
    pub scope: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relationship {
    /// `source`; older files write `type`.
    pub relation_type: Option<String>,
    pub flavor: Option<String>,
    /// `wacs::WA-Catalog/en_ulb`; older files write `target`.
    pub id: Option<String>,
    pub revision: Option<String>,
}

pub(crate) fn from_node(root: &Node, r: &mut Reader) -> BurritoMetadata {
    let format = r.field(root, "format", "");
    match format.as_deref() {
        Some("scripture burrito") => {}
        Some(other) => r.note(
            crate::Check::ManifestValid,
            crate::Severity::Warning,
            "/format",
            format!("/format is \"{other}\", not \"scripture burrito\"."),
        ),
        None => r.required("/format", "format"),
    }
    let meta = meta(root.get("meta"), r);
    let id_authorities = r
        .map(root.get("idAuthorities"), "/idAuthorities")
        .map(|m| {
            m.entries()
                .into_iter()
                .map(|(key, a)| {
                    let at = pointer("/idAuthorities", key);
                    (
                        key.to_owned(),
                        IdAuthority {
                            id: r.field(a, "id", &at),
                            name: r.localized(a.get("name"), &pointer(&at, "name")),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let identification = identification(root.get("identification"), r);
    let confidential = match root.get("confidential") {
        Some(Node::Bool(b)) => Some(*b),
        _ => None,
    };
    let flavor_type = flavor_type(root, r);
    let copyright = copyright(root.get("copyright"), r);
    let languages = r
        .list(root.get("languages"), "/languages")
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let at = format!("/languages/{i}");
            let l = r.map(Some(l), &at)?;
            Some(BurritoLanguage {
                tag: r.field(l, "tag", &at),
                name: r.localized(l.get("name"), &pointer(&at, "name")),
                script_direction: r.field(l, "scriptDirection", &at),
            })
        })
        .collect();
    let localized_names = r
        .map(root.get("localizedNames"), "/localizedNames")
        .map(|m| {
            m.entries()
                .into_iter()
                .map(|(book, n)| {
                    let at = pointer("/localizedNames", book);
                    (
                        book.to_owned(),
                        LocalizedName {
                            short: r.localized(n.get("short"), &pointer(&at, "short")),
                            long: r.localized(n.get("long"), &pointer(&at, "long")),
                            abbr: r.localized(n.get("abbr"), &pointer(&at, "abbr")),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let ingredients = match r.map(root.get("ingredients"), "/ingredients") {
        Some(m) => m
            .entries()
            .into_iter()
            .map(|(path, i)| {
                (
                    path.to_owned(),
                    ingredient(i, &pointer("/ingredients", path), r),
                )
            })
            .collect(),
        None => {
            r.required("/ingredients", "ingredients");
            Vec::new()
        }
    };
    let relationships = r
        .list(root.get("relationships"), "/relationships")
        .iter()
        .enumerate()
        .filter_map(|(i, rel)| {
            let at = format!("/relationships/{i}");
            let rel = r.map(Some(rel), &at)?;
            Some(Relationship {
                relation_type: r
                    .field(rel, "relationType", &at)
                    .or_else(|| r.field(rel, "type", &at)),
                flavor: r.field(rel, "flavor", &at),
                id: r
                    .field(rel, "id", &at)
                    .or_else(|| r.field(rel, "target", &at)),
                revision: r.field(rel, "revision", &at),
            })
        })
        .collect();
    BurritoMetadata {
        format,
        meta,
        id_authorities,
        identification,
        confidential,
        flavor_type,
        copyright,
        languages,
        localized_names,
        ingredients,
        relationships,
    }
}

fn meta(node: Option<&Node>, r: &mut Reader) -> Meta {
    let Some(m) = r.map(node, "/meta") else {
        r.required("/meta", "meta");
        return Meta::default();
    };
    let at = "/meta";
    let generator = r
        .map(m.get("generator"), "/meta/generator")
        .map(|g| Generator {
            software_name: r.field(g, "softwareName", "/meta/generator"),
            software_version: r.field(g, "softwareVersion", "/meta/generator"),
        });
    let meta = Meta {
        version: r.field(m, "version", at),
        category: r.field(m, "category", at),
        generator,
        default_locale: r.field(m, "defaultLocale", at),
        date_created: r.field(m, "dateCreated", at),
        normalization: r.field(m, "normalization", at),
    };
    for (value, key) in [
        (&meta.version, "version"),
        (&meta.category, "category"),
        (&meta.default_locale, "defaultLocale"),
    ] {
        if value.is_none() {
            r.required(&pointer(at, key), &format!("meta.{key}"));
        }
    }
    match &meta.date_created {
        None => r.required("/meta/dateCreated", "meta.dateCreated"),
        Some(date) if !is_date_time(date) => r.note(
            crate::Check::ManifestValid,
            crate::Severity::Warning,
            "/meta/dateCreated",
            format!("/meta/dateCreated is \"{date}\", not a date and time (2026-11-05T14:30:00Z)."),
        ),
        Some(_) => {}
    }
    if let Some(g) = &meta.generator {
        if g.software_name.is_none() {
            r.required(
                "/meta/generator/softwareName",
                "meta.generator.softwareName",
            );
        }
        if g.software_version.is_none() {
            r.required(
                "/meta/generator/softwareVersion",
                "meta.generator.softwareVersion",
            );
        }
    }
    meta
}

fn identification(node: Option<&Node>, r: &mut Reader) -> Identification {
    let Some(id) = r.map(node, "/identification") else {
        r.required("/identification", "identification");
        return Identification::default();
    };
    let at = "/identification";
    let primary: Vec<(String, Vec<(String, PrimaryId)>)> = r
        .map(id.get("primary"), "/identification/primary")
        .map(|p| {
            p.entries()
                .into_iter()
                .map(|(authority, repos)| {
                    let at = pointer("/identification/primary", authority);
                    let repos = r
                        .map(Some(repos), &at)
                        .map(|repos| {
                            repos
                                .entries()
                                .into_iter()
                                .map(|(repo, rev)| {
                                    let at = pointer(&at, repo);
                                    let rev = r.map(Some(rev), &at);
                                    let id = PrimaryId {
                                        revision: rev.and_then(|rev| r.field(rev, "revision", &at)),
                                        timestamp: rev
                                            .and_then(|rev| r.field(rev, "timestamp", &at)),
                                    };
                                    (repo.to_owned(), id)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (authority.to_owned(), repos)
                })
                .collect()
        })
        .unwrap_or_default();
    let identification = Identification {
        name: r.localized(id.get("name"), "/identification/name"),
        abbreviation: r.localized(id.get("abbreviation"), "/identification/abbreviation"),
        description: r.localized(id.get("description"), "/identification/description"),
        primary,
    };
    if identification.primary.is_empty() {
        r.required(&pointer(at, "primary"), "identification.primary");
    }
    primary_rules(&identification.primary, r);
    if identification.abbreviation.is_empty() {
        r.required(&pointer(at, "abbreviation"), "identification.abbreviation");
    }
    identification
}

/// WA's fork: one authority, one `owner/repo` under it, `revision: "latest"`,
/// and a `timestamp` that's a date (`2026`, `2026-11`, `2026-11-04`).
fn primary_rules(primary: &[(String, Vec<(String, PrimaryId)>)], r: &mut Reader) {
    let bad = |r: &mut Reader, at: &str, message: String| {
        r.note(
            crate::Check::ManifestValid,
            crate::Severity::Warning,
            at,
            message,
        );
    };
    let at = "/identification/primary";
    if primary.len() > 1 {
        bad(
            r,
            at,
            format!(
                "{at} names {} authorities; it must name one.",
                primary.len()
            ),
        );
    }
    for (authority, repos) in primary {
        let at = pointer(at, authority);
        if !is_authority_label(authority) {
            bad(
                r,
                &at,
                format!(
                    "\"{authority}\" isn't an authority label (lowercase letters, digits and dashes: wacs)."
                ),
            );
        }
        if repos.len() != 1 {
            bad(
                r,
                &at,
                format!("{at} names {} repos; it must name one.", repos.len()),
            );
        }
        for (repo, id) in repos {
            let at = pointer(&at, repo);
            let mut halves = repo.split('/');
            let path = matches!((halves.next(), halves.next(), halves.next()), (Some(o), Some(n), None)
                if !o.is_empty() && !n.is_empty() && !repo.chars().any(char::is_whitespace));
            if !path {
                bad(
                    r,
                    &at,
                    format!("\"{repo}\" isn't an owner/repo path (Tech_Advance/cdi_reg)."),
                );
            }
            match id.revision.as_deref() {
                Some("latest") => {}
                Some(other) => bad(
                    r,
                    &pointer(&at, "revision"),
                    format!("revision is \"{other}\"; it must be \"latest\"."),
                ),
                None => r.required(&pointer(&at, "revision"), "revision"),
            }
            match id.timestamp.as_deref() {
                Some(t) if is_date(t) => {}
                Some(t) => bad(
                    r,
                    &pointer(&at, "timestamp"),
                    format!("timestamp is \"{t}\"; it must be a date (2026-11-04)."),
                ),
                None => r.required(&pointer(&at, "timestamp"), "timestamp"),
            }
        }
    }
}

/// `^[a-z][a-z0-9-]*[a-z0-9]$`
fn is_authority_label(label: &str) -> bool {
    let b = label.as_bytes();
    b.len() >= 2
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && b[b.len() - 1] != b'-'
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    match parts[..] {
        [y] => digits(y, 4),
        [y, m] => digits(y, 4) && digits(m, 2),
        [y, m, d] => digits(y, 4) && digits(m, 2) && digits(d, 2),
        _ => false,
    }
}

fn flavor_type(root: &Node, r: &mut Reader) -> Option<FlavorType> {
    let Some(t) = r.map(root.get("type"), "/type") else {
        r.required("/type", "type");
        return None;
    };
    let ft = r.map(t.get("flavorType"), "/type/flavorType")?;
    let flavor = r.map(ft.get("flavor"), "/type/flavorType/flavor");
    Some(FlavorType {
        name: r.field(ft, "name", "/type/flavorType"),
        flavor: flavor.and_then(|f| r.field(f, "name", "/type/flavorType/flavor")),
        usfm_version: flavor.and_then(|f| r.field(f, "usfmVersion", "/type/flavorType/flavor")),
        current_scope: scope(ft.get("currentScope"), "/type/flavorType/currentScope", r),
    })
}

fn copyright(node: Option<&Node>, r: &mut Reader) -> Option<Copyright> {
    let Some(c) = r.map(node, "/copyright") else {
        r.required("/copyright", "copyright");
        return None;
    };
    let licenses = r
        .list(c.get("licenses"), "/copyright/licenses")
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let at = format!("/copyright/licenses/{i}");
            let l = r.map(Some(l), &at)?;
            r.field(l, "url", &at)
                .or_else(|| r.field(l, "ingredient", &at))
        })
        .collect();
    let short_statements = r
        .list(c.get("shortStatements"), "/copyright/shortStatements")
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let at = format!("/copyright/shortStatements/{i}");
            let s = r.map(Some(s), &at)?;
            r.field(s, "statement", &at)
        })
        .collect();
    Some(Copyright {
        public_domain: match c.get("publicDomain") {
            Some(Node::Bool(b)) => Some(*b),
            _ => None,
        },
        licenses,
        short_statements,
    })
}

fn ingredient(node: &Node, at: &str, r: &mut Reader) -> Ingredient {
    let Some(i) = r.map(Some(node), at) else {
        return Ingredient::default();
    };
    let ingredient = Ingredient {
        mime_type: r.field(i, "mimeType", at),
        size: r.int(i.get("size"), &pointer(at, "size")),
        md5: r
            .map(i.get("checksum"), &pointer(at, "checksum"))
            .and_then(|c| r.field(c, "md5", &pointer(at, "checksum"))),
        scope: scope(i.get("scope"), &pointer(at, "scope"), r),
    };
    if ingredient.mime_type.is_none() {
        r.required(&pointer(at, "mimeType"), "An ingredient's mimeType");
    }
    if ingredient.size.is_none() {
        r.required(&pointer(at, "size"), "An ingredient's size");
    }
    ingredient
}

fn scope(node: Option<&Node>, at: &str, r: &mut Reader) -> Vec<(String, Vec<String>)> {
    r.map(node, at)
        .map(|m| {
            m.entries()
                .into_iter()
                .map(|(book, ranges)| {
                    (book.to_owned(), r.strings(Some(ranges), &pointer(at, book)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// RFC 3339's date-time, as JSON Schema's `date-time` format means it:
/// `2026-11-05T14:30:00Z`, `2026-11-05T14:30:00.123+02:00`.
fn is_date_time(text: &str) -> bool {
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        b.get(range)
            .is_some_and(|d| d.iter().all(u8::is_ascii_digit))
    };
    if b.len() < 20
        || !digits(0..4)
        || b[4] != b'-'
        || !digits(5..7)
        || b[7] != b'-'
        || !digits(8..10)
        || !matches!(b[10], b'T' | b't')
        || !digits(11..13)
        || b[13] != b':'
        || !digits(14..16)
        || b[16] != b':'
        || !digits(17..19)
    {
        return false;
    }
    let mut rest = &text[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let n = frac.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return false;
        }
        rest = &frac[n..];
    }
    match rest.as_bytes() {
        [b'Z' | b'z'] => true,
        [b'+' | b'-', h1, h2, b':', m1, m2] => [h1, h2, m1, m2].iter().all(|d| d.is_ascii_digit()),
        _ => false,
    }
}

impl BurritoMetadata {
    /// What every format says. Findings about the manifest's shape go to `r`.
    pub(crate) fn manifest(&self, r: &mut Reader) -> Manifest {
        let first = self.languages.first();
        if self.languages.is_empty()
            && self
                .flavor_type
                .as_ref()
                .and_then(|f| f.name.as_deref())
                .is_some_and(|n| matches!(n, "scripture" | "gloss"))
        {
            r.required("/languages", "languages (for scripture and gloss)");
        }
        let tag = first.and_then(|l| l.tag.clone()).unwrap_or_default();
        let flavor = self.flavor_type.as_ref().and_then(|f| f.flavor.clone());
        let abbreviation = self
            .identification
            .abbreviation
            .values()
            .next()
            .map(String::as_str);
        let subject = kind::subject(flavor.as_deref(), abbreviation, None);
        let domain = self
            .flavor_type
            .as_ref()
            .and_then(|f| f.name.as_deref())
            .and_then(Domain::of_flavor_type)
            .or_else(|| subject.as_deref().and_then(Domain::of_subject));

        let mut books: Vec<BookEntry> = Vec::new();
        for (path, ingredient) in &self.ingredients {
            let Some(book) = ingredient
                .scope
                .iter()
                .find_map(|(code, _)| crate::book_of(code))
            else {
                continue;
            };
            if books.iter().any(|b| b.book == book) {
                continue;
            }
            let title = self
                .localized_names
                .iter()
                .find(|(code, _)| crate::book_of(code) == Some(book))
                .map(|(_, names)| names.short.clone())
                .unwrap_or_default();
            books.push(BookEntry {
                book,
                path: Some(path.clone()),
                title,
                sort: None,
            });
        }

        Manifest {
            format: Format::Burrito,
            id: self
                .identification
                .primary
                .iter()
                .find_map(|(_, repos)| repos.first().map(|(repo, _)| repo.clone())),
            name: self.identification.name.clone(),
            abbreviation: self.identification.abbreviation.clone(),
            language: Language {
                tag,
                name: first.map(|l| l.name.clone()).unwrap_or_default(),
                direction: match first.and_then(|l| l.script_direction.as_deref()) {
                    Some("rtl") => Direction::Rtl,
                    _ => Direction::Ltr,
                },
            },
            kind: Kind {
                domain,
                subject,
                flavor,
                declared: Declared {
                    subject: None,
                    r#type: self.flavor_type.as_ref().and_then(|f| f.name.clone()),
                    format: self.content_format(),
                    identifier: None,
                },
            },
            books,
            sources: self
                .relationships
                .iter()
                .filter(|rel| rel.relation_type.as_deref() == Some("source"))
                .filter_map(|rel| {
                    related::from_burrito_id(rel.id.as_deref()?, rel.revision.as_deref())
                })
                .collect(),
            version: None,
            issued: self.meta.date_created.clone(),
            modified: None,
            rights: self
                .copyright
                .as_ref()
                .and_then(|c| c.licenses.first().cloned()),
            publisher: None,
        }
    }

    /// The media type most ingredients have, as written (`text/x-usfm`).
    fn content_format(&self) -> Option<String> {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for (_, i) in &self.ingredients {
            if let Some(m) = i.mime_type.as_deref() {
                match counts.iter_mut().find(|(k, _)| *k == m) {
                    Some(c) => c.1 += 1,
                    None => counts.push((m, 1)),
                }
            }
        }
        // The first of the most common, in file order.
        let most = counts.iter().map(|(_, n)| *n).max()?;
        counts
            .into_iter()
            .find(|(_, n)| *n == most)
            .map(|(m, _)| m.to_owned())
    }
}

#[cfg(test)]
mod tests;
