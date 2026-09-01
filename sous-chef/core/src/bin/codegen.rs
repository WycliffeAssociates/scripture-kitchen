//! Emit the checked-in Sous TypeScript corpus reader from the Rust schema.

use std::path::Path;

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    std::fs::write(root.join("reader.ts"), sous_core::generated_reader_ts())
}
