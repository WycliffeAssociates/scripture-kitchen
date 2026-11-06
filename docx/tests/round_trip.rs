//! USFM -> .docx -> USFM is the same book, compared as onion's USJ.
//!
//! ```text
//! origin.usfm --export--> .docx --import--> back.usfm
//! usj(origin) == usj(back)       for every book export carried whole
//! ```
//!
//! USJ is the yardstick because it has already dropped what USFM's spelling
//! lets vary (line breaks, runs of spaces, where a closer is optional). A book
//! export could not carry whole (its `unsupported` is not empty) is not
//! compared; [`NOT_CARRIED`] names each. `examples/libreoffice.rs` runs the
//! same comparison after a LibreOffice save.
//!
//! Instrument: VOLUME — `testData/exampleCorpora`, the whole test tier.

mod common;

use rayon::prelude::*;

/// Books export reports something in, so none leaves the comparison
/// unnoticed. en_ult's Mark is aligned: every word is a `\w` with attributes
/// inside a `\zaln` milestone, which v1 does not carry.
const NOT_CARRIED: &[&str] = &["en_ult-fixtures/42-MRK.usfm"];

enum Outcome {
    Same,
    NotCarried(String),
    Differs(String),
}

#[test]
fn every_carried_book_round_trips() {
    let outcomes: Vec<(String, Outcome)> = common::corpus()
        .par_iter()
        .map(|path| {
            let name = common::name(path);
            let source = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{name}: {e}"));
            let out = docx::export(&source, &docx::ExportOptions::default());
            if let Some(first) = out.unsupported.first() {
                return (name, Outcome::NotCarried(first.what.clone()));
            }
            let back = docx::import(&out.docx, &docx::Settings::default())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(back.report.is_exact(), "{name}: {:?}", back.report);
            assert_eq!(back.books.len(), 1, "{name}");
            match common::compare(&source, &back.books[0].usfm) {
                None => (name, Outcome::Same),
                Some(d) => (name, Outcome::Differs(d)),
            }
        })
        .collect();

    let differs: Vec<String> = outcomes
        .iter()
        .filter_map(|(n, o)| match o {
            Outcome::Differs(d) => Some(format!("{n}: {d}")),
            _ => None,
        })
        .collect();
    assert!(
        differs.is_empty(),
        "{} books differ:\n{}",
        differs.len(),
        differs.join("\n")
    );
    let not_carried: Vec<(&str, &str)> = outcomes
        .iter()
        .filter_map(|(n, o)| match o {
            Outcome::NotCarried(why) => Some((n.as_str(), why.as_str())),
            _ => None,
        })
        .collect();
    let names: Vec<&str> = not_carried.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names, NOT_CARRIED,
        "the books export cannot carry whole changed: {not_carried:?}"
    );
    let same = outcomes
        .iter()
        .filter(|(_, o)| matches!(o, Outcome::Same))
        .count();
    assert!(same >= 159, "only {same} books compared");
}
