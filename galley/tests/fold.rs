//! The fold pipeline against the stateless oracle: [`FoldCache::lint`] must
//! equal onion's fresh `lex → build → lint` on every text it is handed —
//! cold, warm, edited, re-chunked, fused, evicted, undone.
//!
//! The corpus-scale half is `#[ignore]`d (pass-end gate); everything else is
//! synthetic and fast.

use usfm_galley::{FoldCache, onion};

fn fresh(text: &str) -> onion::lint::LintReport {
    let tokens = onion::lex(text);
    let cst = onion::cst::build(&tokens);
    onion::lint::lint(text.as_bytes(), &tokens, &cst)
}

/// Consumer-visible equality: observations, header facts, and every offered
/// repair through the accessors (the fix arena layout is linkage).
fn assert_matches(folded: &onion::lint::LintReport, text: &str, label: &str) {
    let fresh = fresh(text);
    assert_eq!(folded.book, fresh.book, "{label}: book");
    assert_eq!(
        folded.declared_version, fresh.declared_version,
        "{label}: declared_version"
    );
    assert_eq!(
        folded.observations, fresh.observations,
        "{label}: observations"
    );
    for index in 0..fresh.observations.len() {
        assert_eq!(
            folded.fix(index).map(|fix| (fix.label, folded.edits(fix))),
            fresh.fix(index).map(|fix| (fix.label, fresh.edits(fix))),
            "{label}: fix {index}"
        );
    }
}

const BOOK: &str = "\\id GEN\n\\usfm 3.0\n\\h Genesis\n\\mt1 Genesis\n\
\\c 1\n\\p \\v 1 In the beginning \\v 2 and then\n\
\\c 2\n\\q1 \\v 1 poetry \\add here\\add*\n\
\\c 3\n\\p \\v 1 a\\f + \\fr 3:1 \\ft note\\f* tail\n";

#[test]
fn cold_and_warm_calls_equal_fresh_lint() {
    let mut cache = FoldCache::new(16 << 20);
    assert_matches(&cache.lint(BOOK), BOOK, "cold");
    let cold_misses = cache.misses();
    assert_eq!(cold_misses, 4, "chunk 0 + three chapters");

    assert_matches(&cache.lint(BOOK), BOOK, "warm");
    assert_eq!(cache.misses(), cold_misses, "warm run computes nothing");
}

#[test]
fn one_edited_chapter_is_one_miss_and_undo_is_free() {
    let mut cache = FoldCache::new(16 << 20);
    cache.lint(BOOK);
    let baseline = cache.misses();

    let edited = BOOK.replace("poetry", "poetry edited");
    assert_matches(&cache.lint(&edited), &edited, "edited");
    assert_eq!(cache.misses(), baseline + 1, "one dirty chunk, one miss");

    // Undo: the original chunk's hash is still resident — zero misses.
    assert_matches(&cache.lint(BOOK), BOOK, "undone");
    assert_eq!(cache.misses(), baseline + 1, "undo is a pure hit");
}

#[test]
fn chapter_add_delete_and_split_rechunk_cleanly() {
    let mut cache = FoldCache::new(16 << 20);
    cache.lint(BOOK);

    // A pasted `\c` splits one chunk into two: the split chunk misses (as
    // two new units), the rest hit.
    let split = BOOK.replace("\\v 2 and then", "\\v 2 and then\n\\c 9\n\\p \\v 1 pasted");
    assert_matches(&cache.lint(&split), &split, "chapter pasted");

    // Deleting a whole chapter: remaining chunks all hit.
    let baseline = cache.misses();
    let deleted = BOOK.replace("\\c 2\n\\q1 \\v 1 poetry \\add here\\add*\n", "");
    assert_matches(&cache.lint(&deleted), &deleted, "chapter deleted");
    assert_eq!(cache.misses(), baseline, "deletion recomputes nothing");
}

#[test]
fn a_straddling_sidebar_fuses_and_still_matches() {
    let text = "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\esb \\p in\n\\c 2\n\\p more\n\\esbe\n\\p \\v 1 b\n";
    let mut cache = FoldCache::new(16 << 20);
    assert_matches(&cache.lint(text), text, "fused cold");
    let cold = cache.misses();
    // The fused unit is cached under the fused span's hash — and the open
    // boundary is remembered, so the warm call re-lexes nothing.
    assert_matches(&cache.lint(text), text, "fused warm");
    assert_eq!(cache.misses(), cold, "fused unit hits on the warm call");
}

#[test]
fn a_version_edit_in_chunk_0_invalidates_gated_chunks() {
    // `\pro` is deprecated at 3.0+: flipping chunk 0's `\usfm` changes every
    // later chunk's product, which the (hash, version) key must see.
    let with = "\\id GEN\n\\usfm 3.0\n\\c 1\n\\p \\v 1 \\pro x\\pro* a\n";
    let without = "\\id GEN\n\\c 1\n\\p \\v 1 \\pro x\\pro* a\n";
    let mut cache = FoldCache::new(16 << 20);
    assert_matches(&cache.lint(with), with, "with version");
    assert_matches(&cache.lint(without), without, "without version");
    assert_matches(&cache.lint(with), with, "with version again");
}

#[test]
fn the_byte_budget_evicts_and_correctness_survives() {
    // A budget far below one book's products: every call recomputes, the
    // answer never changes, and residency stays bounded.
    let mut cache = FoldCache::new(512);
    assert_matches(&cache.lint(BOOK), BOOK, "starved cold");
    let first = cache.misses();
    assert_matches(&cache.lint(BOOK), BOOK, "starved warm");
    assert!(cache.misses() > first, "a starved cache recomputes");
    assert!(
        cache.resident_bytes() <= 512 || cache.len() == 1,
        "residency is bounded by the budget (one entry may exceed a tiny one)"
    );
}

#[test]
fn empty_and_single_chunk_books() {
    let mut cache = FoldCache::new(16 << 20);
    for text in ["", "\\id FRT\n\\p front matter only\n", "plain prose"] {
        assert_matches(&cache.lint(text), text, text);
    }
}

/// The recorded measurement (the ladder promotes on numbers, not vibes).
/// Run with:
///     cargo test -p usfm_galley --release --test fold -- --ignored --nocapture
#[test]
#[ignore = "corpus-scale measurement; run --release --ignored at pass end"]
fn bench_fold_en_ult() {
    let median_ms = |runs: usize, mut work: Box<dyn FnMut()>| -> f64 {
        let mut times: Vec<f64> = (0..runs)
            .map(|_| {
                let start = std::time::Instant::now();
                work();
                start.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        times.sort_by(|a, b| a.total_cmp(b));
        times[runs / 2]
    };
    for book in ["19-PSA.usfm", "01-GEN.usfm"] {
        let path = format!(
            "{}/../example-corpora/en_ult/{book}",
            env!("CARGO_MANIFEST_DIR")
        );
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fold bench SKIPPED: {path} not mounted");
            return;
        };
        let mid = text.len() / 2;
        let at = mid + text[mid..].find(' ').expect("a space mid-book");
        let mut edited = text.clone();
        edited.insert(at, 'x');

        let fresh_ms = median_ms(9, {
            let edited = edited.clone();
            Box::new(move || {
                std::hint::black_box(fresh(&edited));
            })
        });
        // Warm the cache on the ORIGINAL text; each measured run then pays
        // the true one-edited-chunk price (the edited chunk's entry is fresh
        // the first time and a hit after, so evict nothing — measure both).
        let mut cache = FoldCache::new(32 << 20);
        cache.lint(&text);
        cache.lint(&edited);
        let folded_warm_ms = median_ms(9, {
            let edited = edited.clone();
            Box::new(move || {
                std::hint::black_box(cache.lint(&edited));
            })
        });
        println!(
            "{book}: {} bytes\n  fresh lex+cst+lint      {fresh_ms:.2}ms\n  folded (all-hit call)   {folded_warm_ms:.2}ms",
            text.len(),
        );
    }
}

/// The corpus-scale law, cache-warm and cache-cold, plus a one-chapter edit
/// per book. `#[ignore]`: minutes-class, part of the pass-end gate.
#[test]
#[ignore = "corpus-scale fold pipeline oracle; run --include-ignored at pass end"]
fn fold_cache_equals_fresh_over_the_corpus() {
    let root = format!("{}/../example-corpora", env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "usfm") {
                paths.push(path);
            }
        }
    }
    if paths.is_empty() {
        eprintln!("fold cache corpus SKIPPED: no *.usfm under example-corpora/");
        return;
    }
    paths.sort();
    for path in &paths {
        let text = std::fs::read_to_string(path).expect("readable book");
        let label = path.display().to_string();
        let mut cache = FoldCache::new(32 << 20);
        assert_matches(&cache.lint(&text), &text, &label);
        // One byte inserted mid-book: warm run, then verify the edit. The
        // midpoint rounds up to a char boundary (Hebrew text mid-byte).
        let mid = (text.len() / 2..text.len())
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0);
        let mut edited = text.clone();
        let at = text[mid..]
            .find(' ')
            .map(|offset| mid + offset)
            .unwrap_or(0);
        edited.insert(at, 'x');
        assert_matches(&cache.lint(&edited), &edited, &format!("{label} (edited)"));
    }
    println!("fold cache oracle held over {} books", paths.len());
}
