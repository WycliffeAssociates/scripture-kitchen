//! What matching one Bible's formatting to another costs.
//!
//!     cargo bench -p usfm_galley --bench overlay
//!
//! The real pair the feature exists for: `bdf_reg` (Bafut, 27 NT books, verse
//! markers and little else) as the TARGET, `en_ulb` as the declared SOURCE,
//! registered with `Retain::Text` because a skeleton is copied out of the
//! source's own bytes. Both are read from `testData/exampleCorpora/`, by hand
//! and by name; an absent corpus panics rather than skipping, which is why
//! nothing here is a `#[test]`.
//!
//! Registration is OUTSIDE the timer — what is measured is the skeleton walk,
//! the pairing, and the diff over the Pantry's warm products.
//!
//! `node` is the hover row: `targetNodeFor` re-extracts both skeletons and
//! re-pairs on every call, so this is what a host would pay per mouse move if
//! nothing were cached. Nothing is cached in this slice.

use std::sync::LazyLock;

use divan::counter::BytesCount;
use usfm_galley::overlay::{self, BlockAddress, OverlayOptions, Placement};
use usfm_galley::{BookId, Pantry, Retain, Role, SourceLanes};

fn main() {
    divan::main()
}

/// The book both sides are largest in, and the one a single-book row uses.
const ONE: &str = "41-MAT.usfm";

/// `(file name, target text, source text)` for every NT book present in BOTH
/// corpora, in canonical file order.
static PAIRS: LazyLock<Vec<(String, String, String)>> = LazyLock::new(|| {
    let root = format!("{}/../testData/exampleCorpora", env!("CARGO_MANIFEST_DIR"));
    let target_dir = format!("{root}/bdf_reg");
    let source_dir = format!("{root}/en_ulb");
    let mut names: Vec<String> = std::fs::read_dir(&target_dir)
        .unwrap_or_else(|error| panic!("{target_dir}: {error}"))
        .map(|entry| entry.expect("readable entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".usfm"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 27, "bdf_reg is the 27-book New Testament");
    names
        .into_iter()
        .map(|name| {
            let read = |dir: &str| {
                let path = format!("{dir}/{name}");
                std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
            };
            let target = read(&target_dir);
            let source = read(&source_dir);
            (name, target, source)
        })
        .collect()
});

fn target_id(name: &str) -> BookId {
    BookId::from(format!("bdf/{name}"))
}

fn source_id(name: &str) -> BookId {
    BookId::from(format!("ulb/{name}"))
}

/// The named books registered: targets keep their text, sources keep theirs
/// too because a skeleton is read out of them.
fn pantry(books: &[&(String, String, String)]) -> Pantry {
    let mut pantry = Pantry::new(512 << 20);
    for (name, target, source) in books {
        pantry
            .update(target_id(name), Role::Target, target)
            .unwrap_or_else(|error| panic!("{name} target: {error}"));
        pantry
            .update_with(
                source_id(name),
                Role::Reference,
                Retain::Text,
                SourceLanes::Lengths,
                source,
            )
            .unwrap_or_else(|error| panic!("{name} source: {error}"));
    }
    pantry
}

fn one() -> Vec<&'static (String, String, String)> {
    vec![
        PAIRS
            .iter()
            .find(|(name, _, _)| name == ONE)
            .unwrap_or_else(|| panic!("{ONE} is in both corpora")),
    ]
}

/// One book's skeleton, per side: the walk alone, no pairing and no diff.
#[divan::bench(args = ["target", "source"])]
fn skeleton(bencher: divan::Bencher, side: &str) {
    let books = one();
    let (name, target, source) = books[0];
    let (id, bytes) = match side {
        "target" => (target_id(name), target.len()),
        _ => (source_id(name), source.len()),
    };
    bencher
        .with_inputs(|| pantry(&books))
        .counter(BytesCount::new(bytes))
        .bench_local_values(|mut pantry| {
            overlay::skeleton(&mut pantry, &id)
                .expect("a registered book has a skeleton")
                .blocks
                .len()
        });
}

/// One book, end to end: both skeletons, the pairing, the diff.
#[divan::bench]
fn one_book(bencher: divan::Bencher) {
    let books = one();
    let (name, target, source) = books[0];
    let (target_id, source_id) = (target_id(name), source_id(name));
    bencher
        .with_inputs(|| pantry(&books))
        .counter(BytesCount::new(target.len() + source.len()))
        .bench_local_values(|mut pantry| {
            overlay::overlay(
                &mut pantry,
                &target_id,
                &source_id,
                &OverlayOptions::default(),
            )
            .expect("a paired book")
            .edits
            .len()
        });
}

/// The whole New Testament, 27 books, one overlay each.
#[divan::bench]
fn new_testament(bencher: divan::Bencher) {
    let books: Vec<&(String, String, String)> = PAIRS.iter().collect();
    let bytes: usize = books
        .iter()
        .map(|(_, target, source)| target.len() + source.len())
        .sum();
    bencher
        .with_inputs(|| pantry(&books))
        .counter(BytesCount::new(bytes))
        .bench_local_values(|mut pantry| {
            let mut edits = 0;
            for (name, _, _) in &books {
                edits += overlay::overlay(
                    &mut pantry,
                    &target_id(name),
                    &source_id(name),
                    &OverlayOptions::default(),
                )
                .expect("a paired book")
                .edits
                .len();
            }
            edits
        });
}

/// The same, read as the report a host would show.
#[divan::bench]
fn new_testament_report(bencher: divan::Bencher) {
    let books: Vec<&(String, String, String)> = PAIRS.iter().collect();
    let bytes: usize = books
        .iter()
        .map(|(_, target, source)| target.len() + source.len())
        .sum();
    bencher
        .with_inputs(|| pantry(&books))
        .counter(BytesCount::new(bytes))
        .bench_local_values(|mut pantry| {
            let mut rows = 0;
            for (name, _, _) in &books {
                let report = overlay::overlay(
                    &mut pantry,
                    &target_id(name),
                    &source_id(name),
                    &OverlayOptions::default(),
                )
                .expect("a paired book")
                .report;
                rows += report.inserted.len()
                    + report.removed.len()
                    + report.collapsed.len()
                    + report.unpaired.len();
            }
            rows
        });
}

/// One address, the hover path: two skeleton walks and a pairing per call.
#[divan::bench]
fn node(bencher: divan::Bencher) {
    let books = one();
    let (name, _, _) = books[0];
    let (target_id, source_id) = (target_id(name), source_id(name));
    let address = BlockAddress {
        sid: "MAT 5:3".into(),
        placement: Placement::Leading,
        ordinal: 1,
        marker: "q".into(),
    };
    bencher
        .with_inputs(|| pantry(&books))
        .bench_local_values(|mut pantry| {
            overlay::target_node_for(
                &mut pantry,
                &target_id,
                &source_id,
                &address,
                &OverlayOptions::default(),
            )
            .expect("an answer")
        });
}

/// Not a timing: what the overlay PROPOSES over the whole NT, printed once,
/// so the medians above have something to be medians of.
#[divan::bench(sample_count = 1, sample_size = 1)]
fn nt_totals(bencher: divan::Bencher) {
    let books: Vec<&(String, String, String)> = PAIRS.iter().collect();
    bencher
        .with_inputs(|| pantry(&books))
        .bench_local_values(|mut pantry: Pantry| {
            let (mut edits, mut inserted, mut removed, mut collapsed, mut unpaired) =
                (0, 0, 0, 0, 0);
            for (name, _, _) in &books {
                let out = overlay::overlay(
                    &mut pantry,
                    &target_id(name),
                    &source_id(name),
                    &OverlayOptions::default(),
                )
                .expect("a paired book");
                edits += out.edits.len();
                inserted += out.report.inserted.len();
                removed += out.report.removed.len();
                collapsed += out.report.collapsed.len();
                unpaired += out.report.unpaired.len();
            }
            println!(
                "\n  NT totals: {edits} edits — inserted {inserted}, removed {removed}, \
                 collapsed {collapsed}, unpaired {unpaired}"
            );
            edits
        });
}
