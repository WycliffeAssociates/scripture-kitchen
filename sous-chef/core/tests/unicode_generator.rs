//! The generated Unicode table is a committed artifact: a fresh run of
//! `gen-unicode` must reproduce it byte for byte.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn a_second_generator_run_reproduces_the_committed_table() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("regenerated-table.rs");
    let status = Command::new(env!("CARGO_BIN_EXE_gen-unicode"))
        .arg(&out)
        .status()
        .expect("the generator binary runs");
    assert!(status.success(), "gen-unicode exited with {status}");

    let regenerated = std::fs::read(&out).expect("the generator wrote its output");
    let committed = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/unicode/table.rs"),
    )
    .expect("the committed table is present");
    assert_eq!(
        regenerated.len(),
        committed.len(),
        "regenerated table is {} bytes, committed is {}; run \
         `cargo run -p sous-core --bin gen-unicode`",
        regenerated.len(),
        committed.len()
    );
    let first_difference = regenerated
        .iter()
        .zip(&committed)
        .position(|(a, b)| a != b);
    assert_eq!(
        first_difference, None,
        "regenerated table diverges; run `cargo run -p sous-core --bin gen-unicode`"
    );
}
