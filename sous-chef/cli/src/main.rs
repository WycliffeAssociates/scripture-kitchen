//! The first walking consumer for Sous Chef.
//!
//! ```text
//! sous --findings --publish out.sous book.usfm
//!   finding target[0] MRK 1:1-1:1 C0Control 11..14 run 3 raw [46..49]
//!   finding target[0] MRK 1:1-1:1 StrandedBackslash 17..19 run 2 raw [52..54]
//!   published 2 findings for 1 books (SOUS v1, UTF-16) to out.sous
//! ```
//!
//! Output is a debug view while the finding contract freezes. The CLI owns
//! filesystem discovery and Onion adaptation; core owns corpus validation,
//! alignment, and the hygiene scan; galley owns UTF-16 publication.
//!
//! Finding offsets are projected UTF-8; `raw` is the retained source run set
//! behind them. The published buffer carries raw-book UTF-16 instead.

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use rayon::prelude::*;
use sous_core::{
    Alignment, Corpus, PackedFinding, ProjectedBook, SnapshotId, align, analyze, hygiene::Hygiene,
};
use usage::Cli;
use usfm_galley::sous::{OnionBook, OnionInputBook, publish_onion_findings};

/// Inspect projected scripture text Sous Chef will analyze.
#[derive(Cli)]
#[usage(bin = "sous", version = "0.1.0")]
struct Args {
    /// Print aggregate input, alignment, and wall-clock throughput after the debug walk.
    #[usage(long)]
    stats: bool,

    /// Print only aggregate statistics, suppressing all book and alignment debug rows.
    #[usage(long)]
    stats_only: bool,

    /// Load books in parallel; the default loader is genuinely serial.
    #[usage(long)]
    parallel: bool,

    /// Optional source file or directory to align against the target.
    #[usage(long)]
    source: Option<PathBuf>,

    /// Print hygiene findings over the target with their raw source location.
    #[usage(long)]
    findings: bool,

    /// Write the target's findings as a complete SOUS corpus buffer in raw-book UTF-16.
    #[usage(long)]
    publish: Option<PathBuf>,

    /// One .sfm/.usfm file or a directory of immediate .sfm/.usfm files.
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
    let started = Instant::now();
    let target = load_input(&args.target, args.parallel)?;
    let target_corpus = Corpus::try_new(&target.books)
        .map_err(|error| format!("invalid target corpus: {error}"))?;
    let source = args
        .source
        .as_deref()
        .map(|path| load_input(path, args.parallel))
        .transpose()?;
    let source_corpus = source
        .as_ref()
        .map(|loaded| Corpus::try_new(&loaded.books))
        .transpose()
        .map_err(|error| format!("invalid source corpus: {error}"))?;
    let alignment = source_corpus
        .as_ref()
        .map(|source| align(&target_corpus, source));
    let stats = (args.stats || args.stats_only).then(|| {
        OperationStats::collect(
            args.parallel,
            &target_corpus,
            source_corpus.as_ref(),
            alignment.as_ref(),
            target.source_bytes,
            source.as_ref().map(|loaded| loaded.source_bytes),
            started,
        )
    });

    if !args.stats_only {
        print_books("target", &target.paths, &target_corpus);
        if let (Some(source), Some(source_corpus)) = (&source, source_corpus.as_ref()) {
            print_books("source", &source.paths, source_corpus);
        }
        if let Some(alignment) = &alignment {
            print_alignment(alignment);
        }
    }
    if args.findings || args.publish.is_some() {
        let findings = hygiene_findings(&target_corpus);
        if args.findings {
            print_findings(&target_corpus, &findings);
        }
        if let Some(path) = &args.publish {
            let buffer = publish(&target.paths, target.sources, &findings)?;
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

/// Hygiene over every target book, in projected UTF-8, ordered by book then
/// offset. Later passes join the same `analyze` call.
fn hygiene_findings(corpus: &Corpus<'_, OnionBook>) -> Vec<PackedFinding> {
    analyze(corpus, &Hygiene).into_rows()
}

fn print_findings(corpus: &Corpus<'_, OnionBook>, findings: &[PackedFinding]) {
    for finding in findings {
        let book = corpus
            .get(finding.book_idx())
            .expect("finding names a corpus book");
        let sous_core::FindingKind::Hygiene(digest) = finding.kind() else {
            continue;
        };
        let span = sous_core::TextRange::new(finding.from(), finding.to())
            .expect("packed spans are ordered");
        let (address, raw) = match book.locate(span) {
            Some(located) => {
                let first = located.first;
                let last = located.last;
                let raw: Vec<_> = located.spans.collect();
                (
                    format!(
                        "{}:{}-{}:{}",
                        first.chapter, first.first, last.chapter, last.last
                    ),
                    raw,
                )
            }
            None => ("?".to_string(), Vec::new()),
        };
        println!(
            "finding target[{}] {} {address} {} {}..{} run {} raw {raw:?}",
            finding.book_idx().get(),
            book.key(),
            digest.class().name(),
            finding.from(),
            finding.to(),
            digest.run(),
        );
    }
}

/// The snapshot identity is a Galley lifecycle decision not yet made; the
/// CLI publishes a zero identity and says so here rather than inventing one.
fn publish(
    paths: &[PathBuf],
    sources: Vec<String>,
    findings: &[PackedFinding],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let books = paths
        .iter()
        .zip(sources)
        .map(|(path, source)| OnionInputBook::new(path.display().to_string(), source))
        .collect();
    Ok(publish_onion_findings(
        books,
        findings,
        SnapshotId::new([0; 16]),
    )?)
}

fn print_books(label: &str, paths: &[PathBuf], corpus: &Corpus<'_, OnionBook>) {
    for (index, book) in corpus.iter() {
        let path = &paths[index.get() as usize];
        let chapters: Vec<_> = book.chapters().collect();
        let verses: Vec<_> = book.verses().collect();
        println!(
            "{label} book[{}] {} -> {} (projected bytes: {}, chapters: {}, verses: {})",
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
}

fn print_alignment(alignment: &Alignment) {
    for unit in alignment.units() {
        println!(
            "alignment {} {:?}: target {:?}, source {:?}",
            unit.book(),
            unit.key(),
            unit.target().ranges(),
            unit.source().ranges()
        );
    }
    for fact in alignment.facts() {
        println!("alignment fact: {fact:?}");
    }
}

struct LoadedBooks {
    paths: Vec<PathBuf>,
    books: Vec<OnionBook>,
    /// Raw book strings, kept so publication can rebase against them.
    sources: Vec<String>,
    source_bytes: usize,
}

fn load_input(input: &Path, parallel: bool) -> Result<LoadedBooks, Box<dyn std::error::Error>> {
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

fn load_book(path: &Path) -> Result<(OnionBook, String), String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let book = OnionBook::parse(&source)
        .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
    Ok((book, source))
}

fn paths_for_input(input: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
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

fn supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("sfm") || extension.eq_ignore_ascii_case("usfm")
        })
}

/// CLI instrumentation, not a benchmark contract. Timing ends before debug printing.
struct OperationStats {
    parallel: bool,
    target: CorpusStats,
    source: Option<CorpusStats>,
    aligned_units: usize,
    alignment_facts: usize,
    elapsed: Duration,
}

struct CorpusStats {
    files: usize,
    source_bytes: usize,
    projected_bytes: usize,
    chapters: usize,
    verses: usize,
    unkeyed_anchors: usize,
}

impl CorpusStats {
    fn collect(corpus: &Corpus<'_, OnionBook>, source_bytes: usize) -> Self {
        let mut projected_bytes = 0;
        let mut chapters = 0;
        let mut verses = 0;
        let mut unkeyed_anchors = 0;
        for (_, book) in corpus.iter() {
            projected_bytes += book.text().len();
            chapters += book.chapters().count();
            verses += book.verses().count();
            unkeyed_anchors += book.unkeyed_anchor_count();
        }
        Self {
            files: corpus.len(),
            source_bytes,
            projected_bytes,
            chapters,
            verses,
            unkeyed_anchors,
        }
    }
}

impl OperationStats {
    fn collect(
        parallel: bool,
        target: &Corpus<'_, OnionBook>,
        source: Option<&Corpus<'_, OnionBook>>,
        alignment: Option<&Alignment>,
        target_source_bytes: usize,
        source_source_bytes: Option<usize>,
        started: Instant,
    ) -> Self {
        Self {
            parallel,
            target: CorpusStats::collect(target, target_source_bytes),
            source: source.map(|corpus| {
                CorpusStats::collect(corpus, source_source_bytes.unwrap_or_default())
            }),
            aligned_units: alignment.map_or(0, |alignment| alignment.units().len()),
            alignment_facts: alignment.map_or(0, |alignment| alignment.facts().len()),
            elapsed: started.elapsed(),
        }
    }

    fn throughput_mib_per_second(&self) -> f64 {
        let seconds = self.elapsed.as_secs_f64();
        if seconds == 0.0 {
            return 0.0;
        }
        let bytes =
            self.target.source_bytes + self.source.as_ref().map_or(0, |source| source.source_bytes);
        bytes as f64 / (1024.0 * 1024.0) / seconds
    }

    fn print(&self) {
        let mode = if self.parallel { "parallel" } else { "serial" };
        eprintln!(
            "stats: mode={mode} target_files={} target_source_bytes={} target_projected_bytes={} target_chapters={} target_verses={} target_unkeyed_anchors={} source_files={} source_source_bytes={} source_projected_bytes={} source_chapters={} source_verses={} source_unkeyed_anchors={} aligned_units={} alignment_facts={} elapsed_ms={:.3} throughput_mib_s={:.2}",
            self.target.files,
            self.target.source_bytes,
            self.target.projected_bytes,
            self.target.chapters,
            self.target.verses,
            self.target.unkeyed_anchors,
            self.source.as_ref().map_or(0, |source| source.files),
            self.source.as_ref().map_or(0, |source| source.source_bytes),
            self.source
                .as_ref()
                .map_or(0, |source| source.projected_bytes),
            self.source.as_ref().map_or(0, |source| source.chapters),
            self.source.as_ref().map_or(0, |source| source.verses),
            self.source
                .as_ref()
                .map_or(0, |source| source.unkeyed_anchors),
            self.aligned_units,
            self.alignment_facts,
            self.elapsed.as_secs_f64() * 1_000.0,
            self.throughput_mib_per_second(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    const MRK: &str = "\\id MRK\n\\c 1\n\\p\n\\v 1 Mark.\n";
    const GEN: &str = "\\id GEN\n\\c 1\n\\p\n\\v 1 Genesis.\n";

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
    fn file_input_is_supported_and_case_insensitive() {
        let temp = TempDir::new();
        let path = temp.0.join("book.USFM");
        fs::write(&path, MRK).unwrap();
        assert_eq!(paths_for_input(&path).unwrap(), vec![path]);
    }

    #[test]
    fn directory_input_filters_immediate_files_and_sorts_paths() {
        let temp = TempDir::new();
        fs::write(temp.0.join("b.USFM"), MRK).unwrap();
        fs::write(temp.0.join("a.sfm"), GEN).unwrap();
        fs::write(temp.0.join("ignore.txt"), "").unwrap();
        fs::create_dir(temp.0.join("nested.usfm")).unwrap();
        fs::write(temp.0.join("nested.usfm").join("c.usfm"), MRK).unwrap();

        let paths = paths_for_input(&temp.0).unwrap();
        let names: Vec<_> = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["a.sfm", "b.USFM"]);
    }

    #[test]
    fn unsupported_file_and_empty_directory_fail_clearly() {
        let temp = TempDir::new();
        let unsupported = temp.0.join("book.txt");
        fs::write(&unsupported, MRK).unwrap();
        assert!(paths_for_input(&unsupported).is_err());
        assert!(paths_for_input(&temp.0).is_err());
    }

    #[test]
    fn serial_and_parallel_loading_preserve_order_and_semantics() {
        let temp = TempDir::new();
        fs::write(temp.0.join("b.usfm"), MRK).unwrap();
        fs::write(temp.0.join("a.sfm"), GEN).unwrap();
        let serial = load_input(&temp.0, false).unwrap();
        let parallel = load_input(&temp.0, true).unwrap();
        assert_eq!(serial.paths, parallel.paths);
        let serial_corpus = Corpus::try_new(&serial.books).unwrap();
        let parallel_corpus = Corpus::try_new(&parallel.books).unwrap();
        let serial_keys: Vec<_> = serial_corpus.iter().map(|(_, book)| book.key()).collect();
        let parallel_keys: Vec<_> = parallel_corpus.iter().map(|(_, book)| book.key()).collect();
        assert_eq!(serial_keys, parallel_keys);
    }

    #[test]
    fn file_file_alignment_uses_book_key_and_target_only_mode_has_no_source() {
        let temp = TempDir::new();
        let target_path = temp.0.join("target.usfm");
        let source_path = temp.0.join("source.sfm");
        fs::write(&target_path, MRK).unwrap();
        fs::write(&source_path, MRK).unwrap();
        let target = load_input(&target_path, false).unwrap();
        let source = load_input(&source_path, false).unwrap();
        let target_corpus = Corpus::try_new(&target.books).unwrap();
        let source_corpus = Corpus::try_new(&source.books).unwrap();
        let alignment = align(&target_corpus, &source_corpus);
        assert_eq!(alignment.units().len(), 1);
        assert!(alignment.facts().is_empty());
        assert!(source_corpus.index_of(target.books[0].key()).is_some());
    }

    #[test]
    fn independently_ordered_directories_align_by_book_key() {
        let temp = TempDir::new();
        let target_dir = temp.0.join("target");
        let source_dir = temp.0.join("source");
        fs::create_dir(&target_dir).unwrap();
        fs::create_dir(&source_dir).unwrap();
        fs::write(target_dir.join("a-MRK.usfm"), MRK).unwrap();
        fs::write(target_dir.join("b-GEN.usfm"), GEN).unwrap();
        fs::write(source_dir.join("a-GEN.usfm"), GEN).unwrap();
        fs::write(source_dir.join("b-MRK.usfm"), MRK).unwrap();

        let target = load_input(&target_dir, false).unwrap();
        let source = load_input(&source_dir, true).unwrap();
        let target_corpus = Corpus::try_new(&target.books).unwrap();
        let source_corpus = Corpus::try_new(&source.books).unwrap();
        let alignment = align(&target_corpus, &source_corpus);

        let mut books: Vec<_> = alignment.units().iter().map(|unit| unit.book()).collect();
        books.sort_by_key(|book| book.as_bytes());
        assert_eq!(books, vec![target.books[1].key(), target.books[0].key()]);
        assert!(alignment.facts().is_empty());
    }

    #[test]
    fn hygiene_findings_publish_through_galley_in_raw_utf16() {
        use sous_core::{CoordinateSpace, CorpusSnapshot, FindingKind, HygieneClass};

        let temp = TempDir::new();
        // Onion masks a lone `\` as a marker; a `\\` pair reaches Sous as
        // content. It sits past a 4-byte char, so the UTF-16 span moves two
        // units less than the raw bytes.
        let path = temp.0.join("mrk.usfm");
        fs::write(&path, "\\id MRK\n\\c 1\n\\p\n\\v 1 An 🧅 \\\\ here.\n").unwrap();
        let target = load_input(&path, false).unwrap();
        let corpus = Corpus::try_new(&target.books).unwrap();
        let findings = hygiene_findings(&corpus);
        assert_eq!(findings.len(), 1);
        let FindingKind::Hygiene(digest) = findings[0].kind() else {
            panic!("hygiene kind")
        };
        assert_eq!(digest.class(), HygieneClass::StrandedBackslash);
        assert_eq!(digest.run(), 2);
        assert_eq!((findings[0].from(), findings[0].to()), (10, 12));

        let buffer = publish(&target.paths, target.sources, &findings).unwrap();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(snapshot.coordinate_space(), CoordinateSpace::Utf16);
        let row = snapshot
            .book(findings[0].book_idx())
            .unwrap()
            .at(0)
            .unwrap();
        // Raw bytes 29..31; the onion at 24..28 is two UTF-16 units.
        assert_eq!((row.from(), row.to()), (27, 29));
    }

    #[test]
    fn mismatched_single_books_are_reported_as_alignment_facts() {
        let temp = TempDir::new();
        let target_path = temp.0.join("target.usfm");
        let source_path = temp.0.join("source.usfm");
        fs::write(&target_path, MRK).unwrap();
        fs::write(&source_path, GEN).unwrap();
        let target = load_input(&target_path, false).unwrap();
        let source = load_input(&source_path, false).unwrap();
        let target_corpus = Corpus::try_new(&target.books).unwrap();
        let source_corpus = Corpus::try_new(&source.books).unwrap();
        let alignment = align(&target_corpus, &source_corpus);

        assert!(alignment.units().is_empty());
        assert_eq!(alignment.facts().len(), 2);
    }
}
