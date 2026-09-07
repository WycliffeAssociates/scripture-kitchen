//! The `--report` page: one self-contained HTML file, the v1 "Punctuation &
//! Symbol Inventory" page fed by v2 counts.
//!
//! ```text
//! sous --report debug/inventory-en_ulb.html testData/exampleCorpora/en_ulb
//!   -> debug/inventory-en_ulb.html
//!      "Character by character" tab: one card per non-letter glyph, plain
//!      English, amber wherever the current `JudgingConfig` fired.
//! ```
//!
//! [`templates/inventory.html`](../../templates/inventory.html) is the v1
//! artifact with its host wrapper stripped and `const CORPORA = [...]`
//! replaced by `const CORPORA = @@CORPORA@@;`, which [`render`] substitutes
//! with one corpus record for the target corpus. The "Tune the judge" and
//! "Capitalization" tabs (and their `const CAP` data and JS judge) are gone;
//! amber now means "a row in `Findings::patterns()` fired", carried as a
//! `flag` field, never recomputed in JS.
//!
//! Key mapping (template key read -> v2 source), everything else copied
//! from the v1 record shape verbatim:
//!
//! | key | source |
//! |---|---|
//! | `name` | the corpus path's file/dir name |
//! | `judging` | [`JudgingConfig::default`], as text |
//! | `glyphs[].g`/`cp`/`uname` | [`ScalarKey::scalar`], `"digits"` for [`ScalarKey::DIGITS`] |
//! | `glyphs[].total`/`books` | merged [`BookAggregate::scalars`] |
//! | `glyphs[].side.{start,end}` | merged [`BookAggregate::pairs`] (letter/space/edge/digit), split by [`Pool`] from [`BookAggregate::runs`] for a `Nonletter` neighbor |
//! | `glyphs[].topo.*.n`/`.books` | merged [`BookAggregate::pairs`], by (prev, next) combination |
//! | `glyphs[].topo.*.flag` | a [`PatternKey::Placement`] row on the matching side/class, or (in-run) any [`PatternKey::RunShape`] row |
//! | `glyphs[].pairs[].{p,n,books}` | merged [`BookAggregate::runs`], the atom immediately after the glyph in a run |
//! | `glyphs[].pairs[].flag` | a [`PatternKey::ExactNeighbor`] row for that partner |
//! | `glyphs[].pairs[].pool` | [`pool_of`] on the partner — a heading only, groups the table, no new numbers |
//! | `glyphs[].runlen.pure` / `.mixed` | merged [`BookAggregate::runs`], runs made entirely of the glyph / runs holding it beside other marks, by length (6 = 6+) |
//! | `glyphs[].runlen.*[].flag` | the [`PatternKey::RunShape`] row with that purity at that bucket |
//! | `glyphs[].rarity.flag` | a [`PatternKey::Rarity`] row for the glyph |
//! | `glyphs[].rarity.samples` | every occurrence, capped at 8, so a glyph whose sole claim is [`PatternKey::Rarity`] still shows one — the gap this closes: `sites::locate` headlines a run by its finest matched pattern, so a rare glyph inside a run some commoner glyph's pattern also headlines listed nowhere before |
//! | `*.samples` | up to 8 per bucket, from one text scan per glyph per book, classified with [`sous_core::sites::Cursor`] |
//!
//! `uname` has no Unicode name lookup (no `unicode_names2` dependency exists
//! in this workspace): it repeats `cp` beside the scalar itself.
//!
//! The **Capitalization** tab reads one more key, `cap[]`, one entry per
//! [`PatternKey::Casing`], [`PatternKey::Doubled`] or [`PatternKey::LetterRun`]
//! row: `kind` says which, `w` is the word (or the pair) its first site landed
//! on, `form` the flagged minority form, `bare`/`separated`, or the letter and
//! its run length, `n`/`d` the fraction, `books` the dispersion, and `samples`
//! up to 8 of that row's sites in the tuple shape the glyph cards use. Amber
//! is the row's own existence — the Rust judge fired it — never a JS
//! recomputation.

use rustc_hash::{FxHashMap, FxHashSet};

use sous_core::judge::{Channel, PatternKey, Side};
use sous_core::sites::Cursor;
use sous_core::substrate::{BookAggregate, ChapterRow, OuterClass, PairKey, ScalarKey};
use sous_core::unicode::{Pool, class_of, pool_of};
use sous_core::{
    Chapter, ChapterObs, ChapterPass, Corpus, FindingKind, JudgingConfig, PackedFinding, Pattern,
    ProjectedBook, Staircase, Substrate, Verse, for_each_chapter,
};
use usfm_galley::sous::OnionBook;

/// Samples kept per bucket before a card stops collecting.
const SAMPLE_CAP: usize = 8;
/// Scalars of context either side of a sample's match, in the short snippet.
const SNIPPET_CONTEXT: usize = 40;

/// The four stand-alone attachment buckets, joint (prev, next) neighbor
/// classes; `in-run` is everything else and is computed separately.
const TOPO: [(&str, OuterClass, OuterClass); 4] = [
    ("Both", OuterClass::Letter, OuterClass::Letter),
    ("StartOnly", OuterClass::Letter, OuterClass::Space),
    ("EndOnly", OuterClass::Space, OuterClass::Letter),
    ("Neither", OuterClass::Space, OuterClass::Space),
];

/// One fired length row as the page shows it: the address, the ratio, both
/// scopes, and the two texts side by side.
///
/// The wire record carries the two deviations and a span; everything else
/// here the CLI recomputed from the two corpora it holds.
pub struct PairedUnit {
    pub address: String,
    pub ratio: f64,
    pub book_z: Option<f64>,
    pub project_z: Option<f64>,
    pub target: String,
    pub source: String,
}

/// One presence row as the page shows it: which side holds the verses, the
/// first key, and how many consecutive keys follow it.
pub struct PresenceUnit {
    pub book_idx: u16,
    pub address: String,
    pub kind: &'static str,
    pub keys: u32,
}

/// The source comparison's contribution to the page; empty when no source
/// was declared, which is what hides the tab.
#[derive(Default)]
pub struct Paired {
    pub units: Vec<PairedUnit>,
    pub presence: Vec<PresenceUnit>,
}

/// The self-contained inventory page for the target corpus.
pub fn render(
    name: &str,
    corpus: &Corpus<'_, OnionBook>,
    patterns: &[Pattern],
    findings: &[PackedFinding],
    paired: &Paired,
) -> String {
    let json = corpus_json(name, corpus, patterns, findings, paired);
    TEMPLATE.replace("@@CORPORA@@", &format!("[{json}]"))
}

/// `len[]`: one record per fired length row, in publication order.
fn lengths_json(paired: &Paired) -> String {
    let rows: Vec<String> = paired
        .units
        .iter()
        .map(|unit| {
            let scope = |value: Option<f64>| match value {
                Some(value) => format!("{value:.2}"),
                None => "null".to_string(),
            };
            format!(
                "{{\"ref\":{},\"ratio\":{:.4},\"zb\":{},\"zp\":{},\"t\":{},\"s\":{}}}",
                json_str(&unit.address),
                unit.ratio,
                scope(unit.book_z),
                scope(unit.project_z),
                json_str(unit.target.trim()),
                json_str(unit.source.trim()),
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

/// `pres[]`: one record per presence row, in publication order.
fn presence_json(paired: &Paired) -> String {
    let rows: Vec<String> = paired
        .presence
        .iter()
        .map(|row| {
            format!(
                "{{\"ref\":{},\"kind\":{},\"keys\":{}}}",
                json_str(&row.address),
                json_str(row.kind),
                row.keys,
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

const TEMPLATE: &str = include_str!("../templates/inventory.html");

// ── Aggregation ─────────────────────────────────────────────────────────

/// One book's folded substrate counts, in corpus order.
fn book_aggregates(corpus: &Corpus<'_, OnionBook>) -> Vec<BookAggregate> {
    let mut out = Vec::with_capacity(corpus.len());
    let mut observations: Vec<(u32, ChapterRow)> = Vec::new();
    for (_, book) in corpus.iter() {
        observations.clear();
        for_each_chapter(book, |start, input| {
            observations.push((start, Substrate.map(input)));
        });
        let rows: Vec<ChapterObs<&ChapterRow>> = observations
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();
        out.push(Substrate.fold(&rows));
    }
    out
}

/// Total and books-touched, merged across every book's copy of one lane.
struct Merged<K> {
    totals: FxHashMap<K, (u64, u8)>,
}

impl<K: Eq + std::hash::Hash + Copy> Merged<K> {
    fn build<'a>(books: impl Iterator<Item = &'a [(K, u32)]>) -> Self
    where
        K: 'a,
    {
        let mut totals: FxHashMap<K, (u64, u8)> = FxHashMap::default();
        for book in books {
            for &(key, count) in book {
                if count == 0 {
                    continue;
                }
                let entry = totals.entry(key).or_insert((0, 0));
                entry.0 += u64::from(count);
                entry.1 = entry.1.saturating_add(1);
            }
        }
        Self { totals }
    }

    fn get(&self, key: K) -> (u64, u8) {
        self.totals.get(&key).copied().unwrap_or((0, 0))
    }
}

/// A glyph's own count and which books hold it, non-letters only.
fn glyph_roster(aggregates: &[BookAggregate]) -> (Merged<ScalarKey>, Vec<ScalarKey>) {
    let merged = Merged::build(aggregates.iter().map(BookAggregate::scalars));
    let mut glyphs: Vec<ScalarKey> = merged
        .totals
        .keys()
        .copied()
        .filter(|key| {
            key.is_digits()
                || key
                    .scalar()
                    .is_some_and(|c| sous_core::substrate::is_nonletter(class_of(c)))
        })
        .collect();
    glyphs.sort_by_key(|key| std::cmp::Reverse(merged.get(*key).0));
    (merged, glyphs)
}

// ── One glyph's data ────────────────────────────────────────────────────

/// A run atom directly after the glyph, inside a cluster.
struct AfterRow {
    partner: char,
    total: u64,
    books: u8,
    flag: bool,
    pool: &'static str,
    samples: Vec<Sample>,
}

/// The pool heading a partner groups under; a display label only, matching
/// [`Pool`]'s own name for every variant an in-run neighbour can be, save
/// [`Pool::Digit`] which never occurs there.
fn pool_name(pool: Pool) -> &'static str {
    match pool {
        Pool::Quote => "Quote",
        Pool::Bracket => "Bracket",
        Pool::Dash => "Dash",
        Pool::Terminal => "Terminal",
        Pool::Separator => "Separator",
        Pool::Digit => "Digit",
        Pool::Symbol => "Symbol",
        Pool::Other => "Other",
    }
}

/// One sample tuple: `[ref, snip, idx, len, cprev, ccur, cidx, cnext]`.
struct Sample {
    reference: String,
    snippet: String,
    idx: usize,
    len: usize,
    cprev: String,
    ccur: String,
    cidx: usize,
    cnext: String,
}

fn corpus_json(
    name: &str,
    corpus: &Corpus<'_, OnionBook>,
    patterns: &[Pattern],
    findings: &[PackedFinding],
    paired: &Paired,
) -> String {
    let aggregates = book_aggregates(corpus);
    let (scalars, glyphs) = glyph_roster(&aggregates);
    let pairs = Merged::build(aggregates.iter().map(BookAggregate::pairs));

    let cfg = JudgingConfig::default();
    let judging = format!(
        "judge: support floor {} \u{b7} rarity floor {} \u{b7} bands {} \u{b7} word floor {} \u{b7} word bands {}",
        cfg.support_floor,
        cfg.rarity_floor,
        shares(&cfg.bands),
        cfg.word_support_floor,
        shares(&cfg.word_bands),
    );

    let verses: Vec<Vec<Verse>> = corpus
        .iter()
        .map(|(_, book)| book.verses().collect())
        .collect();
    let mut glyph_json = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        glyph_json.push(glyph_object(
            glyph,
            &scalars,
            &pairs,
            &aggregates,
            corpus,
            patterns,
            findings,
            &verses,
        ));
    }
    format!(
        r#"{{"name":{},"judging":{},"glyphs":[{}],"cap":[{}],"len":{},"pres":{}}}"#,
        json_str(name),
        json_str(&judging),
        glyph_json.join(","),
        cap_json(corpus, patterns, findings),
        lengths_json(paired),
        presence_json(paired),
    )
}

fn shares(bands: &Staircase) -> String {
    bands
        .steps
        .iter()
        .map(|step| format!("{:.2}%", f64::from(step.share_bp) / 100.0))
        .collect::<Vec<_>>()
        .join("/")
}

// -- Capitalization -----------------------------------------------------

/// One row per firing word pattern: the word its sites landed on, the claim,
/// the fraction, and up to eight of those sites in context.
///
/// The word itself is not on the wire — a pattern carries a hash — so it comes
/// from the text its sites point at, which is that word by construction. A
/// doubled row's span covers both words and the separator, so its `w` reads
/// back as the pair; a letter-run row's span is the word the run sits inside.
fn cap_json(
    corpus: &Corpus<'_, OnionBook>,
    patterns: &[Pattern],
    findings: &[PackedFinding],
) -> String {
    let mut sites: Vec<Vec<&PackedFinding>> = vec![Vec::new(); patterns.len()];
    for finding in findings {
        if let FindingKind::Convention(digest) = finding.kind()
            && let Some(rows) = sites.get_mut(usize::from(digest.pattern().get()))
        {
            rows.push(finding);
        }
    }
    let verses: Vec<Vec<Verse>> = corpus
        .iter()
        .map(|(_, book)| book.verses().collect())
        .collect();

    let mut rows = Vec::new();
    for (index, pattern) in patterns.iter().enumerate() {
        let length;
        let (kind, form) = match pattern.key {
            PatternKey::Casing { form, .. } => ("casing", form.name()),
            PatternKey::Doubled { separated, .. } => {
                ("doubled", if separated { "separated" } else { "bare" })
            }
            // The letter is the row's own glyph, so the page names it without
            // reading the word back out of the text.
            PatternKey::LetterRun { length: run } => {
                length = format!(
                    "{}\u{00d7}{run}",
                    pattern
                        .glyph
                        .scalar()
                        .unwrap_or(char::REPLACEMENT_CHARACTER)
                );
                ("letterrun", length.as_str())
            }
            _ => continue,
        };
        let mut word = String::new();
        let mut samples = Vec::new();
        for finding in &sites[index] {
            let at = finding.book_idx();
            let book = corpus.get(at).expect("a finding names a corpus book");
            let text = book.text();
            if word.is_empty() {
                word = text[finding.from() as usize..finding.to() as usize].to_string();
            }
            if samples.len() < SAMPLE_CAP
                && let Some(sample) = build_sample(
                    text,
                    finding.from(),
                    finding.to() - finding.from(),
                    &verses[at.get() as usize],
                    book.key(),
                )
            {
                samples.push(sample);
            }
        }
        rows.push(format!(
            r#"{{"kind":{},"w":{},"form":{},"n":{},"d":{},"books":{},"sites":{},"samples":[{}]}}"#,
            json_str(kind),
            json_str(&word),
            json_str(form),
            pattern.numerator,
            pattern.denominator,
            pattern.books,
            sites[index].len(),
            samples_json(Some(&samples)),
        ));
    }
    rows.join(",")
}

#[allow(clippy::too_many_arguments)]
fn glyph_object(
    glyph: ScalarKey,
    scalars: &Merged<ScalarKey>,
    pairs: &Merged<PairKey>,
    aggregates: &[BookAggregate],
    corpus: &Corpus<'_, OnionBook>,
    patterns: &[Pattern],
    findings: &[PackedFinding],
    verses: &[Vec<Verse>],
) -> String {
    let (total, books) = scalars.get(glyph);
    let (g, cp, uname) = match glyph.scalar() {
        Some(c) => (
            c.to_string(),
            format!("U+{:04X}", c as u32),
            format!("U+{:04X} {c}", c as u32),
        ),
        None => ("digits".to_string(), String::new(), "digits".to_string()),
    };

    // ── side: before/after neighbour kinds ──
    let mut side_start: FxHashMap<&'static str, u64> = FxHashMap::default();
    let mut side_end: FxHashMap<&'static str, u64> = FxHashMap::default();
    for &prev in &[
        OuterClass::Letter,
        OuterClass::Space,
        OuterClass::Edge,
        OuterClass::Digit,
    ] {
        let mut n = 0u64;
        for &next in &OuterClass::ALL {
            n += pairs.get(PairKey::new(glyph, prev, next)).0;
        }
        if n > 0 {
            *side_start.entry(side_name(prev)).or_insert(0) += n;
        }
    }
    for &next in &[
        OuterClass::Letter,
        OuterClass::Space,
        OuterClass::Edge,
        OuterClass::Digit,
    ] {
        let mut n = 0u64;
        for &prev in &OuterClass::ALL {
            n += pairs.get(PairKey::new(glyph, prev, next)).0;
        }
        if n > 0 {
            *side_end.entry(side_name(next)).or_insert(0) += n;
        }
    }

    // ── after: exact run neighbour, and pure run lengths ──
    let mut after: FxHashMap<char, (u64, u8)> = FxHashMap::default();
    let mut runlen: FxHashMap<(bool, u8), (u64, u8)> = FxHashMap::default();
    let mut nonletter_start: u64 = 0; // this glyph's prev is Nonletter, split by pool
    let mut nonletter_end: u64 = 0;
    let mut pool_start: FxHashMap<&'static str, u64> = FxHashMap::default();
    let mut pool_end: FxHashMap<&'static str, u64> = FxHashMap::default();
    for aggregate in aggregates {
        let mut book_after: FxHashMap<char, u64> = FxHashMap::default();
        let mut book_runlen: FxHashMap<(bool, u8), u64> = FxHashMap::default();
        for (atoms, count) in aggregate.runs() {
            if !atoms.contains(&glyph) {
                continue;
            }
            let pure = atoms.iter().all(|atom| *atom == glyph);
            let bucket = atoms.len().min(sous_core::substrate::RUN_BUCKETS) as u8;
            *book_runlen.entry((pure, bucket)).or_insert(0) += u64::from(count);
            for (position, atom) in atoms.iter().enumerate() {
                if *atom != glyph {
                    continue;
                }
                if let Some(next) = atoms.get(position + 1)
                    && let Some(partner) = next.scalar()
                {
                    *book_after.entry(partner).or_insert(0) += u64::from(count);
                    match pool_of(partner) {
                        Pool::Quote => {
                            *pool_end.entry("run:quote").or_insert(0) += u64::from(count)
                        }
                        _ => *pool_end.entry("run:punct").or_insert(0) += u64::from(count),
                    }
                    nonletter_end += u64::from(count);
                }
                if position > 0
                    && let Some(prev) = atoms.get(position - 1)
                    && let Some(prev_scalar) = prev.scalar()
                {
                    match pool_of(prev_scalar) {
                        Pool::Quote => {
                            *pool_start.entry("run:quote").or_insert(0) += u64::from(count)
                        }
                        _ => *pool_start.entry("run:punct").or_insert(0) += u64::from(count),
                    }
                    nonletter_start += u64::from(count);
                }
            }
        }
        for (&partner, &n) in &book_after {
            let entry = after.entry(partner).or_insert((0, 0));
            entry.0 += n;
            entry.1 = entry.1.saturating_add(1);
        }
        for (&shape, &n) in &book_runlen {
            let entry = runlen.entry(shape).or_insert((0, 0));
            entry.0 += n;
            entry.1 = entry.1.saturating_add(1);
        }
    }
    // The pooled digit key never rides inside a run: fold whatever Nonletter
    // marginal the pairs lane still owes it into `run:punct`, undifferentiated.
    if glyph.is_digits() {
        let mut leftover_start = 0u64;
        let mut leftover_end = 0u64;
        for &other in &OuterClass::ALL {
            leftover_start += pairs
                .get(PairKey::new(glyph, OuterClass::Nonletter, other))
                .0;
            leftover_end += pairs
                .get(PairKey::new(glyph, other, OuterClass::Nonletter))
                .0;
        }
        if leftover_start > 0 {
            *pool_start.entry("run:punct").or_insert(0) += leftover_start;
        }
        if leftover_end > 0 {
            *pool_end.entry("run:punct").or_insert(0) += leftover_end;
        }
    }
    for (name, n) in pool_start {
        *side_start.entry(name).or_insert(0) += n;
    }
    for (name, n) in pool_end {
        *side_end.entry(name).or_insert(0) += n;
    }
    let _ = (nonletter_start, nonletter_end);

    // ── topo: joint (prev, next) buckets ──
    let mut topo_n: FxHashMap<&'static str, (u64, u8)> = FxHashMap::default();
    let mut combo_total = 0u64;
    for (name, prev, next) in TOPO {
        let mut n = 0u64;
        let mut books_touched = 0u8;
        for aggregate in aggregates {
            let count = aggregate
                .pairs()
                .iter()
                .find(|(key, _)| *key == PairKey::new(glyph, prev, next))
                .map_or(0, |(_, count)| *count);
            if count > 0 {
                n += u64::from(count);
                books_touched = books_touched.saturating_add(1);
            }
        }
        combo_total += n;
        topo_n.insert(name, (n, books_touched));
    }
    let in_run_n = total.saturating_sub(combo_total);
    let mut in_run_books = 0u8;
    for aggregate in aggregates {
        let mut book_total = 0u64;
        for &(key, count) in aggregate.scalars() {
            if key == glyph {
                book_total = u64::from(count);
            }
        }
        let mut book_combo = 0u64;
        for (_, prev, next) in TOPO {
            book_combo += u64::from(
                aggregate
                    .pairs()
                    .iter()
                    .find(|(key, _)| *key == PairKey::new(glyph, prev, next))
                    .map_or(0, |(_, count)| *count),
            );
        }
        if book_total.saturating_sub(book_combo) > 0 {
            in_run_books = in_run_books.saturating_add(1);
        }
    }

    // ── flags: which cards the Rust judge already convicted ──
    let placement_fires = |side: Side, class: OuterClass| {
        patterns.iter().any(|p| {
            p.glyph == glyph
                && p.channel == Channel::Placement
                && p.key == PatternKey::Placement { side, class }
        })
    };
    let run_shape_fires = |pure: bool, bucket: u8| {
        patterns.iter().any(|p| {
            p.glyph == glyph
                && p.channel == Channel::RunShape
                && p.key == PatternKey::RunShape { pure, bucket }
        })
    };
    let exact_neighbor_fires = |partner: char| {
        patterns.iter().any(|p| {
            p.glyph == glyph
                && p.channel == Channel::ExactNeighbor
                && p.key == PatternKey::ExactNeighbor(ScalarKey::of(partner))
        })
    };

    let topo_flag = |name: &str, prev: OuterClass, next: OuterClass| -> bool {
        // The cluster card states only its own share; a rare cluster SHAPE
        // is amber on its size button, not here.
        if name == "in-run" {
            return placement_fires(Side::Prev, OuterClass::Nonletter)
                || placement_fires(Side::Next, OuterClass::Nonletter);
        }
        placement_fires(Side::Prev, prev) || placement_fires(Side::Next, next)
    };

    // ── samples: one text scan per book, capped at 8 a bucket ──
    let mut topo_samples: FxHashMap<&'static str, Vec<Sample>> = FxHashMap::default();
    let mut after_samples: FxHashMap<char, Vec<Sample>> = FxHashMap::default();
    let mut runlen_samples: FxHashMap<(bool, u8), Vec<Sample>> = FxHashMap::default();
    let mut rarity_samples: Vec<Sample> = Vec::new();
    for (_, book) in corpus.iter() {
        harvest_samples(
            book,
            glyph,
            &mut topo_samples,
            &mut after_samples,
            &mut runlen_samples,
            &mut rarity_samples,
        );
    }

    let rarity_fires = patterns
        .iter()
        .any(|p| p.glyph == glyph && p.channel == Channel::Rarity && p.key == PatternKey::Rarity);

    let mut side_start_json: Vec<String> = side_start
        .iter()
        .map(|(name, n)| format!(r#""{name}":{n}"#))
        .collect();
    side_start_json.sort();
    let mut side_end_json: Vec<String> = side_end
        .iter()
        .map(|(name, n)| format!(r#""{name}":{n}"#))
        .collect();
    side_end_json.sort();

    let mut topo_json = Vec::new();
    for (name, prev, next) in TOPO {
        let (n, b) = topo_n[name];
        topo_json.push(format!(
            r#""{name}":{{"n":{n},"books":{b},"flag":{},"samples":[{}]}}"#,
            topo_flag(name, prev, next),
            samples_json(topo_samples.get(name)),
        ));
    }
    topo_json.push(format!(
        r#""in-run":{{"n":{in_run_n},"books":{in_run_books},"flag":{},"samples":[{}]}}"#,
        topo_flag("in-run", OuterClass::Edge, OuterClass::Edge),
        samples_json(topo_samples.get("in-run")),
    ));

    let mut after_rows: Vec<AfterRow> = after
        .into_iter()
        .map(|(partner, (n, b))| AfterRow {
            partner,
            total: n,
            books: b,
            flag: exact_neighbor_fires(partner),
            pool: pool_name(pool_of(partner)),
            samples: after_samples.remove(&partner).unwrap_or_default(),
        })
        .collect();
    after_rows.sort_by_key(|row| std::cmp::Reverse(row.total));
    let pairs_json: Vec<String> = after_rows
        .iter()
        .map(|row| {
            format!(
                r#"{{"p":{},"n":{},"books":{},"flag":{},"pool":{},"samples":[{}]}}"#,
                json_str(&row.partner.to_string()),
                row.total,
                row.books,
                row.flag,
                json_str(row.pool),
                samples_json(Some(&row.samples)),
            )
        })
        .collect();

    let runlen_half = |pure: bool| -> (String, String) {
        let mut rows: Vec<String> = runlen
            .iter()
            .filter(|((p, _), _)| *p == pure)
            .map(|(&(_, bucket), &(n, _b))| {
                format!(
                    r#""{bucket}":{{"n":{n},"flag":{}}}"#,
                    run_shape_fires(pure, bucket),
                )
            })
            .collect();
        rows.sort();
        let mut samples: Vec<String> = runlen
            .keys()
            .filter(|(p, _)| *p == pure)
            .map(|&(_, bucket)| {
                format!(
                    r#""{bucket}":[{}]"#,
                    samples_json(runlen_samples.get(&(pure, bucket))),
                )
            })
            .collect();
        samples.sort();
        (rows.join(","), samples.join(","))
    };
    let (runlen_pure_json, runlen_pure_samples) = runlen_half(true);
    let (runlen_mixed_json, runlen_mixed_samples) = runlen_half(false);

    format!(
        r#"{{"g":{g_json},"uname":{uname_json},"cp":{cp_json},"total":{total},"books":{books},"side":{{"start":{{{side_start}}},"end":{{{side_end}}}}},"topo":{{{topo}}},"pairs":[{pairs}],"runlen":{{"pure":{{{runlen_pure}}},"mixed":{{{runlen_mixed}}}}},"runlen_samples":{{"pure":{{{runlen_pure_samples}}},"mixed":{{{runlen_mixed_samples}}}}},"rarity":{{"flag":{rarity_flag},"samples":[{rarity_samples}]}},"sentence":{sentence}}}"#,
        g_json = json_str(&g),
        uname_json = json_str(&uname),
        cp_json = json_str(&cp),
        side_start = side_start_json.join(","),
        side_end = side_end_json.join(","),
        topo = topo_json.join(","),
        pairs = pairs_json.join(","),
        runlen_pure = runlen_pure_json,
        runlen_mixed = runlen_mixed_json,
        runlen_pure_samples = runlen_pure_samples,
        runlen_mixed_samples = runlen_mixed_samples,
        rarity_flag = rarity_fires,
        rarity_samples = samples_json(Some(&rarity_samples)),
        sentence = sentence_json(glyph, corpus, patterns, findings, verses),
    )
}

/// The "Capital expected" list for one glyph: the lowercase words it handed
/// off to where the corpus almost always hands off a capital.
///
/// `null` unless the glyph fired [`Channel::SentenceStart`]; the words come
/// from the sites, whose span IS the word by construction.
fn sentence_json(
    glyph: ScalarKey,
    corpus: &Corpus<'_, OnionBook>,
    patterns: &[Pattern],
    findings: &[PackedFinding],
    verses: &[Vec<Verse>],
) -> String {
    let Some((index, pattern)) = patterns.iter().enumerate().find(|(_, row)| {
        row.glyph == glyph
            && row.channel == Channel::SentenceStart
            && row.key == PatternKey::SentenceStart
    }) else {
        return "null".to_string();
    };
    let mut sites = 0usize;
    let mut samples = Vec::new();
    for finding in findings {
        let FindingKind::Convention(digest) = finding.kind() else {
            continue;
        };
        if usize::from(digest.pattern().get()) != index {
            continue;
        }
        sites += 1;
        if samples.len() < SAMPLE_CAP {
            let at = finding.book_idx();
            let book = corpus.get(at).expect("a finding names a corpus book");
            if let Some(sample) = build_sample(
                book.text(),
                finding.from(),
                finding.to() - finding.from(),
                &verses[at.get() as usize],
                book.key(),
            ) {
                samples.push(sample);
            }
        }
    }
    format!(
        r#"{{"n":{},"d":{},"books":{},"sites":{sites},"samples":[{}]}}"#,
        pattern.numerator,
        pattern.denominator,
        pattern.books,
        samples_json(Some(&samples)),
    )
}

fn side_name(class: OuterClass) -> &'static str {
    match class {
        OuterClass::Letter => "letter",
        OuterClass::Space => "space",
        OuterClass::Edge => "edge",
        OuterClass::Digit => "run:digit",
        OuterClass::Nonletter => "run:punct",
    }
}

fn samples_json(samples: Option<&Vec<Sample>>) -> String {
    let Some(samples) = samples else {
        return String::new();
    };
    samples
        .iter()
        .map(|sample| {
            format!(
                "[{},{},{},{},{},{},{},{}]",
                json_str(&sample.reference),
                json_str(&sample.snippet),
                sample.idx,
                sample.len,
                json_str(&sample.cprev),
                json_str(&sample.ccur),
                sample.cidx,
                json_str(&sample.cnext),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

// ── Sample harvesting: one scan per glyph per book ─────────────────────

fn harvest_samples(
    book: &OnionBook,
    glyph: ScalarKey,
    topo_samples: &mut FxHashMap<&'static str, Vec<Sample>>,
    after_samples: &mut FxHashMap<char, Vec<Sample>>,
    runlen_samples: &mut FxHashMap<(bool, u8), Vec<Sample>>,
    rarity_samples: &mut Vec<Sample>,
) {
    let text = book.text();
    let chapters: Vec<Chapter> = book.chapters().collect();
    let verses: Vec<Verse> = book.verses().collect();
    let cursor = Cursor::new(text, &chapters);
    let mut seen_runs: FxHashSet<(u32, u32)> = FxHashSet::default();

    for (at, scalar) in text.char_indices() {
        let matches = match glyph.scalar() {
            Some(want) => scalar == want,
            None => class_of(scalar).is_decimal_digit(),
        };
        if !matches {
            continue;
        }
        let at = at as u32;
        let width = scalar.len_utf8() as u32;

        // rarity: every occurrence, its own claim independent of run headline
        if rarity_samples.len() < SAMPLE_CAP
            && let Some(sample) = build_sample(text, at, width, &verses, book.key())
        {
            rarity_samples.push(sample);
        }

        // topo bucket
        let prev = cursor.prev_outer(at);
        let next = cursor.next_outer(at);
        let bucket = TOPO
            .iter()
            .find(|(_, p, n)| *p == prev && *n == next)
            .map_or("in-run", |(name, _, _)| name);
        if topo_samples
            .get(bucket)
            .is_none_or(|v| v.len() < SAMPLE_CAP)
            && let Some(sample) = build_sample(text, at, width, &verses, book.key())
        {
            topo_samples.entry(bucket).or_default().push(sample);
        }

        // after: the next atom in the same run, if any
        let run = cursor.run_around(at);
        if !run.is_empty() {
            if let Some((next_at, next_scalar)) = text[(at + width) as usize..run.to() as usize]
                .char_indices()
                .next()
                .map(|(offset, c)| (at + width + offset as u32, c))
            {
                let span_len = (next_at + next_scalar.len_utf8() as u32) - at;
                let needs_more = after_samples
                    .get(&next_scalar)
                    .is_none_or(|v| v.len() < SAMPLE_CAP);
                if needs_more
                    && let Some(sample) = build_sample(text, at, span_len, &verses, book.key())
                {
                    after_samples.entry(next_scalar).or_default().push(sample);
                }
            }

            // runlen: one sample per distinct run, keyed by its shape
            if seen_runs.insert((run.from(), run.to())) {
                let atoms: Vec<char> = text[run.from() as usize..run.to() as usize]
                    .chars()
                    .collect();
                let pure = atoms.iter().all(|&c| Some(c) == glyph.scalar());
                let bucket = atoms.len().min(sous_core::substrate::RUN_BUCKETS) as u8;
                let shape = (pure, bucket);
                if runlen_samples
                    .get(&shape)
                    .is_none_or(|v| v.len() < SAMPLE_CAP)
                    && let Some(sample) =
                        build_sample(text, run.from(), run.len(), &verses, book.key())
                {
                    runlen_samples.entry(shape).or_default().push(sample);
                }
            }
        }
    }
}

/// One sample tuple for the span `at..at+len`, or `None` when it lies
/// outside every verse (front matter a report never samples).
fn build_sample(
    text: &str,
    at: u32,
    len: u32,
    verses: &[Verse],
    book_key: sous_core::BookKey,
) -> Option<Sample> {
    let from = at as usize;
    let to = (at + len) as usize;
    let head_start = floor_char_boundary(text, from.saturating_sub(SNIPPET_CONTEXT * 4));
    let head = &text[head_start..from];
    let head_trimmed = trim_head_scalars(head, SNIPPET_CONTEXT);
    let tail_end = ceil_char_boundary(text, (to + SNIPPET_CONTEXT * 4).min(text.len()));
    let tail = &text[to..tail_end];
    let tail_trimmed = trim_tail_scalars(tail, SNIPPET_CONTEXT);
    let snippet = format!("{head_trimmed}{}{tail_trimmed}", &text[from..to]);
    let idx = code_units(head_trimmed);
    let len_units = code_units(&text[from..to]);

    let verse = verses.iter().position(|v| {
        let span = v.text();
        span.from() <= at && to as u32 <= span.to()
    })?;
    let key = verses[verse].key();
    let reference = if key.first() == key.last() {
        format!("{book_key} {}:{}", key.chapter(), key.first())
    } else {
        format!(
            "{book_key} {}:{}-{}",
            key.chapter(),
            key.first(),
            key.last()
        )
    };
    let span = verses[verse].text();
    let ccur = text[span.from() as usize..span.to() as usize].to_string();
    let cidx = code_units(&text[span.from() as usize..from]);
    let cprev = verse
        .checked_sub(1)
        .and_then(|i| verses.get(i))
        .map(|v| text[v.text().from() as usize..v.text().to() as usize].to_string())
        .unwrap_or_default();
    let cnext = verses
        .get(verse + 1)
        .map(|v| text[v.text().from() as usize..v.text().to() as usize].to_string())
        .unwrap_or_default();

    Some(Sample {
        reference,
        snippet,
        idx,
        len: len_units,
        cprev,
        ccur,
        cidx,
        cnext,
    })
}

fn code_units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The last `count` scalars of `text`, trimmed on a char boundary.
fn trim_head_scalars(text: &str, count: usize) -> &str {
    match text.char_indices().rev().nth(count - 1) {
        Some((at, _)) => &text[at..],
        None => text,
    }
}

/// The first `count` scalars of `text`, trimmed on a char boundary.
fn trim_tail_scalars(text: &str, count: usize) -> &str {
    match text.char_indices().nth(count) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

fn floor_char_boundary(text: &str, mut at: usize) -> usize {
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil_char_boundary(text: &str, mut at: usize) -> usize {
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// A JS/JSON double-quoted string literal.
fn json_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bucket a hand-built join classifies into, and how many hits it
    /// counts: `"a, b ,c , d,e"`, glyph `,`.
    #[test]
    fn topo_buckets_split_by_joint_neighbour_class() {
        let text = "a, b ,c , d,e";
        // Verse spans the whole string so every comma sits inside it.
        let chapter =
            Chapter::new(1, sous_core::TextRange::new(0, text.len() as u32).unwrap()).unwrap();
        let cursor = Cursor::new(text, std::slice::from_ref(&chapter));
        let mut counts: FxHashMap<&'static str, usize> = FxHashMap::default();
        for (at, c) in text.char_indices() {
            if c != ',' {
                continue;
            }
            let at = at as u32;
            let prev = cursor.prev_outer(at);
            let next = cursor.next_outer(at);
            let bucket = TOPO
                .iter()
                .find(|(_, p, n)| *p == prev && *n == next)
                .map_or("in-run", |(name, _, _)| name);
            *counts.entry(bucket).or_insert(0) += 1;
        }
        // "a," -> StartOnly (letter before, space after); " ,c" -> EndOnly
        // (space before, letter after); " , " -> Neither; "d,e" -> Both.
        assert_eq!(counts.get("Both"), Some(&1));
        assert_eq!(counts.get("StartOnly"), Some(&1));
        assert_eq!(counts.get("EndOnly"), Some(&1));
        assert_eq!(counts.get("Neither"), Some(&1));
        assert_eq!(counts.get("in-run"), None);
    }
}
