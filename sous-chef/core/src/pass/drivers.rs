//! Cold analysis, end to end: the walks a host runs when it keeps nothing.
//!
//! ```text
//! analyze(&corpus, &pass)                  -> Findings
//! analyze_paired(&corpus, &pass, ..)       -> the same, plus source ratios
//! for_each_chapter(&book, |start, ch| ..)  -> the map order a resident host reuses
//! ```
//!
//! The resident coordinator publishes the same bytes these do; this is the
//! oracle it is measured against.

use super::*;

/// Calls `visit` with each chapter's projected start and its map input.
///
/// The one place a chapter input is assembled, so [`analyze`] and an
/// incremental host cannot build two different ones from the same book.
pub fn for_each_chapter<B: ProjectedBook>(book: &B, mut visit: impl FnMut(u32, ChapterInput<'_>)) {
    let text = book.text();
    let rows: Vec<Verse> = book.verses().collect();
    let mut verses = Vec::new();
    let mut at = 0;
    for chapter in book.chapters() {
        let span = chapter.text();
        at = collect_verses(&rows, at, chapter, &mut verses);
        visit(
            span.from(),
            ChapterInput {
                text: &text[span.from() as usize..span.to() as usize],
                verses: &verses,
                key: ChapterKey::new(book.key(), chapter.number()),
            },
        );
    }
}

/// [`analyze_with`] under the pass's default config.
///
/// This is the whole-corpus oracle an incremental host is measured against.
pub fn analyze<B: ProjectedBook, P: ChapterPass>(corpus: &Corpus<'_, B>, pass: &P) -> Findings {
    analyze_with(corpus, pass, &P::Config::default())
}

/// [`analyze_paired`] with no source: the pass's own rows and nothing else.
pub fn analyze_with<B: ProjectedBook, P: ChapterPass>(
    corpus: &Corpus<'_, B>,
    pass: &P,
    config: &P::Config,
) -> Findings {
    analyze_paired(corpus, pass, config, &[]).0
}

/// Maps every chapter, folds every book in caller order, judges the corpus
/// once, compares each book's verse lengths against the source book of the
/// same key, and orders the rows.
///
/// The cold oracle for a paired resident host, so it runs the same two steps
/// in the same order. Returns the pairing failures beside the findings: they
/// are structural facts a host reports, never rows.
pub fn analyze_paired<B: ProjectedBook, P: ChapterPass>(
    corpus: &Corpus<'_, B>,
    pass: &P,
    config: &P::Config,
    source: &[SourceLengths<'_>],
) -> (Findings, Paired) {
    let book_lengths: Vec<u32> = corpus
        .books()
        .iter()
        .map(|book| u32::try_from(book.text().len()).expect("corpus validation bounds book length"))
        .collect();
    let mut out = Findings::new(book_lengths);

    let mut observations: Vec<(u32, P::Observation)> = Vec::new();
    let mut aggregates: Vec<P::Aggregate> = Vec::with_capacity(corpus.books().len());
    for (_, book) in corpus.iter() {
        observations.clear();
        for_each_chapter(book, |start, input| {
            observations.push((start, pass.map(input)));
        });
        let rows: Vec<ChapterObs<&P::Observation>> = observations
            .iter()
            .map(|(start, obs)| ChapterObs { start: *start, obs })
            .collect();
        aggregates.push(pass.fold(&rows));
    }
    let views: Vec<&P::Aggregate> = aggregates.iter().collect();
    pass.judge(&views, config, &mut out);

    let paired = match pass.length_config(config) {
        Some(lengths) if !source.is_empty() => {
            let target: Vec<TargetLengths<'_>> = corpus
                .iter()
                .map(|(index, book)| TargetLengths {
                    book: book.key(),
                    verses: pass.verse_lengths(&aggregates[index.get() as usize]),
                    text: book.text(),
                })
                .collect();
            judge_lengths(&target, source, &lengths, &mut out)
        }
        _ => Paired::default(),
    };

    let mut chapters: Vec<Chapter> = Vec::new();
    let mut verses: Vec<Verse> = Vec::new();
    for (index, book) in corpus.iter() {
        chapters.clear();
        chapters.extend(book.chapters());
        verses.clear();
        verses.extend(book.verses());
        pass.locate(
            index,
            book.text(),
            &chapters,
            &verses,
            &aggregates[index.get() as usize],
            &mut out,
        );
    }
    out.finish();
    (out, paired)
}

/// Rebases this chapter's verse rows into `verses`, returning where the next
/// chapter resumes. Rows are non-decreasing by key, so each run is contiguous.
///
/// The one rule for chapter membership: a map and a rescan that disagreed on
/// which verses a chapter holds would site a verse-start capital the walk
/// never counted.
pub(crate) fn collect_verses(
    rows: &[Verse],
    mut at: usize,
    chapter: Chapter,
    verses: &mut Vec<Verse>,
) -> usize {
    verses.clear();
    let span = chapter.text();
    while rows
        .get(at)
        .is_some_and(|row| row.key().chapter() < chapter.number())
    {
        at += 1;
    }
    while let Some(row) = rows
        .get(at)
        .filter(|row| row.key().chapter() == chapter.number())
    {
        let text = row.text();
        debug_assert!(
            span.from() <= text.from() && text.to() <= span.to(),
            "corpus validation places every verse inside its chapter"
        );
        let rebased = TextRange::new(text.from() - span.from(), text.to() - span.from())
            .expect("a chapter-relative range keeps its order");
        verses.push(Verse::new(row.key(), rebased));
        at += 1;
    }
    at
}
