use super::*;

// Why: a value written plain must read back as the same text, or the next
// reader sees a number, a date, a boolean or a different string.
#[test]
fn strings_are_plain_only_when_they_read_back_the_same() {
    for plain in [
        "Baibel",
        "Tok Pisin",
        "CC BY-SA 4.0",
        "./41-MAT.usfm",
        "tpi/reg",
        "1 Timoti",
        "Wycliffe Associates",
    ] {
        assert_eq!(yaml_string(plain), plain);
    }
    for (quoted, as_written) in [
        ("1", "\"1\""),
        ("12", "\"12\""),
        ("1.10", "\"1.10\""),
        ("true", "\"true\""),
        ("null", "\"null\""),
        ("", "\"\""),
        ("a: b", "\"a: b\""),
        ("- x", "\"- x\""),
        ("#tag", "\"#tag\""),
        (" padded", "\" padded\""),
        ("it's", "\"it's\""),
        ("two\nlines", "\"two\\nlines\""),
        ("[x]", "\"[x]\""),
    ] {
        assert_eq!(yaml_string(quoted), as_written, "{quoted:?}");
    }
}

#[test]
fn contributors_are_each_listed_once() {
    let names = ["Mary_Kila", " mary  kila ", "", "Tomas", "TOMAS", "Ana"].map(String::from);
    assert_eq!(contributors(&names), ["Mary_Kila", "Tomas", "Ana"]);
}

#[test]
fn fingerprints() {
    assert_eq!(
        fingerprint(b"abc"),
        Fingerprint {
            md5: "900150983cd24fb0d6963f7d28e17f72".into(),
            size: 3
        }
    );
}
