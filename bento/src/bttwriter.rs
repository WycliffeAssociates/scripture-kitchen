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

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::{Direction, ReadError, Related, without_bom};
use mise::books::BookKey;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct BttWriterManifest {
    #[serde(default, deserialize_with = "flex_string")]
    pub format: Option<String>,
    #[serde(default)]
    pub target_language: Option<TargetLanguage>,
    #[serde(default)]
    pub project: Option<Project>,
    #[serde(default, rename = "type")]
    pub kind: Option<NameBlock>,
    #[serde(default)]
    pub resource: Option<NameBlock>,
    #[serde(default, deserialize_with = "flex_sources")]
    pub source_translations: Vec<SourceTranslation>,
    #[serde(default, deserialize_with = "flex_strings")]
    pub translators: Vec<String>,
    /// Chunks marked done: `front-title`, `01-title`, `01-05`. `None` when the
    /// file has no list (translationCore never writes one).
    #[serde(default, deserialize_with = "flex_chunks")]
    pub finished_chunks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct TargetLanguage {
    #[serde(default, deserialize_with = "flex_string")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Project {
    #[serde(default, deserialize_with = "flex_string")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub name: Option<String>,
}

/// `type` and `resource`: normally `{"id", "name"}`, sometimes a bare string.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum NameBlock {
    Object {
        #[serde(default, deserialize_with = "flex_string")]
        id: Option<String>,
        #[serde(default, deserialize_with = "flex_string")]
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SourceTranslation {
    #[serde(default, deserialize_with = "flex_string")]
    pub language_id: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub resource_id: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub checking_level: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
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

/// A `manifest.json`'s bytes, as written. A leading byte order mark is skipped.
pub fn read(bytes: &[u8]) -> Result<BttWriterManifest, ReadError> {
    serde_json::from_slice(without_bom(bytes)).map_err(|e| ReadError::Json(e.to_string()))
}

impl BttWriterManifest {
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
                let language = source.language_id.as_deref()?.trim().to_lowercase();
                let resource = source.resource_id.as_deref()?.trim().to_lowercase();
                if language.is_empty() || resource.is_empty() {
                    return None;
                }
                let version = source
                    .version
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string);
                Some(Related {
                    language,
                    resource,
                    version,
                })
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

/// A string, or a number or boolean written where a string belongs (a
/// version `20211118`). Anything else reads as absent.
fn flex_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

/// A list of strings; other entries are dropped, and anything but a list
/// reads as empty.
fn flex_strings<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}

fn flex_chunks<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(match value {
        Value::Array(_) => Some(flex_strings(value).unwrap_or_default()),
        _ => None,
    })
}

/// A list of sources, or a map whose values are the sources (`{}` is seen in
/// real files). An entry that isn't an object is dropped.
fn flex_sources<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<SourceTranslation>, D::Error> {
    let items = match Value::deserialize(d)? {
        Value::Array(items) => items,
        Value::Object(map) => map.into_iter().map(|(_, v)| v).collect(),
        _ => Vec::new(),
    };
    Ok(items
        .into_iter()
        .filter(Value::is_object)
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect())
}

#[cfg(test)]
mod tests;
