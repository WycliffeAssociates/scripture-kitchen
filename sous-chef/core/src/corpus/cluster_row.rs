//! Encodes and decodes one cluster entry behind the pattern table. Layout:
//! codec/README.md.
//!
//! ```text
//! Cluster { pattern: 3, atoms: ."'", count: 27, recurring: true, facing: Closing }
//!   → 03 00 · 03 · 21 · 1b 00 00 00 · 2e 00 00 00 · 22 00 00 00 · 27 00 00 00
//! ```

use super::*;
use crate::judge::{Cluster, Pattern, PatternIndex};
use crate::substrate::{Facing, ScalarKey};

/// Bytes one entry takes on the wire.
pub(super) fn entry_len(cluster: &Cluster) -> usize {
    CLUSTER_ENTRY_BYTES + 4 * cluster.atoms.len()
}

pub(super) fn encode_cluster(cluster: &Cluster, out: &mut Vec<u8>) {
    out.extend_from_slice(&cluster.pattern.get().to_le_bytes());
    out.push(cluster.atoms.len() as u8);
    let mut flags = 0;
    if cluster.recurring {
        flags |= CLUSTER_RECURRING;
    }
    if cluster.truncated {
        flags |= CLUSTER_TRUNCATED;
    }
    flags |= Facing::byte(cluster.facing) << CLUSTER_FACING_SHIFT;
    out.push(flags);
    out.extend_from_slice(&cluster.count.to_le_bytes());
    for atom in &cluster.atoms {
        out.extend_from_slice(&atom.raw().to_le_bytes());
    }
}

/// One entry at `at`, and the offset just past it.
pub(super) fn decode_cluster(
    bytes: &[u8],
    at: usize,
    entry: usize,
) -> Result<(Cluster, usize), CorpusWireError> {
    let bad = |field| CorpusWireError::InvalidCluster { entry, field };
    if at + CLUSTER_ENTRY_BYTES > bytes.len() {
        return Err(bad("length"));
    }
    let atoms = usize::from(bytes[at + CLUSTER_ATOM_COUNT_OFFSET]);
    let end = at + CLUSTER_ENTRY_BYTES + 4 * atoms;
    if end > bytes.len() {
        return Err(bad("length"));
    }
    let flags = bytes[at + CLUSTER_FLAGS_OFFSET];
    let low = flags & ((1 << CLUSTER_FACING_SHIFT) - 1);
    if low & !(CLUSTER_RECURRING | CLUSTER_TRUNCATED) != 0 {
        return Err(bad("flags"));
    }
    let facing = Facing::from_byte(flags >> CLUSTER_FACING_SHIFT).map_err(|_| bad("facing"))?;
    let atoms = (0..atoms)
        .map(|index| {
            ScalarKey::from_raw(read_u32(bytes, at + CLUSTER_ENTRY_BYTES + 4 * index))
                .ok_or(bad("atoms"))
        })
        .collect::<Result<Box<[ScalarKey]>, _>>()?;
    let cluster = Cluster {
        pattern: PatternIndex::new(u16::from_le_bytes([
            bytes[at + CLUSTER_PATTERN_OFFSET],
            bytes[at + CLUSTER_PATTERN_OFFSET + 1],
        ])),
        atoms,
        count: read_u32(bytes, at + CLUSTER_COUNT_OFFSET),
        recurring: flags & CLUSTER_RECURRING != 0,
        truncated: flags & CLUSTER_TRUNCATED != 0,
        facing,
    };
    Ok((cluster, end))
}

/// The section's order, entry by entry: pattern ascending, count descending
/// within one pattern, at most [`Cluster::PER_ROW`] each.
#[derive(Default)]
pub(super) struct Order {
    last: Option<(PatternIndex, u32)>,
    run: usize,
}

impl Order {
    /// Refuses a cluster past the table, on a row it cannot belong to, or out
    /// of order behind the last one seen.
    pub(super) fn admit(
        &mut self,
        cluster: &Cluster,
        entry: usize,
        row: impl FnOnce(usize) -> Option<Pattern>,
    ) -> Result<(), CorpusWireError> {
        let bad = |field| CorpusWireError::InvalidCluster { entry, field };
        let row = row(usize::from(cluster.pattern.get())).ok_or(bad("pattern"))?;
        if !cluster.fits(&row) {
            return Err(bad(if row.channel == crate::judge::Channel::RunShape {
                "atoms"
            } else {
                "pattern"
            }));
        }
        self.run = match self.last {
            Some((pattern, count)) if pattern == cluster.pattern => {
                if cluster.count > count || self.run == Cluster::PER_ROW {
                    return Err(bad("order"));
                }
                self.run + 1
            }
            Some((pattern, _)) if pattern > cluster.pattern => return Err(bad("order")),
            _ => 1,
        };
        self.last = Some((cluster.pattern, cluster.count));
        Ok(())
    }
}
