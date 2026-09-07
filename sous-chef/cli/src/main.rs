//! The first walking consumer for Sous Chef.
//!
//! ```text
//! sous --findings --source source.usfm --publish out.sous --report sites.html book.usfm
//!   finding target[0] MRK 1:1-1:1 C0Control 11..14 run 3 raw [46..49]
//!   finding target[0] MRK 1:1-1:1 StrandedBackslash 17..19 run 2 raw [52..54]
//!   length target[0] MRK 1:9 ratio 0.14 z_book -8.72 z_project -6.05
//!   unpaired MRK target-only 2 source-only 0 ambiguous 0 partial-overlap 1
//!   pattern[0] U+002C ',' placement next=Digit 12/9812 0.12% band 4 · 3/66 books 11 sites
//!     site MRK 118..119
//!   published 2 findings for 1 books (SOUS v1, UTF-16) to out.sous
//!   wrote 1 patterns and 11 sites to sites.html
//! ```
//!
//! Output is a debug view while the finding contract freezes. The CLI owns
//! filesystem discovery and Onion adaptation; core owns corpus validation,
//! alignment, and the hygiene scan; galley owns UTF-16 publication.
//!
//! Finding offsets are projected UTF-8; `raw` is the retained source run set
//! behind them. The published buffer carries raw-book UTF-16 instead.
//!
//! A site is listed under its HEADLINE pattern — the finest channel it matched
//! — so a site count under a pattern row is not that row's numerator. The
//! `--report` page shows each site's reasons beside it; `report/` renders it.

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use sous_core::unicode::atoms::count_atoms;
use sous_core::words::LETTER_RUN_MAX;
use sous_core::{
    AlignedUnit, Alignment, AlignmentFact, Brigade, ChapterPass, Corpus, FindingKind,
    PackedFinding, Paired, Pattern, PatternKey, ProjectedBook, ScalarKey, SnapshotId,
    SourceLengths, SourceVerse, SourceWords, TextRange, align, analyze_paired, source_lengths,
};
use usage::Cli;
use usfm_galley::sous::{OnionBook, OnionInputBook, publish_onion_findings};

mod args;
mod lengths;
mod load;
mod patterns;
mod publishing;
mod report;
mod source;
mod stats;
#[cfg(test)]
mod tests;
mod typos;

use args::Args;
use lengths::{
    brigade_findings, copy_rows, paired_rows, presence_rows, print_length_findings, print_presence,
    print_source_copy, print_unpaired,
};
use load::{load_input, supported_extension};
use patterns::{print_findings, print_patterns};
use publishing::{print_alignment, print_books, print_source_books, publish};
use stats::OperationStats;
use typos::run_typos;

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
    let started = Instant::now();
    let target = load_input(&args.target, args.parallel)?;
    let target_corpus = Corpus::try_new(&target.books)
        .map_err(|error| format!("invalid target corpus: {error}"))?;
    if args.typos {
        return run_typos(&target_corpus);
    }
    let source_books = args
        .source
        .as_deref()
        .map(|path| source::load(path, args.parallel))
        .transpose()?;
    let source_corpus = source_books
        .as_deref()
        .map(Corpus::try_new)
        .transpose()
        .map_err(|error| format!("invalid source corpus: {error}"))?;
    let alignment = source_corpus
        .as_ref()
        .map(|source| align(&target_corpus, source));
    let source_bytes = source_books
        .as_deref()
        .map(|books| books.iter().map(|book| book.text().len()).sum());
    let stats = (args.stats || args.stats_only).then(|| {
        OperationStats::collect(
            args.parallel,
            &target_corpus,
            source_corpus.as_ref(),
            alignment.as_ref(),
            target.source_bytes,
            source_bytes,
            started,
        )
    });

    if !args.stats_only {
        print_books("target", &target.paths, &target_corpus);
        if let Some(source_corpus) = source_corpus.as_ref() {
            print_source_books(source_corpus);
        }
        if let Some(alignment) = &alignment {
            print_alignment(alignment);
        }
    }
    if args.findings || args.publish.is_some() || args.report.is_some() {
        // The declared source enters as lengths alone; the alignment above is
        // the same pairing over the same keys, kept for the facts and for the
        // texts the CLI shows beside a fired row.
        // The word lane is walked only for the lane that reads it.
        let lengths: Vec<(sous_core::BookKey, Vec<SourceVerse>, Option<SourceWords>)> =
            source_corpus
                .iter()
                .flat_map(|corpus| corpus.books())
                .map(|book| {
                    (
                        ProjectedBook::key(book),
                        source_lengths(book),
                        args.source_copy.then(|| SourceWords::of(book)),
                    )
                })
                .collect();
        let source: Vec<SourceLengths<'_>> = lengths
            .iter()
            .map(|(key, verses, words)| SourceLengths {
                book: *key,
                verses,
                words: words.as_ref(),
            })
            .collect();
        let (findings, patterns, paired) = brigade_findings(
            &target_corpus,
            &source,
            args.source_copy,
            args.source_copy_min_run,
        );
        if args.findings {
            print_findings(&target_corpus, &findings);
            if let (Some(alignment), Some(source_corpus)) = (&alignment, source_corpus.as_ref()) {
                print_length_findings(&target_corpus, source_corpus, alignment, &findings);
                print_presence(&target_corpus, &paired);
                print_source_copy(&target_corpus, &paired);
                for book in &paired.wordless {
                    println!("sourcecopy unavailable {book} (the source kept no word lane)");
                }
                print_unpaired(alignment);
            }
            print_patterns(&target_corpus, &findings, &patterns);
        }
        if let Some(path) = &args.report {
            let name = args.target.file_name().map_or_else(
                || args.target.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            let paired = alignment
                .as_ref()
                .zip(source_corpus.as_ref())
                .map(|(alignment, source)| report::Paired {
                    units: paired_rows(&target_corpus, source, alignment, &findings),
                    presence: presence_rows(&target_corpus, &paired),
                    copies: copy_rows(&target_corpus, source, &paired),
                })
                .unwrap_or_default();
            let page = report::render(&name, &target_corpus, &patterns, &findings, &paired);
            fs::write(path, &page)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            eprintln!(
                "wrote the {name} inventory ({} patterns judged) to {}",
                patterns.len(),
                path.display()
            );
        }
        if let Some(path) = &args.publish {
            let buffer = publish(&target.paths, target.sources, &findings, &patterns)?;
            fs::write(path, &buffer)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            eprintln!(
                "published {} findings for {} books (SOUS v1, UTF-16) to {}",
                findings.len(),
                target_corpus.len(),
                path.display()
            );
        }
    }
    if let Some(stats) = stats {
        stats.print();
    }
    Ok(())
}
