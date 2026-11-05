use super::*;

fn r(language: &str, resource: &str, version: Option<&str>) -> Option<Related> {
    Some(Related {
        language: language.into(),
        resource: resource.into(),
        version: version.map(Into::into),
    })
}

#[test]
fn sources_are_lowercased_and_need_both_halves() {
    assert_eq!(
        related(" EN ", "ULB", Some(" 12 ")),
        r("en", "ulb", Some("12"))
    );
    assert_eq!(related("en", "ulb", Some(" ")), r("en", "ulb", None));
    assert_eq!(related("", "ulb", None), None);
    assert_eq!(related("en", " ", None), None);
}

// Why: existing projects say `wycliffeassociates::en/ulb`; new ones say
// `wacs::WA-Catalog/en_ulb` (Will, 2026-11-05). Both must read as en/ulb.
#[test]
fn burrito_ids_under_either_authority() {
    assert_eq!(
        from_burrito_id("wycliffeassociates::en/ulb", Some("12")),
        r("en", "ulb", Some("12"))
    );
    assert_eq!(
        from_burrito_id("wacs::WA-Catalog/en_ulb", None),
        r("en", "ulb", None)
    );
    assert_eq!(
        from_burrito_id("wacs::WA-Catalog/es-419_ulb", None),
        r("es-419", "ulb", None)
    );
    for bad in [
        "en/ulb",
        "x::en",
        "x::en/ulb/more",
        "x::en /ulb",
        "a:b::en/ulb",
        "wacs::owner/noresource",
        "::en/ulb",
    ] {
        assert_eq!(from_burrito_id(bad, None), None, "{bad}");
    }
}
