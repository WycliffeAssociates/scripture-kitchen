//! What a resource was translated from, as every format writes it.
//!
//! ```text
//! RC         source: [{identifier: ulb, language: en, version: "12"}]      → en/ulb 12
//! Burrito    {relationType: source, id: "wycliffeassociates::en/ulb"}      → en/ulb
//!            {relationType: source, id: "wacs::WA-Catalog/en_ulb"}         → en/ulb
//! Writer     source_translations: [{language_id: en, resource_id: ulb}]    → en/ulb
//! ```
//!
//! From Sefer's `relationships.ts`: language and resource lowercased and
//! trimmed, and a source missing either is left out. A Burrito id under the
//! `wacs` authority is a WACS repo, `owner/<language>_<resource>`.

use crate::Related;

pub(crate) fn related(language: &str, resource: &str, version: Option<&str>) -> Option<Related> {
    let language = language.trim().to_lowercase();
    let resource = resource.trim().to_lowercase();
    if language.is_empty() || resource.is_empty() {
        return None;
    }
    let version = version
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    Some(Related {
        language,
        resource,
        version,
    })
}

/// A Burrito relationship id: `authority::language/resource`, or under
/// `wacs`, `authority::owner/language_resource`.
pub(crate) fn from_burrito_id(id: &str, revision: Option<&str>) -> Option<Related> {
    let (authority, rest) = id.split_once("::")?;
    let (first, second) = rest.split_once('/')?;
    if authority.is_empty()
        || authority.contains(':')
        || second.contains('/')
        || rest.chars().any(char::is_whitespace)
    {
        return None;
    }
    if authority == "wacs" {
        let (language, resource) = second.split_once('_')?;
        related(language, resource, revision)
    } else {
        related(first, second, revision)
    }
}

#[cfg(test)]
mod tests;
