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
//! `--report` page shows each site's reasons beside it; `report.rs` renders it.

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

mod report;
mod source;

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

    /// Declared source to compare lengths against: USFM like the target, or an
    /// addressless `BOOK C:V<TAB>text` vref file.
    #[usage(long)]
    source: Option<PathBuf>,

    /// Report runs of consecutive target words the declared source already
    /// holds in the paired verse; the lane ships off.
    #[usage(long)]
    source_copy: bool,

    /// Consecutive target words a source-copy run needs before it is a row;
    /// the default is the shipped floor, and below two nothing fires.
    #[usage(long)]
    source_copy_min_run: Option<u32>,

    /// Print hygiene findings over the target with their raw source location.
    #[usage(long)]
    findings: bool,

    /// Write the target's findings as a complete SOUS corpus buffer in raw-book UTF-16.
    #[usage(long)]
    publish: Option<PathBuf>,

    /// Write a self-contained HTML page of every pattern and its sites in context.
    #[usage(long)]
    report: Option<PathBuf>,

    /// Report rare words one edit from a common word — an on-demand review
    /// action, not a channel: runs alone and ignores every other flag.
    #[usage(long)]
    typos: bool,

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

/// Rare words one edit from a frequent word, grouped by target: an on-demand
/// review action (`sous --typos <corpus>`), not a channel and not a wire row.
/// `sous_core::typos` owns the pure algorithm; this owns the corpus walk that
/// feeds it, the rayon parallel sweep over rare words, and the printed
/// report — the parallel loop lives here because `sous-core` stays
/// dependency-light and carries `rayon` as a dev-dependency only.
fn run_typos(corpus: &Corpus<'_, OnionBook>) -> Result<(), Box<dyn std::error::Error>> {
    let words = typo_word_counts(corpus);
    let config = sous_core::typos::TypoConfig::default();
    let alphabet = sous_core::typos::alphabet_of(&words);
    let index = sous_core::typos::WordIndex::build(&words);
    let rare: Vec<&sous_core::typos::WordEntry> = words
        .words
        .iter()
        .filter(|entry| sous_core::typos::is_rare_candidate(entry, &config))
        .collect();
    let matches: Vec<Vec<usize>> = rare
        .par_iter()
        .map(|word| {
            sous_core::typos::rare_word_candidates(word, &words, &index, &alphabet, &config)
        })
        .collect();
    let groups = sous_core::typos::group_candidates(&words, &rare, &matches);
    print_typo_report(&groups);
    Ok(())
}

/// Every case-folded word the corpus holds, counted, with each word's Title
/// occurrences flagged and up to three references to its earliest
/// occurrences kept while walking — the word's TEXT is recovered here, from
/// the CLI's own copy of the projected text, because the pure algorithm
/// never sees a hash it could not turn back into scalars.
fn typo_word_counts(corpus: &Corpus<'_, OnionBook>) -> sous_core::typos::WordCounts {
    const MAX_REFS: usize = 3;
    let mut index: FxHashMap<String, usize> = FxHashMap::default();
    let mut entries: Vec<sous_core::typos::WordEntry> = Vec::new();
    for (_, book) in corpus.iter() {
        let text = book.text();
        sous_core::words::for_each_word(text, &[], |occurrence| {
            let folded: String = text[occurrence.from as usize..occurrence.to as usize]
                .chars()
                .map(|scalar| scalar.to_lowercase().next().unwrap_or(scalar))
                .collect();
            let slot = *index.entry(folded.clone()).or_insert_with(|| {
                entries.push(sous_core::typos::WordEntry {
                    text: folded,
                    count: 0,
                    title_seen: false,
                    refs: Vec::new(),
                });
                entries.len() - 1
            });
            let entry = &mut entries[slot];
            entry.count += 1;
            entry.title_seen |= occurrence.form == sous_core::words::Form::Title;
            if entry.refs.len() < MAX_REFS
                && let Ok(span) = TextRange::new(occurrence.from, occurrence.to)
                && let Some(located) = book.locate(span)
            {
                entry.refs.push(located.first.to_string());
            }
        });
    }
    sous_core::typos::WordCounts { words: entries }
}

/// ```text
/// katika (8367)   ← 12 possible slips
///     katiki (1)   GEN 3:1
///     kutika (1)   EXO 12:5
/// ```
fn print_typo_report(groups: &[sous_core::typos::TypoGroup]) {
    print!("{}", format_typo_report(groups));
}

/// Built as a string so a test can check it without capturing stdout.
fn format_typo_report(groups: &[sous_core::typos::TypoGroup]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "Possible typos: rare words one edit from a common word. Review, do not \
         trust \u{2014} real on agglutinative languages, mostly noise on short-word \
         languages like English."
    )
    .expect("String writes never fail");
    if groups.is_empty() {
        writeln!(out, "(no candidates)").expect("String writes never fail");
        return out;
    }
    for group in groups {
        writeln!(
            out,
            "{} ({})   \u{2190} {} possible slip{}",
            group.target.0,
            group.target.1,
            group.candidates.len(),
            if group.candidates.len() == 1 { "" } else { "s" }
        )
        .expect("String writes never fail");
        for (text, count, refs) in &group.candidates {
            writeln!(out, "    {text} ({count})   {}", refs.join(", "))
                .expect("String writes never fail");
        }
    }
    out
}

/// The product pass over every target book, plus the source comparison: rows
/// in projected UTF-8, ordered by book then offset, and the corpus-level
/// pattern table beside them.
fn brigade_findings(
    corpus: &Corpus<'_, OnionBook>,
    source: &[SourceLengths<'_>],
    source_copy: bool,
    min_run: Option<u32>,
) -> (Vec<PackedFinding>, Vec<Pattern>, Paired) {
    let mut config = <Brigade as ChapterPass>::Config::default();
    config.1.lengths.source_copy = source_copy;
    config.2.lengths.source_copy = source_copy;
    if let Some(min_run) = min_run {
        config.1.lengths.source_copy_min_run = min_run;
        config.2.lengths.source_copy_min_run = min_run;
    }
    let (findings, paired) = analyze_paired(corpus, &Brigade::default(), &config, source);
    let (rows, patterns) = findings.into_parts();
    (rows, patterns, paired)
}

/// One fired length row with everything the wire does not carry: the address,
/// the ratio, and both sides' text.
///
/// The CLI holds both corpora, so it recomputes the ratio from the aligned
/// unit rather than asking the record for a number the record does not have.
fn paired_rows(
    target: &Corpus<'_, OnionBook>,
    source: &Corpus<'_, source::SourceBook>,
    alignment: &Alignment,
    findings: &[PackedFinding],
) -> Vec<report::PairedUnit> {
    let mut units: FxHashMap<(u16, u32, u32), &AlignedUnit> = FxHashMap::default();
    for unit in alignment.units() {
        let Some(index) = target.index_of(unit.book()) else {
            continue;
        };
        let Some(span) = bounding(unit.target().ranges()) else {
            continue;
        };
        units.insert((index.get(), span.from(), span.to()), unit);
    }

    let mut out = Vec::new();
    for finding in findings {
        let FindingKind::LengthProportionality(digest) = finding.kind() else {
            continue;
        };
        let key = (finding.book_idx().get(), finding.from(), finding.to());
        let Some(unit) = units.get(&key) else {
            continue;
        };
        let book = target
            .get(finding.book_idx())
            .expect("a finding names a corpus book");
        let source_book = source
            .index_of(unit.book())
            .and_then(|index| source.get(index))
            .expect("the unit paired with a source book");
        let target_text: String = unit
            .target()
            .ranges()
            .iter()
            .map(|range| &book.text()[range.from() as usize..range.to() as usize])
            .collect();
        let source_text: String = unit
            .source()
            .ranges()
            .iter()
            .map(|range| source_book.slice(*range))
            .collect();
        let long = count_atoms(&target_text);
        let short = count_atoms(&source_text);
        out.push(report::PairedUnit {
            address: format!("{} {}", unit.book(), address_of(unit.key())),
            ratio: if short == 0 {
                0.0
            } else {
                f64::from(long) / f64::from(short)
            },
            book_z: digest
                .book_scope()
                .map(sous_core::QuantizedDeviation::as_f64),
            project_z: digest
                .project_scope()
                .map(sous_core::QuantizedDeviation::as_f64),
            target: target_text,
            source: source_text,
        });
    }
    out
}

/// `1:9`, or `1:9-11` for a bridge.
fn address_of(key: sous_core::VerseKey) -> String {
    if key.first() == key.last() {
        format!("{}:{}", key.chapter(), key.first())
    } else {
        format!("{}:{}-{}", key.chapter(), key.first(), key.last())
    }
}

/// The bounding range of a unit's side — first byte through last, which is
/// the span a row over a bridge names.
fn bounding(ranges: &[TextRange]) -> Option<TextRange> {
    let from = ranges.iter().map(|range| range.from()).min()?;
    let to = ranges.iter().map(|range| range.to()).max()?;
    TextRange::new(from, to).ok()
}

/// One line per fired length row: the verse, its ratio, and both scopes.
fn print_length_findings(
    target: &Corpus<'_, OnionBook>,
    source: &Corpus<'_, source::SourceBook>,
    alignment: &Alignment,
    findings: &[PackedFinding],
) {
    let rows = paired_rows(target, source, alignment, findings);
    let scope = |value: Option<f64>| match value {
        Some(value) => format!("{value:+.2}"),
        None => "-".to_string(),
    };
    for (index, finding) in findings
        .iter()
        .filter(|row| matches!(row.kind(), FindingKind::LengthProportionality(_)))
        .enumerate()
    {
        let Some(row) = rows.get(index) else { continue };
        println!(
            "length target[{}] {} ratio {:.2} z_book {} z_project {}",
            finding.book_idx().get(),
            row.address,
            row.ratio,
            scope(row.book_z),
            scope(row.project_z),
        );
    }
}

/// One line per presence row: the first key it covers, which side holds the
/// verses, and how many consecutive keys. Never a claim that a translation is
/// missing — only that these keys sit on one side of the pairing.
fn print_presence(target: &Corpus<'_, OnionBook>, paired: &Paired) {
    for row in presence_rows(target, paired) {
        println!(
            "presence target[{}] {} {} \u{d7}{}",
            row.book_idx, row.address, row.kind, row.keys
        );
    }
}

/// Every presence row with the book and key the wire lanes do not carry.
fn presence_rows(target: &Corpus<'_, OnionBook>, paired: &Paired) -> Vec<report::PresenceUnit> {
    let mut out = Vec::new();
    for ((index, book), rows) in target.iter().zip(&paired.presence) {
        for row in rows {
            out.push(report::PresenceUnit {
                book_idx: index.get(),
                address: format!("{} {}", book.key(), address_of(row.key())),
                kind: row.kind().name(),
                keys: row.keys(),
            });
        }
    }
    out
}

/// One line per source-copy row: the address, the run against the unit's
/// eligible words, and the shared text itself.
fn print_source_copy(target: &Corpus<'_, OnionBook>, paired: &Paired) {
    for ((index, book), rows) in target.iter().zip(&paired.copies) {
        for row in rows {
            let Some(verse) = verse_at(book, row.span()) else {
                continue;
            };
            println!(
                "sourcecopy target[{}] {} {} {}/{} \u{201c}{}\u{201d}",
                index.get(),
                book.key(),
                address_of(verse.key()),
                row.run(),
                row.eligible(),
                one_line(slice(book, row.span())),
            );
        }
    }
}

/// Every source-copy row with what the wire lanes do not carry: the address,
/// the shared run, the whole target verse, and the source verse behind it.
fn copy_rows(
    target: &Corpus<'_, OnionBook>,
    source: &Corpus<'_, source::SourceBook>,
    paired: &Paired,
) -> Vec<report::CopyUnit> {
    let mut out = Vec::new();
    for ((_, book), rows) in target.iter().zip(&paired.copies) {
        let mirror = source
            .books()
            .iter()
            .find(|other| ProjectedBook::key(*other) == book.key());
        for row in rows {
            let Some(verse) = verse_at(book, row.span()) else {
                continue;
            };
            let counterpart = mirror
                .and_then(|mirror| {
                    mirror
                        .verses()
                        .find(|other| other.key() == verse.key())
                        .map(|other| mirror.slice(other.text()).trim().to_string())
                })
                .unwrap_or_default();
            out.push(report::CopyUnit {
                address: format!("{} {}", book.key(), address_of(verse.key())),
                run: row.run(),
                eligible: row.eligible(),
                shared: slice(book, row.span()).to_string(),
                target: slice(book, verse.text()).trim().to_string(),
                source: counterpart,
            });
        }
    }
    out
}

/// The keyed verse whose projected text holds `span`.
fn verse_at(book: &OnionBook, span: TextRange) -> Option<sous_core::Verse> {
    book.verses()
        .find(|verse| verse.text().from() <= span.from() && span.to() <= verse.text().to())
}

fn slice(book: &OnionBook, span: TextRange) -> &str {
    &book.text()[span.from() as usize..span.to() as usize]
}

/// Projected verse text keeps the newlines a paragraph marker left behind; a
/// debug row is one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Alignment facts as counts per book. An unpaired key is structure; the
/// presence rows above are the reviewable claim over the same keys, and
/// versification shear stays a parked rule.
fn print_unpaired(alignment: &Alignment) {
    let mut counts: FxHashMap<sous_core::BookKey, [u32; 4]> = FxHashMap::default();
    for fact in alignment.facts() {
        let (book, lane) = match *fact {
            AlignmentFact::TargetOnly { book, .. } => (book, 0),
            AlignmentFact::SourceOnly { book, .. } => (book, 1),
            AlignmentFact::AmbiguousDuplicate { book, .. } => (book, 2),
            AlignmentFact::PartialOverlap { book, .. } => (book, 3),
        };
        counts.entry(book).or_default()[lane] += 1;
    }
    let mut books: Vec<_> = counts.into_iter().collect();
    books.sort_by_key(|(book, _)| book.as_bytes());
    for (book, lanes) in books {
        println!(
            "unpaired {book} target-only {} source-only {} ambiguous {} partial-overlap {}",
            lanes[0], lanes[1], lanes[2], lanes[3]
        );
    }
}

/// One line per firing pattern, in emission order, with its sites under it.
fn print_patterns(
    corpus: &Corpus<'_, OnionBook>,
    findings: &[PackedFinding],
    patterns: &[Pattern],
) {
    /// Sites printed per pattern before the tail line.
    const SHOWN: usize = 20;

    let mut sites: Vec<Vec<&PackedFinding>> = vec![Vec::new(); patterns.len()];
    for finding in findings {
        if let FindingKind::Convention(digest) = finding.kind() {
            let at = usize::from(digest.pattern().get());
            if let Some(rows) = sites.get_mut(at) {
                rows.push(finding);
            }
        }
    }
    for (index, pattern) in patterns.iter().enumerate() {
        // A word row names a hash, not a glyph; the CLI has the text, so it
        // shows the word its first site landed on.
        if let Some(hash) = pattern.word_hash() {
            let claim = match pattern.key {
                PatternKey::Casing { form, .. } => form.name().to_string(),
                PatternKey::WordLength { sigma, .. } => format!("{sigma}\u{3c3}"),
                PatternKey::Doubled { separated, .. } => if separated {
                    "doubled, separated"
                } else {
                    "doubled"
                }
                .to_string(),
                _ => unreachable!("a word hash comes from a word channel"),
            };
            let word = first_site_text(corpus, &sites[index]);
            println!(
                "pattern[{index}] word #{hash:016x} {claim}{word} {}/{} {:.2}% band {} \u{b7} {}/{} books {} sites",
                pattern.numerator,
                pattern.denominator,
                f64::from(pattern.share_bp) / 100.0,
                pattern.band.unwrap_or_default(),
                pattern.books,
                corpus.len(),
                sites[index].len(),
            );
            print_sites(corpus, &sites[index], SHOWN);
            continue;
        }
        let evidence = match pattern.key {
            PatternKey::Rarity => "rarity".to_string(),
            PatternKey::Placement { side, class } => {
                format!("placement {}={}", side.name(), class.name())
            }
            PatternKey::RunShape { pure, bucket } => format!(
                "run-shape {} len {bucket}{}",
                if pure { "pure" } else { "mixed" },
                if bucket == 6 { "+" } else { "" }
            ),
            PatternKey::ExactNeighbor(neighbor) => {
                format!("exact-neighbor {}", glyph(neighbor))
            }
            PatternKey::PooledNeighbor(pool) => {
                format!("pooled-neighbor {}", pool.name())
            }
            PatternKey::LetterRun { length } => format!(
                "letter-run {length}{}",
                if length == LETTER_RUN_MAX { "+" } else { "" }
            ),
            // Glyph-side, but the site is the word after it, so the row reads
            // the way a word row does.
            PatternKey::SentenceStart => {
                format!("sentence-start{}", first_site_text(corpus, &sites[index]))
            }
            PatternKey::Casing { .. }
            | PatternKey::WordLength { .. }
            | PatternKey::Doubled { .. } => unreachable!("handled above"),
        };
        let band = match pattern.band {
            Some(step) => format!(" band {step}"),
            None => String::new(),
        };
        println!(
            "pattern[{index}] {} {evidence} {}/{} {:.2}%{band} · {}/{} books {} sites",
            glyph(pattern.glyph),
            pattern.numerator,
            pattern.denominator,
            f64::from(pattern.share_bp) / 100.0,
            pattern.books,
            corpus.len(),
            sites[index].len(),
        );
        print_sites(corpus, &sites[index], SHOWN);
    }
}

/// A pattern's sites, capped, with a tail line for the rest.
/// The text the row's first site landed on, quoted, or nothing when it has
/// none: what a row whose key is not a glyph shows instead of one.
fn first_site_text(corpus: &Corpus<'_, OnionBook>, sites: &[&PackedFinding]) -> String {
    sites.first().map_or_else(String::new, |finding| {
        let book = corpus
            .get(finding.book_idx())
            .expect("a finding names a corpus book");
        format!(
            " {:?}",
            &book.text()[finding.from() as usize..finding.to() as usize]
        )
    })
}

fn print_sites(corpus: &Corpus<'_, OnionBook>, sites: &[&PackedFinding], shown: usize) {
    for finding in sites.iter().take(shown) {
        let book = corpus
            .get(finding.book_idx())
            .expect("a finding names a corpus book");
        println!("  site {} {}..{}", book.key(), finding.from(), finding.to());
    }
    if sites.len() > shown {
        println!("  \u{2026} and {} more", sites.len() - shown);
    }
}

/// `U+002C ','`, or the pooled digit lane.
fn glyph(key: ScalarKey) -> String {
    match key.scalar() {
        Some(scalar) => format!("U+{:04X} {scalar:?}", scalar as u32),
        None => "digits".to_string(),
    }
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
    patterns: &[Pattern],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let books = paths
        .iter()
        .zip(sources)
        .map(|(path, source)| OnionInputBook::new(path.display().to_string(), source))
        .collect();
    Ok(publish_onion_findings(
        books,
        findings,
        patterns,
        SnapshotId::new([0; 16]),
    )?)
}

/// The declared source's own rows, which carry no raw-file coordinates: a
/// vref line locates back to itself and to nothing in USFM.
fn print_source_books(corpus: &Corpus<'_, source::SourceBook>) {
    for (index, book) in corpus.iter() {
        println!(
            "source book[{}] {} -> {} (projected bytes: {}, chapters: {}, verses: {})",
            index.get(),
            ProjectedBook::key(book),
            book.id(),
            book.text().len(),
            book.chapters().count(),
            book.verses().count(),
        );
    }
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
    /// `unkeyed_anchors` is Onion's own count of verse anchors with no numeric
    /// designator; a vref row cannot have one, so a source counts zero.
    fn collect<B: ProjectedBook>(
        corpus: &Corpus<'_, B>,
        source_bytes: usize,
        unkeyed_anchors: usize,
    ) -> Self {
        let mut projected_bytes = 0;
        let mut chapters = 0;
        let mut verses = 0;
        for (_, book) in corpus.iter() {
            projected_bytes += book.text().len();
            chapters += book.chapters().count();
            verses += book.verses().count();
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
        source: Option<&Corpus<'_, source::SourceBook>>,
        alignment: Option<&Alignment>,
        target_source_bytes: usize,
        source_source_bytes: Option<usize>,
        started: Instant,
    ) -> Self {
        let unkeyed = target
            .iter()
            .map(|(_, book)| book.unkeyed_anchor_count())
            .sum();
        Self {
            parallel,
            target: CorpusStats::collect(target, target_source_bytes, unkeyed),
            source: source.map(|corpus| {
                CorpusStats::collect(corpus, source_source_bytes.unwrap_or_default(), 0)
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
        let (findings, patterns, _) = brigade_findings(&corpus, &[], false, None);
        // The one-verse book rosters every glyph it holds, so the pair rides
        // beside a handful of rarity sites.
        let hygiene: Vec<_> = findings
            .iter()
            .filter(|row| matches!(row.kind(), FindingKind::Hygiene(_)))
            .collect();
        assert_eq!(hygiene.len(), 1);
        let FindingKind::Hygiene(digest) = hygiene[0].kind() else {
            panic!("hygiene kind")
        };
        assert_eq!(digest.class(), HygieneClass::StrandedBackslash);
        assert_eq!(digest.run(), 2);
        assert_eq!((hygiene[0].from(), hygiene[0].to()), (10, 12));

        let buffer = publish(&target.paths, target.sources, &findings, &patterns).unwrap();
        let snapshot = CorpusSnapshot::open(&buffer).unwrap();
        assert_eq!(snapshot.coordinate_space(), CoordinateSpace::Utf16);
        let book = snapshot.book(hygiene[0].book_idx()).unwrap();
        let row = (0..book.len())
            .map(|at| book.at(at).unwrap())
            .find(|row| matches!(row.kind(), FindingKind::Hygiene(_)))
            .expect("the pair is a hygiene row");
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

    #[test]
    fn typos_groups_a_rare_word_under_the_frequent_word_it_might_be_a_slip_of() {
        let temp = TempDir::new();
        let path = temp.0.join("mrk.usfm");
        let body = "says ".repeat(205);
        fs::write(&path, format!("\\id MRK\n\\c 1\n\\p\n\\v 1 {body}saws.\n")).unwrap();
        let target = load_input(&path, false).unwrap();
        let corpus = Corpus::try_new(&target.books).unwrap();

        let words = typo_word_counts(&corpus);
        let config = sous_core::typos::TypoConfig::default();
        let groups = sous_core::typos::typo_candidates(&words, &config);
        let report = format_typo_report(&groups);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].target.0, "says");
        assert_eq!(groups[0].candidates[0].0, "saws");
        assert!(report.contains("says (205)"), "{report}");
        assert!(report.contains("saws (1)   MRK 1:1"), "{report}");
    }
}
