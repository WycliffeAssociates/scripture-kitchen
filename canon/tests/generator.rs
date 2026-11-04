//! The generated tables are committed: a fresh `gen-canon` run must reproduce
//! both byte for byte. When this fails, run `cargo run -p canon --bin gen-canon`.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn a_second_generator_run_reproduces_the_committed_tables() {
    let scratch = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let rust = scratch.join("canon-tables.rs");
    let ts = scratch.join("canon-tables.ts");
    let status = Command::new(env!("CARGO_BIN_EXE_gen-canon"))
        .arg(&rust)
        .arg(&ts)
        .status()
        .expect("the generator binary runs");
    assert!(status.success(), "gen-canon exited with {status}");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_reproduces(&rust, &manifest.join("src/tables.rs"));
    assert_reproduces(&ts, &manifest.join("ts/tables.ts"));
}

fn assert_reproduces(fresh: &Path, committed: &Path) {
    let fresh = std::fs::read_to_string(fresh).expect("the generator wrote its output");
    let committed = std::fs::read_to_string(committed).expect("the committed table exists");
    assert!(
        fresh == committed,
        "a table is stale; run `cargo run -p canon --bin gen-canon`"
    );
}
