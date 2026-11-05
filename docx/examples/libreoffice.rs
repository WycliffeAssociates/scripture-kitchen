//! Every book the round-trip test compares, saved by LibreOffice, then
//! imported and compared again. Proves the styles survive a real editor's
//! open and save, not only this crate's own writer.
//!
//! ```text
//! cargo run --release -p docx --example libreoffice
//! ```
//!
//! Needs `soffice` with the Writer module (`libreoffice-writer`). An example
//! and not a test: CI has no LibreOffice, and it runs ignored tests too.

#[path = "../tests/common/mod.rs"]
mod common;

use std::process::Command;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/docx-libreoffice");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("saved")).unwrap();
    let mut books = Vec::new();
    for path in common::corpus() {
        let source = std::fs::read_to_string(&path).unwrap();
        let out = docx::export(&source, &docx::ExportOptions::default());
        if !out.unsupported.is_empty() {
            continue;
        }
        let file = format!("{}.docx", common::name(&path).replace('/', "__"));
        std::fs::write(dir.join(&file), &out.docx).unwrap();
        books.push((common::name(&path), file, source, out.book.is_some()));
    }
    let status = Command::new("soffice")
        .args(["--headless", "--convert-to", "docx", "--outdir", "saved"])
        .args(books.iter().map(|(_, f, _, _)| f))
        .current_dir(&dir)
        .output()
        .expect("soffice on the PATH");
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    let mut differ = 0;
    for (name, file, source, has_id) in &books {
        let saved = std::fs::read(dir.join("saved").join(file)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = docx::import(&saved).unwrap_or_else(|e| panic!("{name}: {e}"));
        // A book with no `\id` has no key to file its source hash under.
        let source_kept = back.books.first().and_then(|b| b.source.as_ref()).is_some() || !has_id;
        let verdict = match common::compare(source, &back.books[0].usfm) {
            None if back.report.is_exact() && source_kept => "same book".to_owned(),
            None => format!("same book, but {:?}, source kept: {source_kept}", back.report),
            Some(d) => {
                differ += 1;
                format!("DIFFERS at {d}")
            }
        };
        println!("{name}: {verdict}");
    }
    println!("{} books through LibreOffice, {differ} differ", books.len());
}
