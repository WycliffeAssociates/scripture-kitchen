//! The snippets a card shows: real text around a real occurrence, trimmed to
//! character boundaries.
//!
//! ```text
//! harvest_samples(&books, &wanted)  -> ["…he said, \u{201c}Go,\u{201d} and…"]
//! ```

use super::*;

pub(super) fn samples_json(samples: Option<&Vec<Sample>>) -> String {
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

pub(super) fn harvest_samples(
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
pub(super) fn build_sample(
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

pub(super) fn code_units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The last `count` scalars of `text`, trimmed on a char boundary.
pub(super) fn trim_head_scalars(text: &str, count: usize) -> &str {
    match text.char_indices().rev().nth(count - 1) {
        Some((at, _)) => &text[at..],
        None => text,
    }
}

/// The first `count` scalars of `text`, trimmed on a char boundary.
pub(super) fn trim_tail_scalars(text: &str, count: usize) -> &str {
    match text.char_indices().nth(count) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

pub(super) fn floor_char_boundary(text: &str, mut at: usize) -> usize {
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub(super) fn ceil_char_boundary(text: &str, mut at: usize) -> usize {
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// A JS/JSON double-quoted string literal.
pub(super) fn json_str(text: &str) -> String {
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
