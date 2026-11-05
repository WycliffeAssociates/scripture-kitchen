use super::*;

fn key(code: &[u8; 3]) -> BookKey {
    BookKey::new(*code)
}

#[test]
fn a_plain_manifest_is_a_project() {
    let m = read(br#"{"target_language": {"id": "tmf", "name": "Toba-Maskoy", "direction": "ltr"},
        "project": {"id": "3jn", "name": "3 John"}, "type": {"id": "text"}, "resource": {"id": "reg"},
        "source_translations": [{"language_id": "en", "resource_id": "ulb", "version": "12"}],
        "translators": ["kpass"], "finished_chunks": ["01-01", "01-05"]}"#)
    .unwrap();
    assert_eq!(
        m.finished_chunks.as_deref(),
        Some(&["01-01".to_string(), "01-05".to_string()][..])
    );
    let p = m.project(None).unwrap();
    assert_eq!(p.book, key(b"3JN"));
    assert_eq!(p.language.id, "tmf");
    assert_eq!(p.language.direction, Direction::Ltr);
    assert_eq!(p.resource, "reg");
    assert_eq!(
        p.sources,
        [Related {
            language: "en".into(),
            resource: "ulb".into(),
            version: Some("12".into())
        }]
    );
    assert_eq!(p.translators, ["kpass"]);
}

#[test]
fn real_files_bend_the_shapes() {
    // A BOM, a number for a version, sources as a map, a bare-string resource,
    // translators with a stray number, a non-list finished_chunks.
    let m = read(b"\xEF\xBB\xBF{\"target_language\": {\"id\": \"ar\", \"direction\": \"rtl\"},
        \"project\": {\"id\": \"MAT\"}, \"resource\": \"ULB\",
        \"source_translations\": {\"0\": {\"language_id\": \"EN\", \"resource_id\": \"ulb\", \"version\": 20211118}, \"1\": 7},
        \"translators\": [\"a\", 3, \"b\"], \"finished_chunks\": {}}")
    .unwrap();
    assert_eq!(m.finished_chunks, None);
    assert_eq!(m.translators, ["a", "b"]);
    let p = m.project(None).unwrap();
    assert_eq!(p.book, key(b"MAT"));
    assert_eq!(p.language.direction, Direction::Rtl);
    assert_eq!(p.resource, "ulb");
    assert_eq!(p.sources[0].language, "en");
    assert_eq!(p.sources[0].version.as_deref(), Some("20211118"));
    assert_eq!(p.sources.len(), 1);
}

#[test]
fn the_repo_name_supplies_a_missing_resource() {
    let m = read(br#"{"target_language": {"id": "tmf"}, "project": {"id": "3jn"}}"#).unwrap();
    assert_eq!(m.project(Some("tmf_3jn_text_reg")).unwrap().resource, "reg");
    assert_eq!(m.project(None), None);
    let m = read(
        br#"{"target_language": {"id": "tmf"}, "project": {"id": "3jn"}, "resource": {"id": " "}}"#,
    )
    .unwrap();
    assert_eq!(m.project(Some("tmf_3jn_text_UDB")).unwrap().resource, "udb");
}

#[test]
fn not_a_book_project() {
    for text in [
        r#"{"target_language": {"id": "tmf"}, "project": {"id": "obs"}, "resource": {"id": "reg"}}"#,
        r#"{"target_language": {"id": "tmf"}, "project": {"id": "xyz"}, "resource": {"id": "reg"}}"#,
        r#"{"target_language": {"id": " "}, "project": {"id": "3jn"}, "resource": {"id": "reg"}}"#,
        r#"{"project": {"id": "3jn"}, "resource": {"id": "reg"}}"#,
        r#"{"target_language": {"id": "tmf"}, "resource": {"id": "reg"}}"#,
        "{}",
    ] {
        assert_eq!(read(text.as_bytes()).unwrap().project(None), None, "{text}");
    }
}

#[test]
fn not_json_is_an_error_not_a_panic() {
    assert!(read(b"").is_err());
    assert!(read(b"{\"project\": ").is_err());
    assert!(read(b"[1, 2]").is_err());
    assert!(read(b"\xFF\xFE{}").is_err());
}
