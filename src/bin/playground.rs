// AGENT: USE THIS FILE TO TEST AND BENCHMARK THE LEXER
//
// Usage:
//   cargo run --release --bin playground             // default corpus dir
//   cargo run --release --bin playground -- <path>   // file or dir of *.usfm
//   samply record -- cargo run --release --bin playground

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_CORPUS: &str = "example-corpora/en_ulb";

fn main() {
    let path = PathBuf::from(
        std::env::args().nth(1).unwrap_or_else(|| DEFAULT_CORPUS.to_string()),
    );

    let sources: Vec<String> = if path.is_dir() {
        let mut paths = Vec::new();
        collect_usfm_paths(&path, &mut paths);
        paths.sort();
        paths.iter().map(|p| read_source(p)).collect()
    } else {
        vec![read_source(&path)]
    };

    let bytes: usize = sources.iter().map(|s| s.len()).sum();
    eprintln!("playground: loaded {} source(s), {bytes} bytes total", sources.len());

    let started = Instant::now();
    for source in &sources {
        std::hint::black_box(usfm_onion_2::lex(source));
    }
    let elapsed = started.elapsed();

    let secs = elapsed.as_secs_f64();
    let docs_per_sec = if secs > 0.0 { sources.len() as f64 / secs } else { 0.0 };
    let mib_per_sec = if secs > 0.0 {
        (bytes as f64 / (1024.0 * 1024.0)) / secs
    } else {
        0.0
    };
    println!(
        "lex docs={} bytes={bytes} elapsed={elapsed:.3?} {docs_per_sec:.1} docs/s {mib_per_sec:.2} MiB/s",
        sources.len()
    );
}

fn collect_usfm_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read dir entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            collect_usfm_paths(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            paths.push(path);
        }
    }
}

fn read_source(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}
