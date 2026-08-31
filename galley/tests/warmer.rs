//! The fold pipeline against the stateless oracle: [`Warmer::lint`] must
//! equal onion's fresh `lex → build → lint` on every text it is handed —
//! cold, warm, edited, re-chunked, fused, evicted, undone.
//!
//! The corpus-scale half is `#[ignore]`d (pass-end gate); everything else is
//! synthetic and fast.

use onion::wire::ParseOptions;
use usfm_galley::{Warmer, onion};

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
    let mut cache = Warmer::new(16 << 20);
    assert_matches(&cache.lint(BOOK), BOOK, "cold");
    let cold_misses = cache.misses();
    assert_eq!(cold_misses, 4, "chunk 0 + three chapters");

    assert_matches(&cache.lint(BOOK), BOOK, "warm");
    assert_eq!(cache.misses(), cold_misses, "warm run computes nothing");
}

#[test]
fn one_edited_chapter_is_one_miss_and_undo_is_free() {
    let mut cache = Warmer::new(16 << 20);
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
    let mut cache = Warmer::new(16 << 20);
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
    let mut cache = Warmer::new(16 << 20);
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
    let mut cache = Warmer::new(16 << 20);
    assert_matches(&cache.lint(with), with, "with version");
    assert_matches(&cache.lint(without), without, "without version");
    assert_matches(&cache.lint(with), with, "with version again");
}

#[test]
fn the_byte_budget_evicts_and_correctness_survives() {
    // A budget far below one book's products: every call recomputes, the
    // answer never changes, and residency stays bounded.
    let mut cache = Warmer::new(512);
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
    let mut cache = Warmer::new(16 << 20);
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
            "{}/../testData/exampleCorpora/en_ult/{book}",
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
        let mut cache = Warmer::new(32 << 20);
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
    let root = format!("{}/../testData/exampleCorpora", env!("CARGO_MANIFEST_DIR"));
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
        eprintln!("fold cache corpus SKIPPED: no *.usfm under testData/exampleCorpora/");
        return;
    }
    paths.sort();
    for path in &paths {
        let text = std::fs::read_to_string(path).expect("readable book");
        let label = path.display().to_string();
        let mut cache = Warmer::new(32 << 20);
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

// ---------------------------------------------------------------------------
// The analyze fold: assembled ingredients must give onion's own answer.
// ---------------------------------------------------------------------------

/// Every `Analysis` field, folded vs fresh. Field-by-field rather than
/// The folded dish must be BYTE-IDENTICAL to a cold one.
///
/// Stronger than the field-by-field comparison this replaced, and shorter: if
/// the bytes agree then every field in every section agrees, including the ones
/// nobody thought to assert.
fn assert_dish_matches(cache: &mut Warmer, text: &str, opts: ParseOptions, why: &str) {
    let folded = cache.parse(text, opts);
    let fresh = onion::wire::plate(&onion::wire::parse(text, opts));
    assert_eq!(folded.len(), fresh.len(), "dish length — {why}");
    if folded != fresh {
        let at = folded
            .iter()
            .zip(&fresh)
            .position(|(a, b)| a != b)
            .expect("lengths agreed, so a byte must differ");
        panic!("dish differs at byte {at} — {why}");
    }
}

#[test]
fn the_folded_dish_is_byte_identical_to_a_cold_one() {
    let all = ParseOptions {
        diagnostics: true,
        toc: true,
        utf16: false,
    };
    let mut cache = Warmer::new(64 << 20);
    // Cold, warm, and after an edit — the three states that exercise a
    // different mix of hits, misses and assembly.
    assert_dish_matches(&mut cache, BOOK, all, "cold");
    assert_dish_matches(&mut cache, BOOK, all, "warm");
    let edited = BOOK.replace("a", "aa");
    assert_dish_matches(&mut cache, &edited, all, "edited");
    assert_dish_matches(&mut cache, BOOK, all, "back again");

    // Every combination of the options, over a warm cache: an option must not
    // drag in a section it did not ask for, or drop one it did.
    for diagnostics in [false, true] {
        for toc in [false, true] {
            for utf16 in [false, true] {
                let opts = ParseOptions {
                    diagnostics,
                    toc,
                    utf16,
                };
                assert_dish_matches(
                    &mut cache,
                    BOOK,
                    opts,
                    &format!("diagnostics={diagnostics} toc={toc} utf16={utf16}"),
                );
            }
        }
    }
}

#[test]
fn the_masked_fold_agrees_with_a_fresh_mask() {
    let mut cache = Warmer::new(64 << 20);
    for filter in [
        onion::mask::Filter::verse_text(),
        onion::mask::Filter::structure(),
    ] {
        let folded = cache.masked(BOOK, &filter);
        let tokens = onion::lex(BOOK);
        let tree = onion::cst::build(&tokens);
        let fresh = onion::mask(BOOK.as_bytes(), &tokens, &tree, &filter);
        assert_eq!(folded.ranges, fresh.ranges, "mask ranges");
        assert_eq!(folded.starts, fresh.starts, "mask starts");
    }
}

/// The corpus law: over every book, folded `analyze` and the verse_text mask
/// equal the fresh pipeline — cold, then warm, then after a one-chapter edit.
#[test]
#[ignore = "corpus-scale oracle; run --release --ignored at pass end"]
fn the_analyze_fold_holds_over_the_corpus() {
    let all = ParseOptions {
        diagnostics: true,
        toc: true,
        utf16: false,
    };
    let root = format!("{}/../testData/exampleCorpora", env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    collect(std::path::Path::new(&root), &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "no corpus under {root}");

    let mut books = 0;
    for path in &paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path.display().to_string();
        let mut cache = Warmer::new(64 << 20);
        assert_dish_matches(&mut cache, &text, all, &name);
        assert_dish_matches(&mut cache, &text, all, &name);

        let filter = onion::mask::Filter::verse_text();
        let folded = cache.masked(&text, &filter);
        let tokens = onion::lex(&text);
        let tree = onion::cst::build(&tokens);
        let fresh = onion::mask(text.as_bytes(), &tokens, &tree, &filter);
        assert_eq!(folded.ranges, fresh.ranges, "verse_text ranges — {name}");

        // One chapter dirtied: the assembly now mixes a fresh unit with hits.
        if let Some(mid) = text.char_indices().nth(text.len() / 2).map(|(i, _)| i) {
            let at = mid + text[mid..].find(' ').unwrap_or(0);
            let mut edited = text.clone();
            edited.insert(at, 'x');
            assert_dish_matches(&mut cache, &edited, all, &name);
        }
        books += 1;
    }
    println!("analyze fold holds over {books} books");
}

fn collect(at: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
    let Ok(dir) = std::fs::read_dir(at) else {
        return;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|e| e == "usfm") {
            into.push(path);
        }
    }
}

#[test]
fn a_one_chapter_book_is_computed_and_never_cached() {
    // Two chunks — front matter plus one chapter — so an entry could only ever
    // serve the front matter. The answer must still be right.
    const EPISTLE: &str = "\\id JUD\n\\h Jude\n\\c 1\n\\p \\v 1 Jude, a servant.\n\\v 2 Mercy.\n";
    assert_eq!(
        onion::chunk::pre_scan(EPISTLE.as_bytes()).starts.len(),
        2,
        "the fixture must sit at the gate"
    );

    let mut cache = Warmer::new(16 << 20);
    assert_matches(&cache.lint(EPISTLE), EPISTLE, "gated cold");
    assert_matches(&cache.lint(EPISTLE), EPISTLE, "gated warm");
    let all = ParseOptions {
        diagnostics: true,
        toc: true,
        utf16: false,
    };
    assert_dish_matches(&mut cache, EPISTLE, all, "gated parse");

    assert_eq!(cache.len(), 0, "a gated book leaves no entries");
    assert_eq!(cache.resident_bytes(), 0, "…and no residency");

    // Typing into it must not accumulate anything either.
    for n in 1..=20 {
        let typed = EPISTLE.replace("Mercy", &format!("{}Mercy", "x".repeat(n)));
        assert_matches(&cache.lint(&typed), &typed, "gated keystroke");
    }
    assert_eq!(cache.len(), 0, "twenty keystrokes, still no entries");

    // And a book ABOVE the gate still caches, so the gate is not global.
    let three_chapters = format!("{EPISTLE}\\c 2\n\\p \\v 1 more\n\\c 3\n\\p \\v 1 yet more\n");
    cache.lint(&three_chapters);
    assert!(!cache.is_empty(), "a multi-chapter book still caches");
}
