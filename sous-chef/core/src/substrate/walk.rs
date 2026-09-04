//! The scalar walk: one pass per chapter, emitting a sorted `ChapterRow`.
//!
//! ```text
//! walk("He said, \u{201C}Go.\u{201D}")
//!   → a row with the scalar, pair, run, and follow lanes filled;
//!     the fold (fold.rs) resolves what crosses a chapter seam
//! ```

use rustc_hash::FxHashMap;

use super::{
    Case, ChapterRow, Edge, FollowCounts, OuterClass, PairKey, ScalarKey, is_nonletter, is_run_atom,
};
use crate::hygiene::{NBSP, SUSPECT, ScalarSites};
use crate::unicode::{
    Class,
    lookup::{ascii_class, trie_at},
};

/// Consecutive ASCII scalars before the eight-byte lane re-arms; the
/// hysteresis the Stage 1 classifier bench measured.
const REARM_AFTER: u32 = 32;
const HIGH_BITS: u64 = 0x8080_8080_8080_8080;
/// No dense id yet, and no run or pending pair open.
const NO_ID: u32 = u32::MAX;
/// Slots in the non-ASCII inventory's open-addressed table. A chapter holds
/// 150-200 distinct scalars at the tier's worst (Amharic, Greek), so half of
/// this stays empty and a probe almost always lands first try.
const PROBE: usize = 512;

/// One distinct nonletter's array-indexed counters, resolved to a scalar key
/// only when the row is built.
struct Slot {
    key: ScalarKey,
    pairs: [u32; OuterClass::COUNT * OuterClass::COUNT],
    follows: FollowCounts,
}

/// The per-scalar state, kept in a local the counters cannot alias.
///
/// Every counter write below goes through a heap pointer, so a compiler that
/// found this state behind the same `&mut` would reload all twelve fields
/// after each one. It is small and never escapes, so it stays in registers.
#[derive(Clone, Copy)]
struct Hot {
    prev: OuterClass,
    /// The previous scalar's own bits, which is what a hygiene site reads.
    prev_class: Class,
    /// Mirrors `ScalarSites::pending`, so the gate reads a register rather
    /// than the counter struct — worth 3pt on Latin.
    site_pending: bool,
    /// A nonletter whose `next` class the following scalar supplies.
    pending: u32,
    pending_prev: OuterClass,
    pending_first: bool,
    /// A run terminal still looking for the letter it hands off to.
    awaiting: u32,
    /// Where the open run started in the atom arena.
    run_open: u32,
    in_word: bool,
    seen_nonspace: bool,
    all_space: bool,
    scalar_count: u32,
    word_count: u32,
}

impl Hot {
    const fn new() -> Self {
        Self {
            prev: OuterClass::Edge,
            prev_class: Class::from_bits(0),
            site_pending: false,
            pending: NO_ID,
            pending_prev: OuterClass::Edge,
            pending_first: false,
            awaiting: NO_ID,
            run_open: NO_ID,
            in_word: false,
            seen_nonspace: false,
            all_space: true,
            scalar_count: 0,
            word_count: 0,
        }
    }
}

/// Per-call scratch: the counters and the intern tables. Nothing here is
/// shared, retained, or locked, so a parallel map needs no lock either.
struct Counters {
    ascii: [u32; 128],
    probe_keys: [u32; PROBE],
    probe_counts: [u32; PROBE],
    probe_live: u32,
    /// Only what the probe table declined to take once it was half full.
    other: FxHashMap<u32, u32>,
    digits: u32,
    nonletter_ascii: [u32; 128],
    nonletter_other: FxHashMap<u32, u32>,
    nonletter_digits: u32,
    slots: Vec<Slot>,
    run_atoms: Vec<u32>,
    run_spans: Vec<(u32, u32)>,
    lead: Edge,
    trail_pair: Option<(ScalarKey, OuterClass)>,
    sites: ScalarSites,
}

pub(crate) fn walk(text: &str) -> ChapterRow {
    let bytes = text.as_bytes();
    let mut counters = Counters::new(bytes.len());
    let mut hot = Hot::new();
    let (mut at, mut armed, mut ascii_run) = (0usize, true, 0u32);
    while at < bytes.len() {
        if armed && at + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"));
            if word & HIGH_BITS == 0 {
                for (offset, &byte) in bytes[at..at + 8].iter().enumerate() {
                    counters.step::<true>(
                        &mut hot,
                        u32::from(byte),
                        ascii_class(byte),
                        at + offset,
                        1,
                    );
                }
                at += 8;
                continue;
            }
            armed = false;
            ascii_run = 0;
        }
        let (class, width) = trie_at(&bytes[at..]);
        counters.step::<false>(&mut hot, scalar_at(&bytes[at..], width), class, at, width);
        if width == 1 {
            ascii_run += 1;
            armed |= ascii_run >= REARM_AFTER;
        } else {
            ascii_run = 0;
        }
        at += width;
    }
    counters.finish(hot, text)
}

/// The code point at `bytes[0]`, whose UTF-8 width `trie_at` already read.
#[inline]
fn scalar_at(bytes: &[u8], width: usize) -> u32 {
    match width {
        1 => u32::from(bytes[0]),
        2 => (u32::from(bytes[0] & 0x1F) << 6) | u32::from(bytes[1] & 0x3F),
        3 => {
            (u32::from(bytes[0] & 0x0F) << 12)
                | (u32::from(bytes[1] & 0x3F) << 6)
                | u32::from(bytes[2] & 0x3F)
        }
        _ => {
            (u32::from(bytes[0] & 0x07) << 18)
                | (u32::from(bytes[1] & 0x3F) << 12)
                | (u32::from(bytes[2] & 0x3F) << 6)
                | u32::from(bytes[3] & 0x3F)
        }
    }
}

impl Counters {
    /// `hint` is the chapter's byte length. Nonletters run 5-15% of scalars
    /// and their runs a tenth of that, so the two arenas are sized from it
    /// once instead of doubling a dozen times per chapter.
    fn new(hint: usize) -> Self {
        Self {
            ascii: [0; 128],
            probe_keys: [0; PROBE],
            probe_counts: [0; PROBE],
            probe_live: 0,
            other: FxHashMap::default(),
            digits: 0,
            nonletter_ascii: [NO_ID; 128],
            nonletter_other: FxHashMap::default(),
            nonletter_digits: NO_ID,
            slots: Vec::new(),
            run_atoms: Vec::with_capacity(hint / 8),
            run_spans: Vec::with_capacity(hint / 32),
            lead: Edge::default(),
            trail_pair: None,
            sites: ScalarSites::new(),
        }
    }

    /// `ASCII` is the eight-byte lane, where no scalar is a mark, a format
    /// character, a noncharacter, or U+00A0: only a pending verdict reaches
    /// the site machine there, so the gate is one branch.
    #[inline(always)]
    fn step<const ASCII: bool>(
        &mut self,
        hot: &mut Hot,
        cp: u32,
        class: Class,
        at: usize,
        width: usize,
    ) {
        let outer = OuterClass::of(class);
        if hot.scalar_count == 0 {
            self.lead.outer = outer;
        }
        if !class.is_whitespace() && !hot.seen_nonspace {
            hot.seen_nonspace = true;
            hot.all_space = false;
            if class.is_alphabetic() {
                self.lead.edge_case = Some(Case::of(class));
            }
        }

        if !class.is_glue() {
            if class.is_decimal_digit() {
                self.digits += 1;
            } else if cp < 128 {
                self.ascii[cp as usize] += 1;
            } else {
                self.count_wide(cp);
            }
        }

        if hot.pending != NO_ID {
            let slot = &mut self.slots[hot.pending as usize];
            slot.pairs[pair_index(hot.pending_prev, outer)] += 1;
            if hot.pending_first {
                self.lead.open_pair = Some((slot.key, outer));
            }
            hot.pending = NO_ID;
        }

        if is_nonletter(class) {
            let id = self.intern(cp, class);
            if is_run_atom(class) {
                if hot.run_open == NO_ID {
                    hot.run_open = self.run_atoms.len() as u32;
                }
                self.run_atoms.push(id);
            } else {
                // A digit breaks the run it interrupts and opens none.
                self.close_run(hot);
            }
            hot.awaiting = NO_ID;
            hot.pending = id;
            hot.pending_prev = hot.prev;
            hot.pending_first = hot.scalar_count == 0;
        } else {
            self.close_run(hot);
            if class.is_alphabetic() {
                if hot.awaiting != NO_ID {
                    self.slots[hot.awaiting as usize].follows.0[Case::of(class) as usize] += 1;
                    hot.awaiting = NO_ID;
                }
            } else if !class.is_whitespace() {
                hot.awaiting = NO_ID;
            }
        }

        if class.is_alphabetic() || class.is_glue() || class.is_decimal_digit() {
            if !hot.in_word {
                hot.word_count += 1;
                hot.in_word = true;
            }
        } else {
            hot.in_word = false;
        }

        let gated = if ASCII {
            hot.site_pending
        } else {
            (class.bits() & SUSPECT != 0) | (cp == NBSP) | hot.site_pending
        };
        if gated {
            let prev = (hot.scalar_count > 0).then_some(hot.prev_class);
            self.sites.step(at, width, cp, class, prev);
            hot.site_pending = self.sites.pending();
        }

        hot.prev = outer;
        hot.prev_class = class;
        hot.scalar_count += 1;
    }

    /// Ends the open run, if any, and leaves its terminal awaiting a letter.
    #[inline(always)]
    fn close_run(&mut self, hot: &mut Hot) {
        if hot.run_open != NO_ID {
            let len = self.run_atoms.len() as u32 - hot.run_open;
            self.run_spans.push((hot.run_open, len));
            hot.run_open = NO_ID;
            hot.awaiting = *self.run_atoms.last().expect("a closed run has atoms");
        }
    }

    /// One non-ASCII scalar into the inventory. The table never fills past
    /// half, so an empty slot always ends the probe and a key is in exactly
    /// one of the two structures.
    #[inline]
    fn count_wide(&mut self, cp: u32) {
        let mut at = (cp.wrapping_mul(0x9E37_79B1) >> 20) as usize & (PROBE - 1);
        loop {
            let key = self.probe_keys[at];
            if key == cp {
                self.probe_counts[at] += 1;
                return;
            }
            if key == 0 {
                if (self.probe_live as usize) * 2 < PROBE {
                    self.probe_keys[at] = cp;
                    self.probe_counts[at] = 1;
                    self.probe_live += 1;
                    return;
                }
                break;
            }
            at = (at + 1) & (PROBE - 1);
        }
        *self.other.entry(cp).or_insert(0) += 1;
    }

    /// The chapter-local dense id of one nonletter; glue never reaches here.
    #[inline]
    fn intern(&mut self, cp: u32, class: Class) -> u32 {
        if class.is_decimal_digit() {
            if self.nonletter_digits == NO_ID {
                self.nonletter_digits = self.push_slot(ScalarKey::DIGITS);
            }
            return self.nonletter_digits;
        }
        if cp < 128 {
            let seen = self.nonletter_ascii[cp as usize];
            if seen != NO_ID {
                return seen;
            }
            let id = self.push_slot(ScalarKey(cp));
            self.nonletter_ascii[cp as usize] = id;
            return id;
        }
        if let Some(&seen) = self.nonletter_other.get(&cp) {
            return seen;
        }
        let id = self.push_slot(ScalarKey(cp));
        self.nonletter_other.insert(cp, id);
        id
    }

    fn push_slot(&mut self, key: ScalarKey) -> u32 {
        let id = self.slots.len() as u32;
        self.slots.push(Slot {
            key,
            pairs: [0; OuterClass::COUNT * OuterClass::COUNT],
            follows: FollowCounts::default(),
        });
        id
    }

    fn finish(mut self, mut hot: Hot, text: &str) -> ChapterRow {
        if hot.pending != NO_ID {
            let slot = &mut self.slots[hot.pending as usize];
            slot.pairs[pair_index(hot.pending_prev, OuterClass::Edge)] += 1;
            let key = slot.key;
            if hot.pending_first {
                self.lead.open_pair = Some((key, OuterClass::Edge));
            }
            self.trail_pair = Some((key, hot.pending_prev));
        }
        self.close_run(&mut hot);
        let hygiene = self.sites.finish(text);

        let trail = Edge {
            outer: hot.prev,
            open_pair: self.trail_pair,
            open_follow: (hot.awaiting != NO_ID).then(|| self.slots[hot.awaiting as usize].key),
            edge_case: None,
            blank: hot.all_space && hot.scalar_count > 0,
        };

        let mut scalars: Vec<(ScalarKey, u32)> =
            Vec::with_capacity(self.other.len() + self.probe_live as usize + 64);
        for (cp, count) in self.ascii.iter().enumerate() {
            if *count > 0 {
                scalars.push((ScalarKey(cp as u32), *count));
            }
        }
        for (key, count) in self.probe_keys.iter().zip(&self.probe_counts) {
            if *count > 0 {
                scalars.push((ScalarKey(*key), *count));
            }
        }
        scalars.extend(
            self.other
                .iter()
                .map(|(cp, count)| (ScalarKey(*cp), *count)),
        );
        if self.digits > 0 {
            scalars.push((ScalarKey::DIGITS, self.digits));
        }
        scalars.sort_unstable_by_key(|entry| entry.0);

        let mut pairs: Vec<(PairKey, u32)> = Vec::with_capacity(self.slots.len() * 3);
        let mut follows: Vec<(ScalarKey, FollowCounts)> = Vec::with_capacity(self.slots.len());
        for slot in &self.slots {
            for (index, count) in slot.pairs.iter().enumerate() {
                if *count > 0 {
                    pairs.push((
                        PairKey::new(
                            slot.key,
                            outer_at(index / OuterClass::COUNT),
                            outer_at(index % OuterClass::COUNT),
                        ),
                        *count,
                    ));
                }
            }
            if slot.follows.total() > 0 {
                follows.push((slot.key, slot.follows));
            }
        }
        pairs.sort_unstable_by_key(|entry| entry.0);
        follows.sort_unstable_by_key(|entry| entry.0);

        let keys: Vec<ScalarKey> = self.slots.iter().map(|slot| slot.key).collect();
        let atoms = &self.run_atoms;
        let mut spans = std::mem::take(&mut self.run_spans);
        spans.sort_unstable_by(|left, right| {
            let a = &atoms[left.0 as usize..][..left.1 as usize];
            let b = &atoms[right.0 as usize..][..right.1 as usize];
            a.iter()
                .map(|id| keys[*id as usize])
                .cmp(b.iter().map(|id| keys[*id as usize]))
        });

        let mut runs: Vec<(u32, u32, u32)> = Vec::with_capacity(spans.len() / 4 + 4);
        let mut run_atoms: Vec<ScalarKey> = Vec::with_capacity(spans.len() / 2 + 4);
        for (start, len) in spans {
            let sequence = &atoms[start as usize..][..len as usize];
            let same = runs.last().is_some_and(|&(at, held, _)| {
                held == len
                    && run_atoms[at as usize..][..held as usize]
                        .iter()
                        .zip(sequence)
                        .all(|(key, id)| *key == keys[*id as usize])
            });
            if same {
                runs.last_mut().expect("just checked").2 += 1;
                continue;
            }
            let at = run_atoms.len() as u32;
            run_atoms.extend(sequence.iter().map(|id| keys[*id as usize]));
            runs.push((at, len, 1));
        }

        ChapterRow {
            scalars: scalars.into_boxed_slice(),
            pairs: pairs.into_boxed_slice(),
            runs: runs.into_boxed_slice(),
            run_atoms: run_atoms.into_boxed_slice(),
            follows: follows.into_boxed_slice(),
            hygiene,
            lead: self.lead,
            trail,
            scalar_count: hot.scalar_count,
            word_count: hot.word_count,
        }
    }
}

#[inline]
const fn pair_index(prev: OuterClass, next: OuterClass) -> usize {
    prev as usize * OuterClass::COUNT + next as usize
}

const fn outer_at(index: usize) -> OuterClass {
    match index {
        0 => OuterClass::Letter,
        1 => OuterClass::Space,
        2 => OuterClass::Digit,
        3 => OuterClass::Nonletter,
        _ => OuterClass::Edge,
    }
}
