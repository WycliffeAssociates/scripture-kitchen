//! What one whole-Bible literal find costs.
//!
//!     cargo bench -p usfm_galley --bench find
//!
//! Three needles, because the cost of a literal search is a function of the
//! needle and not only of the bytes:
//!
//! - `the` — a common English word: memmem's prefilter fires constantly and
//!   the run is dominated by the hits themselves.
//! - `Melchizedek` — rare, and long enough that the prefilter rejects almost
//!   every window. The floor for a whole-Bible scan.
//! - `और` — two Devanagari scalars, six bytes, common in `hin2017`.
//!
//! The English rows register `testData/exampleCorpora/en_ulb` (66 USFM books)
//! in a `Pantry` OUTSIDE the timer and run `Find::in_pantry`, so what is
//! measured is projection + scan + placement, not lexing.
//!
//! `corpora/hin2017.txt` is a vref-style plain text corpus (`GEN 1:1\ttext`),
//! not USFM — there is no Hindi USFM in the repo — so the Devanagari row
//! searches it under an IDENTITY mask: one range covering the whole file, so
//! projected and source offsets coincide. It measures the same scan and
//! placement over a non-Latin script; it does not measure masking.

use std::sync::LazyLock;

use divan::counter::BytesCount;
use usfm_galley::onion::Mask;
use usfm_galley::{Find, Pantry, Role};

fn main() {
    divan::main()
}

const NEEDLES: [&str; 2] = ["the", "Melchizedek"];

/// Two Devanagari scalars — `aur`, "and".
const HINDI: &str = "और";

static ENGLISH: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    let dir = format!(
        "{}/../testData/exampleCorpora/en_ulb",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .map(|entry| entry.expect("readable entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "usfm"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            (
                path.file_name()
                    .expect("named")
                    .to_string_lossy()
                    .into_owned(),
                text,
            )
        })
        .collect()
});

static HIN2017: LazyLock<String> = LazyLock::new(|| {
    let path = format!("{}/../corpora/hin2017.txt", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
});

/// The 66 books registered as targets, ready to search.
fn pantry() -> Pantry {
    let mut pantry = Pantry::new(64 << 20);
    for (id, text) in ENGLISH.iter() {
        pantry
            .update(id.as_str(), Role::Target, text)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
    }
    pantry
}

/// The whole English Bible, projected to verse text and scanned per book.
#[divan::bench(args = NEEDLES)]
fn en_ulb(bencher: divan::Bencher, needle: &str) {
    let bytes: usize = ENGLISH.iter().map(|(_, text)| text.len()).sum();
    bencher
        .with_inputs(pantry)
        .counter(BytesCount::new(bytes))
        .bench_local_values(|mut pantry| {
            Find::literal(needle)
                .in_pantry(&mut pantry, Role::Target)
                .len()
        });
}

/// The Hindi corpus under an identity mask — plain text, not USFM.
#[divan::bench]
fn hin2017(bencher: divan::Bencher) {
    let text = &*HIN2017;
    // One range over the whole file: projected and source offsets coincide.
    let mask = Mask {
        ranges: std::iter::once(0..text.len() as u32).collect(),
        starts: vec![0],
    };
    let find = Find::literal(HINDI);
    bencher
        .counter(BytesCount::new(text.len()))
        .bench(|| find.in_projection(&mask, text.as_bytes()).count());
}
