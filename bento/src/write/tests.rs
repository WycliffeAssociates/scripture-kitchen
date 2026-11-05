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

// Why: the rule decides without a parser, so it's checked against one: any
// string it lets through plain must read back as itself.
#[test]
fn every_plain_string_reads_back_as_itself() {
    let cases = [
        "Baibel",
        "Tok Pisin",
        "1 Timoti",
        "II Cesolonaika",
        "Ɓille",
        "Ọlọ́run",
        "كتاب",
        "a: b",
        "a:b",
        "a #b",
        "a#b",
        "C#",
        "2026-11-05",
        "2026-11-05T14:30:00Z",
        "12",
        "-12",
        "+12",
        "1.10",
        "1e3",
        "1_000",
        "0x1F",
        "0o17",
        ".5",
        "5.",
        ".inf",
        "-.inf",
        ".nan",
        "yes",
        "No",
        "off",
        "ON",
        "y",
        "n",
        "~",
        "null",
        "True",
        "x: y z",
        "x-y",
        "-x",
        "?x",
        ":x",
        "x:",
        "&a",
        "*a",
        "!a",
        "|a",
        ">a",
        "%a",
        "@a",
        "`a",
        "'a",
        "\"a",
        "{a}",
        "[a]",
        "a,b",
        "a, b",
        " a",
        "a ",
        "\ta",
        "a\nb",
        "",
        "1:30",
        "v12",
        "12a",
        "ufw",
        "bible-nt",
        "text/usfm",
        "CC BY-SA 4.0",
        "./41-MAT.usfm",
        "Wycliffe Associates",
        "rc0.2",
        "21-05",
        "21-05.1",
        "en/ulb",
    ];
    for s in cases {
        let written = yaml_string(s);
        let back = crate::tree::from_yaml(&format!("k: {written}\n"))
            .unwrap()
            .node;
        assert_eq!(
            back.get("k"),
            Some(&Node::String(s.to_owned())),
            "{s:?} written as {written}"
        );
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
