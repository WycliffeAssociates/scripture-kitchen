//! The first walking consumer for Sous Chef.
//!
//! Output is deliberately a debug view while the finding contract is being
//! frozen. The typed CLI declaration can grow with the executable instead of
//! being replaced after the engine is usable.

mod onion_book;

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use onion_book::OnionBook;
use sous_core::{Corpus, ProjectedBook};
use usage::Cli;

/// Inspect the projected scripture text Sous Chef will analyze.
#[derive(Cli)]
#[usage(bin = "sous", version = "0.1.0")]
struct Args {
    /// Directory whose immediate .sfm and .usfm files are projected and inspected.
    directory: PathBuf,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sous: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let paths = discover_paths(&args.directory)?;
    let books: Vec<_> = paths
        .iter()
        .map(|path| {
            let source = fs::read_to_string(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            OnionBook::parse(&source)
                .map_err(|error| format!("cannot parse {}: {error}", path.display()))
        })
        .collect::<Result<_, String>>()
        .map_err(std::io::Error::other)?;
    let corpus = Corpus::try_new(&books).map_err(|error| format!("invalid corpus: {error}"))?;

    for (index, book) in corpus.iter() {
        let path = &paths[index.get() as usize];
        let chapters: Vec<_> = book.chapters().collect();
        let verses: Vec<_> = book.verses().collect();
        println!(
            "book[{}] {} -> {} (projected bytes: {}, chapters: {}, verses: {})",
            index.get(),
            book.key(),
            path.display(),
            book.text().len(),
            chapters.len(),
            verses.len()
        );
        println!("  chapters: {chapters:#?}");
        println!("  verses: {verses:#?}");
        if let Some(first_verse) = verses.first()
            && let Some(located) = book.locate(first_verse.text())
        {
            let first = located.first;
            let last = located.last;
            let source_spans: Vec<_> = located.spans.collect();
            println!("  first verse source: {first:?}..{last:?} {source_spans:?}");
        }
    }
    Ok(())
}

fn discover_paths(directory: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    if !directory.is_dir() {
        return Err(format!("{} is not a directory", directory.display()).into());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };
        if extension.eq_ignore_ascii_case("sfm") || extension.eq_ignore_ascii_case("usfm") {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(format!(
            "no .sfm or .usfm files found directly in {}",
            directory.display()
        )
        .into());
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
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
            let path = std::env::temp_dir().join(format!("sous-cli-{suffix}-{id}"));
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
    fn discovery_filters_immediate_usfm_extensions_and_sorts_paths() {
        let temp = TempDir::new();
        fs::write(temp.0.join("b.USFM"), "").unwrap();
        fs::write(temp.0.join("a.sfm"), "").unwrap();
        fs::write(temp.0.join("ignore.txt"), "").unwrap();
        fs::create_dir(temp.0.join("nested.usfm")).unwrap();
        fs::write(temp.0.join("nested.usfm").join("c.usfm"), "").unwrap();

        let paths = discover_paths(&temp.0).unwrap();
        let names: Vec<_> = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["a.sfm", "b.USFM"]);
    }

    #[test]
    fn discovery_rejects_non_directory_and_empty_directory() {
        let temp = TempDir::new();
        assert!(discover_paths(&temp.0.join("missing")).is_err());
        assert!(discover_paths(&temp.0).is_err());
    }
}
