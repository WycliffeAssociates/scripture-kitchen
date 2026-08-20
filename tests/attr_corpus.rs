//! What the attribute interpreter actually reads in 226 real books — the
//! pinned numbers.
//!
//! Same contract as tests/lint_corpus.rs: every nonzero malformed count is
//! EXPLAINED at the byte, not tolerated. A `Malformed` on clean scripture is
//! either real data or a bug in src/attributes.rs, and the comments say which.
//!
//! The corpus is gitignored, so this test skips loudly when it is not mounted.

use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;
use usfm_onion_2::attributes::{AttrEvent, MalformedAttr, attrs};
use usfm_onion_2::{TokenKind, lex};

fn collect_usfm_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_usfm_paths(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            paths.push(path);
        }
    }
}

/// The four malformed shapes, in [`MalformedAttr`] order.
const SHAPES: usize = 4;

fn shape_slot(why: MalformedAttr) -> usize {
    match why {
        MalformedAttr::UnterminatedQuote => 0,
        MalformedAttr::EmptyName => 1,
        MalformedAttr::MissingValue => 2,
        MalformedAttr::BareJunk => 3,
    }
}

#[derive(Default, Clone, Copy)]
struct Tally {
    lists: u64,
    /// `Attr` events with a name — the pair form.
    named: u64,
    /// `Attr` events without one — the bare default-value form.
    bare: u64,
    /// Lists that yielded no event at all (`\w|\w*`).
    empty: u64,
    malformed: [u64; SHAPES],
}

impl Tally {
    fn fold(mut self, other: Self) -> Self {
        self.lists += other.lists;
        self.named += other.named;
        self.bare += other.bare;
        self.empty += other.empty;
        for (slot, count) in other.malformed.iter().enumerate() {
            self.malformed[slot] += count;
        }
        self
    }

    fn malformed_total(&self) -> u64 {
        self.malformed.iter().sum()
    }
}

/// One row per book: `(path, tally, the first malformed site as text)`.
type BookReport = (PathBuf, Tally, Option<String>);

fn sweep() -> Option<(Vec<BookReport>, f64)> {
    let mut paths = Vec::new();
    collect_usfm_paths(Path::new("example-corpora"), &mut paths);
    if paths.is_empty() {
        eprintln!("attr corpus SKIPPED: no *.usfm under example-corpora/");
        return None;
    }
    paths.sort();

    let started = Instant::now();
    let books: Vec<BookReport> = paths
        .par_iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            let bytes = source.as_bytes();
            let tokens = lex(&source);
            let mut tally = Tally::default();
            let mut first_site = None;

            for list in tokens
                .iter()
                .filter(|token| token.kind() == TokenKind::AttrList)
            {
                tally.lists += 1;
                let mut events = 0u64;
                for event in attrs(bytes, list) {
                    events += 1;
                    match event {
                        AttrEvent::Attr(attr) if attr.name.is_empty() => tally.bare += 1,
                        AttrEvent::Attr(_) => tally.named += 1,
                        AttrEvent::Malformed { at, why } => {
                            tally.malformed[shape_slot(why)] += 1;
                            if first_site.is_none() {
                                // The blamed byte, in context, so an
                                // unexplained count can be read straight off
                                // a failing run.
                                let from = (at as usize).saturating_sub(40);
                                let to = (at as usize + 40).min(bytes.len());
                                first_site =
                                    Some(format!("{why:?} @{at}: {:?}", &source[from..to]));
                            }
                        }
                    }
                }
                if events == 0 {
                    tally.empty += 1;
                }
            }
            (path.clone(), tally, first_site)
        })
        .collect();
    let elapsed = started.elapsed().as_secs_f64();
    Some((books, elapsed))
}

#[test]
fn the_corpus_yields_exactly_the_known_attribute_reading() {
    let Some((books, elapsed)) = sweep() else {
        return;
    };
    assert_eq!(
        books.len(),
        226,
        "corpus size changed — re-read the numbers"
    );

    let total = books
        .iter()
        .fold(Tally::default(), |acc, (_, tally, _)| acc.fold(*tally));
    let by_corpus = |corpus: &str| -> Tally {
        books
            .iter()
            .filter(|(path, _, _)| path.to_string_lossy().contains(corpus))
            .fold(Tally::default(), |acc, (_, tally, _)| acc.fold(*tally))
    };

    // Lists per corpus. en_ult is word-aligned scripture, so it carries one
    // `\w` list per word plus one `\zaln-s` list per aligned original-language
    // word — 792_414 + 461_352 — which is why the sweep is worth timing.
    assert_eq!(by_corpus("en_ult").lists, 1_253_766);
    // en_ulb, bdf_reg and examples.bsb are unaligned: no attributes anywhere.
    // (So every number below is en_ult's, and the corpus is not yet an oracle
    // for `\fig`, `\rb` or the bare default form — src/attributes.rs's unit
    // tests are.)
    assert_eq!(by_corpus("en_ulb").lists, 0);
    assert_eq!(by_corpus("bdf_reg").lists, 0);
    assert_eq!(by_corpus("examples.bsb").lists, 0);
    assert_eq!(total.lists, 1_253_766);

    // Every list in the corpus is the pair form, and the total reconciles
    // EXACTLY against `grep -oh 'x-[a-z]*=' | sort | uniq -c`:
    //   * 2 * 1_253_766 — x-occurrence + x-occurrences, on every list of both
    //     kinds;
    //   * 3 *   461_352 — x-strong, x-morph, x-content on every `\zaln-s`;
    //   *       461_341 — x-lemma, on all but 11 of them.
    // Not one bare default value, and not one empty list, in 226 books.
    assert_eq!(total.named, 4_352_929);
    assert_eq!(
        total.named,
        2 * 1_253_766 + 3 * 461_352 + 461_341,
        "the attribute total no longer reconciles against the grep"
    );
    assert_eq!(total.bare, 0);
    assert_eq!(total.empty, 0);

    // ZERO malformed in 226 books — the number that says the grammar matches
    // real data rather than the sketch's imagination. If this ever moves,
    // read the site the sweep prints before touching the pin: it is either
    // genuinely deformed authoring (keep it, explain it here) or a bug in
    // src/attributes.rs (fix that instead).
    let sites: Vec<&String> = books
        .iter()
        .filter_map(|(_, _, site)| site.as_ref())
        .collect();
    assert_eq!(total.malformed_total(), 0, "malformed sites: {sites:?}");
    assert_eq!(total.malformed, [0; SHAPES]);

    // Throughput, not a budget: the interpreter is on-demand code, and this
    // only proves it is not accidentally quadratic. Measured 0.09s wall for
    // 1.25M lists / 4.35M attributes across 8 rayon threads with the lex
    // included — ~74ns per list, which is the LEX's cost more than this
    // module's. The bound below is loose on purpose; it catches a hang, not a
    // regression.
    eprintln!(
        "attr sweep: {} lists, {} attributes, {elapsed:.3}s wall ({:.0}ns/list incl. lex)",
        total.lists,
        total.named + total.bare,
        elapsed * 1e9 / total.lists as f64,
    );
    assert!(elapsed < 60.0, "sweep took {elapsed:.1}s");
}
