//! The substrate row's `hygiene` lane against the scan it replaced.
//!
//! The reference below is `hygiene::scan_scalars` as it stood before D1b: a
//! restarting walk that re-reads a neighbour whenever it needs one, with its
//! eight-byte ASCII lane dropped for plain `chars()` — no ASCII scalar is a
//! mark, a format character, a noncharacter, or U+00A0, so the lane only ever
//! skipped. The production machine is streaming — one call per suspect
//! scalar, one pending verdict, one open run — so the two agreeing exactly is
//! what makes the move onto the walk a refactor rather than a rewrite.
//!
//! Chapters come from `corpora/*.txt` (vref: `BOOK C:V<TAB>text`), grouped by
//! book and chapter as `tests/substrate_reference.rs` groups them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sous_core::substrate::Substrate;
use sous_core::unicode::atoms::widen_to_atoms;
use sous_core::unicode::{Class, bits, class_of};
use sous_core::{BookKey, ChapterInput, ChapterKey, ChapterPass, HygieneClass, TextRange};

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

/// One row of either side: class, span, run length.
type Row = (HygieneClass, u32, u32, u32);

// ── The reference ───────────────────────────────────────────────────────

/// The bits that put a scalar on the slow path.
const SUSPECT: u16 = bits::MARK | bits::FORMAT | bits::NONCHARACTER;
const NBSP: char = '\u{a0}';

fn scan_scalars(text: &str, out: &mut Vec<Row>) {
    let mut at = 0;
    while at < text.len() {
        let c = char_at(text, at);
        let class = class_of(c);
        at = if class.bits() & SUSPECT != 0 {
            suspect_run(text, at, class, out)
        } else if c == NBSP && nbsp_is_suspect(text, at) {
            scalar_run(text, at, HygieneClass::NoBreakSpace, out, |text, at| {
                char_at(text, at) == NBSP
            })
        } else {
            at + c.len_utf8()
        };
    }
}

/// Dispatches one MARK / FORMAT / NONCHARACTER scalar, returning where the
/// walk resumes.
fn suspect_run(text: &str, at: usize, class: Class, out: &mut Vec<Row>) -> usize {
    if class.is_noncharacter() {
        return scalar_run(text, at, HygieneClass::Noncharacter, out, |text, at| {
            class_at(text, at).is_noncharacter()
        });
    }
    if class.is_mark() {
        if !mark_is_free(text, at) {
            return at + class_width(text, at);
        }
        return scalar_run(
            text,
            at,
            HygieneClass::FreeCombiningMark,
            out,
            |text, at| class_at(text, at).is_mark(),
        );
    }
    if format_is_placed(text, at, class) {
        return at + class_width(text, at);
    }
    scalar_run(text, at, HygieneClass::MisplacedFormat, out, |text, at| {
        let class = class_at(text, at);
        class.is_format() && !format_is_placed(text, at, class)
    })
}

fn mark_is_free(text: &str, at: usize) -> bool {
    match prev_class(text, at) {
        None => true,
        Some(prev) => {
            prev.is_whitespace() || prev.is_control() || (prev.is_format() && !prev.is_glue())
        }
    }
}

fn format_is_placed(text: &str, at: usize, class: Class) -> bool {
    let joinable = |class: Class| class.is_alphabetic() || class.is_mark();
    if class.is_extender() {
        return prev_class(text, at).is_some_and(joinable)
            && next_class(text, at).is_some_and(joinable);
    }
    if class.is_prepend() {
        return next_class(text, at).is_some_and(|next| joinable(next) || next.is_decimal_digit());
    }
    false
}

fn nbsp_is_suspect(text: &str, at: usize) -> bool {
    match (prev_class(text, at), next_class(text, at)) {
        (None, _) | (_, None) => true,
        (Some(prev), Some(next)) => prev.is_whitespace() || next.is_whitespace(),
    }
}

fn char_at(text: &str, at: usize) -> char {
    text[at..].chars().next().expect("a char boundary")
}

fn class_at(text: &str, at: usize) -> Class {
    class_of(char_at(text, at))
}

fn class_width(text: &str, at: usize) -> usize {
    char_at(text, at).len_utf8()
}

fn prev_class(text: &str, at: usize) -> Option<Class> {
    text[..at].chars().next_back().map(class_of)
}

/// The scalar after the one starting at `at`.
fn next_class(text: &str, at: usize) -> Option<Class> {
    text[at..].chars().nth(1).map(class_of)
}

/// Consumes the maximal run `member` accepts and pushes one finding.
fn scalar_run(
    text: &str,
    at: usize,
    class: HygieneClass,
    out: &mut Vec<Row>,
    member: impl Fn(&str, usize) -> bool,
) -> usize {
    let mut end = at;
    let mut run = 0u32;
    while end < text.len() && member(text, end) {
        end += class_width(text, end);
        run += 1;
    }
    let exact = TextRange::new(at as u32, end as u32).expect("runs advance forward");
    let span = widen_to_atoms(text, exact);
    out.push((class, span.from(), span.to(), run));
    end
}

fn reference(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    scan_scalars(text, &mut out);
    out
}

// ── The lane ────────────────────────────────────────────────────────────

fn observed(text: &str) -> Vec<Row> {
    Substrate
        .map(ChapterInput {
            text,
            verses: &[],
            key: ChapterKey::new(BookKey::new(*b"MRK"), 1),
        })
        .hygiene()
        .iter()
        .map(|finding| {
            (
                finding.class(),
                finding.span().from(),
                finding.span().to(),
                finding.run(),
            )
        })
        .collect()
}

fn assert_agrees(text: &str) {
    assert_eq!(observed(text), reference(text), "sites for {text:?}");
}

// ── The synthetic sweep ─────────────────────────────────────────────────

/// The pieces a hygiene site can be built from: ordinary letters and spaces,
/// then one of each thing the four classes turn on.
const ALPHABET: &[&str] = &[
    "a", "e", "s", "A", " ", " ", "\n", "\u{301}", "\u{94d}", "\u{200d}", "\u{200c}", "\u{feff}",
    "\u{600}", "\u{a0}", "\u{fdd0}", "\u{fffe}", "7", "\u{967}", ".", ",", "\u{915}", "\u{5d0}",
];

/// Five lines of xorshift64: reproducible from its seed, and no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn the_lane_equals_the_reference_over_a_synthetic_sweep() {
    let mut rng = Rng(0x5EED_D1B0_0000_0001);
    for _ in 0..4000 {
        let pieces = 1 + rng.below(40);
        let mut text = String::new();
        for _ in 0..pieces {
            text.push_str(ALPHABET[rng.below(ALPHABET.len())]);
        }
        assert_agrees(&text);
    }
}

/// The shapes the machine's own branches turn on, spelled out so a break
/// names itself before the sweep or the tier oracle runs.
const SAMPLE: &[&str] = &[
    "",
    " ",
    "\u{301}",
    "\u{301}\u{301}",
    "a\u{301}",
    "a \u{301}\u{301}b",
    "\u{a0}",
    "\u{a0}\u{a0}",
    "a\u{a0}b",
    "a \u{a0}\u{a0} b",
    "\u{200d}\u{200d} a",
    "a\u{200d}\u{200d}b",
    "\u{915}\u{94d}\u{200d}\u{937}",
    "\u{feff}\u{feff}\u{feff}",
    "\u{600}\u{600}7",
    "\u{fdd0}\u{fdd1}\u{fdd2}",
    "\u{fdd0}\u{301}\u{a0}\u{feff}",
    "\u{a0}\u{301}",
    "\u{301}\u{a0}",
    "a\u{a0}\u{301}b",
    "\u{feff}\u{a0}",
    "the quick brown fox jumped over a lazy dog and kept going\u{301}",
];

#[test]
fn the_lane_equals_the_reference_over_the_named_shapes() {
    for text in SAMPLE {
        assert_agrees(text);
    }
}

// ── The corpus oracle ───────────────────────────────────────────────────

fn corpora_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpora")
}

/// `None` for a front-matter row (chapter `?`); anything else that does not
/// fit `BOOK C:V<TAB>text` is a hard failure.
fn parse_ref(line: &str) -> Option<(&str, u32, &str)> {
    let (refpart, text) = line
        .split_once('\t')
        .unwrap_or_else(|| panic!("no tab in vref line: {line:?}"));
    let mut parts = refpart.split_whitespace();
    let book = parts
        .next()
        .unwrap_or_else(|| panic!("no book code in vref line: {line:?}"));
    let cv = parts
        .next()
        .unwrap_or_else(|| panic!("no chapter:verse in vref line: {line:?}"));
    let (chapter, _verse) = cv
        .split_once(':')
        .unwrap_or_else(|| panic!("no ':' in chapter:verse: {line:?}"));
    Some((book, chapter.parse().ok()?, text))
}

fn chapters_of(path: &Path) -> Vec<String> {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
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
    chapters.into_values().collect()
}

#[test]
#[ignore = "exhaustive oracle: every chapter of the 8-corpus tier scanned twice; run --include-ignored at pass end"]
fn the_lane_equals_the_reference_over_the_whole_tier() {
    let dir = corpora_dir();
    for name in CORPORA {
        let path = dir.join(format!("{name}.txt"));
        assert!(
            path.is_file(),
            "test-tier corpus {name} must be present at {}",
            path.display()
        );
        let chapters = chapters_of(&path);
        let mut sites = 0;
        for text in &chapters {
            assert_agrees(text);
            sites += observed(text).len();
        }
        println!("{name:<12}{:>6} chapters  {sites:>6} sites", chapters.len());
    }
}
