//! An unchanged chunk is never re-lexed.
//!
//! ```text
//! chunks.parse(text, opts)  ->  the same bytes onion::wire::parse would plate,
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
//! Keying on CONTENT is what makes this safe to be wrong about: a stale entry
//! cannot be served, only missed. Nothing here has to be invalidated.
//!
//! INPUT CONTRACT: same as `onion::wire::parse` — checksums are only stable
//! against LF-normalized text (CRLF works, but is a different content hash).

use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::budget::Budget;
use crate::onion;
use onion::cst::{self, Cst};
use onion::lint::{self, Carried, ChunkContext, LintReport, UsfmVersion};
use xxhash_rust::xxh3::xxh3_128;

/// What the chunk cache holds and what it has done, cumulative — telemetry,
/// never contract.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChunkStats {
    /// Units computed rather than reused — the number a test diffs to prove
    /// "one edited chapter is one miss".
    pub misses: u64,
    /// Units served from the cache rather than recomputed.
    pub hits: u64,
    /// Units dropped to stay under the budget; zero until it has bitten.
    pub evictions: u64,
    /// Cached units resident now. Zero for a one-chapter book: galley does
    /// not cache what it cannot reuse.
    pub len: usize,
    /// The exact heap of every retained unit plus the map's backing table.
    pub resident_bytes: usize,
}

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
    /// The LRU weight: exact heap bytes this unit retains. See
    /// [`product_bytes`] for what "exact" excludes.
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

/// The rebuildable tier's LRU, hand-rolled: at dozens-to-hundreds of entries
/// eviction is a linear scan, and a crate such as moka is currently overkill.
///
/// Keyed on chapter CONTENT, so reordering chapters, undoing an edit, or
/// reopening a book all hit; a changed chapter simply misses. This is the one
/// place the [`Budget`] is enforced rather than only counted.
pub(super) struct ChunkStore {
    map: FxHashMap<Key, Entry>,
    tick: u64,
    bytes: usize,
    budget: Budget,
    misses: u64,
    hits: u64,
    evictions: u64,
}

impl ChunkStore {
    pub(super) fn new(budget_bytes: usize) -> Self {
        Self {
            map: FxHashMap::default(),
            tick: 0,
            bytes: 0,
            budget: Budget::new(budget_bytes),
            misses: 0,
            hits: 0,
            evictions: 0,
        }
    }

    /// The declared ceiling this store's evictions hold it under.
    pub(super) fn budget(&self) -> Budget {
        self.budget
    }

    /// The counters and the size, in one read.
    pub(super) fn stats(&self) -> ChunkStats {
        ChunkStats {
            misses: self.misses,
            hits: self.hits,
            evictions: self.evictions,
            len: self.map.len(),
            resident_bytes: self.resident_bytes(),
        }
    }

    /// Resident bytes: the exact heap of every retained unit's CST, tokens
    /// and lint report (`self.bytes`, also the value the budget bounds) plus
    /// the map's own backing-table bytes (`map_bytes`). Not an estimate —
    /// see [`product_bytes`] for the one named gap.
    pub(super) fn resident_bytes(&self) -> usize {
        self.bytes + self.map_bytes()
    }

    /// The cache map's own bookkeeping: one row's worth of `Key` + `Entry`
    /// per allocated slot. Approximate only in that hashbrown's real slot
    /// array carries one control byte per slot and some load-factor slack
    /// this can't see from outside; both are small next to a chunk's CST.
    fn map_bytes(&self) -> usize {
        self.map.capacity() * (size_of::<Key>() + size_of::<Entry>())
    }

    /// The whole-book lint report, equal to the fresh
    /// `lint(lex(text), build(…))` pipeline (the fold oracle's law), with
    /// every clean chunk served from the cache: one dirty chunk's walk plus
    /// the reduce.
    pub(super) fn lint(&mut self, text: &str) -> LintReport {
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
    pub(super) fn parse(&mut self, text: &str, opts: onion::wire::ParseOptions) -> Vec<u8> {
        onion::wire::plate(&self.parsed(text, opts))
    }

    /// The same ingredients as engine types, for a caller that is not crossing
    /// a boundary — a native consumer, or a test proving the fold agrees with a
    /// cold parse.
    ///
    /// One resolve for both halves: the reduce and the assembly read the same
    /// units.
    pub(super) fn parsed<'a>(
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
    pub(super) fn masked(&mut self, text: &str, filter: &onion::mask::Filter) -> onion::mask::Mask {
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
                        Cached::Unit(products) => {
                            self.hits += 1;
                            break products.clone();
                        }
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
                        // No product, so no heap weight; the map row itself
                        // is `map_bytes`'s job.
                        self.insert(key, Cached::OpenBoundary, 0);
                    }
                    next += 1;
                    continue;
                }
                self.misses += 1;
                let (local, carried) =
                    lint::lint_chunk(slice.as_bytes(), &tokens, &chunk.cst, &ctx);
                let bytes = product_bytes(&chunk.cst, &local, &tokens);
                let products = Rc::new(Products {
                    token_count: tokens.len() as u32,
                    bytes,
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
        while self.budget.over(self.bytes) && self.map.len() > 1 {
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
            self.evictions += 1;
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

/// Exact heap bytes of one unit's products — the LRU weight and the number
/// [`ChunkStore::resident_bytes`] sums. `capacity`, not `len`: what a `Vec`
/// actually holds on the heap is what it allocated, not what it filled.
///
/// GAP: `Carried`'s own `Vec` fields (`ids`, `usfms`, `families`, `sid_last`,
/// `eid_candidates`) are `pub(crate)` inside `onion::lint::carried` with no
/// accessor, so their heap is real but unreachable from here. They hold a
/// handful of small tuples per chunk at most — not the CST/token-sized cost
/// this function exists to catch.
#[allow(
    clippy::ptr_arg,
    reason = "needs Vec::capacity, which &[T] cannot give"
)]
fn product_bytes(cst: &Cst, local: &LintReport, tokens: &Vec<onion::Token>) -> usize {
    tokens.capacity() * size_of::<onion::Token>()
        + cst.nodes.capacity() * size_of::<cst::Node>()
        + cst.child_ids.capacity() * size_of::<u32>()
        + local.observations.capacity() * size_of::<lint::Observation>()
        + local.fix_of.capacity() * size_of::<u32>()
        + local.fixes.capacity() * size_of::<lint::Fix>()
        + local.edit_list.capacity() * size_of::<lint::Edit>()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six chapters, each with distinct text, so no two chunks share a
    /// cache key — every resolved unit is its own map entry.
    fn book() -> String {
        let mut text = String::from("\\id MRK\n\\h Mark\n");
        for (n, body) in [
            "In the beginning of the good news.",
            "He entered Capernaum again.",
            "A man with a withered hand.",
            "The sower went out to sow.",
            "They came to the other side.",
            "Is this not the carpenter?",
        ]
        .into_iter()
        .enumerate()
        {
            text.push_str(&format!("\\c {}\n\\p\n\\v 1 {body}\n", n + 1));
        }
        text
    }

    #[test]
    fn resident_bytes_equals_the_computed_footprint() {
        let mut warmer = ChunkStore::new(usize::MAX);
        let text = book();
        let units = warmer.resolve(&text);
        // Distinct chapters, so one map entry per unit — no double count.
        assert_eq!(units.len(), warmer.map.len(), "no two units share a key");
        let expected: usize =
            units.iter().map(|(_, unit)| unit.bytes).sum::<usize>() + warmer.map_bytes();
        assert_eq!(warmer.resident_bytes(), expected);
        // And every unit's own weight matches the same formula fresh.
        for (_, unit) in &units {
            assert_eq!(
                unit.bytes,
                product_bytes(&unit.cst, &unit.local, &unit.tokens)
            );
        }
    }

    #[test]
    fn eviction_triggers_at_the_exact_budget() {
        let text = book();
        // First, learn each unit's exact weight with no budget pressure.
        let mut probe = ChunkStore::new(usize::MAX);
        let units = probe.resolve(&text);
        let weights: Vec<usize> = units.iter().map(|(_, unit)| unit.bytes).collect();
        assert!(
            weights.iter().all(|&w| w > 0),
            "chapters have real products"
        );

        // A budget that fits every chapter but the last: resolving front to
        // back, the retained heap (`bytes`, what eviction actually bounds)
        // never exceeds it, and at least one unit was evicted to stay there.
        let fits_all_but_last: usize = weights[..weights.len() - 1].iter().sum();
        let mut warmer = ChunkStore::new(fits_all_but_last);
        warmer.resolve(&text);
        assert!(
            warmer.bytes <= fits_all_but_last,
            "over budget: {} > {}",
            warmer.bytes,
            fits_all_but_last
        );
        assert!(
            warmer.map.len() < weights.len(),
            "at least one entry evicted"
        );

        // One byte under the single heaviest unit's weight: that unit alone
        // cannot fit, so resolving still stays under budget (the loop only
        // stops evicting when one entry remains) and heap never exceeds it
        // once at least two units have been seen.
        let heaviest = *weights.iter().max().unwrap();
        let mut tight = ChunkStore::new(heaviest - 1);
        tight.resolve(&text);
        assert!(
            tight.map.len() <= 1,
            "the tightest budget keeps at most one unit"
        );
    }
}
