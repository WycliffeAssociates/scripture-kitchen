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
//   cargo run --release --bin playground -- --lint             // lex + cst::build + lint (the whole pipeline)
//   cargo run --release --bin playground -- --lint-stats       // untimed: per-code finding counts (and fix counts)
//   cargo run --release --bin playground -- --fix-preview unclosed-note  // …plus before/after windows for one code
//   cargo run --release --bin playground -- --fused            // SINGLE PASS: lex+cst+lint off push_token
//   cargo run --release --bin playground -- --fused-noop       // the fused traversal with the sink OFF (prices the hook)
//   cargo run --release --bin playground -- --scalar            // no-memchr twin (prices SIMD)
//   cargo run --release --bin playground -- --staged            // two-stage structural index (simdjson shape)
//   cargo run --release --bin playground -- --chunked           // chapter-split, lexed serially (prices the split)
//   cargo run --release --bin playground -- --utf16             // byte<->UTF-16 index: drift anchors vs fixed stride
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
    Lint,
    /// Pre-lexed AND pre-built; times the lint pass by itself.
    LintOnly,
    /// The single-pass experiment: lex + cst + lint in ONE traversal.
    Fused,
    /// The same traversal with the sink switched off — prices the hook alone.
    FusedNoop,
    /// The middle rung: lex + cst fused, no lint.
    FusedCst,
    /// Not a lexer mode: prices the byte↔UTF-16 boundary index, both shapes.
    /// Picks its own files (it needs a dense-script one) and ignores `<path>`.
    Utf16,
}

fn main() {
    let mut path: Option<PathBuf> = None;
    let mut mode = Mode::Serial;
    let mut iters: u32 = 1;
    let mut cst_stats = false;
    let mut lint_stats = false;
    let mut fix_preview: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--par" => mode = Mode::Par,
            "--parse-header" => mode = Mode::ParseHeader,
            "--parse-header-only" => mode = Mode::ParseHeaderOnly,
            "--cst" => mode = Mode::Cst,
            "--cst-only" => mode = Mode::CstOnly,
            "--cst-stats" => cst_stats = true,
            "--lint" => mode = Mode::Lint,
            "--lint-only" => mode = Mode::LintOnly,
            "--fused" => mode = Mode::Fused,
            "--fused-noop" => mode = Mode::FusedNoop,
            "--fused-cst" => mode = Mode::FusedCst,
            "--lint-stats" => lint_stats = true,
            // Implies --lint-stats: it is the same sweep, printing before/after
            // windows for the first few fixes of ONE code.
            "--fix-preview" => {
                lint_stats = true;
                fix_preview = args.next();
            }
            "--scalar" => mode = Mode::Scalar,
            "--staged" => mode = Mode::Staged,
            "--sweep-nl" => mode = Mode::SweepNl,
            "--sweep-stops" => mode = Mode::SweepStops,
            "--sweep-cursor" => mode = Mode::SweepCursor,
            "--chunked" => mode = Mode::Chunked,
            "--utf16" => mode = Mode::Utf16,
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
    // Handled before the corpus load: --utf16 names its own three files and
    // would otherwise pay for reading all 66 en_ulb books first.
    if mode == Mode::Utf16 {
        report_utf16();
        return;
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
        Mode::Lint => "lint",
        Mode::LintOnly => "lint-only",
        Mode::Fused => "fused",
        Mode::FusedNoop => "fused-noop",
        Mode::FusedCst => "fused-cst",
        Mode::Par => "par",
        Mode::Scalar => "scalar",
        Mode::Staged => "staged",
        Mode::Chunked => "chunked",
        Mode::ChapterPar => "chpar",
        Mode::SweepNl => "sweep-nl",
        Mode::SweepStops => "sweep-stops",
        Mode::SweepCursor => "sweep-cursor",
        Mode::Utf16 => unreachable!("handled above"),
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
    if lint_stats {
        report_lint_stats(&sources, &path, fix_preview.as_deref());
        return;
    }

    let prelexed: Vec<Vec<usfm_onion_2::Token>> =
        if matches!(mode, Mode::ParseHeaderOnly | Mode::CstOnly | Mode::LintOnly) {
            sources.iter().map(|s| usfm_onion_2::lex(s)).collect()
        } else {
            Vec::new()
        };
    // The lint modes report ns/token, so they need the token count regardless
    // of whether the lex itself is on the clock.
    let tokens_total: u64 = if matches!(
        mode,
        Mode::Lint | Mode::LintOnly | Mode::Fused | Mode::FusedNoop | Mode::FusedCst
    ) {
        if prelexed.is_empty() {
            sources
                .iter()
                .map(|s| usfm_onion_2::lex(s).len() as u64)
                .sum()
        } else {
            prelexed.iter().map(|t| t.len() as u64).sum()
        }
    } else {
        0
    };
    let prebuilt: Vec<usfm_onion_2::cst::Cst> = if mode == Mode::LintOnly {
        prelexed
            .iter()
            .map(|t| usfm_onion_2::cst::build(t))
            .collect()
    } else {
        Vec::new()
    };

    let started = Instant::now();
    for _ in 0..iters {
        run_once(&sources, &prelexed, &prebuilt, mode);
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
    if tokens_total > 0 {
        println!(
            "  tokens={tokens_total} {:.2} ns/token",
            secs * 1e9 / tokens_total as f64
        );
    }
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
        Mode::Lint | Mode::LintOnly => return,               // the real lexer plus two pure passes
        // Identity is tests/fused_identity.rs's job — whole reports, not just
        // token streams, so it cannot be a `fn(&str) -> Vec<Token>` here.
        Mode::Fused | Mode::FusedNoop | Mode::FusedCst => return,
        Mode::Utf16 => return, // not a lexer variant
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

fn run_once(
    sources: &[String],
    prelexed: &[Vec<usfm_onion_2::Token>],
    prebuilt: &[usfm_onion_2::cst::Cst],
    mode: Mode,
) {
    match mode {
        Mode::Lint => {
            for source in sources {
                let tokens = usfm_onion_2::lex(source);
                let cst = usfm_onion_2::cst::build(&tokens);
                std::hint::black_box(usfm_onion_2::lint::lint(source.as_bytes(), &tokens, &cst));
            }
        }
        Mode::Fused => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::fused::analyze_fused(source));
            }
        }
        Mode::FusedNoop => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::fused::lex_noop_sink(source));
            }
        }
        Mode::FusedCst => {
            for source in sources {
                std::hint::black_box(usfm_onion_2::experiments::fused::analyze_fused_cst(source));
            }
        }
        Mode::LintOnly => {
            for ((source, tokens), cst) in sources.iter().zip(prelexed).zip(prebuilt) {
                std::hint::black_box(usfm_onion_2::lint::lint(source.as_bytes(), tokens, cst));
            }
        }
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
        Mode::Utf16 => unreachable!("handled in main before the corpus load"),
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

/// The byte↔UTF-16 boundary index, priced on three deliberately different
/// files: an ASCII-dominant prose book, a dense-Devanagari book (the case that
/// breaks per-drift-change anchors), and the heaviest aligned book.
///
/// Strategy A = one anchor per non-ASCII char; strategy B = one anchor per
/// [`usfm_onion_2::experiments::utf16::STRIDE`] bytes + a SWAR remainder count.
/// See `src/experiments/utf16.rs` for both and for the stride-boundary rule.
fn report_utf16() {
    use usfm_onion_2::experiments::utf16::{
        Anchors, STRIDE, Stride, reference_pairs, utf16_len_scalar, utf16_len_swar,
    };

    const QUERIES: usize = 10_000;
    const REPS: u32 = 8; // min-of-8, the convention everywhere else here

    /// Min-of-`REPS` wall time for one call of `f`, in seconds.
    fn min_of<T>(mut f: impl FnMut() -> T) -> f64 {
        let mut best = f64::INFINITY;
        for _ in 0..REPS {
            let at = Instant::now();
            std::hint::black_box(f());
            best = best.min(at.elapsed().as_secs_f64());
        }
        best
    }

    /// xorshift64* — the offsets must be pre-generated so the RNG is never on
    /// the clock. (Randomness is fine in the playground; it is a measuring
    /// tool, not a workflow.)
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    let files = [
        (
            "en_ulb 19-PSA (prose)",
            "example-corpora/en_ulb/19-PSA.usfm",
        ),
        (
            "hindi-IRV1 origin (dense)",
            "testData/samples-from-wild/hindi-IRV1/origin.usfm",
        ),
        (
            "en_ult 01-GEN (aligned)",
            "example-corpora/en_ult/01-GEN.usfm",
        ),
    ];

    println!("utf16 boundary index — stride={STRIDE}B, min-of-{REPS}, {QUERIES} queries/direction");
    println!(
        "{:<26} {:>10} {:>8} {:>12} {:>10} {:>12} {:>10}",
        "file", "bytes", "non-asc", "A index", "A % src", "B index", "B % src"
    );

    let mut dense_source: Option<String> = None;

    for (label, path) in files {
        let path = Path::new(path);
        if !path.exists() {
            eprintln!("  skipping {label}: {} not mounted", path.display());
            continue;
        }
        let src = read_source(path);
        // Share of BYTES that are non-ASCII — the axis strategy A is sensitive
        // to, and the one that makes hindi-IRV1 the interesting case.
        let non_ascii_bytes = src.bytes().filter(|b| !b.is_ascii()).count();
        let pct_non_ascii = 100.0 * non_ascii_bytes as f64 / src.len().max(1) as f64;

        let a = Anchors::build(&src);
        let b = Stride::build(&src);
        assert_eq!(a.len_utf16(), b.len_utf16(), "{label}: strategies disagree");

        let pct = |n: usize| 100.0 * n as f64 / src.len() as f64;
        println!(
            "{label:<26} {:>10} {:>7.0}% {:>12} {:>9.1}% {:>12} {:>9.1}%",
            src.len(),
            pct_non_ascii,
            a.index_bytes(),
            pct(a.index_bytes()),
            b.index_bytes(),
            pct(b.index_bytes()),
        );

        // Build cost.
        let build_a = min_of(|| Anchors::build(&src));
        let build_b = min_of(|| Stride::build(&src));
        println!(
            "    build      A {:>8.3} ms   B {:>8.3} ms   ({:.1}× faster)",
            build_a * 1000.0,
            build_b * 1000.0,
            build_a / build_b
        );

        // Pre-generate boundary-legal offsets: every query must be a real
        // character boundary, so both strategies answer the same question.
        let pairs = reference_pairs(&src);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ src.len() as u64);
        let probes: Vec<(u32, u32)> = (0..QUERIES)
            .map(|_| pairs[(rng.next() % pairs.len() as u64) as usize])
            .collect();

        let fwd_a = min_of(|| {
            let mut acc = 0u64;
            for &(byte, _) in &probes {
                acc += a.byte_to_utf16(byte) as u64;
            }
            acc
        });
        let fwd_b = min_of(|| {
            let mut acc = 0u64;
            for &(byte, _) in &probes {
                acc += b.byte_to_utf16(byte) as u64;
            }
            acc
        });
        let rev_a = min_of(|| {
            let mut acc = 0u64;
            for &(_, utf16) in &probes {
                acc += a.utf16_to_byte(utf16) as u64;
            }
            acc
        });
        let rev_b = min_of(|| {
            let mut acc = 0u64;
            for &(_, utf16) in &probes {
                acc += b.utf16_to_byte(utf16) as u64;
            }
            acc
        });
        let ns = |secs: f64| secs * 1e9 / QUERIES as f64;
        println!(
            "    byte→utf16 A {:>8.1} ns   B {:>8.1} ns",
            ns(fwd_a),
            ns(fwd_b)
        );
        println!(
            "    utf16→byte A {:>8.1} ns   B {:>8.1} ns",
            ns(rev_a),
            ns(rev_b)
        );
        if label.starts_with("hindi") {
            dense_source = Some(src);
        }
    }

    // The remainder count's raw speed, on the dense file — the only place the
    // SWAR path could plausibly matter.
    if let Some(src) = dense_source {
        let bytes = src.as_bytes();
        let gib = |secs: f64| (bytes.len() as f64 / secs) / (1024.0 * 1024.0 * 1024.0);
        let scalar = min_of(|| utf16_len_scalar(bytes));
        let swar = min_of(|| utf16_len_swar(bytes));
        assert_eq!(utf16_len_scalar(bytes), utf16_len_swar(bytes));
        println!(
            "utf16_len over the dense file: scalar {:>6.2} GiB/s   SWAR {:>6.2} GiB/s   ({:.1}×)",
            gib(scalar),
            gib(swar),
            scalar / swar
        );
    }
}

/// Untimed corpus sweep: what does lint actually FIND in the wild? Per-code
/// counts first, then a sample of each code's sites (book, code name, byte
/// offset of the anchor token) — enough to eyeball a class before pinning its
/// count in tests/lint_corpus.rs.
fn report_lint_stats(sources: &[String], root: &Path, preview_of: Option<&str>) {
    use usfm_onion_2::lint::LINT_ROWS;

    const SAMPLES_PER_CODE: usize = 12;
    const PREVIEWS: usize = 6;

    let names: Vec<String> = if root.is_dir() {
        let mut paths = Vec::new();
        collect_usfm_paths(root, &mut paths);
        paths.sort();
        paths
            .iter()
            .map(|p| {
                p.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    } else {
        vec![
            root.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ]
    };

    let mut counts = [0u64; LINT_ROWS.len()];
    let mut fixed = [0u64; LINT_ROWS.len()];
    let mut samples: Vec<Vec<(String, u32)>> = vec![Vec::new(); LINT_ROWS.len()];
    let mut previews: Vec<(String, &'static str, String, String)> = Vec::new();
    let mut books_without_id = 0u64;
    let mut tokens_total = 0u64;

    for (i, source) in sources.iter().enumerate() {
        let tokens = usfm_onion_2::lex(source);
        let cst = usfm_onion_2::cst::build(&tokens);
        let report = usfm_onion_2::lint::lint(source.as_bytes(), &tokens, &cst);
        tokens_total += tokens.len() as u64;

        let book = match report.book {
            Some(idx) => {
                let token = tokens[idx as usize];
                source[token.start as usize..token.end() as usize].to_string()
            }
            None => {
                books_without_id += 1;
                names.get(i).cloned().unwrap_or_else(|| format!("doc#{i}"))
            }
        };
        for (index, obs) in report.observations.iter().enumerate() {
            let slot = obs.code as usize;
            counts[slot] += 1;
            if samples[slot].len() < SAMPLES_PER_CODE {
                samples[slot].push((book.clone(), tokens[obs.anchor as usize].start));
            }
            let Some(fix) = report.fix(index) else {
                continue;
            };
            fixed[slot] += 1;
            // A window of the repaired text beside the original, for the first
            // few fixes of each code — the only way to see whether a proposal
            // reads sanely in real scripture rather than in a test snippet.
            if previews.len() < PREVIEWS
                && preview_of.is_some_and(|name| name == LINT_ROWS[slot].name)
            {
                let edits = report.edits(fix);
                let at = edits[0].from as usize;
                let window = at.saturating_sub(60)..(at + 60).min(source.len());
                let after = String::from_utf8(usfm_onion_2::lint::apply(source.as_bytes(), edits))
                    .expect("fixes are ASCII");
                let shift = window.start..(window.end + 8).min(after.len());
                previews.push((
                    book.clone(),
                    fix.label,
                    source[window].to_string(),
                    after[shift].to_string(),
                ));
            }
        }
    }

    let total: u64 = counts.iter().sum();
    let fixable: u64 = fixed.iter().sum();
    println!(
        "lint-stats docs={} tokens={tokens_total} findings={total} with-fix={fixable} books-without-id={books_without_id}",
        sources.len()
    );
    for (book, label, before, after) in &previews {
        println!("  fix-preview {book} [{label}]");
        println!("    before {before:?}");
        println!("    after  {after:?}");
    }
    for (slot, row) in LINT_ROWS.iter().enumerate() {
        if counts[slot] == 0 {
            continue;
        }
        println!(
            "  {} x{} ({} with a fix)",
            row.name, counts[slot], fixed[slot]
        );
        for (book, offset) in &samples[slot] {
            println!("    {book} {} @{offset}", row.name);
        }
        if counts[slot] > SAMPLES_PER_CODE as u64 {
            println!("    … {} more", counts[slot] - SAMPLES_PER_CODE as u64);
        }
    }
    for row in LINT_ROWS.iter() {
        if counts[row.code as usize] == 0 {
            println!("  {} x0", row.name);
        }
    }
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
