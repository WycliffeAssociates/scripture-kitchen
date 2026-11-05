//! Rewrites `src/styles.xml` from onion's marker table, and `template.dotx`
//! from it. The freshness tests in `tests/generated.rs` fail until this has
//! run after a table, look or starter change.

fn main() {
    let styles = concat!(env!("CARGO_MANIFEST_DIR"), "/src/styles.xml");
    std::fs::write(styles, docx::styles::styles_xml()).expect("write src/styles.xml");
    println!("wrote {styles}");
    // The template embeds styles.xml at compile time: run twice after a
    // style change, or the template is one build behind.
    let template = concat!(env!("CARGO_MANIFEST_DIR"), "/template.dotx");
    std::fs::write(template, docx::template()).expect("write template.dotx");
    println!("wrote {template}");
}
