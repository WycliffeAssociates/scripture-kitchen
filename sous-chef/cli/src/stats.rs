//! What a run cost, per operation and per corpus.
//!
//! ```text
//! stats  load 41ms  analyze 128ms  publish 6ms   66 books, 4.30 MiB
//! ```

use crate::*;

/// CLI instrumentation, not a benchmark contract. Timing ends before debug printing.
pub(crate) struct OperationStats {
    pub(crate) parallel: bool,
    pub(crate) target: CorpusStats,
    pub(crate) source: Option<CorpusStats>,
    pub(crate) aligned_units: usize,
    pub(crate) alignment_facts: usize,
    pub(crate) elapsed: Duration,
}

pub(crate) struct CorpusStats {
    pub(crate) files: usize,
    pub(crate) source_bytes: usize,
    pub(crate) projected_bytes: usize,
    pub(crate) chapters: usize,
    pub(crate) verses: usize,
    pub(crate) unkeyed_anchors: usize,
}

impl CorpusStats {
    /// `unkeyed_anchors` is Onion's own count of verse anchors with no numeric
    /// designator; a vref row cannot have one, so a source counts zero.
    pub(crate) fn collect<B: ProjectedBook>(
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
    pub(crate) fn collect(
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

    pub(crate) fn throughput_mib_per_second(&self) -> f64 {
        let seconds = self.elapsed.as_secs_f64();
        if seconds == 0.0 {
            return 0.0;
        }
        let bytes =
            self.target.source_bytes + self.source.as_ref().map_or(0, |source| source.source_bytes);
        bytes as f64 / (1024.0 * 1024.0) / seconds
    }

    pub(crate) fn print(&self) {
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
