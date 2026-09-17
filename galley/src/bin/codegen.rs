//! Generator: galley's two wire schemas → both ends of each. Run with
//! `cargo run -p usfm_galley --bin codegen`; the output is CHECKED IN
//! (reviewable diffs; consumers never run this).
//!
//! A thin main: each generator is in the library — `toc::emit` and
//! `find::wire::emit` — so
//! `tests/codegen_output_matches_input.rs` can regenerate to a buffer and fail
//! the build when a checked-in file is stale.

use std::path::Path;

use usfm_galley::find::wire::emit as find_emit;
use usfm_galley::toc::emit as toc_emit;

// Anchored to the crate, not the shell: the same two files are written from
// the workspace root, from galley/, or from anywhere else.
const CRATE: &str = env!("CARGO_MANIFEST_DIR");
const CENSUS_WRITER: &str = "src/toc/generated.rs";
const CENSUS_READER: &str = "toc-reader.ts";
const FIND_WRITER: &str = "src/find/wire/generated.rs";
const FIND_READER: &str = "find-reader.ts";

fn main() -> std::io::Result<()> {
    for (path, fresh) in [
        (CENSUS_WRITER, toc_emit::generated_rs()),
        (CENSUS_READER, toc_emit::reader_ts()),
        (FIND_WRITER, find_emit::generated_rs()),
        (FIND_READER, find_emit::reader_ts()),
    ] {
        let path = Path::new(CRATE).join(path);
        let previous = std::fs::read_to_string(&path).unwrap_or_default();
        std::fs::write(&path, &fresh)?;
        println!(
            "{}: {}",
            path.display(),
            if previous == fresh {
                "unchanged"
            } else {
                "written"
            }
        );
    }
    Ok(())
}
