//! Chapter-granularity chunked lexing: memmem pre-scan for `\n\c ` (which
//! also catches `\r\n\c `), split the source at each hit so every chunk
//! begins with its `\c`, lex chunks independently, rebase spans, concat.
//!
//! Two entry points so the playground can separate the costs:
//! - [`lex_chunked`] — same split, chunks lexed serially. The delta vs
//!   `crate::lex` is pure chunking overhead (pre-scan + rebase + concat).
//! - [`lex_chunked_par`] (feature `par`) — rayon over the chunks. The delta
//!   vs `lex_chunked` is what chapter-level parallelism actually buys
//!   INSIDE one book.
//!
//! Splitting at `\n\c ` is safe for equivalence: a chunk boundary sits right
//! after a newline, newlines always end text/ws runs and clear the
//! delimiter-ws mode, so no token and no scan state ever crosses it. The
//! needle can in principle occur inside content (`\c ` at line start in a
//! footnote would be malformed anyway) — good enough for a benchmark, and
//! the playground verifies output equality against `crate::lex` first.

use memchr::memmem;

use crate::token::Token;

/// Chunk start offsets, ascending, always beginning with 0. Every offset
/// after the first points at the `\` of a line-initial `\c `.
pub fn chapter_chunk_starts(bytes: &[u8]) -> Vec<usize> {
    let mut starts = vec![0usize];
    let finder = memmem::Finder::new(b"\n\\c ");
    let mut from = 0usize;
    while let Some(hit) = finder.find(&bytes[from..]) {
        let newline = from + hit;
        starts.push(newline + 1); // chunk begins AT the backslash
        from = newline + 1;
    }
    starts
}

fn rebased(chunk_tokens: Vec<Token>, base: u32, out: &mut Vec<Token>) {
    out.extend(chunk_tokens.into_iter().map(|token| Token {
        start: token.start + base,
        ..token
    }));
}

/// Split at chapters, lex each chunk serially, rebase, concat.
pub fn lex_chunked(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let starts = chapter_chunk_starts(bytes);
    let mut tokens: Vec<Token> = Vec::with_capacity(source.len() / 6);
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(bytes.len());
        let chunk = crate::lex(&source[start..end]);
        rebased(chunk, start as u32, &mut tokens);
    }
    tokens
}

/// Split at chapters, lex chunks in parallel via rayon, rebase, concat in
/// source order.
#[cfg(feature = "par")]
pub fn lex_chunked_par(source: &str) -> Vec<Token> {
    use rayon::prelude::*;

    let bytes = source.as_bytes();
    let starts = chapter_chunk_starts(bytes);
    let chunked: Vec<Vec<Token>> = starts
        .par_iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = starts.get(i + 1).copied().unwrap_or(bytes.len());
            crate::lex(&source[start..end])
        })
        .collect();

    let mut tokens: Vec<Token> = Vec::with_capacity(source.len() / 6);
    for (chunk, &start) in chunked.into_iter().zip(starts.iter()) {
        rebased(chunk, start as u32, &mut tokens);
    }
    tokens
}
