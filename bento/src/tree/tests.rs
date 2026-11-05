use super::*;

// Why: a Burrito's ingredients are its books, and the file's order is the
// only order they have. serde_json's own map sorts keys.
#[test]
fn json_keeps_key_order_and_numbers_as_written() {
    let node = from_json(br#"{"b": 1, "a": [true, null, 1.5], "c": {"z": "x", "y": 2}}"#).unwrap();
    let keys: Vec<&str> = node.entries().into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, ["b", "a", "c"]);
    assert_eq!(node.get("b"), Some(&Node::Number("1".into())));
    assert_eq!(
        node.get("a"),
        Some(&Node::List(vec![
            Node::Bool(true),
            Node::Null,
            Node::Number("1.5".into())
        ]))
    );
}

#[test]
fn a_key_written_twice_answers_with_the_later_as_json_parse_does() {
    let node = from_json(br#"{"a": 1, "b": 2, "a": 3}"#).unwrap();
    assert_eq!(node.get("a"), Some(&Node::Number("3".into())));
    let entries: Vec<(&str, &Node)> = node.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0], ("a", &Node::Number("3".into())));
}

// Why: YAML 1.2 types plain scalars, and a manifest's version or sort is
// whatever the writer typed: `1.10` must stay `1.10`, `01` a number.
#[test]
fn yaml_scalars_keep_their_text() {
    let read =
        from_yaml("version: 1.10\nsort: 01\nissued: 2025-10-06\nyes: yes\nnothing: ~\nq: '12'\n")
            .unwrap();
    let n = read.node;
    assert_eq!(n.get("version"), Some(&Node::Number("1.10".into())));
    assert_eq!(n.get("sort"), Some(&Node::Number("01".into())));
    assert_eq!(n.get("issued"), Some(&Node::String("2025-10-06".into())));
    assert_eq!(
        n.get("yes"),
        Some(&Node::String("yes".into())),
        "YAML 1.2: yes is text"
    );
    assert_eq!(n.get("nothing"), Some(&Node::Null));
    assert_eq!(n.get("q"), Some(&Node::String("12".into())));
}

#[test]
fn yaml_reads_the_first_document_that_says_anything() {
    let read = from_yaml("---\n---\na: 1\n---\nb: 2\n").unwrap();
    assert_eq!(read.documents, 3);
    assert_eq!(read.node.get("a"), Some(&Node::Number("1".into())));
    assert_eq!(read.node.get("b"), None);
    assert!(from_yaml("a: [1, 2\n").is_err());
}

#[test]
fn pointers_escape_slashes_and_tildes() {
    assert_eq!(
        pointer("/ingredients", "./41-MAT.usfm"),
        "/ingredients/.~141-MAT.usfm"
    );
    assert_eq!(pointer("", "a~b"), "/a~0b");
}
