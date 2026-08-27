//! The fold's lex-leg oracle: per-chunk lex + rebase-concat must be
//! BYTE-IDENTICAL to fresh whole-book lexing (chunk-fold.md's law 1 for
//! the one stage already proven carry-0). The `lex_via_cache` helper is
//! TEST-LOCAL — token caching is a pattern proof, not shipped machinery
//! (ruled 2026-08-27; galley caches downstream products, never tokens).
//! The ignored bench records what chunk-grain reuse buys on real en_ult
//! books — the recorded numbers live in choices.md (pass 19 addendum).
//!
//! The corpus is gitignored, so the corpus-scale halves skip loudly when
//! `example-corpora/` is not mounted.

use std::collections::HashMap;
use std::time::Instant;

use usfm_galley::{Chunks, chunks, concat_absolute, onion};

/// The reuse recipe under test: every chunk through the cache, misses lexed
/// chunk-relative and inserted, the read rebased to absolute.
fn lex_via_cache(
    text: &str,
    scan: &Chunks,
    cache: &mut HashMap<String, Vec<onion::Token>>,
) -> (Vec<onion::Token>, usize) {
    let bytes = text.as_bytes();
    let mut misses = 0;
    let parts: Vec<Vec<onion::Token>> = scan
        .starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = scan
                .starts
                .get(i + 1)
                .map_or(bytes.len(), |&next| next as usize);
            cache
                .entry(scan.checksums[i].clone())
                .or_insert_with(|| {
                    misses += 1;
                    onion::lex(&text[start as usize..end])
                })
                .clone()
        })
        .collect();
    (concat_absolute(&parts, &scan.starts), misses)
}

/// One byte of ASCII text inserted inside the middle chapter — the
/// one-chapter-edited scenario the cache exists for.
fn edit_middle_chapter(text: &str, scan: &Chunks) -> String {
    let mid = scan.starts[scan.starts.len() / 2] as usize;
    let at = text[mid..]
        .find(' ')
        .map(|offset| mid + offset)
        .expect("a chapter contains a space");
    format!("{}{}{}", &text[..at], "x", &text[at..])
}

#[test]
fn cached_chunks_rebased_equal_fresh_lex() {
    let text = "\\id GEN\n\\c 1\n\\p \\v 1 aleph \\w b|lemma=\"c\"\\w*\n\\c 2\n\\p \\v 1 d\n";
    let mut cache = HashMap::new();
    let scan = chunks(text);
    let (reused, misses) = lex_via_cache(text, &scan, &mut cache);
    assert_eq!(reused, onion::lex(text));
    assert_eq!(misses, 3);

    // Edit one chapter: exactly one miss, still byte-identical.
    let edited = edit_middle_chapter(text, &scan);
    let scan = chunks(&edited);
    let (reused, misses) = lex_via_cache(&edited, &scan, &mut cache);
    assert_eq!(reused, onion::lex(&edited));
    assert_eq!(misses, 1);
}

fn en_ult(book: &str) -> Option<String> {
    let path = format!(
        "{}/../example-corpora/en_ult/{book}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).ok();
    if text.is_none() {
        eprintln!("lex reuse SKIPPED: {path} not mounted");
    }
    text
}

#[test]
fn corpus_book_reuse_is_equivalent() {
    let Some(text) = en_ult("19-PSA.usfm") else {
        return;
    };
    let mut cache = HashMap::new();
    let scan = chunks(&text);
    let (reused, _) = lex_via_cache(&text, &scan, &mut cache);
    assert_eq!(reused, onion::lex(&text));

    let edited = edit_middle_chapter(&text, &scan);
    let scan = chunks(&edited);
    let (reused, misses) = lex_via_cache(&edited, &scan, &mut cache);
    assert_eq!(reused, onion::lex(&edited));
    assert_eq!(misses, 1, "one edited chapter is one miss");
}

/// Milliseconds, median of `runs`.
fn median_ms(runs: usize, mut work: impl FnMut()) -> f64 {
    let mut times: Vec<f64> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            work();
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    times.sort_by(|a, b| a.total_cmp(b));
    times[runs / 2]
}

/// The recorded measurement (rung ladder: promotion wants numbers, not
/// vibes). Run with:
///     cargo test -p usfm_galley --release -- --ignored --nocapture
#[test]
#[ignore = "corpus-scale measurement; run --release --ignored at pass end"]
fn bench_lex_reuse_en_ult() {
    for book in ["19-PSA.usfm", "01-GEN.usfm"] {
        let Some(text) = en_ult(book) else { return };
        let scan = chunks(&text);
        let edited = edit_middle_chapter(&text, &scan);

        let fresh = median_ms(9, || {
            std::hint::black_box(onion::lex(&edited));
        });
        let prescan = median_ms(9, || {
            std::hint::black_box(chunks(&edited));
        });
        // Warm cache for the whole ORIGINAL book; the edited text then costs
        // pre_scan + checksums + ONE chunk's lex + the rebase-concat. The
        // edited chunk's entry is evicted before each run (an O(1) remove) so
        // every run pays the true one-miss price, not an all-hit one.
        let mut cache = HashMap::new();
        lex_via_cache(&text, &chunks(&text), &mut cache);
        let warm_keys: std::collections::HashSet<String> =
            cache.keys().cloned().collect();
        let edited_sum = chunks(&edited)
            .checksums
            .iter()
            .find(|sum| !warm_keys.contains(*sum))
            .expect("the edit changed one chunk")
            .clone();
        let reuse = median_ms(9, || {
            cache.remove(&edited_sum);
            let scan = chunks(&edited);
            let (tokens, misses) = lex_via_cache(&edited, &scan, &mut cache);
            assert_eq!(misses, 1);
            std::hint::black_box(tokens);
        });
        let tokens = onion::lex(&text).len();
        println!(
            "{book}: {} bytes, {} chunks, {tokens} tokens (~{}KB cached)\n  \
             fresh lex          {fresh:.2}ms\n  \
             pre-scan+checksum  {prescan:.2}ms\n  \
             reuse (1 edited)   {reuse:.2}ms",
            text.len(),
            scan.starts.len(),
            tokens * size_of::<onion::Token>() / 1024,
        );
    }
}
