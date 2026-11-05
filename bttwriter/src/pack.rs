//! Git objects → one pack and its index, in memory.
//!
//! ```text
//! let mut pack = PackWriter::new();
//! let blob = pack.add(Kind::Blob, usfm.as_bytes(), Some("46-ROM.usfm"));   // id, as git computes it
//! let tree = pack.add(Kind::Tree, &tree_body(&files), Some("tree"));
//! let head = pack.add(Kind::Commit, &commit_body(&commit), None);
//! let FinishedPack { pack, idx, name, stats } = pack.finish();
//!   → .git/objects/pack/{name}.pack and {name}.idx
//! ```
//!
//! An object's id is the SHA-1 of `"{kind} {len}\0"` and its bytes, so any git
//! computes the same one. Each entry is zlib-compressed. An object given a
//! `lane` (a book's file, the root tree) may be stored as an `ofs-delta`
//! against the previous object in that lane: the base's shared prefix and
//! suffix copied, the middle inserted. A save changes one chunk of a book, so
//! a version costs a few dozen bytes instead of the whole book. A delta never
//! changes an id, only how the object is stored.
//!
//! The index is version 2, and the pack's bytes fully determine it, so it is
//! byte-identical to what `git index-pack` writes for the same pack.

use std::collections::BTreeMap;
use std::fmt;

use miniz_oxide::deflate::compress_to_vec_zlib;
use rustc_hash::{FxHashMap, FxHashSet};

/// Longest delta chain. Reading an object resolves its chain, so this bounds
/// the cost of reading any one version.
const MAX_DEPTH: u32 = 50;

/// zlib's default.
const LEVEL: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Commit,
    Tree,
    Blob,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Commit => "commit",
            Kind::Tree => "tree",
            Kind::Blob => "blob",
        }
    }

    /// The type number in a pack entry's header.
    fn code(self) -> u8 {
        match self {
            Kind::Commit => 1,
            Kind::Tree => 2,
            Kind::Blob => 3,
        }
    }
}

const OFS_DELTA: u8 = 6;

/// A git object id: 20 bytes, shown as 40 hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub [u8; 20]);

impl ObjectId {
    /// The id git gives an object of this kind and body.
    pub fn of(kind: Kind, body: &[u8]) -> ObjectId {
        let mut hash = sha1_smol::Sha1::new();
        hash.update(format!("{} {}\0", kind.name(), body.len()).as_bytes());
        hash.update(body);
        ObjectId(hash.digest().bytes())
    }

    pub fn from_hex(hex: &str) -> Option<ObjectId> {
        let bytes = hex.as_bytes();
        if bytes.len() != 40 {
            return None;
        }
        let mut out = [0u8; 20];
        for (i, pair) in bytes.chunks(2).enumerate() {
            let text = std::str::from_utf8(pair).ok()?;
            out[i] = u8::from_str_radix(text, 16).ok()?;
        }
        Some(ObjectId(out))
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Who and when, as a commit records it: Unix seconds, and the zone's offset
/// from UTC in minutes (`+0200` is 120).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub email: String,
    pub at: i64,
    pub offset_minutes: i32,
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.offset_minutes < 0 { '-' } else { '+' };
        let abs = self.offset_minutes.unsigned_abs();
        write!(
            f,
            "{} <{}> {} {sign}{:02}{:02}",
            self.name,
            self.email,
            self.at,
            abs / 60,
            abs % 60
        )
    }
}

/// What a commit object holds.
#[derive(Debug, Clone)]
pub struct CommitFields<'a> {
    pub tree: ObjectId,
    pub parents: &'a [ObjectId],
    pub author: &'a Signature,
    pub committer: &'a Signature,
    /// As stored: a message conventionally ends with `\n`.
    pub message: &'a str,
}

/// A commit object's body.
pub fn commit_body(commit: &CommitFields<'_>) -> Vec<u8> {
    let mut out = format!("tree {}\n", commit.tree);
    for parent in commit.parents {
        out.push_str(&format!("parent {parent}\n"));
    }
    out.push_str(&format!(
        "author {}\ncommitter {}\n\n{}",
        commit.author, commit.committer, commit.message
    ));
    out.into_bytes()
}

/// A tree of files, one level, every entry a regular file (`100644`). Git
/// orders a tree's entries by name, byte by byte, which is `BTreeMap`'s order.
pub fn tree_body(files: &BTreeMap<String, ObjectId>) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, id) in files {
        out.extend_from_slice(b"100644 ");
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        out.extend_from_slice(&id.0);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackStats {
    pub objects: usize,
    pub deltas: usize,
    pub pack_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct FinishedPack {
    pub pack: Vec<u8>,
    pub idx: Vec<u8>,
    /// `pack-{sha of the pack}`: the file stem git expects for both files.
    pub name: String,
    pub stats: PackStats,
}

struct Entry {
    id: ObjectId,
    offset: u64,
    crc: u32,
}

/// The last object in a lane: what the next one may delta against.
struct Held {
    offset: u64,
    body: Vec<u8>,
    depth: u32,
}

/// Collects objects into one pack. Adding an object already added does
/// nothing and returns its id.
pub struct PackWriter {
    /// Starts with a 12-byte header whose object count `finish` fills in.
    bytes: Vec<u8>,
    entries: Vec<Entry>,
    seen: FxHashSet<ObjectId>,
    lanes: FxHashMap<String, Held>,
    deltas: usize,
}

impl Default for PackWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl PackWriter {
    pub fn new() -> PackWriter {
        let mut bytes = Vec::with_capacity(1 << 16);
        bytes.extend_from_slice(b"PACK");
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        PackWriter {
            bytes,
            entries: Vec::new(),
            seen: FxHashSet::default(),
            lanes: FxHashMap::default(),
            deltas: 0,
        }
    }

    /// Adds an object and returns its id. With a `lane`, it may be stored as a
    /// delta against the lane's previous object, when that's smaller.
    pub fn add(&mut self, kind: Kind, body: &[u8], lane: Option<&str>) -> ObjectId {
        let id = ObjectId::of(kind, body);
        if !self.seen.insert(id) {
            return id;
        }
        let offset = self.bytes.len() as u64;
        let start = self.bytes.len();

        let base = lane
            .and_then(|lane| self.lanes.get(lane))
            .filter(|held| held.depth < MAX_DEPTH);
        let delta = base
            .map(|held| (held, delta_of(&held.body, body)))
            .filter(|(_, delta)| delta.len() < body.len());
        let depth = match delta {
            Some((held, delta)) => {
                entry_header(OFS_DELTA, delta.len() as u64, &mut self.bytes);
                offset_varint(offset - held.offset, &mut self.bytes);
                self.bytes
                    .extend_from_slice(&compress_to_vec_zlib(&delta, LEVEL));
                self.deltas += 1;
                held.depth + 1
            }
            None => {
                entry_header(kind.code(), body.len() as u64, &mut self.bytes);
                self.bytes
                    .extend_from_slice(&compress_to_vec_zlib(body, LEVEL));
                0
            }
        };

        let crc = crc32(&self.bytes[start..]);
        self.entries.push(Entry { id, offset, crc });
        if let Some(lane) = lane {
            self.lanes.insert(
                lane.to_owned(),
                Held {
                    offset,
                    body: body.to_vec(),
                    depth,
                },
            );
        }
        id
    }

    pub fn finish(mut self) -> FinishedPack {
        let count = u32::try_from(self.entries.len()).expect("fewer than 2^32 objects");
        self.bytes[8..12].copy_from_slice(&count.to_be_bytes());
        let pack_sha = sha1(&self.bytes);
        self.bytes.extend_from_slice(&pack_sha);

        let mut sorted: Vec<&Entry> = self.entries.iter().collect();
        sorted.sort_by_key(|entry| entry.id);

        let mut idx = Vec::with_capacity(8 + 256 * 4 + sorted.len() * 28 + 40);
        idx.extend_from_slice(&[0xff, 0x74, 0x4f, 0x63]);
        idx.extend_from_slice(&2u32.to_be_bytes());
        let mut at = 0usize;
        for first in 0..=255u8 {
            while at < sorted.len() && sorted[at].id.0[0] <= first {
                at += 1;
            }
            idx.extend_from_slice(&(at as u32).to_be_bytes());
        }
        for entry in &sorted {
            idx.extend_from_slice(&entry.id.0);
        }
        for entry in &sorted {
            idx.extend_from_slice(&entry.crc.to_be_bytes());
        }
        // Offsets past 2^31 go in a table of 64-bit offsets, and the 32-bit
        // slot holds the table index with the top bit set.
        let mut large: Vec<u64> = Vec::new();
        for entry in &sorted {
            let slot = if entry.offset < 0x8000_0000 {
                entry.offset as u32
            } else {
                large.push(entry.offset);
                0x8000_0000 | (large.len() as u32 - 1)
            };
            idx.extend_from_slice(&slot.to_be_bytes());
        }
        for offset in large {
            idx.extend_from_slice(&offset.to_be_bytes());
        }
        idx.extend_from_slice(&pack_sha);
        let idx_sha = sha1(&idx);
        idx.extend_from_slice(&idx_sha);

        let name = format!("pack-{}", ObjectId(pack_sha));
        let stats = PackStats {
            objects: self.entries.len(),
            deltas: self.deltas,
            pack_bytes: self.bytes.len(),
        };
        FinishedPack {
            pack: self.bytes,
            idx,
            name,
            stats,
        }
    }
}

fn sha1(bytes: &[u8]) -> [u8; 20] {
    sha1_smol::Sha1::from(bytes).digest().bytes()
}

/// A pack entry's header: the type in bits 4–6 of the first byte, then the
/// size, four bits first and seven per byte after, little-endian.
fn entry_header(kind: u8, size: u64, out: &mut Vec<u8>) {
    let mut byte = (kind << 4) | (size & 0x0f) as u8;
    let mut rest = size >> 4;
    while rest > 0 {
        out.push(byte | 0x80);
        byte = (rest & 0x7f) as u8;
        rest >>= 7;
    }
    out.push(byte);
}

/// A delta's base and result sizes: seven bits per byte, little-endian.
fn size_varint(value: u64, out: &mut Vec<u8>) {
    let mut rest = value;
    loop {
        let byte = (rest & 0x7f) as u8;
        rest >>= 7;
        if rest == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// How far back an `ofs-delta`'s base is: big-endian sevens, each byte after
/// the first adding one before the shift, so no two encodings mean one value.
fn offset_varint(value: u64, out: &mut Vec<u8>) {
    let mut bytes = vec![(value & 0x7f) as u8];
    let mut rest = value >> 7;
    while rest > 0 {
        rest -= 1;
        bytes.push(0x80 | (rest & 0x7f) as u8);
        rest >>= 7;
    }
    bytes.reverse();
    out.extend_from_slice(&bytes);
}

/// Copy `size` bytes of the base from `from`, at most 64 KiB per instruction:
/// an opcode whose low bits say which offset and size bytes follow, zeros
/// left out.
fn copy_ops(from: u64, size: u64, out: &mut Vec<u8>) {
    let mut offset = from;
    let mut left = size;
    while left > 0 {
        let take = left.min(0xffff);
        let at = out.len();
        out.push(0x80);
        for i in 0..4 {
            let byte = (offset >> (8 * i)) as u8;
            if byte != 0 {
                out[at] |= 1 << i;
                out.push(byte);
            }
        }
        for i in 0..2 {
            let byte = (take >> (8 * i)) as u8;
            if byte != 0 {
                out[at] |= 1 << (4 + i);
                out.push(byte);
            }
        }
        offset += take;
        left -= take;
    }
}

/// Insert these bytes, at most 127 per instruction.
fn insert_ops(bytes: &[u8], out: &mut Vec<u8>) {
    for piece in bytes.chunks(127) {
        out.push(piece.len() as u8);
        out.extend_from_slice(piece);
    }
}

/// `target` as a delta on `base`: their shared prefix and suffix copied, the
/// rest inserted.
fn delta_of(base: &[u8], target: &[u8]) -> Vec<u8> {
    let most = base.len().min(target.len());
    let head = base.iter().zip(target).take_while(|(a, b)| a == b).count();
    let tail = base[head..]
        .iter()
        .rev()
        .zip(target[head..].iter().rev())
        .take(most - head)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = Vec::new();
    size_varint(base.len() as u64, &mut out);
    size_varint(target.len() as u64, &mut out);
    if head > 0 {
        copy_ops(0, head as u64, &mut out);
    }
    insert_ops(&target[head..target.len() - tail], &mut out);
    if tail > 0 {
        copy_ops((base.len() - tail) as u64, tail as u64, &mut out);
    }
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            table[n] = c;
            n += 1;
        }
        table
    };
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc = TABLE[((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    crc ^ 0xffff_ffff
}

#[cfg(test)]
mod tests;
