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
    terminals: Option<TerminalTable>,
}

impl Findings {
    /// The lengths are the caller's book table, in book-index order.
    pub fn new(book_lengths: Vec<u32>) -> Self {
        Self {
            book_lengths,
            book: None,
            rows: Vec::new(),
            patterns: Vec::new(),
            terminals: None,
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

    pub fn rows(&self) -> &[PackedFinding] {
        &self.rows
    }

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
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

    /// Puts every row in publication order: stable by `(book_idx, from, to)`,
    /// so a tie keeps the pushing judge's turn. A host calls it once, after
    /// the last judge.
    pub fn finish(&mut self) {
        self.rows
            .sort_by_key(|row| (row.book_idx().get(), row.from(), row.to()));
    }

    pub fn into_parts(self) -> (Vec<PackedFinding>, Vec<Pattern>) {
        (self.rows, self.patterns)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}
