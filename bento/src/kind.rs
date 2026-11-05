//! One vocabulary for what a resource is, whatever its manifest says.
//!
//! ```text
//! subject "Aligned Bible"      → Bible               scripture
//! identifier "tn"              → Translation Notes   parascriptural
//! book hint "2th" (Writer)     → Bible               scripture
//! subject "Something New"      → Something New       (no domain)
//! ```
//!
//! From the hub's `canonical_subject` and `Domain` (hub `crates/manifest`).

/// What a resource is, broadly: Scripture Burrito's `flavorType` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Domain {
    /// Bible text.
    Scripture,
    /// Retellings: Open Bible Stories.
    Gloss,
    /// Indexed by book, chapter and verse, but not Scripture: notes, words, questions.
    Parascriptural,
    /// Everything else: Translation Academy.
    Peripheral,
}

impl Domain {
    /// A Burrito `flavorType.name`.
    pub fn of_flavor_type(name: &str) -> Option<Domain> {
        match name {
            "scripture" => Some(Domain::Scripture),
            "gloss" => Some(Domain::Gloss),
            "parascriptural" => Some(Domain::Parascriptural),
            "peripheral" => Some(Domain::Peripheral),
            _ => None,
        }
    }

    /// The domain of a canonical subject.
    pub fn of_subject(subject: &str) -> Option<Domain> {
        match subject {
            "Bible" => Some(Domain::Scripture),
            "Open Bible Stories" => Some(Domain::Gloss),
            "Translation Notes"
            | "Translation Words"
            | "Translation Questions"
            | "OBS Translation Notes"
            | "TSV OBS Study Notes" => Some(Domain::Parascriptural),
            "Translation Academy" => Some(Domain::Peripheral),
            _ => None,
        }
    }
}

/// The canonical subject from, in order, what the manifest says its subject
/// is, its identifier, and a book code (a Writer project's only signal). A
/// subject we don't know is kept as written, trimmed.
pub(crate) fn subject(
    declared: Option<&str>,
    identifier: Option<&str>,
    book: Option<&str>,
) -> Option<String> {
    fn lookup(s: &str) -> Option<&'static str> {
        let lower = s.trim().to_ascii_lowercase();
        match lower.as_str() {
            "bible" | "aligned bible" | "open bible" | "texttranslation" | "audiotranslation"
            | "typesetscripture" | "ulb" | "udb" | "reg" => Some("Bible"),
            "translator notes" | "translation notes" | "notes" | "tn" => Some("Translation Notes"),
            "translation words" | "tw" => Some("Translation Words"),
            "translation questions" | "tq" => Some("Translation Questions"),
            "translation academy" | "translation manual" | "ta" => Some("Translation Academy"),
            "open bible stories" | "textstories" | "obs" => Some("Open Bible Stories"),
            other if crate::book_of(other).is_some() => Some("Bible"),
            _ => None,
        }
    }
    [declared, identifier, book]
        .into_iter()
        .flatten()
        .find_map(lookup)
        .map(str::to_owned)
        .or_else(|| {
            declared
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
}

#[cfg(test)]
mod tests;
