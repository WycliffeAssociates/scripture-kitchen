//! One glyph's card: the counts, the neighbours, and why it is amber.
//!
//! ```text
//! glyph_object(',', &merged, &fired)  -> { "glyph": ",", "flag": true, .. }
//! ```

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn glyph_object(
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
pub(super) fn sentence_json(
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

pub(super) fn side_name(class: OuterClass) -> &'static str {
    match class {
        OuterClass::Letter => "letter",
        OuterClass::Space => "space",
        OuterClass::Edge => "edge",
        OuterClass::Digit => "run:digit",
        OuterClass::Nonletter => "run:punct",
    }
}
