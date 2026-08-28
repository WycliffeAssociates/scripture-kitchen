//! The corpus every bench in this directory measures over: read, lexed and
//! built ONCE per process, so a `pass_only` bench pays for nothing beneath the
//! pass it names.
//!
//! `ONION_BENCH_CORPUS` picks a different tree; the default is en_ulb
//! (66 books, ~4.5 MB).

// Each bench file compiles its own copy and uses only the layers it needs —
// `rewrite` never touches the prebuilt trees, `export` never the token counter.
#![allow(dead_code)]

use std::sync::LazyLock;

use divan::counter::{BytesCount, ItemsCount};
use usfm_onion::{Token, cst::Cst};

const DEFAULT_CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../testData/exampleCorpora/en_ulb"
);

/// One process-wide build of the corpus at every layer a bench might start from.
pub struct Corpus {
    pub sources: Vec<String>,
    pub tokens: Vec<Vec<Token>>,
    pub trees: Vec<Cst>,
    pub bytes: u64,
    pub token_count: u64,
}

pub static CORPUS: LazyLock<Corpus> = LazyLock::new(load);

fn load() -> Corpus {
    let root = std::env::var("ONION_BENCH_CORPUS").unwrap_or_else(|_| DEFAULT_CORPUS.to_string());
    let mut paths = Vec::new();
    collect_usfm(std::path::Path::new(&root), &mut paths);
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no *.usfm under {root} — set ONION_BENCH_CORPUS"
    );

    let sources: Vec<String> = paths
        .iter()
        .map(|p| std::fs::read_to_string(p).expect("a readable book"))
        .collect();
    let tokens: Vec<Vec<Token>> = sources.iter().map(|s| usfm_onion::lex(s)).collect();
    let trees: Vec<Cst> = tokens.iter().map(|t| usfm_onion::cst::build(t)).collect();

    Corpus {
        bytes: sources.iter().map(|s| s.len() as u64).sum(),
        token_count: tokens.iter().map(|t| t.len() as u64).sum(),
        sources,
        tokens,
        trees,
    }
}

pub fn collect_usfm(at: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_usfm(&path, into);
        } else if path.extension().is_some_and(|e| e == "usfm") {
            into.push(path);
        }
    }
}

pub fn bytes() -> BytesCount {
    BytesCount::new(CORPUS.bytes)
}

pub fn tokens() -> ItemsCount {
    ItemsCount::new(CORPUS.token_count)
}

/// Pre-lexed and pre-built, zipped with their sources — the `pass_only` shape.
pub fn built() -> impl Iterator<Item = (&'static String, &'static Vec<Token>, &'static Cst)> {
    let c: &'static Corpus = &CORPUS;
    c.sources
        .iter()
        .zip(&c.tokens)
        .zip(&c.trees)
        .map(|((s, t), cst)| (s, t, cst))
}
