//! `src/fold_tables.rs`: what `canon::fold` turns each character into.
//!
//! Sefer folds with `text.normalize("NFKD")` and then drops U+0300..U+036F,
//! so "Éxodo" is "exodo". canon has no Unicode library, so the generator
//! writes that per-character answer down: every scalar whose NFKD, less those
//! marks, differs from itself. Hangul syllables are left out: their NFKD is
//! arithmetic, and `fold` does it in a few lines rather than 11,172 rows.
//!
//! NFKD also sorts each run of combining marks by combining class, which no
//! per-character table can do, so the classes are written too and `fold`
//! sorts. U+0300..U+036F are left out: they are dropped either way.

use std::fmt::Write as _;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::canonical_combining_class;

const HANGUL: std::ops::RangeInclusive<u32> = 0xAC00..=0xD7A3;

/// Every changed character, in code point order, with what it becomes.
pub fn table() -> Vec<(char, String)> {
    (0..=0x10FFFF_u32)
        .filter(|cp| !HANGUL.contains(cp))
        .filter_map(char::from_u32)
        .filter_map(|c| {
            let folded: String = std::iter::once(c)
                .nfkd()
                .filter(|d| !('\u{300}'..='\u{36f}').contains(d))
                .collect();
            let mut same = [0; 4];
            (folded != *c.encode_utf8(&mut same)).then_some((c, folded))
        })
        .collect()
}

/// Every mark with a non-zero combining class, outside U+0300..U+036F.
pub fn classes() -> Vec<(char, u8)> {
    (0..=0x10FFFF_u32)
        .filter_map(char::from_u32)
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .map(|c| (c, canonical_combining_class(c)))
        .filter(|(_, class)| *class != 0)
        .collect()
}

/// Every character whose canonical decomposition (NFD) is longer than one
/// character, with its length. Hangul syllables are arithmetic, as in `fold`.
pub fn nfd_lengths() -> Vec<(char, u8)> {
    (0..=0x10FFFF_u32)
        .filter(|cp| !HANGUL.contains(cp))
        .filter_map(char::from_u32)
        .filter_map(|c| {
            let len = std::iter::once(c).nfd().count();
            (len > 1).then(|| (c, u8::try_from(len).expect("an NFD under 256 characters")))
        })
        .collect()
}

pub fn nfd_rust(lengths: &[(char, u8)], header: &str) -> String {
    let mut out = format!("// {header}\n\n");
    out.push_str(
        "/// Each character whose NFD is more than one character, with how many,\n\
         /// sorted for binary search. Hangul syllables are `nfd_len`'s arithmetic.\n",
    );
    out.push_str("#[rustfmt::skip]\npub(crate) static NFD_LEN: &[(char, u8)] = &[\n");
    for (c, len) in lengths {
        let _ = writeln!(out, "    ({c:?}, {len}),");
    }
    out.push_str("];\n");
    out
}

pub fn rust(table: &[(char, String)], classes: &[(char, u8)], header: &str) -> String {
    let mut out = format!("// {header}\n\n");
    out.push_str(
        "/// Each character whose NFKD, less U+0300..U+036F, is not itself, with that\n\
         /// result, sorted for binary search. Hangul syllables are `fold`'s arithmetic.\n",
    );
    out.push_str("#[rustfmt::skip]\npub(crate) static FOLD: &[(char, &str)] = &[\n");
    for (c, folded) in table {
        let _ = writeln!(out, "    ({c:?}, {folded:?}),");
    }
    out.push_str("];\n\n");
    out.push_str(
        "/// Each combining mark's canonical combining class (none of U+0300..U+036F),\n\
         /// sorted for binary search: `fold` orders runs of marks by it, as NFKD does.\n",
    );
    out.push_str("#[rustfmt::skip]\npub(crate) static COMBINING: &[(char, u8)] = &[\n");
    for (c, class) in classes {
        let _ = writeln!(out, "    ({c:?}, {class}),");
    }
    out.push_str("];\n");
    out
}
