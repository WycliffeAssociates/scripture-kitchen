use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("sous-cli-process-{suffix}-{id}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn stats_only_suppresses_debug_output_and_implies_stats() {
    let temp = TempDir::new();
    let book = temp.0.join("MRK.usfm");
    fs::write(&book, "\\id MRK\n\\c 1\n\\p\n\\v 1 Mark.\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_sous"))
        .arg("--stats-only")
        .arg(&book)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("stats: mode=serial target_files=1 "));
}
