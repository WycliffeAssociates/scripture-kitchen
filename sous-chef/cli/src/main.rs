//! The first walking consumer for Sous Chef.
//!
//! Output is deliberately a debug view while the finding contract is being
//! frozen. The typed CLI declaration can grow with the executable instead of
//! being replaced after the engine is usable.

mod onion_book;

use std::{path::PathBuf, process::ExitCode};

use onion_book::OnionBook;
use sous_core::ProjectedBook;
use usage::Cli;

/// Inspect the projected scripture text Sous Chef will analyze.
#[derive(Cli)]
#[usage(bin = "sous", version = "0.1.0")]
struct Args {
    /// One UTF-8 USFM book to project and inspect.
    target: PathBuf,
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
    let source = std::fs::read_to_string(&args.target)?;
    let book = OnionBook::parse(&source)?;
    let chapters: Vec<_> = book.chapters().collect();
    let verses: Vec<_> = book.verses().collect();

    println!("book: {:?}", std::str::from_utf8(&book.book_code())?);
    println!("projected bytes: {}", book.text().len());
    println!("chapters: {chapters:#?}");
    println!("verses: {verses:#?}");
    if let Some(first_verse) = verses.first()
        && let Some(located) = book.locate(first_verse.text())
    {
        let first = located.first;
        let last = located.last;
        let source_spans: Vec<_> = located.spans.collect();
        println!("first verse source: {first:?}..{last:?} {source_spans:?}");
    }
    Ok(())
}
