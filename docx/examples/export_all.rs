//! Exports every test-tier book the round trip carries into a directory, one
//! .docx each: material for trying the importer on documents by hand.
//!
//! ```text
//! cargo run --release -p docx --example export_all -- <out-dir>
//! ```

#[path = "../tests/common/mod.rs"]
#[allow(dead_code)]
mod common;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("an output directory"));
    std::fs::create_dir_all(&dir).unwrap();
    let mut n = 0;
    for path in common::corpus() {
        let source = std::fs::read_to_string(&path).unwrap();
        let out = docx::export(&source, &docx::ExportOptions::default());
        if out.unsupported.is_empty() {
            std::fs::write(
                dir.join(format!("{}.docx", common::name(&path).replace('/', "__"))),
                &out.docx,
            )
            .unwrap();
            n += 1;
        }
    }
    println!("{n} books in {}", dir.display());
}
