//! Rewrites `src/styles.xml` from onion's marker table. The freshness test in
//! `tests/styles.rs` fails until this has run after a table or look change.

fn main() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/styles.xml");
    std::fs::write(path, docx::styles::styles_xml()).expect("write src/styles.xml");
    println!("wrote {path}");
}
