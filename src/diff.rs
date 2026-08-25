//! `diff`: two documents, one gapless interleave of their verses.
//!
//! ```text
//! baseline  \id GEN ␊ \c 1 ␊ \v 1 one ␊ \v 2 two ␊
//! current   \id GEN ␊ \c 1 ␊ \v 2 two ␊ \v 1 one ␊
//!
//! let d = diff(baseline, current);
//! d.units  == [ GEN 0:0 Shared/Unchanged, GEN 1:0 Shared/Unchanged,
//!               GEN 1:2 Coalesced/Moved ]      ← one decision, two slots
//! d.slots  == [ Shared, Shared, PairCurrent, Shared, PairBaseline ]
//!
//! to_edits(&d, &decisions, MergeSide::Current)      // the replay
//! apply_splices(baseline, current, &edits) == current
//! ```
//!
//! Three layers, and this file is all three because they are one walk:
//!
//! 1. **Identity** — a BLOCK is the bytes between two consecutive [`Toc`]
//!    anchors (a `\c`, a `\v`, or the start of the file). The Toc already
//!    computed that fact, so nothing is re-derived per token and nothing owns
//!    text: a block is a [`Range<u32>`] plus a `Copy` [`Addr`].
//! 2. **Alignment** — Myers (via `similar`) over the block addresses, then the
//!    two-tier coalescing that turns a moved verse into ONE decision holding
//!    two slots.
//! 3. **Projection** — a merge is a walk of the slots emitting each unit's
//!    chosen side, as bytes ([`merge`]) or as replay splices ([`to_edits`]).
//!
//! Both inputs are read-only and nothing is shared between them: this is the
//! crate's first two-input API, and it runs lex + toc per side (the CST only
//! when a text diff is asked for).
//!
//! Ported from `usfm_onion`'s `src/diff/` — the algorithm is trusted and
//! carried over intact; what changed is the boundary, from cloned token vectors
//! to byte ranges of the caller's own two sources.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;

use similar::{Algorithm, ChangeTag, TextDiff, capture_diff_slices};

use crate::edit::SpliceEdit;
use crate::mask::{Filter, Mask, mask};
use crate::scanner::lex;
use crate::toc::{Toc, toc};
use crate::token::{Token, TokenKind};

// ---------------------------------------------------------------- addressing

/// What cut a block open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlockKind {
    /// The bytes before the first `\c` — `BOOK 0:0`.
    FrontMatter,
    /// A `\c` and whatever precedes its first `\v` — `BOOK 1:0`.
    ChapterOpen,
    /// A `\v` and its text — `BOOK 1:1`, `BOOK 1:1-3`.
    Verse,
}

/// A block's derived address: the sid string a UI shows, and the identity a
/// decision is keyed by — never minted, always re-derivable from the Toc.
///
/// ```text
/// GEN 0:0             front matter
/// GEN 1:0             chapter open
/// GEN 1:1-3           a bridge reports its whole range
/// GEN 1:1_dup_1       the second `\v 1` in one chapter occurrence
/// GEN 1:1_cdup_1      the first `\v 1` of a REOPENED chapter 1
/// GEN 1:1_cdup_1_dup_1
/// ```
///
/// The suffix order (`_cdup` then `_dup`) and their positional numbering are
/// onion's, kept byte-for-byte so a consumer's stored decisions still name the
/// same units. `_cdup_N` rides in the VERSE segment on purpose: the chapter
/// segment stays a bare integer, which is what keeps [`Addr::key`] parseable
/// for a repeated chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Addr {
    pub book: [u8; 3],
    pub chapter: u16,
    /// The verse numbers the designator names; `0` for a non-verse block and
    /// for a malformed designator (the Toc's own degrade-never-repair rule).
    pub first: u16,
    pub last: u16,
    /// Which occurrence of this CHAPTER number this block sits in.
    pub cdup: u16,
    /// Which occurrence of this verse range within that chapter occurrence.
    pub vdup: u16,
    pub kind: BlockKind,
}

/// The pairing key: book + chapter + verse START, and nothing else.
///
/// "Pair-loose" — it drops the range end and both dup counters, so a verse that
/// moved, was rebridged, or lost a duplicate can still pair. It never crosses a
/// verse NUMBER: a renumber typo is a delete plus an add, never a move.
type Key = ([u8; 3], u16, u16);

impl Addr {
    pub fn key(&self) -> Key {
        (self.book, self.chapter, self.first)
    }
}

impl core::fmt::Display for Addr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let end = self.book.iter().position(|b| *b == 0).unwrap_or(3);
        let code = String::from_utf8_lossy(&self.book[..end]);
        // A sid has to name something; three question marks read as "unknown
        // book" where three NUL bytes read as a corrupted string (toc.rs).
        f.write_str(if code.is_empty() { "###" } else { &code })?;
        write!(f, " {}:{}", self.chapter, self.first)?;
        if self.last > self.first {
            write!(f, "-{}", self.last)?;
        }
        if self.cdup > 0 {
            write!(f, "_cdup_{}", self.cdup)?;
        }
        if self.vdup > 0 {
            write!(f, "_dup_{}", self.vdup)?;
        }
        Ok(())
    }
}

/// One block: the bytes, and what they are called.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    range: Range<u32>,
    addr: Addr,
}

/// Cuts a source into blocks at its Toc anchors. Gapless and total: the blocks
/// TILE `0..len`, which is what makes the slot partition total.
fn blocks(len: u32, toc: &Toc) -> Vec<Block> {
    let mut cuts: Vec<Addr> = Vec::with_capacity(toc.verses.len() + toc.chapters.len());
    let mut offsets: Vec<u32> = Vec::with_capacity(cuts.capacity());

    let mut chapter_occurrence: HashMap<u16, u16> = HashMap::new();
    let mut verse_occurrence: HashMap<(u16, u16), u16> = HashMap::new();
    let mut chapter = 0u16;
    let mut cdup = 0u16;

    let (mut ci, mut vi) = (1usize, 0usize);
    loop {
        let next_chapter = toc.chapters.get(ci);
        let next_verse = toc.verses.get(vi);
        let take_chapter = match (next_chapter, next_verse) {
            (Some(c), Some(v)) => c.start < v.at,
            (Some(_), None) => true,
            _ => false,
        };
        if take_chapter {
            let row = next_chapter.expect("checked above");
            chapter = row.number;
            let seen = chapter_occurrence.entry(chapter).or_insert(0);
            cdup = *seen;
            *seen += 1;
            // Verse duplicate counting resets on every chapter OCCURRENCE, the
            // same way it resets on every `\c`.
            verse_occurrence.clear();
            offsets.push(row.start);
            cuts.push(Addr {
                book: toc.book,
                chapter,
                first: 0,
                last: 0,
                cdup,
                vdup: 0,
                kind: BlockKind::ChapterOpen,
            });
            ci += 1;
        } else if let Some(anchor) = next_verse {
            let seen = verse_occurrence
                .entry((anchor.first, anchor.last))
                .or_insert(0);
            let vdup = *seen;
            *seen += 1;
            offsets.push(anchor.at);
            cuts.push(Addr {
                book: toc.book,
                chapter,
                first: anchor.first,
                last: anchor.last,
                cdup,
                vdup,
                kind: BlockKind::Verse,
            });
            vi += 1;
        } else {
            break;
        }
    }

    let mut out = Vec::with_capacity(cuts.len() + 1);
    let first_cut = offsets.first().copied().unwrap_or(len);
    // An empty front matter (a file opening on `\c`) is no block at all —
    // onion's partition never emits a block for zero tokens.
    if first_cut > 0 {
        out.push(Block {
            range: 0..first_cut,
            addr: Addr {
                book: toc.book,
                chapter: 0,
                first: 0,
                last: 0,
                cdup: 0,
                vdup: 0,
                kind: BlockKind::FrontMatter,
            },
        });
    }
    for (index, addr) in cuts.into_iter().enumerate() {
        let end = offsets.get(index + 1).copied().unwrap_or(len);
        out.push(Block {
            range: offsets[index]..end,
            addr,
        });
    }
    out
}

// -------------------------------------------------------------------- shapes

/// Which side of a decision a caller chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MergeSide {
    Baseline,
    Current,
}

/// What one slot in the interleave emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRole {
    /// Aligned on both sides: emits whichever side was chosen.
    Shared,
    BaselineOnly,
    CurrentOnly,
    /// The baseline half of a coalesced pair — emits only if its ONE decision
    /// chose Baseline.
    PairBaseline,
    PairCurrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Shared,
    Added,
    Deleted,
    Coalesced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Unchanged,
    Modified,
    Added,
    Deleted,
    Moved,
}

/// The nearest preceding aligned slot — what a UI hangs a floating hunk on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub unit: u32,
    /// Which side's address the anchor speaks: a `PairBaseline` slot anchors by
    /// the baseline sid, everything else by the current one.
    pub side: MergeSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub unit: u32,
    pub role: SlotRole,
    pub after: Option<Anchor>,
}

/// How many blocks on each side share a unit's pairing key — the CROSS-document
/// narration ("this verse number appears 2x baseline, 1x current") the Toc's
/// within-one-document occurrence counts cannot state on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DupContext {
    pub baseline_count: u32,
    pub current_count: u32,
}

impl DupContext {
    pub fn is_dup(&self) -> bool {
        self.baseline_count > 1 || self.current_count > 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoveredSide {
    Baseline,
    Current,
}

/// Narration for a one-sided VERSE whose number a true bridge covers on the
/// opposite side of a coalesced pair. UI only — merge never reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoveredBy {
    pub unit: u32,
    pub addr: Addr,
    pub side: CoveredSide,
}

/// One decision: what a reviewer accepts or rejects as a single act.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionUnit {
    /// The derived sid string, made unique with an `@N` suffix if two blocks
    /// render the same address. Opaque to consumers — decisions travel as
    /// `{unitId: side}` and nothing may parse this.
    pub id: String,
    pub kind: UnitKind,
    pub status: Status,
    /// Bytes of the BASELINE source. Empty (and `baseline_addr` `None`) for an
    /// Added unit — presence is the address, never the range.
    pub baseline: Range<u32>,
    /// Bytes of the CURRENT source. Empty for a Deleted unit.
    pub current: Range<u32>,
    pub baseline_addr: Option<Addr>,
    pub current_addr: Option<Addr>,
    /// A coalesced pair whose two slots are out of relational order.
    pub displaced: bool,
    /// A coalesced pair that is byte-equal but differently addressed — the
    /// verse did not change, its NUMBER did.
    pub relabeled: bool,
    pub dup_context: DupContext,
    pub covered_by: Option<CoveredBy>,
    /// The two sides differ only in whitespace.
    pub is_whitespace_change: bool,
    /// Not whitespace-only, but the reader-visible text is the same: markup
    /// changed and nothing else.
    pub is_usfm_structure_change: bool,
}

/// The interleave: every byte of both inputs, in exactly one bearing slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSkeleton {
    pub slots: Vec<Slot>,
    pub units: Vec<DecisionUnit>,
    pub baseline_len: u32,
    pub current_len: u32,
}

/// A staged decision naming no unit in this skeleton. The caller must abort and
/// re-diff — there is no fuzzy stale-id fallback, and no output is assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    UnknownUnitId(String),
}

impl core::fmt::Display for MergeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownUnitId(id) => write!(f, "unknown decision unit id: {id}"),
        }
    }
}

impl std::error::Error for MergeError {}

/// `{unitId: side}` — the consumer contract, verbatim.
pub type Decisions = BTreeMap<String, MergeSide>;

// ------------------------------------------------------------------ the diff

/// Diffs two whole USFM sources. Lexes and indexes each side independently; no
/// state is shared between them.
pub fn diff(baseline: &str, current: &str) -> DiffSkeleton {
    let baseline_tokens = lex(baseline);
    let current_tokens = lex(current);
    diff_lexed(
        baseline.as_bytes(),
        &baseline_tokens,
        current.as_bytes(),
        &current_tokens,
    )
}

fn diff_lexed(
    baseline: &[u8],
    baseline_tokens: &[Token],
    current: &[u8],
    current_tokens: &[Token],
) -> DiffSkeleton {
    let baseline_toc = toc(baseline, baseline_tokens);
    let current_toc = toc(current, current_tokens);
    build_skeleton(
        Side {
            source: baseline,
            tokens: baseline_tokens,
            blocks: blocks(baseline.len() as u32, &baseline_toc),
        },
        Side {
            source: current,
            tokens: current_tokens,
            blocks: blocks(current.len() as u32, &current_toc),
        },
    )
}

struct Side<'a> {
    source: &'a [u8],
    tokens: &'a [Token],
    blocks: Vec<Block>,
}

impl Side<'_> {
    fn bytes(&self, range: &Range<u32>) -> &[u8] {
        &self.source[range.start as usize..range.end as usize]
    }
}

/// Matched `(baseline, current)` block indices from a Myers LCS over the block
/// addresses. `similar`'s Myers, no second LCS implementation.
fn myers_pairs(baseline: &[Addr], current: &[Addr]) -> Vec<(usize, usize)> {
    if baseline.is_empty() || current.is_empty() {
        return Vec::new();
    }
    if baseline == current {
        return (0..baseline.len()).map(|index| (index, index)).collect();
    }

    let mut pairs = Vec::new();
    let (mut bi, mut ci) = (0usize, 0usize);
    for op in capture_diff_slices(Algorithm::Myers, baseline, current) {
        for (tag, slice) in op.iter_slices(baseline, current) {
            let len = slice.len();
            match tag {
                ChangeTag::Equal => {
                    pairs.extend((0..len).map(|offset| (bi + offset, ci + offset)));
                    bi += len;
                    ci += len;
                }
                ChangeTag::Delete => bi += len,
                ChangeTag::Insert => ci += len,
            }
        }
    }
    pairs
}

enum Step {
    Shared(usize),
    BaselineOnly(usize),
    CurrentOnly(usize),
}

fn build_skeleton(baseline: Side<'_>, current: Side<'_>) -> DiffSkeleton {
    let baseline_addrs: Vec<Addr> = baseline.blocks.iter().map(|b| b.addr).collect();
    let current_addrs: Vec<Addr> = current.blocks.iter().map(|b| b.addr).collect();

    let pairs = myers_pairs(&baseline_addrs, &current_addrs);
    let shared_baseline: HashSet<usize> = pairs.iter().map(|&(b, _)| b).collect();
    let shared_current: HashSet<usize> = pairs.iter().map(|&(_, c)| c).collect();

    // Between LCS anchors: the baseline-only run, then the current-only run,
    // then the shared block — the supersequence order.
    let mut steps = Vec::with_capacity(baseline.blocks.len() + current.blocks.len());
    let (mut bi, mut ci) = (0usize, 0usize);
    for &(pb, pc) in &pairs {
        while bi < pb {
            steps.push(Step::BaselineOnly(bi));
            bi += 1;
        }
        while ci < pc {
            steps.push(Step::CurrentOnly(ci));
            ci += 1;
        }
        steps.push(Step::Shared(pb));
        bi = pb + 1;
        ci = pc + 1;
    }
    while bi < baseline.blocks.len() {
        steps.push(Step::BaselineOnly(bi));
        bi += 1;
    }
    while ci < current.blocks.len() {
        steps.push(Step::CurrentOnly(ci));
        ci += 1;
    }

    let baseline_only: Vec<usize> = (0..baseline.blocks.len())
        .filter(|index| !shared_baseline.contains(index))
        .collect();
    let current_only: Vec<usize> = (0..current.blocks.len())
        .filter(|index| !shared_current.contains(index))
        .collect();

    let ordered_pairs = coalesce(&baseline, &baseline_only, &current, &current_only);
    let paired_baseline: HashSet<usize> = ordered_pairs.iter().map(|&(b, _)| b).collect();
    let paired_current: HashSet<usize> = ordered_pairs.iter().map(|&(_, c)| c).collect();

    // dup_context counts EVERY block sharing a pairing key, shared and
    // off-Myers alike.
    let mut baseline_key_count: HashMap<Key, u32> = HashMap::new();
    for addr in &baseline_addrs {
        *baseline_key_count.entry(addr.key()).or_insert(0) += 1;
    }
    let mut current_key_count: HashMap<Key, u32> = HashMap::new();
    for addr in &current_addrs {
        *current_key_count.entry(addr.key()).or_insert(0) += 1;
    }

    // Creation order is the prototype's addUnit order: shared (Myers order),
    // coalesced (pairing order), deleted (baseline order), added (current
    // order) — it is what the `@N` id tiebreak and the fixtures both pin.
    let mut units: Vec<DecisionUnit> = Vec::new();
    let mut unit_for_baseline: HashMap<usize, u32> = HashMap::new();
    let mut unit_for_current: HashMap<usize, u32> = HashMap::new();
    let mut taken_ids: HashSet<String> = HashSet::new();

    let mut push = |units: &mut Vec<DecisionUnit>,
                    kind: UnitKind,
                    b: Option<usize>,
                    c: Option<usize>|
     -> u32 {
        let index = units.len() as u32;
        let baseline_block = b.map(|i| &baseline.blocks[i]);
        let current_block = c.map(|i| &current.blocks[i]);
        // The decision key is CURRENT-major where a current side exists: an
        // editor's staged decisions name what it is looking at.
        let want = current_block
            .or(baseline_block)
            .expect("a unit has at least one side")
            .addr
            .to_string();
        let id = unique_id(&mut taken_ids, want);
        let baseline_range = baseline_block.map(|b| b.range.clone()).unwrap_or(0..0);
        let current_range = current_block.map(|b| b.range.clone()).unwrap_or(0..0);
        let byte_equal = match (baseline_block, current_block) {
            (Some(_), Some(_)) => baseline.bytes(&baseline_range) == current.bytes(&current_range),
            _ => false,
        };
        let status = match kind {
            UnitKind::Shared if byte_equal => Status::Unchanged,
            UnitKind::Shared => Status::Modified,
            UnitKind::Deleted => Status::Deleted,
            UnitKind::Added => Status::Added,
            // Finalized once slot positions are known: Unchanged (same
            // relational position) or Moved (displaced). Moved is the safe
            // interim value.
            UnitKind::Coalesced if byte_equal => Status::Moved,
            UnitKind::Coalesced => Status::Modified,
        };
        let both = baseline_block.is_some() && current_block.is_some();
        let (is_whitespace_change, is_usfm_structure_change) = if both && !byte_equal {
            classify(&baseline, &baseline_range, &current, &current_range)
        } else {
            (false, false)
        };
        let key = baseline_block
            .or(current_block)
            .expect("a unit has at least one side")
            .addr
            .key();
        units.push(DecisionUnit {
            id,
            kind,
            status,
            baseline: baseline_range,
            current: current_range,
            baseline_addr: baseline_block.map(|b| b.addr),
            current_addr: current_block.map(|b| b.addr),
            displaced: false,
            relabeled: matches!(kind, UnitKind::Coalesced)
                && byte_equal
                && baseline_block.map(|b| b.addr) != current_block.map(|b| b.addr),
            dup_context: DupContext {
                baseline_count: baseline_key_count.get(&key).copied().unwrap_or(0),
                current_count: current_key_count.get(&key).copied().unwrap_or(0),
            },
            covered_by: None,
            is_whitespace_change,
            is_usfm_structure_change,
        });
        index
    };

    for &(pb, pc) in &pairs {
        let index = push(&mut units, UnitKind::Shared, Some(pb), Some(pc));
        unit_for_baseline.insert(pb, index);
        unit_for_current.insert(pc, index);
    }
    for &(b, c) in &ordered_pairs {
        let index = push(&mut units, UnitKind::Coalesced, Some(b), Some(c));
        unit_for_baseline.insert(b, index);
        unit_for_current.insert(c, index);
    }
    for &b in &baseline_only {
        if paired_baseline.contains(&b) {
            continue;
        }
        let index = push(&mut units, UnitKind::Deleted, Some(b), None);
        unit_for_baseline.insert(b, index);
    }
    for &c in &current_only {
        if paired_current.contains(&c) {
            continue;
        }
        let index = push(&mut units, UnitKind::Added, None, Some(c));
        unit_for_current.insert(c, index);
    }

    let mut slots: Vec<Slot> = Vec::with_capacity(steps.len());
    for step in &steps {
        let (unit, role) = match *step {
            Step::Shared(pb) => (unit_for_baseline[&pb], SlotRole::Shared),
            Step::BaselineOnly(b) => {
                let unit = unit_for_baseline[&b];
                let role = match units[unit as usize].kind {
                    UnitKind::Coalesced => SlotRole::PairBaseline,
                    _ => SlotRole::BaselineOnly,
                };
                (unit, role)
            }
            Step::CurrentOnly(c) => {
                let unit = unit_for_current[&c];
                let role = match units[unit as usize].kind {
                    UnitKind::Coalesced => SlotRole::PairCurrent,
                    _ => SlotRole::CurrentOnly,
                };
                (unit, role)
            }
        };
        slots.push(Slot {
            unit,
            role,
            after: None,
        });
    }

    finalize_displacement(&mut units, &slots);
    finalize_covered_by(&mut units);
    finalize_anchors(&mut slots, &units);

    DiffSkeleton {
        slots,
        units,
        baseline_len: baseline.source.len() as u32,
        current_len: current.source.len() as u32,
    }
}

/// Off-Myers blocks sharing a pairing key, paired in two tiers: byte-identical
/// text first (stream order among ties), then the positional leftovers
/// first-with-first. Keys are visited in baseline stream first-occurrence order
/// so unit creation order is deterministic.
fn coalesce(
    baseline: &Side<'_>,
    baseline_only: &[usize],
    current: &Side<'_>,
    current_only: &[usize],
) -> Vec<(usize, usize)> {
    let mut by_key: HashMap<Key, Vec<usize>> = HashMap::new();
    let mut key_order: Vec<Key> = Vec::new();
    for &index in baseline_only {
        let key = baseline.blocks[index].addr.key();
        if by_key.entry(key).or_default().is_empty() {
            key_order.push(key);
        }
        by_key.get_mut(&key).expect("just inserted").push(index);
    }
    let mut current_by_key: HashMap<Key, Vec<usize>> = HashMap::new();
    for &index in current_only {
        current_by_key
            .entry(current.blocks[index].addr.key())
            .or_default()
            .push(index);
    }

    let mut out = Vec::new();
    for key in &key_order {
        let bis = by_key.get(key).cloned().unwrap_or_default();
        let cis = current_by_key.get(key).cloned().unwrap_or_default();
        let mut used: HashSet<usize> = HashSet::new();
        let mut left: Vec<usize> = Vec::new();

        for &b in &bis {
            let matched = cis.iter().copied().find(|c| {
                !used.contains(c)
                    && baseline.bytes(&baseline.blocks[b].range)
                        == current.bytes(&current.blocks[*c].range)
            });
            match matched {
                Some(c) => {
                    used.insert(c);
                    out.push((b, c));
                }
                None => left.push(b),
            }
        }

        let remaining: Vec<usize> = cis.iter().copied().filter(|c| !used.contains(c)).collect();
        for (offset, &b) in left.iter().enumerate() {
            if let Some(&c) = remaining.get(offset) {
                out.push((b, c));
            }
        }
    }
    out
}

/// The rendered address, made unique in CREATION order: `@1`, `@2`, … Two
/// blocks render the same address only when a designator was malformed (a
/// `\v 2"` addresses as `BOOK c:0`, like the chapter open), so this is the
/// degrade path, not a normal one.
fn unique_id(taken: &mut HashSet<String>, want: String) -> String {
    if taken.insert(want.clone()) {
        return want;
    }
    for suffix in 1u32.. {
        let candidate = format!("{want}@{suffix}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("the suffix range is unbounded")
}

/// A coalesced pair is displaced iff its current slot precedes its baseline
/// slot, or a Shared slot sits strictly between them. One-sided Added/Deleted
/// slots between the two do not count.
fn finalize_displacement(units: &mut [DecisionUnit], slots: &[Slot]) {
    let mut baseline_slot: HashMap<u32, usize> = HashMap::new();
    let mut current_slot: HashMap<u32, usize> = HashMap::new();
    for (index, slot) in slots.iter().enumerate() {
        match slot.role {
            SlotRole::PairBaseline => {
                baseline_slot.insert(slot.unit, index);
            }
            SlotRole::PairCurrent => {
                current_slot.insert(slot.unit, index);
            }
            _ => {}
        }
    }

    for (index, unit) in units.iter_mut().enumerate() {
        if !matches!(unit.kind, UnitKind::Coalesced) {
            continue;
        }
        let index = index as u32;
        let (Some(&base), Some(&cur)) = (baseline_slot.get(&index), current_slot.get(&index))
        else {
            continue;
        };
        let between = slots[base.min(cur) + 1..base.max(cur)]
            .iter()
            .any(|slot| matches!(slot.role, SlotRole::Shared));
        unit.displaced = cur < base || between;
        // A byte-different pair stays Modified however far it moved;
        // displacement is still narrated on the flag.
        if !matches!(unit.status, Status::Modified) {
            unit.status = if unit.displaced {
                Status::Moved
            } else {
                Status::Unchanged
            };
        }
    }
}

fn finalize_covered_by(units: &mut [DecisionUnit]) {
    let bridges: Vec<(u32, Option<Addr>, Option<Addr>)> = units
        .iter()
        .enumerate()
        .filter(|(_, unit)| matches!(unit.kind, UnitKind::Coalesced))
        .map(|(index, unit)| (index as u32, unit.baseline_addr, unit.current_addr))
        .collect();

    for unit in units.iter_mut() {
        let deleted = match unit.kind {
            UnitKind::Deleted => true,
            UnitKind::Added => false,
            _ => continue,
        };
        let own = if deleted {
            unit.baseline_addr
        } else {
            unit.current_addr
        };
        let Some(own) = own else { continue };
        // A one-sided BRIDGE is its own range event, not something a
        // neighbouring bridge covers; and an unnumbered block covers nothing.
        if own.first == 0 || own.first != own.last {
            continue;
        }
        for &(candidate, baseline_addr, current_addr) in &bridges {
            let cover = if deleted { current_addr } else { baseline_addr };
            let Some(cover) = cover else { continue };
            if cover.last <= cover.first || cover.book != own.book || cover.chapter != own.chapter {
                continue;
            }
            if own.first <= cover.last && own.last >= cover.first {
                unit.covered_by = Some(CoveredBy {
                    unit: candidate,
                    addr: cover,
                    side: if deleted {
                        CoveredSide::Current
                    } else {
                        CoveredSide::Baseline
                    },
                });
                break;
            }
        }
    }
}

/// Every slot's `after` is the nearest preceding aligned slot. A one-sided
/// Added/Deleted slot never becomes an anchor — it is the thing that needs one.
fn finalize_anchors(slots: &mut [Slot], units: &[DecisionUnit]) {
    let mut last: Option<Anchor> = None;
    for slot in slots.iter_mut() {
        slot.after = last;
        let side = match slot.role {
            SlotRole::PairBaseline => MergeSide::Baseline,
            SlotRole::Shared | SlotRole::PairCurrent => MergeSide::Current,
            _ => continue,
        };
        let unit = &units[slot.unit as usize];
        let addr = match side {
            MergeSide::Baseline => unit.baseline_addr,
            MergeSide::Current => unit.current_addr,
        };
        if addr.is_some() {
            last = Some(Anchor {
                unit: slot.unit,
                side,
            });
        }
    }
}

// --------------------------------------------------------------- classifiers

/// `(is_whitespace_change, is_usfm_structure_change)` for two byte ranges that
/// are known to differ.
///
/// Both walks are one pass with no allocation and short-circuit on the first
/// divergence; the structure walk runs only when the whitespace one says no,
/// because a whole-corpus reformat is overwhelmingly whitespace-only.
fn classify(
    baseline: &Side<'_>,
    baseline_range: &Range<u32>,
    current: &Side<'_>,
    current_range: &Range<u32>,
) -> (bool, bool) {
    let ws = eq_stripped(
        baseline.bytes(baseline_range).iter().copied(),
        current.bytes(current_range).iter().copied(),
    );
    let usfm = !ws
        && eq_stripped(
            reader_bytes(baseline, baseline_range),
            reader_bytes(current, current_range),
        );
    (ws, usfm)
}

fn eq_stripped(a: impl Iterator<Item = u8>, b: impl Iterator<Item = u8>) -> bool {
    a.filter(|byte| !byte.is_ascii_whitespace())
        .eq(b.filter(|byte| !byte.is_ascii_whitespace()))
}

/// The bytes of a range's `Text` tokens — reader-visible text at TOKEN grain.
/// Markers, designators, note callers, book codes and attribute lists are
/// skipped by kind; the CST never enters.
fn reader_bytes<'a>(side: &'a Side<'a>, range: &Range<u32>) -> impl Iterator<Item = u8> + 'a {
    token_slice(side.tokens, range)
        .iter()
        .filter(|token| token.kind() == TokenKind::Text)
        .flat_map(|token| {
            side.source[token.start as usize..token.end() as usize]
                .iter()
                .copied()
        })
}

/// The tokens inside a block. Free: block boundaries ARE token starts, so two
/// binary searches recover the slice exactly.
fn token_slice<'a>(tokens: &'a [Token], range: &Range<u32>) -> &'a [Token] {
    let start = tokens.partition_point(|token| token.start < range.start);
    let end = tokens.partition_point(|token| token.start < range.end);
    &tokens[start..end]
}

// ------------------------------------------------------------------- merging

/// Rejects a decision naming no unit. Runs before ANY output is assembled: a
/// stale id must never half-produce a document.
fn check_decisions(skeleton: &DiffSkeleton, decisions: &Decisions) -> Result<(), MergeError> {
    let known: HashSet<&str> = skeleton.units.iter().map(|unit| unit.id.as_str()).collect();
    match decisions.keys().find(|id| !known.contains(id.as_str())) {
        Some(id) => Err(MergeError::UnknownUnitId(id.clone())),
        None => Ok(()),
    }
}

/// Walks the slots and hands each emitted chunk to `emit` as `(side, range)`.
/// The pure projection both merge shapes are: never trims, never normalizes,
/// never reserializes.
fn project(
    skeleton: &DiffSkeleton,
    decisions: &Decisions,
    default_side: MergeSide,
    mut emit: impl FnMut(MergeSide, Range<u32>),
) -> Result<(), MergeError> {
    check_decisions(skeleton, decisions)?;
    for slot in &skeleton.slots {
        let unit = &skeleton.units[slot.unit as usize];
        let side = decisions.get(&unit.id).copied().unwrap_or(default_side);
        match slot.role {
            // An Unchanged Shared unit holds the same bytes on both sides, so
            // reading it out of the baseline is the same document and costs
            // [`to_edits`] nothing — which is what makes a book against itself
            // an EMPTY edit list instead of one splice per verse.
            SlotRole::Shared if unit.status == Status::Unchanged => {
                emit(MergeSide::Baseline, unit.baseline.clone())
            }
            SlotRole::Shared => match side {
                MergeSide::Baseline => emit(MergeSide::Baseline, unit.baseline.clone()),
                MergeSide::Current => emit(MergeSide::Current, unit.current.clone()),
            },
            SlotRole::BaselineOnly | SlotRole::PairBaseline => {
                if side == MergeSide::Baseline {
                    emit(MergeSide::Baseline, unit.baseline.clone());
                }
            }
            SlotRole::CurrentOnly | SlotRole::PairCurrent => {
                if side == MergeSide::Current {
                    emit(MergeSide::Current, unit.current.clone());
                }
            }
        }
    }
    Ok(())
}

/// The merged document. `baseline` and `current` must be the sources the
/// skeleton was built from.
pub fn merge(
    skeleton: &DiffSkeleton,
    baseline: &[u8],
    current: &[u8],
    decisions: &Decisions,
    default_side: MergeSide,
) -> Result<Vec<u8>, MergeError> {
    let mut out = Vec::with_capacity(baseline.len());
    project(skeleton, decisions, default_side, |side, range| {
        let source = match side {
            MergeSide::Baseline => baseline,
            MergeSide::Current => current,
        };
        out.extend_from_slice(&source[range.start as usize..range.end as usize]);
    })?;
    Ok(out)
}

/// Reverting one unit IS a one-decision merge: `{id: Baseline}` over a
/// `Current` default. An unknown id is an error, never a fuzzy match.
pub fn revert(
    skeleton: &DiffSkeleton,
    baseline: &[u8],
    current: &[u8],
    unit_id: &str,
) -> Result<Vec<u8>, MergeError> {
    let mut decisions = Decisions::new();
    decisions.insert(unit_id.to_string(), MergeSide::Baseline);
    merge(skeleton, baseline, current, &decisions, MergeSide::Current)
}

/// The same merge as replay splices over the BASELINE — sorted by `from`,
/// non-overlapping, and byte-identical to [`merge`] once applied.
///
/// Baseline blocks appear in slot order in ascending byte order (the interleave
/// walks each side forward), so a dropped baseline block is a gap and a chosen
/// current block is an insertion at the gap's edge.
pub fn to_edits(
    skeleton: &DiffSkeleton,
    decisions: &Decisions,
    default_side: MergeSide,
) -> Result<Vec<SpliceEdit>, MergeError> {
    let mut edits: Vec<SpliceEdit> = Vec::new();
    // The open replacement run: baseline `from..to` and the current ranges
    // going in its place.
    let mut open = false;
    let mut from = 0u32;
    let mut to = 0u32;
    let mut inserts: Vec<Range<u32>> = Vec::new();
    let mut consumed = 0u32;

    let flush = |edits: &mut Vec<SpliceEdit>,
                 open: &mut bool,
                 from: u32,
                 to: u32,
                 inserts: &mut Vec<Range<u32>>| {
        if !*open {
            return;
        }
        *open = false;
        if inserts.is_empty() {
            if from < to {
                edits.push(SpliceEdit {
                    from,
                    to,
                    insert: 0..0,
                });
            }
            return;
        }
        // A run holds one deletion (always its last act) and any number of
        // insertions; the extra inserts ride as empty splices at the run's end,
        // which apply in list order.
        for (index, insert) in inserts.drain(..).enumerate() {
            edits.push(SpliceEdit {
                from: if index == 0 { from } else { to },
                to,
                insert,
            });
        }
    };

    project(
        skeleton,
        decisions,
        default_side,
        |side, range| match side {
            MergeSide::Current => {
                if !open {
                    open = true;
                    from = consumed;
                    to = consumed;
                }
                match inserts.last_mut() {
                    Some(last) if last.end == range.start => last.end = range.end,
                    _ => inserts.push(range),
                }
            }
            MergeSide::Baseline => {
                if range.start > consumed {
                    if !open {
                        open = true;
                        from = consumed;
                    }
                    to = range.start;
                }
                flush(&mut edits, &mut open, from, to, &mut inserts);
                consumed = range.end;
            }
        },
    )?;

    if consumed < skeleton.baseline_len {
        if !open {
            open = true;
            from = consumed;
        }
        to = skeleton.baseline_len;
    }
    flush(&mut edits, &mut open, from, to, &mut inserts);
    Ok(edits)
}

// ----------------------------------------------------------------- text diff

/// Requested granularity for the intra-unit text diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDiffMode {
    #[default]
    None,
    /// UAX-29 word boundaries.
    Words,
    /// Grapheme clusters.
    Chars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDiffRun {
    pub text: String,
    pub kind: RunKind,
}

/// Per-unit presentation metadata, riding BESIDE the unit — the skeleton, the
/// slots and the merge are byte-identical whether or not this was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTextDiff {
    /// Kinds: `Unchanged` | `Removed`.
    pub baseline: Vec<TextDiffRun>,
    /// Kinds: `Unchanged` | `Added`.
    pub current: Vec<TextDiffRun>,
}

/// One side's reader-visible bytes: the [`Filter::reader_text`] mask over a
/// source, sliceable by a unit's byte range.
pub struct ReaderText<'a> {
    pub source: &'a [u8],
    pub mask: Mask,
}

impl<'a> ReaderText<'a> {
    /// Builds the CST and the mask. This is the ONLY place the diff needs a
    /// tree — alignment is a token-grain fact.
    pub fn new(source: &'a [u8], tokens: &[Token]) -> Self {
        let cst = crate::cst::build(tokens);
        Self {
            source,
            mask: mask(source, tokens, &cst, &Filter::reader_text()),
        }
    }

    /// The reader-visible text inside a byte range, in one allocation.
    pub fn slice(&self, range: &Range<u32>) -> String {
        let first = self
            .mask
            .ranges
            .partition_point(|kept| kept.end <= range.start);
        let mut out = String::new();
        for kept in &self.mask.ranges[first..] {
            if kept.start >= range.end {
                break;
            }
            let start = kept.start.max(range.start) as usize;
            let end = kept.end.min(range.end) as usize;
            out.push_str(
                core::str::from_utf8(&self.source[start..end])
                    .expect("mask ranges are token spans, so they fall on character boundaries"),
            );
        }
        out
    }
}

/// Word/grapheme runs inside one unit. Pure, deterministic, and status-gated:
/// `Unchanged`/`Moved` yield `None` (a pure move must not highlight),
/// `Added`/`Deleted` one unbroken run, only `Modified` split runs.
pub fn unit_text_diff(
    unit: &DecisionUnit,
    baseline: &ReaderText<'_>,
    current: &ReaderText<'_>,
    mode: TextDiffMode,
) -> Option<UnitTextDiff> {
    if mode == TextDiffMode::None {
        return None;
    }
    match unit.status {
        Status::Unchanged | Status::Moved => None,
        Status::Added => Some(UnitTextDiff {
            baseline: Vec::new(),
            current: single_run(current.slice(&unit.current), RunKind::Added),
        }),
        Status::Deleted => Some(UnitTextDiff {
            baseline: single_run(baseline.slice(&unit.baseline), RunKind::Removed),
            current: Vec::new(),
        }),
        Status::Modified => Some(split_runs(
            &baseline.slice(&unit.baseline),
            &current.slice(&unit.current),
            mode,
        )),
    }
}

/// Diffs and computes every unit's text runs, index-aligned with
/// `skeleton.units`. Serial.
pub fn diff_with_text(
    baseline: &str,
    current: &str,
    mode: TextDiffMode,
) -> (DiffSkeleton, Vec<Option<UnitTextDiff>>) {
    let baseline_tokens = lex(baseline);
    let current_tokens = lex(current);
    let skeleton = diff_lexed(
        baseline.as_bytes(),
        &baseline_tokens,
        current.as_bytes(),
        &current_tokens,
    );
    if mode == TextDiffMode::None {
        let texts = vec![None; skeleton.units.len()];
        return (skeleton, texts);
    }
    let baseline_text = ReaderText::new(baseline.as_bytes(), &baseline_tokens);
    let current_text = ReaderText::new(current.as_bytes(), &current_tokens);
    let texts = skeleton
        .units
        .iter()
        .map(|unit| unit_text_diff(unit, &baseline_text, &current_text, mode))
        .collect();
    (skeleton, texts)
}

fn single_run(text: String, kind: RunKind) -> Vec<TextDiffRun> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![TextDiffRun { text, kind }]
    }
}

fn split_runs(baseline: &str, current: &str, mode: TextDiffMode) -> UnitTextDiff {
    let diff = match mode {
        TextDiffMode::Words => TextDiff::from_unicode_words(baseline, current),
        TextDiffMode::Chars => TextDiff::from_graphemes(baseline, current),
        TextDiffMode::None => unreachable!("the caller returns early for None"),
    };

    let mut baseline_runs = Vec::new();
    let mut current_runs = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.as_str().unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        match change.tag() {
            ChangeTag::Equal => {
                push_run(&mut baseline_runs, text, RunKind::Unchanged);
                push_run(&mut current_runs, text, RunKind::Unchanged);
            }
            ChangeTag::Delete => push_run(&mut baseline_runs, text, RunKind::Removed),
            ChangeTag::Insert => push_run(&mut current_runs, text, RunKind::Added),
        }
    }
    UnitTextDiff {
        baseline: baseline_runs,
        current: current_runs,
    }
}

fn push_run(runs: &mut Vec<TextDiffRun>, text: &str, kind: RunKind) {
    match runs.last_mut() {
        Some(last) if last.kind == kind => last.text.push_str(text),
        _ => runs.push(TextDiffRun {
            text: text.to_string(),
            kind,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addrs(source: &str) -> Vec<String> {
        let tokens = lex(source);
        let table = toc(source.as_bytes(), &tokens);
        blocks(source.len() as u32, &table)
            .iter()
            .map(|block| block.addr.to_string())
            .collect()
    }

    fn spans(source: &str) -> Vec<Range<u32>> {
        let tokens = lex(source);
        let table = toc(source.as_bytes(), &tokens);
        blocks(source.len() as u32, &table)
            .iter()
            .map(|block| block.range.clone())
            .collect()
    }

    #[test]
    fn blocks_tile_the_whole_source() {
        for source in [
            "",
            "\\c 1\n",
            "\\id GEN\n\\c 1\n\\p \\v 1 a \\v 2 b\n",
            "\\id GEN\n\\h Genesis\n",
            "no markers at all\n",
        ] {
            let spans = spans(source);
            let mut at = 0u32;
            for span in &spans {
                assert_eq!(span.start, at, "gap before {span:?} in {source:?}");
                at = span.end;
            }
            assert_eq!(at, source.len() as u32, "blocks must reach the end");
        }
    }

    #[test]
    fn a_file_opening_on_a_chapter_has_no_front_matter_block() {
        assert_eq!(addrs("\\c 1\n\\v 1 a\n"), vec!["### 1:0", "### 1:1"]);
        // …and an empty file is no blocks at all.
        assert!(addrs("").is_empty());
    }

    #[test]
    fn addresses_render_the_toc_facts() {
        assert_eq!(
            addrs("\\id GEN\n\\c 1\n\\p\n\\v 1-3 a\n\\v 4 b\n"),
            vec!["GEN 0:0", "GEN 1:0", "GEN 1:1-3", "GEN 1:4"]
        );
        // A book with no `\id` still names something.
        assert_eq!(addrs("\\c 2\n\\v 5 a\n"), vec!["### 2:0", "### 2:5"]);
    }

    #[test]
    fn a_malformed_designator_degrades_to_a_verse_zero_address() {
        // `\v 2"` (the real en_ulb ZEC 12:7 shape) carves a designator the
        // reader refuses, so the Toc row keeps 0 — and the block addresses as
        // `GEN 1:0`, colliding with the chapter open. The `@N` tiebreak is what
        // keeps the two decisions distinct.
        let source = "\\id GEN\n\\c 1\n\\v 2\" b\n";
        assert_eq!(addrs(source), vec!["GEN 0:0", "GEN 1:0", "GEN 1:0"]);
        let skeleton = diff(source, source);
        let ids: Vec<&str> = skeleton.units.iter().map(|unit| unit.id.as_str()).collect();
        assert_eq!(ids, vec!["GEN 0:0", "GEN 1:0", "GEN 1:0@1"]);

        // A verse with NO designator at all (the designator gate makes
        // `\v Then` one of these) cuts the same block at the same marker anchor
        // and addresses the same way — the tiebreak carries the distinction.
        let source = "\\id GEN\n\\c 1\n\\v Then He declared\n";
        assert_eq!(addrs(source), vec!["GEN 0:0", "GEN 1:0", "GEN 1:0"]);
        let skeleton = diff(source, source);
        let ids: Vec<&str> = skeleton.units.iter().map(|unit| unit.id.as_str()).collect();
        assert_eq!(ids, vec!["GEN 0:0", "GEN 1:0", "GEN 1:0@1"]);
    }

    #[test]
    fn a_units_token_slice_is_exactly_its_bytes() {
        let source = "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\v 2 b\n";
        let tokens = lex(source);
        let table = toc(source.as_bytes(), &tokens);
        for block in blocks(source.len() as u32, &table) {
            let slice = token_slice(&tokens, &block.range);
            assert_eq!(slice.first().map(|t| t.start), Some(block.range.start));
            assert_eq!(slice.last().map(|t| t.end()), Some(block.range.end));
        }
    }

    #[test]
    fn a_pure_insertion_is_one_zero_width_splice() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 a\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 a\n\\v 2 b\n";
        let skeleton = diff(baseline, current);
        let edits = to_edits(&skeleton, &Decisions::new(), MergeSide::Current).unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].from, baseline.len() as u32);
        assert_eq!(edits[0].to, baseline.len() as u32);
        assert_eq!(
            &current[edits[0].insert.start as usize..edits[0].insert.end as usize],
            "\\v 2 b\n"
        );
    }

    #[test]
    fn a_pure_deletion_is_one_empty_splice() {
        let baseline = "\\id GEN\n\\c 1\n\\v 1 a\n\\v 2 b\n";
        let current = "\\id GEN\n\\c 1\n\\v 1 a\n";
        let skeleton = diff(baseline, current);
        let edits = to_edits(&skeleton, &Decisions::new(), MergeSide::Current).unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].insert.is_empty());
        assert_eq!(
            &baseline[edits[0].from as usize..edits[0].to as usize],
            "\\v 2 b\n"
        );
    }

    #[test]
    fn the_classifiers_read_at_token_grain() {
        // Whitespace only.
        let skeleton = diff("\\id G\n\\c 1\n\\v 1 a b\n", "\\id G\n\\c 1\n\\v 1 a  b\n");
        let unit = skeleton
            .units
            .iter()
            .find(|unit| unit.status == Status::Modified)
            .unwrap();
        assert!(unit.is_whitespace_change && !unit.is_usfm_structure_change);

        // Markup only: the Text tokens are equal, the markers are not.
        let skeleton = diff(
            "\\id G\n\\c 1\n\\v 1 a b\n",
            "\\id G\n\\c 1\n\\v 1 a \\bk b\\bk*\n",
        );
        let unit = skeleton
            .units
            .iter()
            .find(|unit| unit.status == Status::Modified)
            .unwrap();
        assert!(unit.is_usfm_structure_change && !unit.is_whitespace_change);

        // A real content change is neither.
        let skeleton = diff("\\id G\n\\c 1\n\\v 1 a b\n", "\\id G\n\\c 1\n\\v 1 a c\n");
        let unit = skeleton
            .units
            .iter()
            .find(|unit| unit.status == Status::Modified)
            .unwrap();
        assert!(!unit.is_usfm_structure_change && !unit.is_whitespace_change);
    }

    #[test]
    fn an_empty_document_diffs_against_a_real_one() {
        let current = "\\id GEN\n\\c 1\n\\v 1 a\n";
        let skeleton = diff("", current);
        assert!(
            skeleton
                .units
                .iter()
                .all(|unit| unit.kind == UnitKind::Added)
        );
        let edits = to_edits(&skeleton, &Decisions::new(), MergeSide::Current).unwrap();
        assert_eq!(
            crate::edit::apply_splices(b"", current.as_bytes(), &edits),
            current.as_bytes()
        );
        assert!(diff("", "").units.is_empty());
    }

    #[test]
    fn a_reader_text_slice_is_the_masked_bytes_of_that_block_alone() {
        let source = "\\id GEN\n\\c 1\n\\v 1 Jesus wept.\\f + \\ft why\\f*\n\\v 2 Then he rose.\n";
        let tokens = lex(source);
        let text = ReaderText::new(source.as_bytes(), &tokens);
        let table = toc(source.as_bytes(), &tokens);
        let blocks = blocks(source.len() as u32, &table);
        // Note prose rides in undifferentiated (onion's v1 choice, kept), and
        // nothing synthetic is inserted where the markers were.
        assert_eq!(text.slice(&blocks[2].range), "Jesus wept.why\n");
        assert_eq!(text.slice(&blocks[3].range), "Then he rose.\n");
    }
}
