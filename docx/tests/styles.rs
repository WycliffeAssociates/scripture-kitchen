//! `src/styles.xml` is what onion's marker table and the curated looks would
//! produce now. It is checked in so the export never computes it, and so a
//! style change is a reviewable diff.

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
