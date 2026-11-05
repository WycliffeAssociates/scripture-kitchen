//! `src/fold_tables.rs`: what `canon::fold` turns each character into.
//!
//! Sefer folds with `text.normalize("NFKD")` and then drops U+0300..U+036F,
//! so "Éxodo" is "exodo". canon has no Unicode library, so the generator
//! writes that per-character answer down: every scalar whose NFKD, less those
//! marks, differs from itself. Hangul syllables are left out: their NFKD is
//! arithmetic, and `fold` does it in a few lines rather than 11,172 rows.

use std::fmt::Write as _;

use unicode_normalization::UnicodeNormalization;

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

pub fn rust(table: &[(char, String)], header: &str) -> String {
    let mut out = format!("// {header}\n\n");
    out.push_str(
        "/// Each character whose NFKD, less U+0300..U+036F, is not itself, with that\n\
         /// result, sorted for binary search. Hangul syllables are `fold`'s arithmetic.\n",
    );
    out.push_str("#[rustfmt::skip]\npub(crate) static FOLD: &[(char, &str)] = &[\n");
    for (c, folded) in table {
        let _ = writeln!(out, "    ({c:?}, {folded:?}),");
    }
    out.push_str("];\n");
    out
}
