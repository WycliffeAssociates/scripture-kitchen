//! canon's generated tables are committed: a fresh `gen-canon` run must
//! reproduce all four byte for byte. When this fails, run `cargo run -p canon-gen`.

use std::path::{Path, PathBuf};
use std::process::Command;

const TABLES: [&str; 4] = [
    "src/tables.rs",
    "ts/tables.ts",
    "src/scheme_tables.rs",
    "ts/scheme-tables.ts",
];

#[test]
fn a_second_generator_run_reproduces_the_committed_tables() {
    let scratch = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("canon-gen");
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let status = Command::new(env!("CARGO_BIN_EXE_gen-canon"))
        .arg(&scratch)
        .status()
        .expect("the generator binary runs");
    assert!(status.success(), "gen-canon exited with {status}");

    let canon = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    for table in TABLES {
        let name = table.rsplit('/').next().unwrap();
        assert_reproduces(&scratch.join(name), &canon.join(table));
    }
}

fn assert_reproduces(fresh: &Path, committed: &Path) {
    let fresh_text = std::fs::read_to_string(fresh).expect("the generator wrote its output");
    let committed_text = std::fs::read_to_string(committed).expect("the committed table exists");
    assert!(
        fresh_text == committed_text,
        "{} is stale; run `cargo run -p canon-gen`",
        committed.display()
    );
}
