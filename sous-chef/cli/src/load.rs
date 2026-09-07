//! Filesystem discovery: what counts as a book, and what a directory yields.
//!
//! ```text
//! paths_for_input("corpus/")  -> [corpus/41-MAT.usfm, corpus/42-MRK.usfm, ..]
//! ```

use crate::*;

pub(crate) struct LoadedBooks {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) books: Vec<OnionBook>,
    /// Raw book strings, kept so publication can rebase against them.
    pub(crate) sources: Vec<String>,
    pub(crate) source_bytes: usize,
}

pub(crate) fn load_input(
    input: &Path,
    parallel: bool,
) -> Result<LoadedBooks, Box<dyn std::error::Error>> {
    let paths = paths_for_input(input)?;
    let loaded = if parallel {
        paths
            .par_iter()
            .map(|path| load_book(path))
            .collect::<Result<Vec<_>, _>>()
    } else {
        paths
            .iter()
            .map(|path| load_book(path))
            .collect::<Result<Vec<_>, _>>()
    }
    .map_err(std::io::Error::other)?;
    let source_bytes = loaded.iter().map(|(_, source)| source.len()).sum();
    let (books, sources) = loaded.into_iter().unzip();
    Ok(LoadedBooks {
        paths,
        books,
        sources,
        source_bytes,
    })
}

pub(crate) fn load_book(path: &Path) -> Result<(OnionBook, String), String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let book = OnionBook::parse(&source)
        .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
    Ok((book, source))
}

pub(crate) fn paths_for_input(input: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(input)
        .map_err(|error| format!("cannot inspect {}: {error}", input.display()))?;
    if metadata.is_file() {
        if !supported_extension(input) {
            return Err(format!(
                "unsupported input file {}; expected .sfm or .usfm",
                input.display()
            )
            .into());
        }
        return Ok(vec![input.to_path_buf()]);
    }
    if !metadata.is_dir() {
        return Err(format!(
            "{} is neither a regular file nor a directory",
            input.display()
        )
        .into());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(input)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if supported_extension(&path) {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(format!(
            "no .sfm or .usfm files found directly in {}",
            input.display()
        )
        .into());
    }
    Ok(paths)
}

pub(crate) fn supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("sfm") || extension.eq_ignore_ascii_case("usfm")
        })
}
