//! The zip container a .docx is: write with raw deflate, read stored or
//! deflated entries.
//!
//! ```text
//! write(&[("word/document.xml", bytes), …])  ->  one .zip, entries in that order
//! Archive::open(zip)?.get("word/styles.xml")  ->  that one part, inflated now
//! read(zip)                                  ->  [Entry { name, data }, …], every part
//! ```
//!
//! Import opens the archive and inflates only the parts it reads, so an
//! embedded video costs nothing and an image in a codec we lack fails nothing.
//!
//! Every timestamp is 1980-01-01, so the same parts always make the same bytes.
//! No zip64: an office document never needs it, and reading one is an error.

use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec_with_limit;

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
/// MS-DOS date for 1980-01-01: day 1, month 1, year 0.
const DOS_DATE: u16 = 0x0021;
/// The largest part an import inflates. A Bible's document.xml is tens of MB.
const MAX_INFLATED: usize = 512 << 20;

#[cfg(test)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipError {
    NotAZip,
    Truncated,
    Zip64,
    Encrypted,
    Method(u16),
    Inflate(String),
    Crc(String),
}

impl core::fmt::Display for ZipError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAZip => write!(f, "not a zip file (no end-of-central-directory record)"),
            Self::Truncated => write!(f, "the zip file is truncated"),
            Self::Zip64 => write!(f, "zip64 archives are not supported"),
            Self::Encrypted => write!(f, "the document is encrypted"),
            Self::Method(m) => write!(f, "unsupported compression method {m}"),
            Self::Inflate(name) => write!(f, "{name}: could not be decompressed"),
            Self::Crc(name) => write!(f, "{name}: checksum mismatch"),
        }
    }
}

pub fn write(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let crc = crc32(data);
        let packed = compress_to_vec(data, 6);
        let offset = out.len() as u32;
        put32(&mut out, LOCAL);
        put16(&mut out, 20); // version needed
        put16(&mut out, 0x0800); // UTF-8 names
        put16(&mut out, 8); // deflate
        put16(&mut out, 0); // time
        put16(&mut out, DOS_DATE);
        put32(&mut out, crc);
        put32(&mut out, packed.len() as u32);
        put32(&mut out, data.len() as u32);
        put16(&mut out, name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);

        put32(&mut central, CENTRAL);
        put16(&mut central, 20); // made by
        put16(&mut central, 20); // needed
        put16(&mut central, 0x0800);
        put16(&mut central, 8);
        put16(&mut central, 0);
        put16(&mut central, DOS_DATE);
        put32(&mut central, crc);
        put32(&mut central, packed.len() as u32);
        put32(&mut central, data.len() as u32);
        put16(&mut central, name.len() as u16);
        put16(&mut central, 0); // extra
        put16(&mut central, 0); // comment
        put16(&mut central, 0); // disk
        put16(&mut central, 0); // internal attributes
        put32(&mut central, 0); // external attributes
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let central_at = out.len() as u32;
    out.extend_from_slice(&central);
    put32(&mut out, END);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, entries.len() as u16);
    put16(&mut out, entries.len() as u16);
    put32(&mut out, central.len() as u32);
    put32(&mut out, central_at);
    put16(&mut out, 0);
    out
}

/// One central-directory record: where a part is and how it is packed.
struct Meta {
    name: String,
    method: u16,
    crc: u32,
    packed_len: usize,
    local: usize,
}

/// A zip whose central directory is read and whose parts are not, yet.
pub struct Archive<'a> {
    zip: &'a [u8],
    entries: Vec<Meta>,
}

impl<'a> Archive<'a> {
    pub fn open(zip: &'a [u8]) -> Result<Self, ZipError> {
        let end = find_end(zip).ok_or(ZipError::NotAZip)?;
        let count = u16_at(zip, end + 10)? as usize;
        let central_at = u32_at(zip, end + 16)?;
        if central_at == u32::MAX || count == 0xFFFF {
            return Err(ZipError::Zip64);
        }
        let mut at = central_at as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if u32_at(zip, at)? != CENTRAL {
                return Err(ZipError::Truncated);
            }
            let flags = u16_at(zip, at + 8)?;
            let method = u16_at(zip, at + 10)?;
            let crc = u32_at(zip, at + 16)?;
            let packed_len = u32_at(zip, at + 20)?;
            let len = u32_at(zip, at + 24)?;
            let name_len = u16_at(zip, at + 28)? as usize;
            let extra_len = u16_at(zip, at + 30)? as usize;
            let comment_len = u16_at(zip, at + 32)? as usize;
            let local = u32_at(zip, at + 42)?;
            let name = String::from_utf8_lossy(slice(zip, at + 46, name_len)?).into_owned();
            at += 46 + name_len + extra_len + comment_len;
            if packed_len == u32::MAX || len == u32::MAX || local == u32::MAX {
                return Err(ZipError::Zip64);
            }
            if flags & 1 != 0 {
                return Err(ZipError::Encrypted);
            }
            if name.ends_with('/') {
                continue;
            }
            entries.push(Meta { name, method, crc, packed_len: packed_len as usize, local: local as usize });
        }
        Ok(Self { zip, entries })
    }

    #[cfg(test)]
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    /// OPC part names are case-insensitive.
    fn find(&self, name: &str) -> Option<&Meta> {
        let name = name.trim_start_matches('/');
        self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// One part, inflated and checked; `None` when the archive has no such part.
    pub fn get(&self, name: &str) -> Result<Option<Vec<u8>>, ZipError> {
        let Some(e) = self.find(name) else { return Ok(None) };
        let zip = self.zip;
        if u32_at(zip, e.local)? != LOCAL {
            return Err(ZipError::Truncated);
        }
        let skip = u16_at(zip, e.local + 26)? as usize + u16_at(zip, e.local + 28)? as usize;
        let packed = slice(zip, e.local + 30 + skip, e.packed_len)?;
        let data = match e.method {
            0 => packed.to_vec(),
            8 => decompress_to_vec_with_limit(packed, MAX_INFLATED).map_err(|_| ZipError::Inflate(e.name.clone()))?,
            m => return Err(ZipError::Method(m)),
        };
        if crc32(&data) != e.crc {
            return Err(ZipError::Crc(e.name.clone()));
        }
        Ok(Some(data))
    }
}

/// Every part, inflated: for tests that rewrite a whole package.
#[cfg(test)]
pub fn read(zip: &[u8]) -> Result<Vec<Entry>, ZipError> {
    let archive = Archive::open(zip)?;
    let mut out = Vec::new();
    for name in archive.names() {
        if let Some(data) = archive.get(name)? {
            out.push(Entry { name: name.to_owned(), data });
        }
    }
    Ok(out)
}

fn find_end(zip: &[u8]) -> Option<usize> {
    if zip.len() < 22 {
        return None;
    }
    let lowest = zip.len().saturating_sub(22 + 0xFFFF);
    (lowest..=zip.len() - 22)
        .rev()
        .find(|&at| zip[at..at + 4] == END.to_le_bytes())
}

fn slice(zip: &[u8], at: usize, len: usize) -> Result<&[u8], ZipError> {
    zip.get(at..at.checked_add(len).ok_or(ZipError::Truncated)?)
        .ok_or(ZipError::Truncated)
}

fn u16_at(zip: &[u8], at: usize) -> Result<u16, ZipError> {
    let b = slice(zip, at, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(zip: &[u8], at: usize) -> Result<u32, ZipError> {
    let b = slice(zip, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn round_trip() {
        let big = "In the beginning ".repeat(2000);
        let zip = write(&[("a.xml", b"<a/>"), ("word/b.xml", big.as_bytes())]);
        let back = read(&zip).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "a.xml");
        assert_eq!(back[0].data, b"<a/>");
        assert_eq!(back[1].data, big.as_bytes());
    }

    #[test]
    fn garbage_is_not_a_zip() {
        assert!(matches!(read(b"hello"), Err(ZipError::NotAZip)));
    }
}
