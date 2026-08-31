//! The warmer: prepared plates, kept hot.
//!
//! **This is a cache.** The name is the kitchen the crate is named for — a
//! warmer holds food that is already cooked so it does not have to be cooked
//! again — and the metaphor stops there. What it holds is per-chunk products;
//! what it saves is re-deriving them.
//!
//! ```text
//! warmer.parse(text, opts)  ->  the same bytes onion::wire::parse would plate,
//!                               with every unchanged chunk served from memory
//! ```
//!
//! Per keystroke the pipeline otherwise pays: one pre-scan (~1ms/5MB) and one
//! checksum per "chapter" (the byte range from `\n\c`). Onion's internals
//! (lex/cst/lint) deliberately model per-chapter units of work, with
//! cross-book concerns collected into an explicit `Carried`, and whole-book
//! products (the tree, the findings) completed by a final reduce.
//!
//! That map-then-reduce IS a fold, and the word is kept for it — `reduce`,
//! `assemble`, `Products`. What is not a fold is this module, which used to be
//! called one: a cache that happens to use a fold is still a cache.
//!
//! Entries are chunk-relative and NEVER mutate; absolute offsets go out the
//! door. The cache key is (content hash, chunk 0's carry-outs): the declared
//! `\usfm` version and the is-scripture bit gate local rules, so a chunk's
//! product is a function of its bytes AND that context. A dirty `\c` boundary
//! (a sidebar straddling the seam) is remembered as [`Cached::OpenBoundary`],
//! so repeat calls widen to the fused unit without re-lexing to rediscover it.
//!
//! Keying on CONTENT is what makes the warmer safe to be wrong about: a stale
//! entry cannot be served, only missed. Nothing here has to be invalidated.
//!
//! INPUT CONTRACT: same as `onion::wire::parse` — checksums are only stable
//! against LF-normalized text (CRLF works, but is a different content hash).

use std::collections::HashMap;
use std::rc::Rc;

use crate::onion;
use onion::cst::{self, Cst};
use onion::lint::{self, Carried, ChunkContext, LintReport, UsfmVersion};
use xxhash_rust::xxh3::xxh3_128;

/// One cached unit's products — a chunk, or a fused run of chunks whose
/// interior boundaries were dirty. Everything chunk-relative.
struct Products {
    cst: Cst,
    /// The chunk's own token stream, `start` CHUNK-RELATIVE like everything
    /// else here. Rebased on assembly, never mutated in place.
    tokens: Vec<onion::Token>,
    /// The map half of lint (`onion::lint::lint_chunk`): chunk-relative,
    /// every finding this chunk can prove alone.
    local: LintReport,
    /// What the map OBSERVED across the boundary — reduce's input, and (for
    /// chunk 0) the carry-outs the rest of the book keys on.
    carried: Carried,
    token_count: u32,
    /// The LRU weight: estimated resident bytes.
    bytes: usize,
}

enum Cached {
    Unit(Rc<Products>),
    /// This span's chunk build ended with an open stack: the unit fuses with
    /// its successor. Remembering the verdict costs one map slot and saves
    /// re-lexing the span to rediscover it every call.
    OpenBoundary,
}

struct Entry {
    cached: Cached,
    last_tick: u64,
    bytes: usize,
}

/// The context half of a cache key. Chunk 0's product does not depend on a
/// passed context (its own scan is the source), so it keys on content alone;
/// every later chunk keys on chunk 0's carry-outs too.
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum ContextKey {
    SelfScanned,
    Carried {
        version: Option<UsfmVersion>,
        scripture: bool,
    },
}

#[derive(PartialEq, Eq, Hash)]
struct Key {
    hash: u128,
    context: ContextKey,
}

/// At or below this many chunks a book has AT MOST ONE chapter, so an entry can
/// only ever serve the front matter. Measured over en_ulb's five two-chunk
/// books (3JN, 2JN, PHM, OBA, JUD): a folded parse is 1.02–1.14x fresh, i.e.
/// break-even — while fifty keystrokes in Jude would leave 52 entries and 74 KB
/// resident for a 3.9 KB book. Bypassing is not about speed; it keeps entries
/// that never pay from evicting the large-book units that do.
const UNCACHED_CHUNKS: usize = 2;

/// The cache itself: a hand-rolled byte-budget LRU.
///
/// Hand-rolled per chunk-fold.md's ruling — at dozens-to-hundreds of entries
/// eviction is a linear scan, and a crate such as moka is currently overkill.
///
/// One per open document. Keyed on chapter CONTENT, so reordering chapters,
/// undoing an edit, or reopening a book all hit; a changed chapter simply
/// misses. The budget is a declared ceiling on resident products, which is
/// what makes this safe to keep across a session — wasm linear memory grows
/// and never shrinks, so a high-water mark is permanent.
pub struct Warmer {
    map: HashMap<Key, Entry>,
    tick: u64,
    bytes: usize,
    budget: usize,
    misses: u64,
}

impl Warmer {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            map: HashMap::new(),
            tick: 0,
            bytes: 0,
            budget: budget_bytes,
            misses: 0,
        }
    }

    /// Units computed rather than reused, cumulative — the number a test
    /// diffs to prove "one edited chapter is one miss".
    pub fn misses(&self) -> u64 {
        self.misses
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Resident product bytes (estimated), the value the budget bounds.
    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }

    /// The whole-book lint report, equal to the fresh
    /// `lint(lex(text), build(…))` pipeline (the fold oracle's law), with
    /// every clean chunk served from the cache: one dirty chunk's walk plus
    /// the reduce.
    pub fn lint(&mut self, text: &str) -> LintReport {
        let units = self.resolve(text);
        let (byte_bases, token_bases) = bases(&units);
        reduce(&units, &byte_bases, &token_bases)
    }

    /// The whole book, plated — with the lex, the CST build and the lint walk
    /// served from the cache for every chunk whose bytes did not change.
    ///
    /// The ingredients are assembled, not re-derived: `cst::concat` shifts the
    /// per-chunk trees into one, the token streams concatenate under their byte
    /// bases, and the folded report is the one `onion` would have computed. The
    /// dish those produce is byte-identical to a cold `onion::wire::parse`.
    pub fn parse(&mut self, text: &str, opts: onion::wire::ParseOptions) -> Vec<u8> {
        onion::wire::plate(&self.parsed(text, opts))
    }

    /// The same ingredients as engine types, for a caller that is not crossing
    /// a boundary — a native consumer, or a test proving the fold agrees with a
    /// cold parse.
    ///
    /// One resolve for both halves: the reduce and the assembly read the same
    /// units.
    pub fn parsed<'a>(
        &mut self,
        text: &'a str,
        opts: onion::wire::ParseOptions,
    ) -> onion::wire::Parsed<'a> {
        let units = self.resolve(text);
        let lint = opts.diagnostics.then(|| {
            let (byte_bases, token_bases) = bases(&units);
            reduce(&units, &byte_bases, &token_bases)
        });
        let (tokens, cst) = assemble(&units);
        let toc = opts.toc.then(|| onion::toc::toc(text.as_bytes(), &tokens));
        onion::wire::Parsed::from_parts(text, tokens, cst, lint, toc, opts)
    }

    /// One recipe's mask over the same assembled ingredients — what a
    /// downstream consumer of verse text reads. Not folded per chunk: the walk
    /// is cheap once the tree it walks is free.
    pub fn masked(&mut self, text: &str, filter: &onion::mask::Filter) -> onion::mask::Mask {
        let units = self.resolve(text);
        let (tokens, tree) = assemble(&units);
        onion::mask(text.as_bytes(), &tokens, &tree, filter)
    }

    /// Resolve units front to back, serving from the cache or computing. Chunk
    /// 0's product carries the context every later key needs, so the order is
    /// load-bearing.
    fn resolve(&mut self, text: &str) -> Vec<(u32, Rc<Products>)> {
        let bytes = text.as_bytes();
        let starts = onion::chunk::pre_scan(bytes).starts;

        // Resolve units front to back: chunk 0's product carries the context
        // every later key needs.
        let mut units: Vec<(u32, Rc<Products>)> = Vec::new();
        let mut ctx = ChunkContext::default();
        let mut context_key = ContextKey::SelfScanned;
        // A one-chapter book is computed and thrown away — see UNCACHED_CHUNKS.
        let cacheable = starts.len() > UNCACHED_CHUNKS;
        let mut at = 0usize;
        while at < starts.len() {
            let start = starts[at] as usize;
            let mut next = at + 1;
            let unit = loop {
                let end = starts.get(next).map_or(bytes.len(), |&s| s as usize);
                let key = Key {
                    hash: xxh3_128(&bytes[start..end]),
                    context: context_key,
                };
                if let Some(entry) = self.map.get_mut(&key).filter(|_| cacheable) {
                    self.tick += 1;
                    entry.last_tick = self.tick;
                    match &entry.cached {
                        Cached::Unit(products) => break products.clone(),
                        Cached::OpenBoundary if next < starts.len() => {
                            next += 1;
                            continue;
                        }
                        // A stale open verdict on what is now the final
                        // span cannot widen; recompute below.
                        Cached::OpenBoundary => {}
                    }
                }
                let slice = &text[start..end];
                let tokens = onion::lex(slice);
                let chunk = cst::build_chunk(&tokens, end == bytes.len());
                if chunk.open_at_end && next < starts.len() {
                    if cacheable {
                        self.insert(key, Cached::OpenBoundary, 64);
                    }
                    next += 1;
                    continue;
                }
                self.misses += 1;
                let (local, carried) =
                    lint::lint_chunk(slice.as_bytes(), &tokens, &chunk.cst, &ctx);
                let products = Rc::new(Products {
                    token_count: tokens.len() as u32,
                    bytes: product_bytes(&chunk.cst, &local, &tokens),
                    cst: chunk.cst,
                    tokens,
                    local,
                    carried,
                });
                if cacheable {
                    let weight = products.bytes;
                    self.insert(key, Cached::Unit(products.clone()), weight);
                }
                break products;
            };
            if at == 0 {
                ctx = ChunkContext {
                    declared_version: unit.carried.declared_version(),
                    book_is_scripture: Some(unit.carried.book_is_scripture()),
                };
                context_key = ContextKey::Carried {
                    version: unit.carried.declared_version(),
                    scripture: unit.carried.book_is_scripture(),
                };
            }
            units.push((start as u32, unit));
            at = next;
        }

        units
    }

    fn insert(&mut self, key: Key, cached: Cached, bytes: usize) {
        self.tick += 1;
        if let Some(old) = self.map.insert(
            key,
            Entry {
                cached,
                last_tick: self.tick,
                bytes,
            },
        ) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        // Evict least-recently-used while over budget: a linear scan over a
        // small map, microseconds. Never evicts what was just inserted (it
        // holds the newest tick, and going under budget stops the loop
        // before the map is empty in any sane configuration).
        while self.bytes > self.budget && self.map.len() > 1 {
            let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, entry)| entry.last_tick)
                .map(|(key, _)| Key {
                    hash: key.hash,
                    context: key.context,
                })
            else {
                break;
            };
            let evicted = self.map.remove(&oldest).expect("just found");
            self.bytes -= evicted.bytes;
        }
    }
}

/// THE REDUCE: seams and finish over the recorded summaries — ~50 tiny
/// structs, microseconds, no source, no tokens, no CST.
fn reduce(units: &[(u32, Rc<Products>)], byte_bases: &[u32], token_bases: &[u32]) -> LintReport {
    let summaries: Vec<&Carried> = units.iter().map(|(_, unit)| &unit.carried).collect();
    let (book, version) = units
        .first()
        .map(|(_, unit)| (unit.local.book, unit.local.declared_version))
        .unwrap_or_default();
    let seam = lint::reduce(&summaries, token_bases, byte_bases, book, version);
    let locals: Vec<&LintReport> = units.iter().map(|(_, unit)| &unit.local).collect();
    lint::merge_reports(&locals, token_bases, byte_bases, seam)
}

/// A unit's byte base and token base, index-aligned with `units`.
fn bases(units: &[(u32, Rc<Products>)]) -> (Vec<u32>, Vec<u32>) {
    let byte_bases = units.iter().map(|(base, _)| *base).collect();
    let token_bases = units
        .iter()
        .scan(0u32, |sum, (_, unit)| {
            let base = *sum;
            *sum += unit.token_count;
            Some(base)
        })
        .collect();
    (byte_bases, token_bases)
}

/// Per-chunk products → the whole book's ingredients. Tokens shift by their
/// chunk's byte base; the trees go through `cst::concat`, whose own oracle is
/// `concat(build_chunk each) == build(lex(whole))`.
fn assemble(units: &[(u32, Rc<Products>)]) -> (Vec<onion::Token>, Cst) {
    let total = units.iter().map(|(_, u)| u.tokens.len()).sum();
    let mut tokens: Vec<onion::Token> = Vec::with_capacity(total);
    for (base, unit) in units {
        tokens.extend(unit.tokens.iter().map(|token| onion::Token {
            start: token.start + base,
            ..*token
        }));
    }
    let parts: Vec<&Cst> = units.iter().map(|(_, unit)| &unit.cst).collect();
    let counts: Vec<u32> = units.iter().map(|(_, unit)| unit.token_count).collect();
    (tokens, cst::concat(&parts, &counts))
}

/// Estimated resident size of one unit's products — the LRU weight. An
/// estimate is enough: the budget bounds memory class, not exact bytes.
fn product_bytes(cst: &Cst, local: &LintReport, tokens: &[onion::Token]) -> usize {
    size_of_val(tokens)
        + cst.nodes.len() * 16
        + cst.child_ids.len() * 4
        + local.observations.len() * 16
        + local.fix_of.len() * 4
        + local.fixes.len() * 24
        + local.edit_list.len() * 24
        + 128
}
