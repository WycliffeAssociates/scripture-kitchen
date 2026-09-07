//! One corpus record: the merged aggregates every tab reads.
//!
//! ```text
//! corpus_json(&aggregates, &findings, ..)  -> { "glyphs": [...], "cap": {...} }
//! ```

use super::*;

// ── Aggregation ─────────────────────────────────────────────────────────

/// One book's folded substrate counts, in corpus order.
pub(super) fn book_aggregates(corpus: &Corpus<'_, OnionBook>) -> Vec<BookAggregate> {
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
pub(super) struct Merged<K> {
    pub(super) totals: FxHashMap<K, (u64, u8)>,
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

    pub(super) fn get(&self, key: K) -> (u64, u8) {
        self.totals.get(&key).copied().unwrap_or((0, 0))
    }
}

/// A glyph's own count and which books hold it, non-letters only.
pub(super) fn glyph_roster(aggregates: &[BookAggregate]) -> (Merged<ScalarKey>, Vec<ScalarKey>) {
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
pub(super) struct AfterRow {
    pub(super) partner: char,
    pub(super) total: u64,
    pub(super) books: u8,
    pub(super) flag: bool,
    pub(super) pool: &'static str,
    pub(super) samples: Vec<Sample>,
}

/// The pool heading a partner groups under; a display label only, matching
/// [`Pool`]'s own name for every variant an in-run neighbour can be, save
/// [`Pool::Digit`] which never occurs there.
pub(super) fn pool_name(pool: Pool) -> &'static str {
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
pub(super) struct Sample {
    pub(super) reference: String,
    pub(super) snippet: String,
    pub(super) idx: usize,
    pub(super) len: usize,
    pub(super) cprev: String,
    pub(super) ccur: String,
    pub(super) cidx: usize,
    pub(super) cnext: String,
}

pub(super) fn corpus_json(
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
        r#"{{"name":{},"judging":{},"glyphs":[{}],"cap":[{}],"len":{},"pres":{},"copy":{}}}"#,
        json_str(name),
        json_str(&judging),
        glyph_json.join(","),
        cap_json(corpus, patterns, findings),
        lengths_json(paired),
        presence_json(paired),
        copies_json(paired),
    )
}

pub(super) fn shares(bands: &Staircase) -> String {
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
pub(super) fn cap_json(
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
