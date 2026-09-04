//! How many patterns the default staircase fires per corpus, against the v1
//! band sweep's p50 10 / p90 36 / p95 44 rows.
//!
//! ```text
//! cargo run -p sous-core --release --example pattern_volume
//! ```
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text`), groups lines by
//! book+chapter as `examples/inventory_census.rs` does, then maps, folds, and
//! judges each corpus exactly as `analyze_with` would and counts the pattern
//! table. When the 1,504-corpus vref tier is on this machine the same figures
//! are swept over it and reported as percentiles; absent, that section is
//! skipped.
//!
//! Not a test: it prints tables for the ledger rather than asserting on them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sous_core::substrate::{Edge, fold_book};
use sous_core::{
    BookAggregate, BookKey, Channel, ChapterInput, ChapterKey, ChapterObs, ChapterPass, ChapterRow,
    Findings, JudgingConfig, Pattern, PatternKey, ScalarKey, Substrate,
};

const CORPORA: &[&str] = &[
    "WA-en-ulb",
    "amh",
    "francl",
    "grcsr",
    "hin2017",
    "nya",
    "spaRV1909",
    "swhulb",
];

/// Will's laptop-local sibling checkout of the v1 spike's vref corpora.
const VREF: &str = "/Users/willkelly/Documents/Work/Code/scripture-sous-chef/corpora/vref";

/// The channels a volume row splits by, in emission order.
const CHANNELS: [Channel; 4] = [
    Channel::ExactNeighbor,
    Channel::RunShape,
    Channel::Placement,
    Channel::Rarity,
];

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

fn main() {
    let config = JudgingConfig::default();
    let dir = corpora_dir();

    println!("### test tier (corpora/*.txt) ###\n");
    println!(
        "{:<12}{:>8}{:>10}{:>10}{:>10}{:>10}",
        "corpus", "total", "exact", "runshape", "placement", "rarity"
    );
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        print_row(name, &patterns_of(&raw, &path, &config));
    }

    let candidates = [dir.join("calibration-corpora"), PathBuf::from(VREF)];
    match candidates.iter().find(|path| path.is_dir()) {
        Some(fleet) => {
            println!("\n### fleet sweep ({}) ###\n", fleet.display());
            sweep(fleet, &config);
        }
        None => println!(
            "\n(no fleet corpora at {:?} — skipping the sweep)",
            candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
        ),
    }
}

// ── One corpus ──────────────────────────────────────────────────────────

/// `BOOK C:V<TAB>text`; `None` for a front-matter row (chapter `?`).
fn parse_ref(line: &str) -> Option<(&str, u32, &str)> {
    let (refpart, text) = line
        .split_once('\t')
        .unwrap_or_else(|| panic!("no tab in vref line: {line:?}"));
    let mut parts = refpart.split_whitespace();
    let book = parts.next()?;
    let cv = parts.next()?;
    let (chapter, _verse) = cv.split_once(':')?;
    Some((book, chapter.parse().ok()?, text))
}

/// A `map` call needs *a* book key, not the right one: the walk is a pure
/// function of chapter text.
fn book_key_of(code: &str) -> BookKey {
    let mut bytes = [b' '; 3];
    for (slot, byte) in bytes.iter_mut().zip(code.as_bytes()) {
        *slot = byte.to_ascii_uppercase();
    }
    BookKey::new(bytes)
}

/// Map, fold, judge — the three steps `analyze_with` composes, over vref
/// lines rather than a `ProjectedBook`.
fn patterns_of(raw: &str, path: &Path, config: &JudgingConfig) -> Vec<Pattern> {
    let mut chapters: BTreeMap<(String, u32), String> = BTreeMap::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((book, chapter, text)) = parse_ref(line) else {
            continue;
        };
        let entry = chapters.entry((book.to_string(), chapter)).or_default();
        if !entry.is_empty() {
            entry.push(' ');
        }
        entry.push_str(text);
    }
    assert!(!chapters.is_empty(), "{} has no chapters", path.display());

    let mut aggregates: Vec<BookAggregate> = Vec::new();
    let mut lengths: Vec<u32> = Vec::new();
    let mut book: Vec<(u32, ChapterRow)> = Vec::new();
    let mut current = String::new();
    let mut at = 0u32;

    let flush = |book: &mut Vec<(u32, ChapterRow)>,
                 at: &mut u32,
                 aggregates: &mut Vec<BookAggregate>,
                 lengths: &mut Vec<u32>| {
        if book.is_empty() {
            return;
        }
        let rows: Vec<ChapterObs<&ChapterRow>> = book
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();
        aggregates.push(fold_book(&rows, &mut Edge::default()));
        lengths.push(*at);
        book.clear();
        *at = 0;
    };

    for ((code, chapter), text) in &chapters {
        if *code != current {
            flush(&mut book, &mut at, &mut aggregates, &mut lengths);
            current = code.clone();
        }
        let row = Substrate.map(ChapterInput {
            text,
            verses: &[],
            key: ChapterKey::new(book_key_of(code), *chapter as u16),
        });
        book.push((at, row));
        at += u32::try_from(text.len()).expect("a chapter fits u32");
    }
    flush(&mut book, &mut at, &mut aggregates, &mut lengths);

    let views: Vec<&BookAggregate> = aggregates.iter().collect();
    let mut findings = Findings::new(lengths);
    Substrate.judge(&views, config, &mut findings);
    findings.patterns().to_vec()
}

fn by_channel(patterns: &[Pattern]) -> [usize; 4] {
    let mut counts = [0usize; 4];
    for pattern in patterns {
        let at = CHANNELS
            .iter()
            .position(|channel| *channel == pattern.channel)
            .expect("the judge emits no other channel");
        counts[at] += 1;
    }
    counts
}

fn print_row(name: &str, patterns: &[Pattern]) {
    let counts = by_channel(patterns);
    println!(
        "{:<12}{:>8}{:>10}{:>10}{:>10}{:>10}",
        name,
        patterns.len(),
        counts[0],
        counts[1],
        counts[2],
        counts[3]
    );
}

// ── The fleet ───────────────────────────────────────────────────────────

fn percentile(sorted: &[usize], p: f64) -> usize {
    if sorted.is_empty() {
        return 0;
    }
    let at = ((p * (sorted.len() as f64 - 1.0)).round() as usize).min(sorted.len() - 1);
    sorted[at]
}

fn spread(values: &mut [usize]) -> (usize, usize, usize, usize) {
    values.sort_unstable();
    (
        percentile(values, 0.5),
        percentile(values, 0.9),
        percentile(values, 0.95),
        values.last().copied().unwrap_or(0),
    )
}

fn sweep(dir: &Path, config: &JudgingConfig) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();
    if files.is_empty() {
        println!("(the fleet directory has no .txt files — nothing to sweep)");
        return;
    }

    let mut totals: Vec<usize> = Vec::new();
    let mut per_channel: [Vec<usize>; 4] = Default::default();
    let mut largest: Vec<(usize, String, Vec<Pattern>)> = Vec::new();
    let mut skipped = 0usize;

    for path in &files {
        let Ok(raw) = std::fs::read_to_string(path) else {
            skipped += 1;
            continue;
        };
        // ~1,500 unaudited exports, so a file that does not group into
        // chapters is skipped rather than fatal; this is a sweep, not a gate.
        let Ok(patterns) = std::panic::catch_unwind(|| patterns_of(&raw, path, config)) else {
            skipped += 1;
            continue;
        };
        let counts = by_channel(&patterns);
        totals.push(patterns.len());
        for (lane, count) in per_channel.iter_mut().zip(counts) {
            lane.push(count);
        }
        let name = path
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        largest.push((patterns.len(), name, patterns));
    }

    println!(
        "{} corpora attempted, {} skipped (unreadable or unparseable)\n",
        files.len(),
        skipped
    );
    let (p50, p90, p95, max) = spread(&mut totals);
    println!(
        "{:<12}{:>8}{:>8}{:>8}{:>8}",
        "lane", "p50", "p90", "p95", "max"
    );
    println!("{:<12}{p50:>8}{p90:>8}{p95:>8}{max:>8}", "total");
    for (channel, values) in CHANNELS.iter().zip(per_channel.iter_mut()) {
        let (p50, p90, p95, max) = spread(values);
        println!(
            "{:<12}{p50:>8}{p90:>8}{p95:>8}{max:>8}",
            channel.name().to_lowercase()
        );
    }
    println!("\ncomparator (v1 band sweep): p50 10 / p90 36 / p95 44 rows per corpus");

    largest.sort_by_key(|row| std::cmp::Reverse(row.0));
    println!("\nten largest corpora by pattern count, with their three widest rows:");
    for (count, name, patterns) in largest.iter().take(10) {
        println!("  {name:<24} {count} patterns");
        let mut widest = patterns.clone();
        widest.sort_by_key(|row| std::cmp::Reverse(row.numerator));
        for pattern in widest.iter().take(3) {
            println!("      {}", describe(pattern));
        }
    }
}

/// One pattern as the CLI prints it, minus the table position.
fn describe(pattern: &Pattern) -> String {
    let evidence = match pattern.key {
        PatternKey::Rarity => "rarity".to_string(),
        PatternKey::Placement { side, class } => {
            format!("placement {}={}", side.name(), class.name())
        }
        PatternKey::RunShape { pure, bucket } => format!(
            "run-shape {} len {bucket}",
            if pure { "pure" } else { "mixed" }
        ),
        PatternKey::ExactNeighbor(neighbor) => format!("exact-neighbor {}", glyph(neighbor)),
    };
    format!(
        "{} {evidence} {}/{} {:.2}%",
        glyph(pattern.glyph),
        pattern.numerator,
        pattern.denominator,
        f64::from(pattern.share_bp) / 100.0
    )
}

fn glyph(key: ScalarKey) -> String {
    match key.scalar() {
        Some(scalar) => format!("U+{:04X} {scalar:?}", scalar as u32),
        None => "digits".to_string(),
    }
}
