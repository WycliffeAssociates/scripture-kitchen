use super::*;
use crate::tree::from_json;

fn checks(r: Reader) -> Vec<(Check, Severity, Option<String>)> {
    r.into_findings()
        .into_iter()
        .map(|f| (f.check, f.severity, f.at))
        .collect()
}

// Why: rule 1, reading never changes anything, means leniency is said, not
// silent: each forgiven value is a finding pointing at it.
#[test]
fn forgiven_values_are_read_and_said() {
    let n = from_json(
        br#"{"v": 12, "b": true, "r": "en/tn", "s": "01", "o": {"x": 1}, "l": ["a", 2]}"#,
    )
    .unwrap();
    let mut r = Reader::new(Vec::new());
    assert_eq!(r.field(&n, "v", ""), Some("12".into()));
    assert_eq!(r.field(&n, "b", ""), Some("true".into()));
    assert_eq!(r.strings(n.get("r"), "/r"), ["en/tn"]);
    assert_eq!(r.int(n.get("s"), "/s"), Some(1));
    assert_eq!(r.field(&n, "o", ""), None);
    assert_eq!(r.strings(n.get("l"), "/l"), ["a", "2"]);
    assert_eq!(r.field(&n, "missing", ""), None);
    assert_eq!(
        checks(r),
        [
            (Check::LenientValue, Severity::Info, Some("/v".into())),
            (Check::LenientValue, Severity::Info, Some("/b".into())),
            (Check::LenientValue, Severity::Info, Some("/r".into())),
            (Check::LenientValue, Severity::Info, Some("/s".into())),
            (Check::ManifestValid, Severity::Warning, Some("/o".into())),
            (Check::LenientValue, Severity::Info, Some("/l/1".into())),
        ]
    );
}

#[test]
fn what_cant_be_read_is_left_unread_with_a_warning() {
    let n = from_json(
        br#"{"n": "twelve", "f": 1.5, "m": [1], "t": {"en": "Bible", "fr": 3, "x": []}}"#,
    )
    .unwrap();
    let mut r = Reader::new(Vec::new());
    assert_eq!(r.int(n.get("n"), "/n"), None);
    assert_eq!(r.int(n.get("f"), "/f"), None);
    assert!(r.map(n.get("m"), "/m").is_none());
    let t = r.localized(n.get("t"), "/t");
    assert_eq!(
        t,
        LocalizedText::from([("en".into(), "Bible".into()), ("fr".into(), "3".into())])
    );
    let found = checks(r);
    assert_eq!(
        found
            .iter()
            .filter(|(c, _, _)| *c == Check::ManifestValid)
            .count(),
        4,
        "{found:?}"
    );
}
