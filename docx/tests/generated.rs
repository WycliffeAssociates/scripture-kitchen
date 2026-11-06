//! The checked-in generated files are what the code would produce now.
//!
//! - `src/styles.xml`: onion's marker table and the curated looks. Checked in
//!   so the export never computes it, and a style change is a reviewable diff.
//! - `template.dotx`: every style and a starter book, for writing USFM in Word
//!   from scratch. Its bytes are fixed: the zip carries no timestamps.

const CHECKED_IN: &str = include_str!("../src/styles.xml");

#[test]
fn styles_xml_is_not_stale() {
    let fresh = docx::styles::styles_xml();
    assert!(
        fresh == CHECKED_IN,
        "{}",
        ticket::stale::first_difference(
            "docx/src/styles.xml",
            "cargo run -p docx --bin codegen",
            &fresh,
            CHECKED_IN
        )
    );
}

const TEMPLATE: &[u8] = include_bytes!("../template.dotx");

#[test]
fn template_dotx_is_not_stale() {
    assert!(
        docx::template() == TEMPLATE,
        "docx/template.dotx is stale: run `cargo run -p docx --bin codegen` (twice after a style change)"
    );
}
