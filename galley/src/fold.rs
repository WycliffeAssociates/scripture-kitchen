//! Use case: Per keystroke the pipeline pays: one pre-scan (~1ms/5MB), one checksum
//! checksum per "chapter" (byte range from \n\c). Onion internals (lex/cst/lint) intentionally model as per chapter units of work, where cross book concerns are added to an explicit Carry struct, and whole book products (cst, lint diagnostics) are completed via a final reduce
//! Entries are chunk-relative and NEVER mutate; Absolute offsets out the door.
//! The cache key is (content hash, chunk 0's carry-outs): the declared `\usfm` version and
//! the is-scripture bit gate local rules, so a chunk's product is a function
//! of its bytes AND that context. A dirty `\c` boundary (a sidebar
//! straddling the seam) is remembered as [`Cached::OpenBoundary`], so repeat
//! calls widen to the fused unit without re-lexing to rediscover it.
//!
//! INPUT CONTRACT: same as `onion::analyze` — checksums are only stable
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
    #[allow(dead_code)]
    cst: Cst,
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

/// handrolled byte-budget LRU (chunk-fold.md's ruling: at dozens-to-
/// hundreds of entries, eviction is a linear scan and a crate such as moka is currently overkill
pub struct FoldCache {
    map: HashMap<Key, Entry>,
    tick: u64,
    bytes: usize,
    budget: usize,
    misses: u64,
}

impl FoldCache {
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
        let bytes = text.as_bytes();
        let starts = onion::chunk::pre_scan(bytes).starts;

        // Resolve units front to back: chunk 0's product carries the context
        // every later key needs.
        let mut units: Vec<(u32, Rc<Products>)> = Vec::new();
        let mut ctx = ChunkContext::default();
        let mut context_key = ContextKey::SelfScanned;
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
                match self.map.get_mut(&key) {
                    Some(entry) => {
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
                    None => {}
                }
                let slice = &text[start..end];
                let tokens = onion::lex(slice);
                let chunk = cst::build_chunk(&tokens, end == bytes.len());
                if chunk.open_at_end && next < starts.len() {
                    self.insert(key, Cached::OpenBoundary, 64);
                    next += 1;
                    continue;
                }
                self.misses += 1;
                let (local, carried) =
                    lint::lint_chunk(slice.as_bytes(), &tokens, &chunk.cst, &ctx);
                let products = Rc::new(Products {
                    token_count: tokens.len() as u32,
                    bytes: product_bytes(&chunk.cst, &local),
                    cst: chunk.cst,
                    local,
                    carried,
                });
                let weight = products.bytes;
                self.insert(key, Cached::Unit(products.clone()), weight);
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

        // THE REDUCE: seams and finish over the recorded summaries — ~50
        // tiny structs, microseconds, no source, no tokens, no CST.
        let byte_bases: Vec<u32> = units.iter().map(|(base, _)| *base).collect();
        let token_bases: Vec<u32> = units
            .iter()
            .scan(0u32, |sum, (_, unit)| {
                let base = *sum;
                *sum += unit.token_count;
                Some(base)
            })
            .collect();
        let summaries: Vec<&Carried> = units.iter().map(|(_, unit)| &unit.carried).collect();
        let (book, version) = units
            .first()
            .map(|(_, unit)| (unit.local.book, unit.local.declared_version))
            .unwrap_or_default();
        let seam = lint::reduce(&summaries, &token_bases, &byte_bases, book, version);

        let locals: Vec<&LintReport> = units.iter().map(|(_, unit)| &unit.local).collect();
        lint::merge_reports(&locals, &token_bases, &byte_bases, seam)
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

/// Estimated resident size of one unit's products — the LRU weight. An
/// estimate is enough: the budget bounds memory class, not exact bytes.
fn product_bytes(cst: &Cst, local: &LintReport) -> usize {
    cst.nodes.len() * 16
        + cst.child_ids.len() * 4
        + local.observations.len() * 16
        + local.fix_of.len() * 4
        + local.fixes.len() * 24
        + local.edit_list.len() * 24
        + 128
}
