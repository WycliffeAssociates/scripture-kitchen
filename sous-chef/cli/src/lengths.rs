//! The length lanes printed: ratios, presence, source-copy runs, and what
//! did not pair.
//!
//! ```text
//! length target[0] MRK 1:9 ratio 0.14 z_book -8.72 z_project -6.05
//! unpaired MRK target-only 2 source-only 0 ambiguous 0 partial-overlap 1
//! ```

use crate::*;

/// The product pass over every target book, plus the source comparison: rows
/// in projected UTF-8, ordered by book then offset, and the corpus-level
/// pattern table beside them.
pub(crate) fn brigade_findings(
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
pub(crate) fn paired_rows(
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
pub(crate) fn address_of(key: sous_core::VerseKey) -> String {
    if key.first() == key.last() {
        format!("{}:{}", key.chapter(), key.first())
    } else {
        format!("{}:{}-{}", key.chapter(), key.first(), key.last())
    }
}

/// The bounding range of a unit's side — first byte through last, which is
/// the span a row over a bridge names.
pub(crate) fn bounding(ranges: &[TextRange]) -> Option<TextRange> {
    let from = ranges.iter().map(|range| range.from()).min()?;
    let to = ranges.iter().map(|range| range.to()).max()?;
    TextRange::new(from, to).ok()
}

/// One line per fired length row: the verse, its ratio, and both scopes.
pub(crate) fn print_length_findings(
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
pub(crate) fn print_presence(target: &Corpus<'_, OnionBook>, paired: &Paired) {
    for row in presence_rows(target, paired) {
        println!(
            "presence target[{}] {} {} \u{d7}{}",
            row.book_idx, row.address, row.kind, row.keys
        );
    }
}

/// Every presence row with the book and key the wire lanes do not carry.
pub(crate) fn presence_rows(
    target: &Corpus<'_, OnionBook>,
    paired: &Paired,
) -> Vec<report::PresenceUnit> {
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
pub(crate) fn print_source_copy(target: &Corpus<'_, OnionBook>, paired: &Paired) {
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
pub(crate) fn copy_rows(
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
pub(crate) fn verse_at(book: &OnionBook, span: TextRange) -> Option<sous_core::Verse> {
    book.verses()
        .find(|verse| verse.text().from() <= span.from() && span.to() <= verse.text().to())
}

pub(crate) fn slice(book: &OnionBook, span: TextRange) -> &str {
    &book.text()[span.from() as usize..span.to() as usize]
}

/// Projected verse text keeps the newlines a paragraph marker left behind; a
/// debug row is one line.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Alignment facts as counts per book. An unpaired key is structure; the
/// presence rows above are the reviewable claim over the same keys, and
/// versification shear stays a parked rule.
pub(crate) fn print_unpaired(alignment: &Alignment) {
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
