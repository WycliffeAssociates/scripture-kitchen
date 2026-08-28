//! The `\c` pre-scan: chunk starts and the line-ending census, one pass,
//! no lexing.
//!
//! ```text
//! "\\id GEN\n\\c 1\n\\p a\n\\c 2\n\\p b\n"
//!    → starts [0, 8, 18]   lf 5   crlf 0
//!
//! "\\id GEN\r\n\\c 1\r\n\\p a\r\n"
//!    → starts [0, 10]      lf 0   crlf 3
//! ```
//!
//! Chunk 0 is the front matter (everything before the first line-initial
//! `\c `); every later start points AT the backslash of one. The starts
//! TILE the source: chunk `i` is `starts[i]..starts[i+1]` (the last runs to
//! the end), so a per-chunk consumer (checksums, chapter-grain ingest)
//! covers every byte.
//!
//! The census counts how each `\n` arrived — preceded by `\r` or bare — so
//! a writer can put a normalized document BACK in the ending style it came
//! in (the engine's readers are LF-canonical; see analyze's input
//! contract). A lone `\r` is neither: old-Mac endings are not a thing this
//! census reports, and such a byte simply counts nothing.

use memchr::memchr_iter;

/// What one pass over the raw bytes learns. See the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreScan {
    /// Ascending, always beginning with 0. Every element after the first
    /// points at the `\` of a line-initial `\c `.
    pub starts: Vec<u32>,
    /// `\n` not preceded by `\r`.
    pub lf: u32,
    /// `\r\n` pairs.
    pub crlf: u32,
}

impl PreScan {
    /// The ending a writer should emit to match the source's majority.
    /// Ties (including the zero-newline document) fall to `"\n"`: the
    /// engine's canonical form.
    pub fn dominant_ending(&self) -> &'static str {
        if self.crlf > self.lf { "\r\n" } else { "\n" }
    }
}

/// One memchr sweep over the newlines: each `\n` answers both questions —
/// how it arrived (the census) and whether a line-initial `\c ` follows it
/// (a chunk start).
pub fn pre_scan(bytes: &[u8]) -> PreScan {
    let mut starts = vec![0u32];
    let (mut lf, mut crlf) = (0u32, 0u32);
    for at in memchr_iter(b'\n', bytes) {
        if at > 0 && bytes[at - 1] == b'\r' {
            crlf += 1;
        } else {
            lf += 1;
        }
        if bytes[at + 1..].starts_with(b"\\c ") {
            starts.push(at as u32 + 1);
        }
    }
    PreScan { starts, lf, crlf }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_land_on_line_initial_c_only() {
        for (source, expected) in [
            (
                "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\c 2\n\\p \\v 1 b\n",
                vec![0, 8, 23],
            ),
            (
                "\\id GEN\r\n\\c 1\r\n\\p a\r\n\\c 2\r\n\\p b\r\n",
                vec![0, 9, 21],
            ),
            ("front matter only, no chapters", vec![0]),
            ("", vec![0]),
            // A `\c` mid-line (inside a footnote) is not a chunk boundary.
            (
                "\\id GEN\n\\p a footnote saying \\c mid-line is not a start\n\\c 3\n",
                vec![0, 56],
            ),
        ] {
            assert_eq!(pre_scan(source.as_bytes()).starts, expected, "{source:?}");
        }
    }

    #[test]
    fn the_census_tells_lf_from_crlf() {
        let scan = pre_scan(b"\\id GEN\n\\c 1\n\\p a\n");
        assert_eq!((scan.lf, scan.crlf), (3, 0));
        assert_eq!(scan.dominant_ending(), "\n");

        let scan = pre_scan(b"\\id GEN\r\n\\c 1\r\n\\p a\r\n");
        assert_eq!((scan.lf, scan.crlf), (0, 3));
        assert_eq!(scan.dominant_ending(), "\r\n");

        // Mixed: majority wins; a tie is canonical LF.
        let scan = pre_scan(b"a\r\nb\nc\r\n");
        assert_eq!((scan.lf, scan.crlf), (1, 2));
        assert_eq!(scan.dominant_ending(), "\r\n");
        let scan = pre_scan(b"a\r\nb\n");
        assert_eq!(scan.dominant_ending(), "\n");

        // A lone \r counts nothing.
        let scan = pre_scan(b"a\rb\nc");
        assert_eq!((scan.lf, scan.crlf), (1, 0));
    }

    #[test]
    fn chunks_tile_the_source() {
        let source = b"\\id GEN\r\n\\c 1\r\n\\p a\r\n\\c 2\r\n\\p b\r\n";
        let scan = pre_scan(source);
        assert_eq!(scan.starts[0], 0);
        assert!(scan.starts.windows(2).all(|w| w[0] < w[1]));
        // Every start after the first points AT a `\c ` backslash.
        for &start in &scan.starts[1..] {
            assert!(source[start as usize..].starts_with(b"\\c "));
        }
    }
}
