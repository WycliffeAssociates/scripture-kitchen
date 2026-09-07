//! galley — the workflows crate (piece 5 of the five-crate layout).
//!
//! ```text
//!              mise — spec tables, borrow-free structures, zero deps
//!                ▲                                  ▲
//! usfm_onion ◄── onion-wasm          scripture_sous_chef ◄── sous-wasm
//!     ▲              ▲                        ▲                   ▲
//!     │              │ (wasm feature)         │                   │
//!     └────────── galley ◄────────────────────┴───────────────────┘
//! ```
//!
//! The OPINIONATED layer over the engines: checksumming, ingest recipes,
//! find, onion↔sous coordination — anything that might be considered
//! stateful lives here and NEVER in the engines. The engines never see
//! each other; galley is the only place they meet.
//!
//! The accepted ownership, cache, coordinate, and findings-publication model
//! for the composed Onion + Sous host is recorded in
//! `galley/docs/analysis-host.md`. The [`wasm`] handle implements the resident
//! half of it: whole books by id, one complete publication out.

/// The whole engine, as a module: `galley::onion::lex`, `galley::onion::
/// analyze` — nothing hidden, so a consumer never needs to reach around
/// galley. Galley's own top-level names are the curated layer on top.
pub use usfm_onion as onion;

pub mod corpus;
pub mod find;
pub mod pantry;
pub mod sous;
pub mod warmer;
pub use find::{Find, Hit, Hits, SourceSpan};
pub use mise::utf16::{Utf16Table, utf16_table};
pub use pantry::{
    BookId, Entry, Fingerprint, Pantry, PantryError, RawChecksum, Retain, Role, SourceLanes,
    fingerprint,
};
pub use sous::{Expediter, ObservationKey};
pub use warmer::Warmer;

#[cfg(feature = "wasm")]
pub mod wasm;

/// The checksum algorithm the hex strings below come from. A consumer
/// caching against these keys stores this tag alongside; a future
/// algorithm change bumps the tag and every old key simply misses.
pub const CHECKSUM_VERSION: &str = "xxh3-128-v1";

/// A book chopped at line-initial `\c `, each chunk content-addressed.
///
/// ```text
/// chunks("\\id GEN\n\\c 1\n\\p a\n\\c 2\n\\p b\n")
///   → starts    [0, 8, 18]
///     checksums ["2e16e27c478de5aa3f10a17534e9f221",
///                "780553e7c40c607999ac482507a789f9",
///                "d270baad9a878869956940da34f78936"]
///     lf 5   crlf 0
/// ```
///
/// The recipe behind the frontend's `(checksum → work)` cache: chunk 0 is
/// the front matter, chunks tile the source, and a checksum keys ONLY its
/// chunk's bytes — no book name, no position — so pure reordering,
/// relabeling, or a chapter shared verbatim between books all hit.
///
/// INPUT CONTRACT: checksums are only stable against LF-normalized text
/// (the same contract `onion::analyze` documents). The `lf`/`crlf` census
/// is the fact a writer needs to put a normalized document back in the
/// ending style it arrived in — see [`onion::chunk::PreScan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunks {
    /// Byte offsets, ascending from 0; chunk `i` is `starts[i]..starts[i+1]`
    /// (the last runs to the end of the text).
    pub starts: Vec<u32>,
    /// One 32-char lowercase hex xxh3-128 per chunk, index-aligned with
    /// `starts`. Strings on purpose: a checksum is an IDENTITY whose
    /// consumer-side job is to key a map, not an offset for the u32 plane.
    pub checksums: Vec<String>,
    /// The line-ending census — `\n` bare vs `\r\n` — from the same pass.
    pub lf: u32,
    /// See `lf`.
    pub crlf: u32,
}

impl Chunks {
    /// The ending a writer should emit to match the source's majority;
    /// ties fall to `"\n"`, the engine's canonical form.
    pub fn dominant_ending(&self) -> &'static str {
        if self.crlf > self.lf { "\r\n" } else { "\n" }
    }
}

/// One `onion::chunk::pre_scan` + one xxh3-128 per chunk. See [`Chunks`].
pub fn chunks(text: &str) -> Chunks {
    let bytes = text.as_bytes();
    let scan = onion::chunk::pre_scan(bytes);
    let checksums = scan
        .starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = scan
                .starts
                .get(i + 1)
                .map_or(bytes.len(), |&next| next as usize);
            format!(
                "{:032x}",
                xxhash_rust::xxh3::xxh3_128(&bytes[start as usize..end])
            )
        })
        .collect();
    Chunks {
        starts: scan.starts,
        checksums,
        lf: scan.lf,
        crlf: scan.crlf,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_are_content_addressed_and_position_free() {
        let ab = chunks("\\id GEN\n\\c 1\n\\p a\n\\c 2\n\\p b\n");
        let ba = chunks("\\id GEN\n\\c 2\n\\p b\n\\c 1\n\\p a\n");
        assert_eq!(ab.starts, vec![0, 8, 18]);
        assert_eq!(ab.checksums.len(), 3);
        // Front matter identical; the two chapters swap places but keep
        // their identities — pure reordering is all cache hits.
        assert_eq!(ab.checksums[0], ba.checksums[0]);
        assert_eq!(ab.checksums[1], ba.checksums[2]);
        assert_eq!(ab.checksums[2], ba.checksums[1]);
        // 32 lowercase hex chars each.
        for sum in &ab.checksums {
            assert_eq!(sum.len(), 32);
            assert!(sum.bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn one_changed_chunk_changes_one_checksum() {
        let before = chunks("\\id GEN\n\\c 1\n\\p a\n\\c 2\n\\p b\n");
        let after = chunks("\\id GEN\n\\c 1\n\\p a!\n\\c 2\n\\p b\n");
        assert_eq!(before.checksums[0], after.checksums[0]);
        assert_ne!(before.checksums[1], after.checksums[1]);
        assert_eq!(before.checksums[2], after.checksums[2]);
    }

    #[test]
    fn the_census_rides_along() {
        let crlf = chunks("\\id GEN\r\n\\c 1\r\n\\p a\r\n");
        assert_eq!((crlf.lf, crlf.crlf), (0, 3));
        assert_eq!(crlf.dominant_ending(), "\r\n");
        assert_eq!(chunks("\\id GEN\n\\c 1\n").dominant_ending(), "\n");
    }
}
