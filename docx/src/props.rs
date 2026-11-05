//! `docProps/custom.xml`: which books a document holds and the exact USFM
//! each was exported from.
//!
//! ```text
//! usfm.books                     GEN
//! usfm.GEN.source.git-blob-sha1  9f2c…   git's object id for the USFM's bytes
//! usfm.GEN.source.xxh3-64        1a2b…   onion's dish-header hash, 16 hex digits
//! ```
//!
//! Optional on import: a document from anywhere has none, and is read from
//! its styles alone.

use crate::Source;
use crate::dom::Element;
use crate::xml;

const FMTID: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";

pub fn write(book: Option<&str>, source: &Source) -> String {
    let mut props: Vec<(String, String)> = Vec::new();
    if let Some(code) = book {
        props.push(("usfm.books".to_owned(), code.to_owned()));
        props.push((format!("usfm.{code}.source.git-blob-sha1"), source.git_blob_sha1.clone()));
        props.push((format!("usfm.{code}.source.xxh3-64"), source.xxh3_64.clone()));
    }
    let mut s = String::from(xml::DECLARATION);
    s.push_str(r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">"#);
    for (pid, (name, value)) in (2..).zip(&props) {
        s.push_str(&format!(
            r#"<property fmtid="{FMTID}" pid="{pid}" name="{}"><vt:lpwstr>{}</vt:lpwstr></property>"#,
            xml::attr(name),
            xml::text(value)
        ));
    }
    s.push_str("</Properties>\n");
    s
}

/// Every custom property, as (name, value text).
pub fn read(root: &Element) -> Vec<(String, String)> {
    root.elements()
        .filter(|e| e.name.ends_with("property"))
        .filter_map(|e| Some((e.attr("name")?.to_owned(), e.text())))
        .collect()
}

/// The source a book was exported from, if the properties name it.
pub fn source(props: &[(String, String)], code: &str) -> Option<Source> {
    let get = |key: String| props.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
    Some(Source {
        git_blob_sha1: get(format!("usfm.{code}.source.git-blob-sha1"))?,
        xxh3_64: get(format!("usfm.{code}.source.xxh3-64")).unwrap_or_default(),
    })
}
