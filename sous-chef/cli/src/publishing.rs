//! The publication and the corpus listings beside it.
//!
//! ```text
//! published 2 findings for 1 books (SOUS v1, UTF-16) to out.sous
//! ```

use crate::*;

/// The snapshot identity is a Galley lifecycle decision not yet made; the
/// CLI publishes a zero identity and says so here rather than inventing one.
pub(crate) fn publish(
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
pub(crate) fn print_source_books(corpus: &Corpus<'_, source::SourceBook>) {
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

pub(crate) fn print_books(label: &str, paths: &[PathBuf], corpus: &Corpus<'_, OnionBook>) {
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

pub(crate) fn print_alignment(alignment: &Alignment) {
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
