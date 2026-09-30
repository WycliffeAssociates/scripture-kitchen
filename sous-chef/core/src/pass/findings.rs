//! The rows a publication carries, in the order they were pushed.
//!
//! ```text
//! findings.open_book(BookIndex::new(0)?)
//! findings.push(span, kind)?      // inside the book that is open
//! findings.finish()               // the pattern table is numbered here
//! ```
//!
//! A span is checked against the book that is open when it is pushed, so a
//! row can never name a coordinate outside the book it was found in.

use super::*;

/// The judge sink: rows in projected book coordinates, each naming its book.
///
/// A host builds one per invocation, names a book before judging it, and
/// publishes the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Findings {
    book_lengths: Vec<u32>,
    book: Option<BookIndex>,
    rows: Vec<PackedFinding>,
    patterns: Vec<Pattern>,
    clusters: Vec<Cluster>,
    terminals: Option<TerminalTable>,
    terminal_counts: Vec<TerminalCount>,
    explained: Explained,
}

impl Findings {
    /// The lengths are the caller's book table, in book-index order.
    pub fn new(book_lengths: Vec<u32>) -> Self {
        Self {
            book_lengths,
            book: None,
            rows: Vec::new(),
            patterns: Vec::new(),
            clusters: Vec::new(),
            terminals: None,
            terminal_counts: Vec::new(),
            explained: Explained::default(),
        }
    }

    /// Names the book every later [`push`](Self::push) belongs to.
    pub fn open_book(&mut self, book: BookIndex) {
        self.book = Some(book);
    }

    /// Records one finding; the span is checked against the open book.
    ///
    /// Panics if no book is open: a row must never land in a book by default.
    pub fn push(&mut self, span: TextRange, kind: FindingKind) -> Result<(), CodecError> {
        let book = self.book.expect("open_book before push");
        let row = PackedFinding::new(span.from(), span.to(), book, kind, &self.book_lengths)?;
        self.rows.push(row);
        Ok(())
    }

    /// Records one corpus-level pattern and returns its table position.
    ///
    /// Patterns belong to the corpus, not to a book, so they may be pushed
    /// either side of any [`open_book`](Self::open_book). A table past
    /// `u16::MAX` rows saturates here and the encoder refuses it.
    pub fn push_pattern(&mut self, pattern: Pattern) -> PatternIndex {
        let index = PatternIndex::at(self.patterns.len());
        self.patterns.push(pattern);
        index
    }

    /// Records one cluster a `RunShape` row lists, after that row.
    pub fn push_cluster(&mut self, cluster: Cluster) {
        self.clusters.push(cluster);
    }

    pub fn rows(&self) -> &[PackedFinding] {
        &self.rows
    }

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Every listed cluster, in pattern order.
    pub fn clusters(&self) -> &[Cluster] {
        &self.clusters
    }

    /// The corpus's terminal table, once a judge has learned it.
    ///
    /// Corpus evidence rather than a row: `Substrate` publishes it, the word
    /// channels read it to split free from forced, and `Words::locate` reads
    /// the same one so a rescan cannot disagree with the counts. `None` until
    /// something publishes one, which is not the same as an empty table — a
    /// corpus may genuinely capitalize after nothing.
    pub const fn terminals(&self) -> Option<&TerminalTable> {
        self.terminals.as_ref()
    }

    pub fn set_terminals(&mut self, table: TerminalTable) {
        self.terminals = Some(table);
    }

    /// The raw counts behind the terminal table, which a publication carries
    /// so a reader can say what follows a mark; no threshold touches them.
    pub fn terminal_counts(&self) -> &[TerminalCount] {
        &self.terminal_counts
    }

    pub fn set_terminal_counts(&mut self, counts: Vec<TerminalCount>) {
        self.terminal_counts = counts;
    }

    /// What the firing rows leave to a finer judgment: corpus evidence the
    /// substrate rescan reads so its sites agree with each row's numerator.
    pub const fn explained(&self) -> &Explained {
        &self.explained
    }

    pub fn set_explained(&mut self, explained: Explained) {
        self.explained = explained;
    }

    /// Puts every row in publication order: stable by `(book_idx, from, to)`,
    /// so a tie keeps the pushing judge's turn. A host calls it once, after
    /// the last judge.
    pub fn finish(&mut self) {
        self.rows
            .sort_by_key(|row| (row.book_idx().get(), row.from(), row.to()));
    }

    /// Rows, patterns, clusters and terminal counts: everything a
    /// publication encodes.
    pub fn into_parts(
        self,
    ) -> (
        Vec<PackedFinding>,
        Vec<Pattern>,
        Vec<Cluster>,
        Vec<TerminalCount>,
    ) {
        (
            self.rows,
            self.patterns,
            self.clusters,
            self.terminal_counts,
        )
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}
