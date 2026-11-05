use super::*;
use crate::tree::from_json;

// Why: `bento.ts` declares these shapes by hand; the names and the absent
// keys are the contract a TypeScript caller reads.
#[test]
fn a_read_is_bento_ts_read() {
    let yaml = b"dublin_core:\n  identifier: tn\n  title: Notes\n  language: {identifier: tn, title: Setswana}\n  source: [{identifier: ulb, language: en}]\nprojects:\n  - {identifier: luk, path: ./43-LUK.usfm, sort: 41, title: Luke}\n";
    let (read, doc) = crate::read_with_document(Format::Rc, yaml);
    let json = read_json(&read, doc);
    assert_eq!(
        json,
        serde_json::from_str::<serde_json::Value>(r#"{"manifest":{"format":"rc","name":{"tn":"Notes"},"abbreviation":{"tn":"tn"},"language":{"tag":"tn","name":{"tn":"Setswana"},"direction":"ltr"},"kind":{"domain":"parascriptural","subject":"Translation Notes","declared":{"identifier":"tn"}},"books":[{"book":"LUK","path":"./43-LUK.usfm","title":{"tn":"Luke"},"sort":41}],"sources":[{"relation":"source","language":"en","resource":"ulb"}]},"detail":{"format":"rc","document":{"dublin_core":{"identifier":"tn","language":{"identifier":"tn","title":"Setswana"},"source":[{"identifier":"ulb","language":"en"}],"title":"Notes"},"projects":[{"identifier":"luk","path":"./43-LUK.usfm","sort":41,"title":"Luke"}]}},"findings":[{"check":"book-title-translated","severity":"info","at":"/projects/0/title","book":"LUK","message":"LUK's title is Luke, the English name, in a tn project."},{"check":"sort-canon","severity":"info","at":"/projects/0/sort","book":"LUK","message":"LUK's sort is 41; its place in the canon is 42."}]}"#).unwrap()
    );
}

fn read_json(read: &Read, doc: Option<Node>) -> serde_json::Value {
    serde_json::from_str(&super::read(read, Some(Format::Rc), doc).to_json()).unwrap()
}

#[test]
fn a_project_reads_from_create_bibles_argument() {
    let node = from_json(
        br#"{"title": "Baibel", "abbreviation": "reg", "language": {"tag": "tpi", "name": "Tok Pisin", "direction": "ltr"},
        "books": [{"book": "mat", "title": "Matyu"}], "canon": "protestant66",
        "sources": [{"language": "en", "resource": "ulb", "version": "12"}], "contributors": ["Mary"],
        "publisher": "WA", "creator": "c", "rights": "r", "checkingLevel": "1", "copyright": "x",
        "generator": {"name": "Sefer", "version": "1"}, "createdAt": "2026-11-05T00:00:00Z", "repository": "o/r"}"#,
    )
    .unwrap();
    let given = books(&node, "books", vec![b"\\id MAT".to_vec()]).unwrap();
    let p = project(&node, given).unwrap();
    assert_eq!(p.books[0].book.to_string(), "MAT");
    assert_eq!(p.sources[0].version.as_deref(), Some("12"));
    assert_eq!(p.generator, ("Sefer".to_string(), "1".to_string()));

    assert_eq!(
        books(&node, "books", Vec::new()).unwrap_err(),
        "books: 1 books but 0 byte arrays"
    );
    let bad = from_json(br#"{"title": 3}"#).unwrap();
    assert_eq!(
        project(&bad, Vec::new()).unwrap_err(),
        "language is required"
    );
    assert_eq!(
        formats(&["epub".into()]).unwrap_err(),
        "epub isn't a format bento writes (burrito, rc)"
    );
}
