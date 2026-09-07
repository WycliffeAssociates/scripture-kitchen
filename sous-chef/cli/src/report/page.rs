//! The page itself: the template, the substitution, and the length lanes'
//! own tables.
//!
//! ```text
//! render(&corpus_json, &lengths, ..)  -> one self-contained HTML file
//! ```

use super::*;

/// Samples kept per bucket before a card stops collecting.
pub(super) const SAMPLE_CAP: usize = 8;

/// Scalars of context either side of a sample's match, in the short snippet.
pub(super) const SNIPPET_CONTEXT: usize = 40;

/// The four stand-alone attachment buckets, joint (prev, next) neighbor
/// classes; `in-run` is everything else and is computed separately.
pub(super) const TOPO: [(&str, OuterClass, OuterClass); 4] = [
    ("Both", OuterClass::Letter, OuterClass::Letter),
    ("StartOnly", OuterClass::Letter, OuterClass::Space),
    ("EndOnly", OuterClass::Space, OuterClass::Letter),
    ("Neither", OuterClass::Space, OuterClass::Space),
];

/// One fired length row as the page shows it: the address, the ratio, both
/// scopes, and the two texts side by side.
///
/// The wire record carries the two deviations and a span; everything else
/// here the CLI recomputed from the two corpora it holds.
pub struct PairedUnit {
    pub address: String,
    pub ratio: f64,
    pub book_z: Option<f64>,
    pub project_z: Option<f64>,
    pub target: String,
    pub source: String,
}

/// One presence row as the page shows it: which side holds the verses, the
/// first key, and how many consecutive keys follow it.
pub struct PresenceUnit {
    pub book_idx: u16,
    pub address: String,
    pub kind: &'static str,
    pub keys: u32,
}

/// One source-copy row as the page shows it: the run against the unit's
/// eligible words, the shared text, and both verses behind it.
pub struct CopyUnit {
    pub address: String,
    pub run: u32,
    pub eligible: u32,
    pub shared: String,
    pub target: String,
    pub source: String,
}

/// The source comparison's contribution to the page; empty when no source
/// was declared, which is what hides the tab.
#[derive(Default)]
pub struct Paired {
    pub units: Vec<PairedUnit>,
    pub presence: Vec<PresenceUnit>,
    pub copies: Vec<CopyUnit>,
}

/// The self-contained inventory page for the target corpus.
pub fn render(
    name: &str,
    corpus: &Corpus<'_, OnionBook>,
    patterns: &[Pattern],
    findings: &[PackedFinding],
    paired: &Paired,
) -> String {
    let json = corpus_json(name, corpus, patterns, findings, paired);
    TEMPLATE.replace("@@CORPORA@@", &format!("[{json}]"))
}

/// `len[]`: one record per fired length row, in publication order.
pub(super) fn lengths_json(paired: &Paired) -> String {
    let rows: Vec<String> = paired
        .units
        .iter()
        .map(|unit| {
            let scope = |value: Option<f64>| match value {
                Some(value) => format!("{value:.2}"),
                None => "null".to_string(),
            };
            format!(
                "{{\"ref\":{},\"ratio\":{:.4},\"zb\":{},\"zp\":{},\"t\":{},\"s\":{}}}",
                json_str(&unit.address),
                unit.ratio,
                scope(unit.book_z),
                scope(unit.project_z),
                json_str(unit.target.trim()),
                json_str(unit.source.trim()),
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

/// `pres[]`: one record per presence row, in publication order.
pub(super) fn presence_json(paired: &Paired) -> String {
    let rows: Vec<String> = paired
        .presence
        .iter()
        .map(|row| {
            format!(
                "{{\"ref\":{},\"kind\":{},\"keys\":{}}}",
                json_str(&row.address),
                json_str(row.kind),
                row.keys,
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

/// `copy[]`: one record per source-copy row, in publication order.
pub(super) fn copies_json(paired: &Paired) -> String {
    let rows: Vec<String> = paired
        .copies
        .iter()
        .map(|row| {
            format!(
                "{{\"ref\":{},\"run\":{},\"elig\":{},\"w\":{},\"t\":{},\"s\":{}}}",
                json_str(&row.address),
                row.run,
                row.eligible,
                json_str(row.shared.trim()),
                json_str(row.target.trim()),
                json_str(row.source.trim()),
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

pub(super) const TEMPLATE: &str = include_str!("../../templates/inventory.html");
