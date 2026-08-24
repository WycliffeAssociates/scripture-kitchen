//! The chain oracle: the composition rule, executed.
//!
//! Each of the three maps is single-purpose, and a real consumer chains them —
//! so this test walks the chains end to end on real bytes and checks that every
//! hop inverts:
//!
//! ```text
//! source byte ──Mask::from_source──▶ mask byte ──to_source──▶ source byte
//! source byte ──Utf16Index(source)──▶ utf16 ──to_byte──▶ source byte
//! mask byte  ──Utf16Index(mask text)──▶ utf16 ──to_byte──▶ mask byte
//! source byte ──Toc::locate──▶ sid, which a direct scan of the anchors agrees with
//! ```
//!
//! Sampled at deterministic pseudo-random KEPT bytes (a fixed LCG — nothing
//! here varies between runs), snapped down onto character boundaries: a byte
//! inside a multi-byte character has no UTF-16 name of its own, and a character
//! never straddles a mask range, so the snap keeps the byte kept.
//!
//! The Hindi book is the point of the non-ASCII file: byte 15 and UTF-16 15 are
//! different positions there, which is exactly the drift a wrong chain hides.

use std::path::Path;

use usfm_onion_2::mask::{Filter, mask};
use usfm_onion_2::{Toc, cst, lex, toc, utf16_index};

/// Sample points per file per hop.
const SAMPLES: usize = 1_000;

/// The files: one dense non-ASCII book, two ASCII ones for volume.
const FILES: [&str; 3] = [
    "testData/samples-from-wild/hindi-IRV1/origin.usfm",
    "example-corpora/en_ulb/19-PSA.usfm",
    "example-corpora/en_ulb/41-MAT.usfm",
];

/// An LCG (Numerical Recipes' constants). A test that samples must sample the
/// same points every run, so no clock and no thread-local RNG.
struct Rng(u32);

impl Rng {
    fn next(&mut self, modulo: usize) -> usize {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as usize % modulo
    }
}

/// The verse extent a direct scan says holds `at`: the last anchor at or before
/// it, unless that anchor belongs to an earlier chapter. `Toc::verse_at`'s
/// binary searches must agree with this walk.
fn scanned_verse(toc: &Toc, at: u32) -> (u16, Option<(u16, u16)>) {
    let chapter = toc
        .chapters
        .iter()
        .rev()
        .find(|row| row.start <= at)
        .expect("chapter rows tile the source");
    let Some(anchor) = toc
        .verses
        .iter()
        .rev()
        .find(|v| v.at <= at && v.at >= chapter.start)
    else {
        // Inside a chapter but ahead of its first verse: a real answer, and the
        // one `locate` degrades to `(0, 0)`.
        return (chapter.number, None);
    };
    let end = toc
        .verses
        .iter()
        .find(|v| v.at > anchor.at)
        .map_or(chapter.end, |v| v.at)
        .min(chapter.end);
    assert!(
        (anchor.at..end).contains(&at),
        "the scanned extent must contain the byte it was found for"
    );
    (chapter.number, Some((anchor.first, anchor.last)))
}

#[test]
fn every_hop_inverts_on_real_bytes() {
    let mut checked = 0u64;
    let mut files = 0u64;

    for file in FILES {
        let path = Path::new(file);
        if !path.exists() {
            eprintln!("chain oracle: skipping {file} — not mounted");
            continue;
        }
        files += 1;
        let source = std::fs::read_to_string(path).expect("readable book");
        let bytes = source.as_bytes();
        let tokens = lex(&source);
        let cst = cst::build(&tokens);
        let toc = toc(bytes, &tokens);
        let m = mask(bytes, &tokens, &cst, &Filter::verse_text());
        let source_index = utf16_index(bytes);
        let mask_text = m.text(bytes);
        let mask_index = utf16_index(mask_text.as_bytes());

        assert!(!m.is_empty(), "{file}: nothing survived verse_text");

        // The kept bytes that start a character — the pool the sample draws
        // from. A continuation byte is dropped from the pool rather than
        // snapped, so the sample is uniform over the positions UTF-16 can name.
        let kept: Vec<u32> = m
            .ranges
            .iter()
            .flat_map(|range| range.clone())
            .filter(|b| bytes[*b as usize] & 0xC0 != 0x80)
            .collect();
        assert!(kept.len() > SAMPLES, "{file}: too few kept bytes to sample");

        let mut rng = Rng(0x5EED_1234);
        for _ in 0..SAMPLES {
            let src = kept[rng.next(kept.len())];

            // ---- mask ↔ source ------------------------------------------
            let mask_off = m
                .from_source(src)
                .unwrap_or_else(|| panic!("{file}: kept byte {src} has no mask offset"));
            assert_eq!(
                m.to_source(mask_off),
                src,
                "{file}: mask↔source does not invert at {src}"
            );
            assert_eq!(
                mask_text.as_bytes()[mask_off as usize],
                bytes[src as usize],
                "{file}: the mask byte at {mask_off} is not the source byte at {src}"
            );

            // ---- source ↔ utf16 -----------------------------------------
            let units = source_index.to_utf16(src);
            assert_eq!(
                source_index.to_byte(units),
                src,
                "{file}: source↔utf16 does not invert at {src}"
            );

            // ---- the rare chain: utf16 over the MASK TEXT ---------------
            let mask_units = mask_index.to_utf16(mask_off);
            assert_eq!(
                mask_index.to_byte(mask_units),
                mask_off,
                "{file}: mask text↔utf16 does not invert at {mask_off}"
            );
            assert_eq!(
                m.to_source(mask_index.to_byte(mask_units)),
                src,
                "{file}: the whole utf16→mask→source chain missed at {src}"
            );

            // ---- source → sid, and the scan agrees ----------------------
            let sid = toc.locate(src);
            let (chapter, verse) = scanned_verse(&toc, src);
            assert_eq!(
                (sid.chapter, (sid.first, sid.last)),
                (chapter, verse.unwrap_or((0, 0))),
                "{file}: locate({src}) disagrees with a direct scan of the anchors"
            );
            // Verse text is verse text: a kept TEXT byte is inside some verse's
            // extent, never front matter. (Newlines are their own tokens and
            // survive everywhere, which is why they are excepted.)
            assert!(
                verse.is_some() || bytes[src as usize] == b'\n',
                "{file}: kept byte {src} is in no verse extent"
            );
            // The same sid off the utf16 hop — what a JS editor asking "what
            // verse is my cursor in?" actually runs.
            assert_eq!(
                toc.locate(source_index.to_byte(units)),
                sid,
                "{file}: utf16→byte→locate lost the reference at {src}"
            );

            checked += 1;
        }
    }

    eprintln!("chain oracle: {files} files × {checked} sampled bytes × 5 hops");
    assert!(files > 0, "no chain-oracle file is mounted");
}

/// The one thing a sample cannot see: a byte the mask DROPPED has no home in
/// the masked view, and says so instead of clamping.
#[test]
fn a_dropped_byte_has_no_mask_offset() {
    let source = "\\id GEN\n\\c 1\n\\p \\v 1 Jesus wept.\\f + \\ft why\\f*\n";
    let bytes = source.as_bytes();
    let tokens = lex(source);
    let cst = cst::build(&tokens);
    let m = mask(bytes, &tokens, &cst, &Filter::verse_text());
    let inside_the_note = source.find("why").unwrap() as u32;
    assert_eq!(m.from_source(inside_the_note), None);
    let wept = source.find("wept").unwrap() as u32;
    let at = m.from_source(wept).expect("verse text survives");
    assert_eq!(m.to_source(at), wept);
}
