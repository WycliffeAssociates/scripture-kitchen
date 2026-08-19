// AGENT: USE THIS FILE TO TEST AND BENCHMARK THE LEXER
//
// Usage:
//   cargo run --release --bin playground                        // serial, default corpus
//   cargo run --release --bin playground -- <path>              // file or dir of *.usfm
//   cargo run --release --bin playground -- --iters 100         // repeat for stable timing / profiling
//   cargo run --release --bin playground -- --parse-header       // lex + ParseHeader (the whole pipeline)
//   cargo run --release --bin playground -- --parse-header-only  // pre-lexed; times the SECOND PASS alone
//   cargo run --release --bin playground -- --cst              // lex + cst::build (the whole pipeline)
//   cargo run --release --bin playground -- --cst-only         // pre-lexed; times cst::build alone
//   cargo run --release --bin playground -- --cst-stats        // untimed: CloseReason distribution over the corpus
//   cargo run --release --bin playground -- --scalar            // no-memchr twin (prices SIMD)
//   cargo run --release --bin playground -- --staged            // two-stage structural index (simdjson shape)
//   cargo run --release --bin playground -- --chunked           // chapter-split, lexed serially (prices the split)
//   cargo run --release --features par --bin playground -- --par           // rayon over docs
//   cargo run --release --features par --bin playground -- --chpar         // rayon over CHAPTERS within each doc
//   samply record -- ./target/release/playground --iters 200    // profile (build first)
//
// Serial is the honest per-core measurement; --par answers "what does the
// whole corpus cost wall-clock" (embarrassingly parallel over books, so it
// mostly measures core count). --scalar / --chunked / --chpar run the
// src/experiments/ variants; each is verified token-for-token against the
// real lexer on the loaded corpus before any timing starts.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_CORPUS: &str = "example-corpora/en_ulb";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Serial,
    /// Lex, then index the stream — what a real caller that wants a toc pays.
    ParseHeader,
    ParseHeaderOnly,
    Par,
    Scalar,
    Staged,
    Chunked,
    ChapterPar,
    // The stop-cost ladder (experiments::sweeps): scan ceiling → full stop
    // set → the text arm's cursor-restart pattern. Full lex minus SweepCursor
    // = the work inside the stops.
    SweepNl,
    SweepStops,
    SweepCursor,
    Cst,
    CstOnly,
}

fn main() {
    let mut path: Option<PathBuf> = None;
    let mut mode = Mode::Serial;
    let mut iters: u32 = 1;
    let mut cst_stats = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--par" => mode = Mode::Par,
            "--parse-header" => mode = Mode::ParseHeader,
            "--parse-header-only" => mode = Mode::ParseHeaderOnly,
            "--cst" => mode = Mode::Cst,
            "--cst-only" => mode = Mode::CstOnly,
            "--cst-stats" => cst_stats = true,
            "--scalar" => mode = Mode::Scalar,
            "--staged" => mode = Mode::Staged,
            "--sweep-nl" => mode = Mode::SweepNl,
            "--sweep-stops" => mode = Mode::SweepStops,
            "--sweep-cursor" => mode = Mode::SweepCursor,
            "--chunked" => mode = Mode::Chunked,
            "--chpar" => mode = Mode::ChapterPar,
            "--iters" => {
                iters = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--iters takes a number");
            }
            other => path = Some(PathBuf::from(other)),
        }
    }
    let path = path.unwrap_or_else(|| PathBuf::from(DEFAULT_CORPUS));

    let sources: Vec<String> = if path.is_dir() {
        let mut paths = Vec::new();
        collect_usfm_paths(&path, &mut paths);
        paths.sort();
        paths.iter().map(|p| read_source(p)).collect()
    } else {
        vec![read_source(&path)]
    };

    let bytes: usize = sources.iter().map(|s| s.len()).sum();
    let mode_name = match mode {
        Mode::Serial => "serial",
        Mode::ParseHeader => "parse-header",
        Mode::ParseHeaderOnly => "parse-header-only",
        Mode::Cst => "cst",
        Mode::CstOnly => "cst-only",
        Mode::Par => "par",
        Mode::Scalar => "scalar",
        Mode::Staged => "staged",
        Mode::Chunked => "chunked",
        Mode::ChapterPar => "chpar",
        Mode::SweepNl => "sweep-nl",
        Mode::SweepStops => "sweep-stops",
        Mode::SweepCursor => "sweep-cursor",
    };
    eprintln!(
        "playground: loaded {} source(s), {bytes} bytes total, iters={iters}, mode={mode_name}",
        sources.len()
    );

    verify_variant(&sources, mode);

    // Lexed OUTSIDE the clock: --parse-header-only prices the second pass by itself,
    // so the lex it walks over must not be in the measurement.
    if cst_stats {
        report_cst_stats(&sources);
        return;
    }

    let prelexed: Vec<Vec<usfm_onion_2::Token>> =
        if matches!(mode, Mode::ParseHeaderOnly | Mode::CstOnly) {
            sources.iter().map(|s| usfm_onion_2::lex(s)).collect()
        } else {
            Vec::new()
        };

    let started = Instant::now();
    for _ in 0..iters {
        run_once(&sources, &prelexed, mode);
    }
    let elapsed = started.elapsed();

    let secs = elapsed.as_secs_f64() / iters as f64;
    let docs_per_sec = if secs > 0.0 {
        sources.len() as f64 / secs
    } else {
        0.0
    };
    let mib_per_sec = if secs > 0.0 {
        (bytes as f64 / (1024.0 * 1024.0)) / secs
    } else {
        0.0
    };
    println!(
        "lex docs={} bytes={bytes} mode={mode_name} avg-per-iter={:.3}ms {docs_per_sec:.1} docs/s {mib_per_sec:.2} MiB/s",
        sources.len(),
        secs * 1000.0
    );
}

/// Experiment variants must produce byte-identical token streams. Checked
/// once per run, outside the timing loop; a mismatch aborts loudly.
fn verify_variant(sources: &[String], mode: Mode) {
    let run: fn(&str) -> Vec<usfm_onion_2::Token> = match mode {
        Mode::Serial | Mode::Par => return, // the reference itself
        // Sweeps produce counts, not token streams — nothing to verify.
        Mode::SweepNl | Mode::SweepStops | Mode::SweepCursor => return,
        Mode::ParseHeader | Mode::ParseHeaderOnly => return, // the real lexer plus a pure pass
        Mode::Cst | Mode::CstOnly => return,                 // the real lexer plus a pure pass
        Mode::Scalar => usfm_onion_2::experiments::scalar::lex,
        Mode::Staged => usfm_onion_2::experiments::staged::lex,
        Mode::Chunked => usfm_onion_2::experiments::chapter_par::lex_chunked,
        Mode::ChapterPar => {
            #[cfg(feature = "par")]
            {
                usfm_onion_2::experiments::chapter_par::lex_chunked_par
            }
            #[cfg(not(feature = "par"))]
            panic!(
                "--chpar needs the feature: cargo run --release --features par --bin playground -- --chpar"
            )
        }
    };
    for (i, source) in sources.iter().enumerate() {
        let variant = run(source);
        // The variants are FROZEN pre-4.2 lexers: since the per-class ws fold
        // landed, boundaries legitimately differ (closers and unresolved
        // markers no longer absorb their trailing space). What must still
        // hold for a variant is the partition invariant itself.
        let mut cursor = 0usize;
        for (t, v) in variant.iter().enumerate() {
            assert_eq!(
                v.start as usize, cursor,
                "doc {i} token {t}: variant stream is not a partition"
            );
            cursor += v.len as usize;
        }
        assert_eq!(cursor, source.len(), "doc {i}: variant partition short");
    }
    eprintln!(
        "verify: variant streams are lossless partitions (frozen pre-4.2 — boundaries may differ from crate::lex)"
    );
}

fn run_once(sources: &[String], prelexed: &[Vec<usfm_onion_2::Token>], mode: Mode) {
    match mode {
        Mode::ParseHeader => {
            for source in sources {
                let tokens = usfm_onion_2::lex(source);
                std::hint::black_box(usfm_onion_2::ParseHeader::from_tokens(&tokens, source));
            }
        }
        Mode::ParseHeaderOnly => {
            for (source, tokens) in sources.iter().zip(prelexed) {
                std::hint::black_box(usfm_onion_2::ParseHeader::from_tokens(tokens, source));
            }
        }
        Mode::Cst => {
            for source in sources {
                let tokens = usfm_onion_2::lex(source);
                std::hint::black_box(usfm_onion_2::cst::build(&tokens));
            }
        }
        Mode::CstOnly => {
            for tokens in prelexed {
                std::hint::black_box(usfm_onion_2::cst::build(tokens));
            }
        }
        Mode::Serial => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::lex(source));
            }
        }
        Mode::Scalar => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::scalar::lex(source));
            }
        }
        Mode::Staged => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::staged::lex(source));
            }
        }
        Mode::Chunked => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::chapter_par::lex_chunked(source));
            }
        }
        Mode::SweepNl => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::sweeps::sweep_nl(source));
            }
        }
        Mode::SweepStops => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::sweeps::sweep_stops(source));
            }
        }
        Mode::SweepCursor => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::sweeps::sweep_cursor(source));
            }
        }
        Mode::Par => {
            #[cfg(feature = "par")]
            {
                use rayon::prelude::*;
                sources.par_iter().for_each(|source| {
                    std::hint::black_box(usfm_onion_2::lex(source));
                });
            }
            #[cfg(not(feature = "par"))]
            panic!(
                "--par needs the feature: cargo run --release --features par --bin playground -- --par"
            );
        }
        Mode::ChapterPar => {
            #[cfg(feature = "par")]
            {
                // Books stay serial; parallelism is INSIDE each book, over
                // its chapters — that's the strategy being priced.
                for source in sources {
                    std::hint::black_box(usfm_onion_2::experiments::chapter_par::lex_chunked_par(
                        source,
                    ));
                }
            }
            #[cfg(not(feature = "par"))]
            panic!(
                "--chpar needs the feature: cargo run --release --features par --bin playground -- --chpar"
            );
        }
    }
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

/// Untimed corpus sweep: how do frames actually CLOSE in the wild? Clean
/// books should be nearly all Explicit/Implicit; Recovery clusters are
/// either real data damage or a walker/table gap — eyeball them.
fn report_cst_stats(sources: &[String]) {
    use usfm_onion_2::cst::CloseReason;
    let mut totals = [0u64; 4];
    let mut nodes_total = 0u64;
    let mut tokens_total = 0u64;
    let mut worst: Vec<(u64, usize)> = Vec::new();
    let mut by_marker: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for (i, source) in sources.iter().enumerate() {
        let tokens = usfm_onion_2::lex(source);
        let cst = usfm_onion_2::cst::build(&tokens);
        tokens_total += tokens.len() as u64;
        nodes_total += cst.nodes.len() as u64;
        let mut recoveries = 0u64;
        for node in &cst.nodes[1..] {
            let reason = node.close_reason();
            totals[reason as usize] += 1;
            if reason == CloseReason::Recovery {
                recoveries += 1;
            }
        }
        if recoveries > 0 {
            worst.push((recoveries, i));
        }
        for node in &cst.nodes[1..] {
            if node.close_reason() == CloseReason::Recovery {
                let idx = tokens[node.token as usize].marker_idx;
                let name = usfm_onion_2::tables::generated::name(idx);
                *by_marker.entry(name).or_insert(0u64) += 1;
            }
        }
    }
    println!(
        "cst-stats docs={} tokens={tokens_total} nodes={nodes_total} explicit={} implicit={} recovery={} eof={}",
        sources.len(),
        totals[CloseReason::Explicit as usize],
        totals[CloseReason::Implicit as usize],
        totals[CloseReason::Recovery as usize],
        totals[CloseReason::Eof as usize],
    );
    let mut by_marker: Vec<_> = by_marker.into_iter().collect();
    by_marker.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    for (name, count) in by_marker.iter().take(10) {
        println!("  recovery marker \\{name} x{count}");
    }
    worst.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    for (count, doc) in worst.iter().take(10) {
        println!("  recovery x{count} in doc #{doc}");
    }
}
