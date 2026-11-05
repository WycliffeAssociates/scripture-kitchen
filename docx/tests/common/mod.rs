//! What the round-trip test and the by-hand examples compare with: onion's
//! USJ, and the first JSON path where two differ.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Every `.usfm`/`.sfm` file in onion's test tier, sorted. Fails loudly when
/// the tier is missing.
pub fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testData/exampleCorpora");
    let mut files = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap_or_else(|e| panic!("{}: {e}", root.display())) {
        for f in std::fs::read_dir(dir.unwrap().path()).unwrap() {
            let path = f.unwrap().path();
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("usfm" | "sfm")) {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(files.len() > 150, "the test tier is 160 books, found {}", files.len());
    files
}

/// `corpora-dir/file.usfm`: unique where the file name alone is not.
pub fn name(path: &Path) -> String {
    let dir = path.parent().unwrap().file_name().unwrap().to_string_lossy();
    format!("{dir}/{}", path.file_name().unwrap().to_string_lossy())
}

pub fn usj(usfm: &str) -> Value {
    let tokens = usfm_onion::lex(usfm);
    let cst = usfm_onion::cst::build(&tokens);
    serde_json::from_str(&usfm_onion::usj::usj(usfm.as_bytes(), &tokens, &cst)).unwrap()
}

/// The first JSON path where two values differ, with both sides.
pub fn first_difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => x
            .iter()
            .find_map(|(k, v)| match y.get(k) {
                Some(w) => first_difference(v, w, &format!("{path}.{k}")),
                None => Some(format!("{path}.{k}: missing after the round trip")),
            })
            .or_else(|| {
                y.keys().find(|k| !x.contains_key(*k)).map(|k| format!("{path}.{k}: added by the round trip"))
            }),
        (Value::Array(x), Value::Array(y)) => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (v, w))| first_difference(v, w, &format!("{path}[{i}]")))
            .or_else(|| {
                (x.len() != y.len()).then(|| {
                    let i = x.len().min(y.len());
                    format!("{path}: {} items became {}; first extra {}", x.len(), y.len(), x.get(i).or(y.get(i)).unwrap())
                })
            }),
        _ => (a != b).then(|| format!("{path}: {a} became {b}")),
    }
}

/// Is `back` the same book as `source`? `None` when it is.
pub fn compare(source: &str, back: &str) -> Option<String> {
    first_difference(&usj(source), &usj(back), "")
}
