// AGENT: USE THIS FILE TO TEST AND BENCHMARK THE LEXER
//
// Usage:
//   cargo run --release --bin playground                        // serial, default corpus
//   cargo run --release --bin playground -- <path>              // file or dir of *.usfm
//   cargo run --release --bin playground -- --iters 100         // repeat for stable timing / profiling
//   cargo run --release --bin playground -- --toc               // lex + toc (the whole pipeline)
//   cargo run --release --bin playground -- --toc-only          // pre-lexed; times the SECOND PASS alone
//   cargo run --release --bin playground -- --toc-trace <file>  // untimed: a readable Toc listing on stdout
//   cargo run --release --bin playground -- --mask             // lex + cst + mask, BOTH recipes
//   cargo run --release --bin playground -- --mask-only        // pre-lexed+built; times the mask walk alone
//   cargo run --release --bin playground -- --mask-trace <file>            // untimed: the masked text of a book on stdout
//   cargo run --release --bin playground -- --mask-trace <file> --mask-chapter 3  // …one chapter of it
//   cargo run --release --bin playground -- --mask-trace <file> --mask-recipe structure  // …one recipe only
//   cargo run --release --bin playground -- --vref <file>                   // untimed: one line per verse ("GEN 1:1\ttext") on stdout
//   cargo run --release --bin playground -- --vref <file> --vref-chapter 1  // …one chapter of it
//   cargo run --release --bin playground -- --vref-only        // pre-lexed+built; times toc + mask + render alone
//   cargo run --release --bin playground -- --cst              // lex + cst::build (the whole pipeline)
//   cargo run --release --bin playground -- --cst-only         // pre-lexed; times cst::build alone
//   cargo run --release --bin playground -- --cst-stats        // untimed: CloseReason distribution over the corpus
//   cargo run --release --bin playground -- --lint             // lex + cst::build + lint (the whole pipeline)
//   cargo run --release --bin playground -- --lint-stats       // untimed: per-code finding counts (and fix counts)
//   cargo run --release --bin playground -- --usj-only         // pre-lexed+built; times the USJ serialization alone
//   cargo run --release --bin playground -- --usx-only         // pre-lexed+built; times the USX serialization alone
//   cargo run --release --bin playground -- --html-only        // pre-lexed+built; times the HTML serialization alone
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
// Serial is the honest per-core measurement; --par answers "what does the whole
// corpus cost wall-clock" (embarrassingly parallel over books, so it mostly
// measures core count). The src/experiments/ variants are verified against the
// real lexer on the loaded corpus before any timing starts.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use usfm_onion_2::mask::{Filter, Mask};

const DEFAULT_CORPUS: &str = "example-corpora/en_ulb";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Serial,
    /// Lex, then index the stream — what a real caller that wants a toc pays.
    Toc,
    TocOnly,
    Par,
    Scalar,
    Staged,
    Chunked,
    ChapterPar,
    // The stop-cost ladder (experiments::sweeps): scan ceiling → full stop set →
    // the text arm's cursor-restart pattern. Full lex minus SweepCursor = the
    // work inside the stops.
    SweepNl,
    SweepStops,
    SweepCursor,
    Cst,
    CstOnly,
    /// Lex + build + mask, both recipes — what a caller that wants a masked
    /// view pays end to end.
    Mask,
    MaskOnly,
    /// Pre-lexed+built: the vref render alone — toc + verse_text mask + the
    /// string assembly a keys/lines pair costs.
    VrefOnly,
    Lint,
    /// The `*Only` family is pre-lexed AND pre-built: the lex and the build are
    /// off the clock, so the timing is the named pass alone.
    LintOnly,
    UsjOnly,
    UsxOnly,
    HtmlOnly,
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
    let mut toc_trace = false;
    let mut mask_trace: Option<PathBuf> = None;
    let mut mask_chapter: Option<u16> = None;
    let mut mask_recipe: Option<String> = None;
    let mut vref_trace: Option<PathBuf> = None;
    let mut vref_chapter: Option<u16> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--par" => mode = Mode::Par,
            "--toc" => mode = Mode::Toc,
            "--toc-only" => mode = Mode::TocOnly,
            "--toc-trace" => toc_trace = true,
            "--mask" => mode = Mode::Mask,
            "--mask-only" => mode = Mode::MaskOnly,
            "--mask-trace" => mask_trace = args.next().map(PathBuf::from),
            "--mask-chapter" => {
                mask_chapter = Some(
                    args.next()
                        .and_then(|n| n.parse().ok())
                        .expect("--mask-chapter takes a chapter number"),
                );
            }
            "--mask-recipe" => mask_recipe = args.next(),
            "--vref" => vref_trace = args.next().map(PathBuf::from),
            "--vref-chapter" => {
                vref_chapter = Some(
                    args.next()
                        .and_then(|n| n.parse().ok())
                        .expect("--vref-chapter takes a chapter number"),
                );
            }
            "--vref-only" => mode = Mode::VrefOnly,
            "--cst" => mode = Mode::Cst,
            "--cst-only" => mode = Mode::CstOnly,
            "--cst-stats" => cst_stats = true,
            "--lint" => mode = Mode::Lint,
            "--lint-only" => mode = Mode::LintOnly,
            "--usj-only" => mode = Mode::UsjOnly,
            "--usx-only" => mode = Mode::UsxOnly,
            "--html-only" => mode = Mode::HtmlOnly,
            "--fused" => mode = Mode::Fused,
            "--fused-noop" => mode = Mode::FusedNoop,
            "--fused-cst" => mode = Mode::FusedCst,
            "--lint-stats" => lint_stats = true,
            // Implies --lint-stats: the same sweep, plus before/after windows
            // for the first few fixes of ONE code.
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
    // Before the corpus load: --utf16 names its own three files and would
    // otherwise pay for reading all 66 en_ulb books first.
    if mode == Mode::Utf16 {
        report_utf16();
        return;
    }
    // Same reason: --vref and --mask-trace name their own one file.
    if let Some(path) = &vref_trace {
        print!("{}", vref_listing(&read_source(path), vref_chapter));
        return;
    }
    // Same reason: --mask-trace names its own one file.
    if let Some(path) = &mask_trace {
        print!(
            "{}",
            mask_listing(&read_source(path), mask_chapter, mask_recipe.as_deref())
        );
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
        Mode::Toc => "toc",
        Mode::TocOnly => "toc-only",
        Mode::Cst => "cst",
        Mode::CstOnly => "cst-only",
        Mode::Mask => "mask",
        Mode::MaskOnly => "mask-only",
        Mode::VrefOnly => "vref-only",
        Mode::Lint => "lint",
        Mode::LintOnly => "lint-only",
        Mode::UsjOnly => "usj-only",
        Mode::UsxOnly => "usx-only",
        Mode::HtmlOnly => "html-only",
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

    if cst_stats {
        report_cst_stats(&sources);
        return;
    }
    if lint_stats {
        report_lint_stats(&sources, &path, fix_preview.as_deref());
        return;
    }
    if toc_trace {
        for source in &sources {
            print!("{}", toc_listing(source));
        }
        return;
    }

    // Lexed OUTSIDE the clock, so an `--*-only` mode prices its own pass and
    // nothing else.
    let prelexed: Vec<Vec<usfm_onion_2::Token>> = if matches!(
        mode,
        Mode::TocOnly
            | Mode::CstOnly
            | Mode::MaskOnly
            | Mode::VrefOnly
            | Mode::LintOnly
            | Mode::UsjOnly
            | Mode::UsxOnly
            | Mode::HtmlOnly
    ) {
        sources.iter().map(|s| usfm_onion_2::lex(s)).collect()
    } else {
        Vec::new()
    };
    // The lint modes report ns/token, so they need the token count regardless
    // of whether the lex itself is on the clock.
    let tokens_total: u64 = if matches!(
        mode,
        Mode::Toc
            | Mode::TocOnly
            | Mode::Mask
            | Mode::MaskOnly
            | Mode::VrefOnly
            | Mode::Lint
            | Mode::LintOnly
            | Mode::Fused
            | Mode::FusedNoop
            | Mode::FusedCst
            | Mode::UsjOnly
            | Mode::UsxOnly
            | Mode::HtmlOnly
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
    let prebuilt: Vec<usfm_onion_2::cst::Cst> = if matches!(
        mode,
        Mode::LintOnly
            | Mode::MaskOnly
            | Mode::VrefOnly
            | Mode::UsjOnly
            | Mode::UsxOnly
            | Mode::HtmlOnly
    ) {
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

/// Checks an experiment variant against the real lexer once per run, outside the
/// timing loop; a mismatch aborts loudly.
fn verify_variant(sources: &[String], mode: Mode) {
    let run: fn(&str) -> Vec<usfm_onion_2::Token> = match mode {
        Mode::Serial | Mode::Par => return, // the reference itself
        // Sweeps produce counts, not token streams — nothing to verify.
        Mode::SweepNl | Mode::SweepStops | Mode::SweepCursor => return,
        // These all run the real lexer plus pure passes over its output.
        Mode::Toc | Mode::TocOnly => return,
        Mode::Cst | Mode::CstOnly => return,
        Mode::Mask | Mode::MaskOnly | Mode::VrefOnly => return,
        Mode::Lint | Mode::LintOnly => return,
        Mode::UsjOnly | Mode::UsxOnly | Mode::HtmlOnly => return,
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
        // The variants are FROZEN pre-4.2 lexers, so their token BOUNDARIES may
        // legitimately differ from `crate::lex`. What a variant must still
        // satisfy is the partition invariant itself.
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
        Mode::UsjOnly => {
            #[cfg(feature = "usj")]
            for ((source, tokens), cst) in sources.iter().zip(prelexed).zip(prebuilt) {
                std::hint::black_box(usfm_onion_2::usj::usj(source.as_bytes(), tokens, cst));
            }
            #[cfg(not(feature = "usj"))]
            panic!(
                "--usj-only needs the feature: cargo run --release --bin playground -- --usj-only"
            );
        }
        Mode::UsxOnly => {
            #[cfg(feature = "usx")]
            for ((source, tokens), cst) in sources.iter().zip(prelexed).zip(prebuilt) {
                std::hint::black_box(usfm_onion_2::usx::usx(source.as_bytes(), tokens, cst));
            }
            #[cfg(not(feature = "usx"))]
            panic!(
                "--usx-only needs the feature: cargo run --release --bin playground -- --usx-only"
            );
        }
        Mode::HtmlOnly => {
            #[cfg(feature = "html")]
            for ((source, tokens), cst) in sources.iter().zip(prelexed).zip(prebuilt) {
                std::hint::black_box(usfm_onion_2::html::html(source.as_bytes(), tokens, cst));
            }
            #[cfg(not(feature = "html"))]
            panic!(
                "--html-only needs the feature: cargo run --release --bin playground -- --html-only"
            );
        }
        Mode::Mask => {
            for source in sources {
                let tokens = usfm_onion_2::lex(source);
                let cst = usfm_onion_2::cst::build(&tokens);
                for filter in [Filter::verse_text(), Filter::structure()] {
                    std::hint::black_box(usfm_onion_2::mask(
                        source.as_bytes(),
                        &tokens,
                        &cst,
                        &filter,
                    ));
                }
            }
        }
        Mode::MaskOnly => {
            for ((source, tokens), cst) in sources.iter().zip(prelexed).zip(prebuilt) {
                for filter in [Filter::verse_text(), Filter::structure()] {
                    std::hint::black_box(usfm_onion_2::mask(
                        source.as_bytes(),
                        tokens,
                        cst,
                        &filter,
                    ));
                }
            }
        }
        Mode::VrefOnly => {
            for ((source, tokens), cst) in sources.iter().zip(prelexed).zip(prebuilt) {
                let bytes = source.as_bytes();
                let toc = usfm_onion_2::toc(bytes, tokens);
                let m = usfm_onion_2::mask(bytes, tokens, cst, &Filter::verse_text());
                std::hint::black_box(usfm_onion_2::vref::keys(&toc, &m, bytes));
                std::hint::black_box(usfm_onion_2::vref::lines(&toc, &m, bytes, true));
            }
        }
        Mode::Toc => {
            for source in sources {
                let tokens = usfm_onion_2::lex(source);
                std::hint::black_box(usfm_onion_2::toc(source.as_bytes(), &tokens));
            }
        }
        Mode::TocOnly => {
            for (source, tokens) in sources.iter().zip(prelexed) {
                std::hint::black_box(usfm_onion_2::toc(source.as_bytes(), tokens));
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
                // Books stay serial; the parallelism being priced is INSIDE each
                // book, over its chapters.
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

/// The byte↔UTF-16 boundary index, priced on three deliberately different files:
/// an ASCII-dominant prose book, a dense-Devanagari book (the case that breaks
/// per-drift-change anchors), and the heaviest aligned book. Strategy A = one
/// anchor per non-ASCII char; strategy B = one anchor per
/// [`usfm_onion_2::utf16::STRIDE`] bytes + a SWAR remainder count — the shape
/// that won, now the production `Utf16Index`.
fn report_utf16() {
    use usfm_onion_2::experiments::utf16::{Anchors, reference_pairs, utf16_len_scalar};
    use usfm_onion_2::utf16::{STRIDE, Utf16Index, utf16_len};

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

    /// xorshift64*, used only to pre-generate offsets — the RNG must never be on
    /// the clock.
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
        // to, and what makes hindi-IRV1 the interesting case.
        let non_ascii_bytes = src.bytes().filter(|b| !b.is_ascii()).count();
        let pct_non_ascii = 100.0 * non_ascii_bytes as f64 / src.len().max(1) as f64;

        let a = Anchors::build(&src);
        let b = Utf16Index::new(src.as_bytes());
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

        let build_a = min_of(|| Anchors::build(&src));
        let build_b = min_of(|| Utf16Index::new(src.as_bytes()));
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
                acc += b.to_utf16(byte) as u64;
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
                acc += b.to_byte(utf16) as u64;
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
        let swar = min_of(|| utf16_len(bytes));
        assert_eq!(utf16_len_scalar(bytes), utf16_len(bytes));
        println!(
            "utf16_len over the dense file: scalar {:>6.2} GiB/s   SWAR {:>6.2} GiB/s   ({:.1}×)",
            gib(scalar),
            gib(swar),
            scalar / swar
        );
    }
}

/// The Toc as a human reads it: the chapter table, then the first few verse
/// anchors of each chapter with the sid `locate` gives at that byte.
fn toc_listing(source: &str) -> String {
    const ANCHORS_PER_CHAPTER: usize = 6;

    let tokens = usfm_onion_2::lex(source);
    let toc = usfm_onion_2::toc(source.as_bytes(), &tokens);
    let mut out = String::new();
    out.push_str(&format!(
        "{} — {} chapter rows, {} verse anchors, {} bytes\n\n",
        toc.locate(0),
        toc.chapters.len(),
        toc.verses.len(),
        source.len(),
    ));
    out.push_str("  ch            bytes  verses  first anchors\n");
    for row in &toc.chapters {
        let anchors: Vec<&usfm_onion_2::VerseAnchor> = toc
            .verses
            .iter()
            .filter(|v| v.at >= row.start && v.at < row.end)
            .collect();
        let mut listed: Vec<String> = anchors
            .iter()
            .take(ANCHORS_PER_CHAPTER)
            .map(|v| format!("{}@{}", toc.locate(v.at), v.at))
            .collect();
        if anchors.len() > ANCHORS_PER_CHAPTER {
            listed.push(format!("… {} more", anchors.len() - ANCHORS_PER_CHAPTER));
        }
        out.push_str(&format!(
            "{:>4}  {:>7}..{:<7}  {:>5}  {}\n",
            row.number,
            row.start,
            row.end,
            anchors.len(),
            listed.join("  "),
        ));
    }
    out
}

/// One book's masked text, both recipes, optionally narrowed to one chapter
/// through the Toc's own `chapter_span` — the dumps a human reads to decide
/// whether a recipe is right.
fn mask_listing(source: &str, chapter: Option<u16>, only: Option<&str>) -> String {
    let bytes = source.as_bytes();
    let tokens = usfm_onion_2::lex(source);
    let cst = usfm_onion_2::cst::build(&tokens);
    let toc = usfm_onion_2::toc(bytes, &tokens);
    let window = match chapter {
        Some(n) => toc
            .chapter_span(n)
            .unwrap_or_else(|| panic!("no chapter {n} in this book")),
        None => 0..bytes.len() as u32,
    };

    let mut out = String::new();
    for (recipe, filter) in [
        ("verse_text", Filter::verse_text()),
        ("structure", Filter::structure()),
    ] {
        if only.is_some_and(|name| name != recipe) {
            continue;
        }
        let m = usfm_onion_2::mask(bytes, &tokens, &cst, &filter);
        let text = window_text(bytes, &m, &window);
        out.push_str(&format!(
            "=== {} {} — {recipe}: {} of {} window bytes kept, {} ranges whole-book\n\n",
            toc.locate(window.start),
            chapter.map_or_else(|| "whole book".to_string(), |n| format!("chapter {n}")),
            text.len(),
            window.end - window.start,
            m.ranges.len(),
        ));
        out.push_str(&text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// One book as vref lines — `sid`, a tab, the verse's text — optionally
/// narrowed to one chapter. The regenerator for debug/vref.*.txt.
fn vref_listing(source: &str, chapter: Option<u16>) -> String {
    let bytes = source.as_bytes();
    let tokens = usfm_onion_2::lex(source);
    let cst = usfm_onion_2::cst::build(&tokens);
    let toc = usfm_onion_2::toc(bytes, &tokens);
    let m = usfm_onion_2::mask(bytes, &tokens, &cst, &Filter::verse_text());

    let mut out = String::new();
    for (sid, text) in usfm_onion_2::verses(&toc, &m, bytes) {
        if chapter.is_some_and(|n| n != sid.chapter) {
            continue;
        }
        out.push_str(&format!("{sid}\t{}\n", text.trim()));
    }
    out
}

/// The masked bytes that fall inside one source window.
fn window_text(source: &[u8], m: &Mask, window: &std::ops::Range<u32>) -> String {
    let mut out = String::new();
    for range in &m.ranges {
        let start = range.start.max(window.start) as usize;
        let end = range.end.min(window.end) as usize;
        if start < end {
            out.push_str(std::str::from_utf8(&source[start..end]).expect("UTF-8 source"));
        }
    }
    out
}

/// Untimed corpus sweep: what does lint actually FIND in the wild? Per-code
/// counts, then a sample of each code's sites (book, code, anchor offset) —
/// enough to eyeball a class before pinning its count in tests/lint_corpus.rs.
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
            // Repaired text beside the original: the only way to see whether a
            // proposal reads sanely in real scripture, not in a test snippet.
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

/// Untimed corpus sweep: how do frames actually CLOSE in the wild? Clean books
/// are nearly all Explicit/Implicit; a Recovery cluster is either real data
/// damage or a walker/table gap — eyeball them.
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
