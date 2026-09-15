//! The generated Unicode tables are committed artifacts: a fresh run of
//! `gen-unicode` must reproduce them byte for byte.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn a_second_generator_run_reproduces_the_committed_tables() {
    let scratch = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let table = scratch.join("regenerated-table.rs");
    let pools = scratch.join("regenerated-pools.rs");
    let status = Command::new(env!("CARGO_BIN_EXE_gen-unicode"))
        .arg(&table)
        .arg(&pools)
        .status()
        .expect("the generator binary runs");
    assert!(status.success(), "gen-unicode exited with {status}");

    // The class table's home is `mise`; the pools stay with the rules that
    // read them.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_reproduces(&table, &manifest.join("../../mise/src/unicode/table.rs"));
    assert_reproduces(&pools, &manifest.join("src/unicode/pools.rs"));
}

fn assert_reproduces(regenerated: &PathBuf, committed: &PathBuf) {
    let fresh = std::fs::read(regenerated).expect("the generator wrote its output");
    let held = std::fs::read(committed).unwrap_or_else(|error| {
        panic!("committed {} must be present: {error}", committed.display())
    });
    assert_eq!(
        fresh.len(),
        held.len(),
        "regenerated {} is {} bytes, committed is {}; run \
         `cargo run -p sous-core --bin gen-unicode`",
        committed.display(),
        fresh.len(),
        held.len(),
    );
    assert_eq!(
        fresh.iter().zip(&held).position(|(a, b)| a != b),
        None,
        "regenerated {} diverges; run `cargo run -p sous-core --bin gen-unicode`",
        committed.display(),
    );
}
