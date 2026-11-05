use super::*;

#[test]
fn subjects_fall_back_through_identifier_and_book() {
    assert_eq!(
        subject(Some("Aligned Bible"), None, None).as_deref(),
        Some("Bible")
    );
    assert_eq!(
        subject(Some("Text"), Some("reg"), None).as_deref(),
        Some("Bible")
    );
    assert_eq!(
        subject(None, Some("tn"), None).as_deref(),
        Some("Translation Notes")
    );
    assert_eq!(subject(None, None, Some("2th")).as_deref(), Some("Bible"));
    assert_eq!(
        subject(Some(" Something New "), None, None).as_deref(),
        Some("Something New")
    );
    assert_eq!(subject(None, Some("xyz"), None), None);
}

#[test]
fn domains() {
    assert_eq!(
        Domain::of_subject("Translation Words"),
        Some(Domain::Parascriptural)
    );
    assert_eq!(
        Domain::of_subject("Open Bible Stories"),
        Some(Domain::Gloss)
    );
    assert_eq!(
        Domain::of_flavor_type("peripheral"),
        Some(Domain::Peripheral)
    );
    assert_eq!(Domain::of_flavor_type("x-custom"), None);
}
