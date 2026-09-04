//! Resident working memory of the sous-chef host (galley), over a real
//! 66-book corpus, at rest and under keystroke churn — broken down by owner.
//!
//! ```text
//! cargo run -p usfm_galley --release --example working_set
//! SOUS_WARMER_MIB=16 cargo run -p usfm_galley --release --example working_set
//! ```
//!
//! A counting `#[global_allocator]` wraps `std::alloc::System` so every
//! number below is the allocator's OWN ledger, not an estimate derived from
//! struct sizes: live bytes (heap in use right now), peak live bytes (the
//! high-water mark — what matters for wasm, whose linear memory never
//! shrinks), and allocation call counts. Those are then set beside what the
//! public API itself reports (`Pantry::resident_bytes`, `Pantry::text_bytes`,
//! `Expediter::resident_bytes`, `Warmer::resident_bytes`) — now exact, not
//! residuals — to see what the host's own bookkeeping does and does not
//! account for.
//!
//! `SOUS_WARMER_MIB` sets the Warmer's byte budget (default 64); the corpus
//! runs once per budget so the report below can be compared across sizes.
//!
//! Corpus and loading: `en_ulb`, the same 66-book fixture
//! `galley/benches/expediter.rs` uses. Keystroke simulation follows the same
//! recipe as that bench (insert one character into a middle chapter's first
//! text token) but cycles across ten books, not only MRK, so the churn isn't
//! one book's story. Each keystroke's `update` + `publish` is timed with
//! `Instant`; the report is the median of the 200.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;
use sous_core::Brigade;
use usfm_galley::sous::Expediter;
use usfm_galley::{Role, onion};

// ------------------------------------------------------------- the allocator

/// Wraps `System`, tracking live bytes, the all-time peak, and call counts —
/// entirely in atomics so it is sound as a `#[global_allocator]`.
struct CountingAlloc;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            if new_size >= layout.size() {
                let grown = new_size - layout.size();
                let live = LIVE.fetch_add(grown, Ordering::Relaxed) + grown;
                PEAK.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new_ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

fn live_bytes() -> usize {
    LIVE.load(Ordering::Relaxed)
}
fn peak_bytes() -> usize {
    PEAK.load(Ordering::Relaxed)
}
fn alloc_calls() -> usize {
    ALLOCS.load(Ordering::Relaxed)
}

fn fmt_bytes(bytes: usize) -> String {
    format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
}

// -------------------------------------------------------------- the corpus

const CORPUS_DIR: &str = "en_ulb";

fn load_corpus() -> Vec<(String, String)> {
    let dir = format!(
        "{}/../testData/exampleCorpora/{CORPUS_DIR}",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "usfm"))
        .collect();
    paths.sort();
    let books: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    assert_eq!(books.len(), 66, "{dir} is a whole-Bible corpus");
    books
}

/// A char-boundary offset inside a middle chapter's first nonempty text
/// token — the same spot `galley/benches/expediter.rs` types into for MRK,
/// generalized to any book so the churn below can cycle across several.
fn edit_point(text: &str) -> usize {
    let starts = onion::chunk::pre_scan(text.as_bytes()).starts;
    let chunk = if starts.len() > 1 {
        starts.len() / 2
    } else {
        0
    };
    let from = starts[chunk];
    let to = starts.get(chunk + 1).copied().unwrap_or(text.len() as u32);
    let tokens = onion::lex(text);
    tokens
        .iter()
        .find(|token| {
            token.kind() == onion::TokenKind::Text
                && token.start >= from
                && token.start < to
                && token.len > 0
        })
        // Small books (front matter plus one chapter) may not have a hit in
        // the chosen window; fall back to the first text token at all.
        .or_else(|| {
            tokens
                .iter()
                .find(|token| token.kind() == onion::TokenKind::Text && token.len > 0)
        })
        .map(|token| token.start as usize)
        .unwrap_or(text.len())
}

// ------------------------------------------------------------ the checkpoints

struct Checkpoint {
    label: String,
    live: usize,
    peak: usize,
    allocs: usize,
}

fn snapshot(label: impl Into<String>) -> Checkpoint {
    Checkpoint {
        label: label.into(),
        live: live_bytes(),
        peak: peak_bytes(),
        allocs: alloc_calls(),
    }
}

fn print_row(row: &Checkpoint, extra: &str) {
    println!(
        "{:<34} {:>12} {:>12} {:>10}  {extra}",
        row.label,
        fmt_bytes(row.live),
        fmt_bytes(row.peak),
        row.allocs,
    );
}

fn header() {
    println!(
        "{:<34} {:>12} {:>12} {:>10}  notes",
        "phase", "live", "peak", "allocs"
    );
    println!("{}", "-".repeat(100));
}

/// `SOUS_WARMER_MIB`, MiB, default 64.
fn warmer_budget_bytes() -> usize {
    let mib = std::env::var("SOUS_WARMER_MIB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64);
    mib << 20
}

/// The middle of a sorted duration slice — sorts a clone, leaves the input.
fn median(durations: &[Duration]) -> Duration {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn main() {
    header();
    let budget = warmer_budget_bytes();
    println!("Warmer budget: {}\n", fmt_bytes(budget));

    // ---- 1. baseline: 66 raw texts loaded into Strings -------------------
    let corpus = load_corpus();
    let raw_total: usize = corpus.iter().map(|(_, text)| text.len()).sum();
    let after_load = snapshot("1. 66 raw texts loaded");
    print_row(
        &after_load,
        &format!("raw text total = {}", fmt_bytes(raw_total)),
    );

    // Ten books spread across the corpus, so the churn below is not just one
    // book's story. Each keeps its own growing copy, separate from the
    // Pantry's retained one, so the harness can insert without re-reading.
    let churn_books: Vec<usize> = (0..10).map(|i| i * corpus.len() / 10).collect();
    let edit_points: FxHashMap<usize, usize> = churn_books
        .iter()
        .map(|&i| (i, edit_point(&corpus[i].1)))
        .collect();
    let mut live_texts: FxHashMap<usize, String> = churn_books
        .iter()
        .map(|&i| (i, corpus[i].1.clone()))
        .collect();

    // ---- 2. register all 66 as Role::Target -------------------------------
    let mut sous = Expediter::new(Brigade::default(), budget);
    for (id, text) in &corpus {
        sous.update(id.as_str(), Role::Target, text).unwrap();
    }
    let after_register = snapshot("2. all 66 registered (Target)");
    print_row(
        &after_register,
        &format!(
            "resident_bytes={} (no chapter mapped yet)",
            fmt_bytes(sous.resident_bytes())
        ),
    );

    // ---- 3. first publish (cold) -------------------------------------------
    let wire_cold = sous.publish().unwrap();
    let after_cold = snapshot("3. first publish (cold)");
    print_row(
        &after_cold,
        &format!(
            "wire={} resident_bytes={} last_mapped={}",
            fmt_bytes(wire_cold.len()),
            fmt_bytes(sous.resident_bytes()),
            sous.last_mapped(),
        ),
    );
    let cold_wire_len = wire_cold.len();
    drop(wire_cold);

    // ---- 4. second publish, nothing changed --------------------------------
    let wire_warm = sous.publish().unwrap();
    let after_warm = snapshot("4. second publish (unchanged)");
    print_row(
        &after_warm,
        &format!(
            "wire={} last_mapped={}",
            fmt_bytes(wire_warm.len()),
            sous.last_mapped()
        ),
    );
    assert_eq!(
        wire_warm.len(),
        cold_wire_len,
        "unchanged republish, same wire"
    );
    drop(wire_warm);

    // ---- 5. 200 keystrokes across 10 books, publishing after each ---------
    let mut prev_step = 0usize;
    let mut allocs_at_prev = alloc_calls();
    let mut keystroke_times: Vec<Duration> = Vec::with_capacity(200);
    for step in 1..=200usize {
        let index = churn_books[(step - 1) % churn_books.len()];
        let text = live_texts.get_mut(&index).unwrap();
        let at = edit_points[&index].min(text.len());
        text.insert(at, 'x');
        let started = Instant::now();
        sous.update(corpus[index].0.as_str(), Role::Target, text.as_str())
            .unwrap();
        sous.publish().unwrap();
        keystroke_times.push(started.elapsed());

        if step == 50 || step == 100 || step == 200 {
            let checkpoint = snapshot(format!("5. after {step} keystrokes+publishes"));
            let allocs_in_interval = checkpoint.allocs - allocs_at_prev;
            let keystrokes_in_interval = step - prev_step;
            print_row(
                &checkpoint,
                &format!(
                    "resident_bytes={} avg allocs/(keystroke+publish) over [{prev_step},{step}] = {:.0} warmer hits/misses={}/{}",
                    fmt_bytes(sous.resident_bytes()),
                    allocs_in_interval as f64 / keystrokes_in_interval as f64,
                    sous.pantry().warmer().hits(),
                    sous.pantry().warmer().misses(),
                ),
            );
            prev_step = step;
            allocs_at_prev = checkpoint.allocs;
        }
    }

    // -------------------------------------------------------- the breakdown
    //
    // Every owner below is now a DIRECT accessor, not a residual:
    // `Pantry::text_bytes()` is the retained text alone, `Warmer::
    // resident_bytes()` is the exact heap of every retained chunk's CST,
    // tokens and lint report (`Fingerprint::resident_bytes` is public too,
    // folded into the Warmer/Pantry split below rather than read on its
    // own). What's left of `Pantry::resident_bytes()` once those two are
    // subtracted is the Pantry's own per-book products (`Toc` + `Mask` +
    // `Utf16Table` + struct overhead) — still a subtraction, but of two
    // exact numbers rather than an estimate standing in for one.
    let text_bytes = sous.pantry().text_bytes();
    let warmer_bytes = sous.pantry().warmer().resident_bytes();
    let pantry_bytes = sous.pantry().resident_bytes();
    let pantry_products = pantry_bytes
        .saturating_sub(warmer_bytes)
        .saturating_sub(text_bytes);
    let expediter_bytes = sous.resident_bytes();
    let sous_cache = expediter_bytes.saturating_sub(pantry_bytes);
    let (warmer_hits, warmer_misses) = (
        sous.pantry().warmer().hits(),
        sous.pantry().warmer().misses(),
    );
    let keystroke_median = median(&keystroke_times);

    // The harness's own bookkeeping is on the same heap as the host's: the
    // original 66 texts (`corpus`, held for the whole run) and the ten
    // growing edited copies (`live_texts`) are ours, not the Expediter's.
    // Excluded here so "unattributed" is not just our own test fixture.
    let harness_bytes = raw_total + live_texts.values().map(String::len).sum::<usize>();
    let live_now = live_bytes();
    let peak_now = peak_bytes();
    let host_live = live_now.saturating_sub(harness_bytes);
    let accounted = text_bytes + pantry_products + warmer_bytes + sous_cache;
    let unattributed = host_live.saturating_sub(accounted);

    println!();
    println!(
        "breakdown after 200 keystrokes, by owner (host-only; harness's own corpus copies excluded):"
    );
    println!("{:<58} {:>12}", "owner", "bytes");
    println!("{}", "-".repeat(72));
    println!(
        "{:<58} {:>12}",
        "Pantry retained text (66 books, post-churn)",
        fmt_bytes(text_bytes)
    );
    println!(
        "{:<58} {:>12}",
        "Pantry products (mask+toc+utf16+fingerprint)",
        fmt_bytes(pantry_products)
    );
    println!(
        "{:<58} {:>12}",
        "Warmer / LRU chunk cache (Pantry::warmer().resident_bytes)",
        fmt_bytes(warmer_bytes)
    );
    println!(
        "{:<58} {:>12}",
        "Sous cache (observations+tables+aggregates+rings)",
        fmt_bytes(sous_cache)
    );
    println!(
        "{:<58} {:>12}",
        "  = Expediter::resident_bytes() total",
        fmt_bytes(expediter_bytes)
    );
    println!("{}", "-".repeat(72));
    println!(
        "{:<58} {:>12}",
        "allocator live heap, host-only (ground truth)",
        fmt_bytes(host_live)
    );
    println!(
        "{:<58} {:>12}",
        "allocator PEAK live heap, whole run (ground truth)",
        fmt_bytes(peak_now)
    );
    println!(
        "{:<58} {:>12}",
        "unattributed (host-only live minus the four owners above)",
        fmt_bytes(unattributed)
    );
    println!(
        "  ({} / {} declared vs. accounted; {:.1}% of host-only live heap is unattributed)",
        fmt_bytes(accounted),
        fmt_bytes(host_live),
        100.0 * unattributed as f64 / host_live.max(1) as f64,
    );
    println!();
    println!(
        "Warmer hits={warmer_hits} misses={warmer_misses} (of {} lookups)",
        warmer_hits + warmer_misses
    );
    println!(
        "keystroke (update+publish) median over 200: {:.1} µs",
        keystroke_median.as_secs_f64() * 1e6
    );

    // ---- 7. what resident_bytes() misses: an isolated construction check --
    //
    // An empty Expediter allocates nothing (its maps are `FxHashMap::default()`,
    // which does not reserve capacity until first insert), so its own
    // `resident_bytes()` and the allocator's delta should both read zero —
    // proving the gap above is about MISSING ACCESSORS on a populated host,
    // not about the ledgers disagreeing on an empty one.
    let before_isolated = live_bytes();
    let empty = Expediter::new(Brigade::default(), 64 << 20);
    let after_isolated = live_bytes();
    println!();
    println!(
        "isolated check — empty Expediter::new(): resident_bytes()={}, allocator delta={}",
        empty.resident_bytes(),
        after_isolated.saturating_sub(before_isolated),
    );
    drop(empty);

    println!();
    println!(
        "resident_observations={} resident_tables={} last_folded={}",
        sous.resident_observations(),
        sous.resident_tables(),
        sous.last_folded(),
    );
}
