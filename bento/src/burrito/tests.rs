use super::*;

// Why: WA's fork wants `meta.dateCreated` as a date-time but a primary
// `timestamp` as a date; Sefer wrote each as the other. Both rules must be exact.
#[test]
fn date_times_and_dates_are_the_schemas() {
    for good in [
        "2026-11-05T14:30:00Z",
        "2026-11-05T14:30:00.123+02:00",
        "2026-09-06T00:00:00.000Z",
        "2026-11-05t14:30:00z",
    ] {
        assert!(is_date_time(good), "{good}");
    }
    for bad in [
        "2026-11-05",
        "2026-11-05T14:30Z",
        "2026-11-05T14:30:00",
        "2026-11-05T14:30:00.Z",
        "2026-11-05 14:30:00Z",
    ] {
        assert!(!is_date_time(bad), "{bad}");
    }
    for good in ["2026", "2026-11", "2026-11-04"] {
        assert!(is_date(good), "{good}");
    }
    for bad in ["26", "2026-1", "2026-11-04T00:00:00Z", "2026-11-04-01", ""] {
        assert!(!is_date(bad), "{bad}");
    }
}

#[test]
fn authority_labels() {
    assert!(is_authority_label("wacs"));
    assert!(is_authority_label("wycliffeassociates"));
    assert!(is_authority_label("a-1"));
    for bad in ["w", "Wacs", "wacs-", "1wacs", "wa cs"] {
        assert!(!is_authority_label(bad), "{bad}");
    }
}
