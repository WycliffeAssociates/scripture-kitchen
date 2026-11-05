//! Health checks: what's wrong with a manifest, as findings, Door43-style.
//!
//! ```text
//! read(…).findings     the manifest alone   title-set, language-set, book-title-translated, sort-canon
//!                                           (and what reading forgave: lenient-value, manifest-valid)
//! check(&read, &repo)  the repo too         repo-name-language
//! ```
//!
//! Each finding names its [`Check`], so a page can list every check as passed
//! or failed. USFM validity and alignment are onion's and galley's; whether a
//! release was published is the forge's.

use crate::tree::pointer;
use crate::{Check, Detail, Format, Manifest, ManifestFinding, Read, Severity};

/// What a check may know about the repo beyond its manifest.
#[derive(Debug, Clone, Default)]
pub struct Repo<'a> {
    /// `owner/repo`, or just the repo's name: `Tech_Advance/tpi_reg`.
    pub name: Option<&'a str>,
}

/// The checks that need more than the manifest. Pure: the caller hands over
/// what it has, and a check with nothing to look at finds nothing.
pub fn check(read: &Read, repo: &Repo<'_>) -> Vec<ManifestFinding> {
    let mut out = Vec::new();
    let Some(manifest) = &read.manifest else {
        return out;
    };
    if let Some(name) = repo.name {
        let repo_name = name.rsplit('/').next().unwrap_or(name);
        let prefix = repo_name.split('_').next().unwrap_or(repo_name);
        let tag = &manifest.language.tag;
        if !tag.is_empty() && !prefix.eq_ignore_ascii_case(tag) {
            out.push(ManifestFinding {
                check: Check::RepoNameLanguage,
                severity: Severity::Warning,
                at: None,
                book: None,
                message: format!(
                    "The repo is named {repo_name}, but its manifest's language is {tag}."
                ),
            });
        }
    }
    out
}

/// The checks a manifest answers alone, added to what reading found.
pub(crate) fn manifest_only(manifest: &Manifest, detail: &Detail, out: &mut Vec<ManifestFinding>) {
    let finding = |check, severity, at: Option<String>, message: String| ManifestFinding {
        check,
        severity,
        at,
        book: None,
        message,
    };
    let (name_at, language_at) = match manifest.format {
        Format::Rc => ("/dublin_core/title", "/dublin_core/language/identifier"),
        Format::Burrito => ("/identification/name", "/languages/0/tag"),
        Format::BttWriter => ("/resource/name", "/target_language/id"),
    };

    // A Writer manifest names its resource's kind ("Regular"), not the work.
    if manifest.format != Format::BttWriter {
        let codes: Vec<&str> = manifest.abbreviation.values().map(String::as_str).collect();
        if manifest.name.values().all(|n| n.trim().is_empty()) {
            out.push(finding(
                Check::TitleSet,
                Severity::Warning,
                Some(name_at.into()),
                "The resource has no name.".into(),
            ));
        } else if let Some(n) = manifest
            .name
            .values()
            .find(|n| codes.iter().any(|c| c.eq_ignore_ascii_case(n.trim())))
        {
            out.push(finding(
                Check::TitleSet,
                Severity::Warning,
                Some(name_at.into()),
                format!("The resource's name is its code, {n}; give it a name of its own."),
            ));
        }
    }

    if manifest.language.tag.trim().is_empty() {
        out.push(finding(
            Check::LanguageSet,
            Severity::Warning,
            Some(language_at.into()),
            "The manifest names no language.".into(),
        ));
    }

    let english = {
        let tag = manifest.language.tag.to_ascii_lowercase();
        tag == "en" || tag.starts_with("en-")
    };
    for entry in &manifest.books {
        let Some(book) = canon::book(entry.book) else {
            continue;
        };
        if !english && !manifest.language.tag.is_empty() {
            let untranslated = entry.title.values().find(|title| {
                let title = title.trim();
                title.eq_ignore_ascii_case(book.english)
                    || book.aliases.iter().any(|a| title.eq_ignore_ascii_case(a))
            });
            if let Some(title) = untranslated {
                out.push(ManifestFinding {
                    check: Check::BookTitleTranslated,
                    severity: Severity::Info,
                    at: Some(title_at(detail, entry.book)),
                    book: Some(entry.book),
                    message: format!(
                        "{}'s title is {title}, the English name, in a {} project.",
                        entry.book, manifest.language.tag
                    ),
                });
            }
        }
        if let (Format::Rc, Some(sort), Some(place)) =
            (manifest.format, entry.sort, book.canonical_index)
            && sort != i64::from(place)
        {
            out.push(ManifestFinding {
                check: Check::SortCanon,
                severity: Severity::Info,
                at: Some(rc_project_at(detail, entry.book, "sort")),
                book: Some(entry.book),
                message: format!(
                    "{}'s sort is {sort}; its place in the canon is {place}.",
                    entry.book
                ),
            });
        }
    }
}

fn rc_project_at(detail: &Detail, book: mise::books::BookKey, field: &str) -> String {
    let Detail::Rc(rc) = detail else {
        return String::new();
    };
    let i = rc
        .projects
        .iter()
        .position(|p| p.identifier.as_deref().and_then(crate::book_of) == Some(book))
        .unwrap_or(0);
    pointer(&format!("/projects/{i}"), field)
}

fn title_at(detail: &Detail, book: mise::books::BookKey) -> String {
    match detail {
        Detail::Rc(_) => rc_project_at(detail, book, "title"),
        Detail::Burrito(b) => {
            let code = b
                .localized_names
                .iter()
                .find(|(code, _)| crate::book_of(code) == Some(book))
                .map_or_else(|| book.to_string(), |(code, _)| code.clone());
            pointer(&pointer("/localizedNames", &code), "short")
        }
        Detail::BttWriter(_) => "/project/name".into(),
    }
}
