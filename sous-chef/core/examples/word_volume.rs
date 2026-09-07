//! How many casing rows a corpus fires at each candidate word staircase, so
//! `word_bands` and `word_support_floor` are set from the fleet and not from
//! the glyph defaults.
//!
//! ```text
//! cargo run -p sous-core --release --example word_volume
//!   corpus      floor  glyph  half  fifth  tenth
//!   WA-en-ulb       5    430   214     96     44
//!   …
//!   fleet, floor 20 / tenth      p50 14   p90 61   p95 92   max 640
//! ```
//!
//! Reads `corpora/*.txt` (vref: `BOOK C:V<TAB>text`) the way
//! `examples/pattern_volume.rs` does, but keeps each line's span so a verse
//! start is forced here exactly as it is under a real projected book, then
//! sweeps the same figures over the 1,504-corpus vref tier. That tier is
//! local-only and not committed; run this on a machine that has it, or not at
//! all — it panics rather than skip silently.
//!
//! It also prints the terminal table each test-tier corpus learned, which is
//! what decides forced from free.
//!
//! Not a test: it prints tables for the ledger rather than asserting on them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sous_core::judge::{BandStep, Staircase, merged_follows};
use sous_core::substrate::{Case, Edge, fold_book as fold_glyphs};
use sous_core::words::{WordAggregate, WordRow, Words, fold_book as fold_words};
use sous_core::{
    BookAggregate, BookKey, Channel, ChapterInput, ChapterKey, ChapterObs, ChapterPass, ChapterRow,
    Findings, JudgingConfig, Substrate, TextRange, Verse, VerseKey,
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

/// The support floors swept, in rows.
const FLOORS: [u32; 3] = [5, 10, 20];

/// The candidate staircases: the glyph ladder, then the same rungs at a half,
/// a fifth, and a tenth of its shares.
const LADDERS: [(&str, u16); 4] = [("glyph", 1), ("half", 2), ("fifth", 5), ("tenth", 10)];

fn ladder(divisor: u16) -> Staircase {
    Staircase::new(Staircase::DEFAULT_STEPS.map(|step| BandStep {
        share_bp: step.share_bp / divisor,
        ..step
    }))
    .expect("the default bounds ascend whatever the shares are")
}

fn config(floor: u32, divisor: u16) -> JudgingConfig {
    JudgingConfig {
        word_support_floor: floor,
        word_bands: ladder(divisor),
        ..JudgingConfig::default()
    }
}

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

fn main() {
    let dir = corpora_dir();

    println!("### test tier (corpora/*.txt): casing rows ###\n");
    print!("{:<12}{:>7}", "corpus", "floor");
    for (name, _) in LADDERS {
        print!("{name:>8}");
    }
    println!();
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
        let corpus = Corpus::of(&raw, &path);
        for floor in FLOORS {
            print!(
                "{:<12}{floor:>7}",
                if floor == FLOORS[0] { name } else { "" }
            );
            for (_, divisor) in LADDERS {
                print!("{:>8}", corpus.casing_rows(&config(floor, divisor)));
            }
            println!();
        }
    }

    println!("\n### terminal tables the test tier learned ###\n");
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        let raw = std::fs::read_to_string(&path).expect("read above");
        let corpus = Corpus::of(&raw, &path);
        println!("{name:<12}forces {}", corpus.forcing());
        println!("{:<12}busiest {}", "", corpus.handoffs());
    }

    let candidates = [dir.join("calibration-corpora"), PathBuf::from(VREF)];
    let fleet = candidates
        .iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| panic!("no fleet corpora at {candidates:?}; this tier is local-only"));
    println!("\n### fleet sweep ({}) ###\n", fleet.display());
    sweep(fleet);
}

// ── One corpus ──────────────────────────────────────────────────────────

/// `BOOK C:V<TAB>text`; `None` for a front-matter row (chapter `?`).
fn parse_ref(line: &str) -> Option<(&str, u32, u32, &str)> {
    let (refpart, text) = line
        .split_once('\t')
        .unwrap_or_else(|| panic!("no tab in vref line: {line:?}"));
    let mut parts = refpart.split_whitespace();
    let book = parts.next()?;
    let cv = parts.next()?;
    let (chapter, verse) = cv.split_once(':')?;
    Some((book, chapter.parse().ok()?, verse.parse().ok()?, text))
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

/// One corpus folded once, so a staircase sweep re-judges instead of
/// re-walking.
struct Corpus {
    glyphs: Vec<BookAggregate>,
    words: Vec<WordAggregate>,
    lengths: Vec<u32>,
}

impl Corpus {
    fn of(raw: &str, path: &Path) -> Self {
        // `(book, chapter) → (text, verse spans in that text)`.
        let mut chapters: BTreeMap<(String, u32), (String, Vec<Verse>)> = BTreeMap::new();
        for line in raw.lines() {
            if line.is_empty() {
                continue;
            }
            let Some((book, chapter, verse, text)) = parse_ref(line) else {
                continue;
            };
            let entry = chapters.entry((book.to_string(), chapter)).or_default();
            if !entry.0.is_empty() {
                entry.0.push(' ');
            }
            let from = entry.0.len() as u32;
            entry.0.push_str(text);
            let span = TextRange::new(from, entry.0.len() as u32).expect("a verse grows forward");
            let Ok(key) = VerseKey::new(chapter as u16, verse as u16, verse as u16) else {
                continue;
            };
            entry.1.push(Verse::new(key, span));
        }
        assert!(!chapters.is_empty(), "{} has no chapters", path.display());

        let mut held = Self {
            glyphs: Vec::new(),
            words: Vec::new(),
            lengths: Vec::new(),
        };
        let mut book: Vec<(u32, ChapterRow, WordRow)> = Vec::new();
        let mut current = String::new();
        let mut at = 0u32;
        for ((code, chapter), (text, verses)) in &chapters {
            if *code != current {
                held.flush(&mut book, &mut at);
                current = code.clone();
            }
            let input = ChapterInput {
                text,
                verses,
                key: ChapterKey::new(book_key_of(code), *chapter as u16),
            };
            book.push((at, Substrate.map(input), Words.map(input)));
            at += u32::try_from(text.len()).expect("a chapter fits u32");
        }
        held.flush(&mut book, &mut at);
        held
    }

    fn flush(&mut self, book: &mut Vec<(u32, ChapterRow, WordRow)>, at: &mut u32) {
        if book.is_empty() {
            return;
        }
        let glyphs: Vec<ChapterObs<&ChapterRow>> = book
            .iter()
            .map(|(start, obs, _)| ChapterObs { start: *start, obs })
            .collect();
        let words: Vec<ChapterObs<&WordRow>> = book
            .iter()
            .map(|(start, _, obs)| ChapterObs { start: *start, obs })
            .collect();
        self.glyphs.push(fold_glyphs(&glyphs, &mut Edge::default()));
        self.words.push(fold_words(&words));
        self.lengths.push(*at);
        book.clear();
        *at = 0;
    }

    /// Judges both passes into one sink, the way `Brigade` does, and returns
    /// it: the substrate publishes the terminal table the word channel reads.
    fn judged(&self, config: &JudgingConfig) -> Findings {
        let glyphs: Vec<&BookAggregate> = self.glyphs.iter().collect();
        let words: Vec<&WordAggregate> = self.words.iter().collect();
        let mut findings = Findings::new(self.lengths.clone());
        Substrate.judge(&glyphs, config, &mut findings);
        Words.judge(&words, config, &mut findings);
        findings
    }

    fn casing_rows(&self, config: &JudgingConfig) -> usize {
        self.judged(config)
            .patterns()
            .iter()
            .filter(|row| row.channel == Channel::Casing)
            .count()
    }

    /// The busiest handoffs, whether they force or not: `glyph upper/cased
    /// share`, which is the number the table thresholds.
    fn handoffs(&self) -> String {
        let glyphs: Vec<&BookAggregate> = self.glyphs.iter().collect();
        let mut rows = merged_follows(&glyphs);
        rows.sort_by_key(|(_, counts)| {
            std::cmp::Reverse(counts.get(Case::Upper) + counts.get(Case::Lower))
        });
        rows.iter()
            .take(6)
            .filter_map(|(key, counts)| {
                let upper = u64::from(counts.get(Case::Upper));
                let cased = upper + u64::from(counts.get(Case::Lower));
                let scalar = key.scalar()?;
                let share = (upper * 10_000).checked_div(cased).unwrap_or(0);
                Some(format!("{scalar:?} {upper}/{cased} {share}bp"))
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    /// The glyphs this corpus puts a capital after, as the table learned them.
    fn forcing(&self) -> String {
        let findings = self.judged(&JudgingConfig::default());
        let table = findings.terminals().expect("the substrate published one");
        let names: Vec<String> = table
            .forcing()
            .iter()
            .filter_map(|key| key.scalar())
            .map(|scalar| format!("{scalar:?}"))
            .collect();
        if names.is_empty() {
            "(nothing forces)".to_string()
        } else {
            names.join(" ")
        }
    }
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

fn sweep(dir: &Path) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} holds no .txt corpora", dir.display());

    let candidates: Vec<(String, JudgingConfig)> = FLOORS
        .iter()
        .flat_map(|floor| {
            LADDERS.iter().map(move |(name, divisor)| {
                (
                    format!("floor {floor:>2} / {name}"),
                    config(*floor, *divisor),
                )
            })
        })
        .collect();
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); candidates.len()];
    let mut cased = 0usize;
    let mut skipped = 0usize;

    for path in &files {
        let Ok(raw) = std::fs::read_to_string(path) else {
            skipped += 1;
            continue;
        };
        // ~1,500 unaudited exports, so a file that does not group into
        // chapters is skipped rather than fatal; this is a sweep, not a gate.
        let Ok(corpus) = std::panic::catch_unwind(|| Corpus::of(&raw, path)) else {
            skipped += 1;
            continue;
        };
        if corpus.words.iter().any(WordAggregate::cased) {
            cased += 1;
        }
        for (lane, (_, config)) in rows.iter_mut().zip(&candidates) {
            lane.push(corpus.casing_rows(config));
        }
    }

    println!(
        "{} corpora attempted, {} skipped (unreadable or unparseable), {cased} hold a cased letter\n",
        files.len(),
        skipped
    );
    println!(
        "{:<22}{:>8}{:>8}{:>8}{:>8}{:>10}",
        "candidate", "p50", "p90", "p95", "max", "silent"
    );
    for ((name, _), values) in candidates.iter().zip(rows.iter_mut()) {
        let silent = values.iter().filter(|count| **count == 0).count();
        let (p50, p90, p95, max) = spread(values);
        println!("{name:<22}{p50:>8}{p90:>8}{p95:>8}{max:>8}{silent:>10}");
    }
    println!("\ntarget: p50 10-20 rows per corpus, the glyph channels' own volume");
}
