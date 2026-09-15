//! Emit the checked-in Sous TypeScript corpus reader from the Rust schema.
//!
//! Two outputs, one generator: `sous-chef/reader.ts` is the reader's home, and
//! `galley/sous-reader.ts` is the copy the `usfm-galley` package exports —
//! a package cannot export a path above its own directory.

use std::path::Path;

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reader = sous_core::generated_reader_ts();
    std::fs::write(root.join("sous-chef/reader.ts"), &reader)?;
    std::fs::write(root.join("galley/sous-reader.ts"), &reader)
}
