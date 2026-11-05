//! Imports every .docx/.odt in a directory with the default settings, writing
//! `<name>.usfm` and `<name>.sum` (detected mode, skipped numbers, guesses) the
//! way a script over `@sefer/document-import` writes them, so the two can be
//! diffed file by file.
//!
//! ```text
//! cargo run --release -p docx --example import_dir -- <in-dir> <out-dir>
//! ```

use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    std::fs::create_dir_all(output).unwrap();
    for entry in std::fs::read_dir(input).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let back = docx::import(&std::fs::read(&path).unwrap(), &docx::Settings::default()).unwrap();
        let r = &back.report;
        let usfm: String = back.books.iter().map(|b| b.usfm.as_str()).collect();
        let skipped: Vec<_> = r
            .skipped
            .iter()
            .map(|s| {
                let reason = match s.reason {
                    docx::SkipReason::Sequence => "sequence",
                    docx::SkipReason::Versification => "versification",
                    docx::SkipReason::Thousands => "thousands",
                };
                json!({"ref": s.r#ref, "number": s.number, "reason": reason})
            })
            .collect();
        let guesses: Vec<_> = r.guesses.iter().map(|g| json!([g.r#ref, g.marker, g.reason, g.excerpt])).collect();
        let summary = format!(
            "detected={} skipped={} guesses={} chapters={}\n{}\n{}\n",
            r.detected.name(),
            r.skipped.len(),
            r.guesses.len(),
            r.chapters.len(),
            serde_json::to_string(&skipped).unwrap(),
            serde_json::to_string(&guesses).unwrap(),
        );
        std::fs::write(format!("{output}/{name}.usfm"), usfm).unwrap();
        std::fs::write(format!("{output}/{name}.sum"), summary).unwrap();
    }
}
