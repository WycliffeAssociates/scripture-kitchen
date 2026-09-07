//! The pattern table printed, each row with the sites it headlines.
//!
//! ```text
//! pattern[0] U+002C ',' placement next=Digit 12/9812 0.12% band 4
//!   site MRK 118..119
//! ```

use crate::*;

/// One line per firing pattern, in emission order, with its sites under it.
pub(crate) fn print_patterns(
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
pub(crate) fn first_site_text(corpus: &Corpus<'_, OnionBook>, sites: &[&PackedFinding]) -> String {
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

pub(crate) fn print_sites(corpus: &Corpus<'_, OnionBook>, sites: &[&PackedFinding], shown: usize) {
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
pub(crate) fn glyph(key: ScalarKey) -> String {
    match key.scalar() {
        Some(scalar) => format!("U+{:04X} {scalar:?}", scalar as u32),
        None => "digits".to_string(),
    }
}

pub(crate) fn print_findings(corpus: &Corpus<'_, OnionBook>, findings: &[PackedFinding]) {
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
