//! The detached UTF-16 table against Onion's borrowing index, over the
//! committed 8-corpus test tier.
//!
//! ```text
//! for every character boundary of every corpus:
//!     Utf16Table::to_utf16(byte) == Utf16Index::to_utf16(byte)
//! and the table costs ≤ 25% of the raw bytes it replaces.
//! ```
//!
//! Byte source: the committed test tier only. A missing file is a loud
//! failure, never a silent skip.

use usfm_galley::onion::utf16::utf16_index;
use usfm_galley::utf16_table;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../sous-chef/corpora/");
const FILES: [&str; 8] = [
    "WA-en-ulb.txt",
    "amh.txt",
    "francl.txt",
    "grcsr.txt",
    "hin2017.txt",
    "nya.txt",
    "spaRV1909.txt",
    "swhulb.txt",
];

fn corpus(name: &str) -> String {
    let path = format!("{ROOT}{name}");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("test-tier corpus {path} must be present: {error}"))
}

/// The table's whole reason to exist: the same answers with no source bytes.
#[test]
#[ignore = "exhaustive oracle: every character boundary of ~35 MB — 21 s debug, 0.6 s release"]
fn the_table_equals_onions_index_at_every_boundary_of_the_tier() {
    for name in FILES {
        let text = corpus(name);
        let table = utf16_table(text.as_bytes());
        let index = utf16_index(text.as_bytes());
        assert_eq!(table.len_utf16(), index.len_utf16(), "{name} total");
        for (byte, _) in text.char_indices() {
            let byte = byte as u32;
            assert_eq!(table.to_utf16(byte), index.to_utf16(byte), "{name} @{byte}");
        }
        let end = text.len() as u32;
        assert_eq!(table.to_utf16(end), index.to_utf16(end), "{name} @end");
    }
}

/// The size claim the retention budget rests on.
#[test]
fn the_table_costs_at_most_a_quarter_of_the_raw_bytes() {
    let mut report = Vec::new();
    for name in FILES {
        let text = corpus(name);
        let table = utf16_table(text.as_bytes());
        let ratio = table.index_bytes() as f64 / text.len() as f64;
        report.push(format!("{name}: {:.2}%", ratio * 100.0));
        assert!(
            table.index_bytes() * 4 <= text.len(),
            "{name}: {} table bytes for {} of source",
            table.index_bytes(),
            text.len()
        );
    }
    println!("detached UTF-16 table size — {}", report.join(", "));
}
