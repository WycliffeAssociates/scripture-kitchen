//! What `Expediter::resident_bytes()` claims against what a counting
//! allocator says is actually live, for the Expediter alone.
//!
//! Before W1d, an aggregate counted `size_of::<P::Aggregate>()` and nothing
//! of the heap a pass hangs off it — 32 B for a `WordAggregate` holding
//! thousands of `WordTotal` rows (evidence.md, 2026-09-04 "the ~58 MiB").
//! `ChapterPass::aggregate_bytes` closes that; this file is the proof.
//!
//! Instrument: VOLUME — the whole test tier, `testData/exampleCorpora/en_ulb`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use sous_core::Brigade;
use usfm_galley::Role;
use usfm_galley::sous::Expediter;

// ------------------------------------------------------------- the allocator

/// Wraps `System`, tracking only live bytes — sound as a `#[global_allocator]`
/// because every count is an atomic.
struct CountingAlloc;

static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
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
            if new_size >= layout.size() {
                LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new_ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

// ------------------------------------------------------------ the corpus

/// The committed 66-book corpus the benches and `equivalence.rs` use.
fn en_ulb() -> Vec<(String, String)> {
    let dir = format!(
        "{}/../testData/exampleCorpora/en_ulb",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "usfm")
        })
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

/// `Expediter::resident_bytes()` is a claim about the Expediter's own state,
/// so the corpus loader's raw `String`s are freed inside the measured window
/// once `update` has made the Pantry's own copy — allocated and freed here,
/// they net to zero and cannot inflate the delta either way.
#[test]
fn resident_bytes_is_within_a_tenth_of_the_allocators_delta() {
    let before = LIVE.load(Ordering::Relaxed);
    let mut sous = Expediter::new(Brigade::default(), 64 << 20);
    {
        let books = en_ulb();
        for (id, text) in &books {
            sous.update(id.as_str(), Role::Target, text).unwrap();
        }
    }
    sous.publish().unwrap();
    let delta = LIVE.load(Ordering::Relaxed) - before;

    let claimed = sous.resident_bytes();
    let off_by = claimed.abs_diff(delta);
    assert!(
        off_by * 10 <= delta,
        "claimed {claimed} B against the allocator's {delta} B live, off by {}%",
        off_by * 100 / delta
    );
}
