//! The drift gates. These re-derive every scalar's class from the pinned UCD
//! extracts and from `std`, so the committed table cannot rot quietly.
//!
//! They sit here rather than in `mise` because the extracts and the generator
//! do: `mise` takes no dependencies, and an oracle written against the file
//! format needs the files.

use std::path::PathBuf;

use mise::unicode::{Class, bits, class_of, lookup::trie_at};

use super::{Pool, pool_of};

fn ucd(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/ucd")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "pinned UCD extract {} must be present: {error}",
            path.display()
        )
    })
}

/// An oracle written against the file format rather than against the
/// generator: it never coalesces, never blocks, and never reads `table.rs`.
fn oracle() -> Vec<u16> {
    // (file, property, bits) — the whole mapping, in one readable place.
    const RULES: &[(&str, &str, u16)] = &[
        ("DerivedGeneralCategory.txt", "Mn", bits::MARK),
        ("DerivedGeneralCategory.txt", "Mc", bits::MARK),
        ("DerivedGeneralCategory.txt", "Me", bits::MARK),
        ("DerivedGeneralCategory.txt", "Pc", bits::PUNCTUATION),
        ("DerivedGeneralCategory.txt", "Pd", bits::PUNCTUATION),
        ("DerivedGeneralCategory.txt", "Ps", bits::PUNCTUATION),
        ("DerivedGeneralCategory.txt", "Pe", bits::PUNCTUATION),
        ("DerivedGeneralCategory.txt", "Pi", bits::PUNCTUATION),
        ("DerivedGeneralCategory.txt", "Pf", bits::PUNCTUATION),
        ("DerivedGeneralCategory.txt", "Po", bits::PUNCTUATION),
        ("DerivedGeneralCategory.txt", "Sm", bits::SYMBOL),
        ("DerivedGeneralCategory.txt", "Sc", bits::SYMBOL),
        ("DerivedGeneralCategory.txt", "Sk", bits::SYMBOL),
        ("DerivedGeneralCategory.txt", "So", bits::SYMBOL),
        ("DerivedGeneralCategory.txt", "Nd", bits::DECIMAL_DIGIT),
        ("DerivedGeneralCategory.txt", "Cc", bits::CONTROL),
        ("DerivedGeneralCategory.txt", "Cf", bits::FORMAT),
        ("DerivedCoreProperties.txt", "Alphabetic", bits::ALPHABETIC),
        ("DerivedCoreProperties.txt", "Uppercase", bits::UPPERCASE),
        ("DerivedCoreProperties.txt", "Lowercase", bits::LOWERCASE),
        ("DerivedCoreProperties.txt", "InCB; Linker", bits::LINKER),
        ("PropList.txt", "White_Space", bits::WHITESPACE),
        ("GraphemeBreakProperty.txt", "Extend", bits::EXTENDER),
        ("GraphemeBreakProperty.txt", "SpacingMark", bits::EXTENDER),
        ("GraphemeBreakProperty.txt", "ZWJ", bits::EXTENDER),
        (
            "GraphemeBreakProperty.txt",
            "Control",
            bits::COMPLEX | bits::GCB_CONTROL,
        ),
        (
            "GraphemeBreakProperty.txt",
            "CR",
            bits::COMPLEX | bits::GCB_CONTROL,
        ),
        (
            "GraphemeBreakProperty.txt",
            "LF",
            bits::COMPLEX | bits::GCB_CONTROL,
        ),
        (
            "GraphemeBreakProperty.txt",
            "Prepend",
            bits::COMPLEX | bits::PREPEND,
        ),
        (
            "GraphemeBreakProperty.txt",
            "Regional_Indicator",
            bits::COMPLEX,
        ),
        ("GraphemeBreakProperty.txt", "L", bits::COMPLEX),
        ("GraphemeBreakProperty.txt", "V", bits::COMPLEX),
        ("GraphemeBreakProperty.txt", "T", bits::COMPLEX),
        ("GraphemeBreakProperty.txt", "LV", bits::COMPLEX),
        ("GraphemeBreakProperty.txt", "LVT", bits::COMPLEX),
        ("emoji-data.txt", "Extended_Pictographic", bits::COMPLEX),
    ];

    let mut want = vec![0u16; 0x11_0000];
    let mut current = String::new();
    let mut text = String::new();
    for &(file, property, bit) in RULES {
        if current != file {
            text = ucd(file);
            current = file.to_owned();
        }
        for line in text.lines() {
            let Some(body) = line.split('#').next().map(str::trim) else {
                continue;
            };
            let mut fields = body.split(';').map(str::trim);
            let Some(scalars) = fields.next().filter(|field| !field.is_empty()) else {
                continue;
            };
            let named = fields.collect::<Vec<_>>().join("; ");
            if named != property {
                continue;
            }
            let mut ends = scalars.split("..");
            let lo = u32::from_str_radix(ends.next().expect("a low scalar"), 16).unwrap();
            let hi = ends
                .next()
                .map_or(lo, |hi| u32::from_str_radix(hi, 16).unwrap());
            for cp in lo..=hi {
                want[cp as usize] |= bit;
            }
        }
    }
    for cp in 0xFDD0..=0xFDEFu32 {
        want[cp as usize] |= bits::NONCHARACTER;
    }
    for plane in 0..=0x10u32 {
        want[((plane << 16) | 0xFFFE) as usize] |= bits::NONCHARACTER;
        want[((plane << 16) | 0xFFFF) as usize] |= bits::NONCHARACTER;
    }
    want
}

fn scalars() -> impl Iterator<Item = char> {
    (0..=0x10_FFFFu32).filter_map(char::from_u32)
}

#[test]
fn table_matches_a_fresh_ucd_parse_for_every_scalar() {
    let want = oracle();
    let mut drifted = 0u32;
    let mut samples = Vec::new();
    for c in scalars() {
        let got = class_of(c).bits();
        let expected = want[c as usize];
        if got != expected {
            drifted += 1;
            if samples.len() < 8 {
                samples.push(format!(
                    "U+{:04X} table {got:#06x} ucd {expected:#06x}",
                    c as u32
                ));
            }
        }
    }
    assert_eq!(
        drifted, 0,
        "scalars drifted from UCD 17.0.0; first: {samples:?}"
    );
}

/// The G2 pools, re-derived from the extracts without reading `pools.rs`:
/// the same precedence `unicode::Pool` documents, applied to every scalar.
fn pool_oracle() -> Vec<Pool> {
    const RULES: &[(&str, &[&str], Pool)] = &[
        ("PropList.txt", &["Terminal_Punctuation"], Pool::Separator),
        ("PropList.txt", &["Sentence_Terminal"], Pool::Terminal),
        ("PropList.txt", &["Dash"], Pool::Dash),
        (
            "DerivedGeneralCategory.txt",
            &["Ps", "Pe", "Pi", "Pf"],
            Pool::Bracket,
        ),
        ("PropList.txt", &["Quotation_Mark"], Pool::Quote),
    ];

    let mut want = vec![Pool::Other; 0x11_0000];
    // Lowest precedence first, each pass overwriting: first match wins.
    for &(file, properties, pool) in RULES {
        let text = ucd(file);
        for line in text.lines() {
            let Some(body) = line.split('#').next().map(str::trim) else {
                continue;
            };
            let mut fields = body.split(';').map(str::trim);
            let Some(scalars) = fields.next().filter(|field| !field.is_empty()) else {
                continue;
            };
            let named = fields.collect::<Vec<_>>().join("; ");
            if !properties.contains(&named.as_str()) {
                continue;
            }
            let mut ends = scalars.split("..");
            let lo = u32::from_str_radix(ends.next().expect("a low scalar"), 16).unwrap();
            let hi = ends
                .next()
                .map_or(lo, |hi| u32::from_str_radix(hi, 16).unwrap());
            for cp in lo..=hi {
                want[cp as usize] = pool;
            }
        }
    }
    // The two pools a Class bit fixes, below every property above them.
    for cp in 0..0x11_0000u32 {
        if want[cp as usize] != Pool::Other {
            continue;
        }
        let Some(scalar) = char::from_u32(cp) else {
            continue;
        };
        let class = class_of(scalar);
        if class.is_decimal_digit() {
            want[cp as usize] = Pool::Digit;
        } else if class.is_symbol() {
            want[cp as usize] = Pool::Symbol;
        }
    }
    want
}

#[test]
fn the_pool_table_matches_a_fresh_ucd_parse_for_every_scalar() {
    let want = pool_oracle();
    let mut samples = Vec::new();
    let mut drifted = 0u32;
    for c in scalars() {
        let got = pool_of(c);
        if got != want[c as usize] {
            drifted += 1;
            if samples.len() < 8 {
                samples.push(format!(
                    "U+{:04X} table {got:?} ucd {:?}",
                    c as u32, want[c as usize]
                ));
            }
        }
    }
    assert_eq!(
        drifted, 0,
        "pools drifted from UCD 17.0.0; first: {samples:?}"
    );
    // No decimal digit carries one of the four pinned properties, which is
    // what lets `pool_of` answer `Digit` before it searches.
    assert!(
        scalars().all(|c| !class_of(c).is_decimal_digit() || pool_of(c) == Pool::Digit),
        "an Nd scalar was claimed by a punctuation pool"
    );
}

/// First match wins, and the two pairs the charter kept apart stay apart.
#[test]
fn pool_precedence_is_first_match_wins() {
    assert_eq!(
        pool_of('\u{ab}'),
        Pool::Quote,
        "« is a quote, not a bracket"
    );
    assert_eq!(pool_of('('), Pool::Bracket);
    assert_eq!(
        pool_of('\u{2212}'),
        Pool::Dash,
        "MINUS SIGN is Sm and a Dash"
    );
    assert_eq!(pool_of('\u{964}'), Pool::Terminal, "danda");
    assert_eq!(pool_of('\u{1362}'), Pool::Terminal, "Ethiopic full stop");
    assert_eq!(pool_of('\u{60c}'), Pool::Separator, "Arabic comma");
    assert_eq!(pool_of('\u{967}'), Pool::Digit, "Devanagari one");
    assert_eq!(pool_of('$'), Pool::Symbol);
    assert_eq!(pool_of('a'), Pool::Other);
    assert_eq!(pool_of('_'), Pool::Other, "Pc is no pool of its own");
    for (raw, pool) in Pool::ALL.iter().enumerate() {
        assert_eq!(Pool::from_raw(raw as u8), Some(*pool));
    }
    assert_eq!(Pool::from_raw(Pool::ALL.len() as u8), None);
}

#[test]
fn std_char_predicates_agree_over_every_scalar() {
    // std's Unicode version must match the pin. If this fails, bump the pin
    // — do not relax the test.
    let mut counts = [0u32; 4];
    for c in scalars() {
        let class = class_of(c);
        counts[0] += u32::from(class.is_alphabetic() != c.is_alphabetic());
        counts[1] += u32::from(class.is_uppercase() != c.is_uppercase());
        counts[2] += u32::from(class.is_lowercase() != c.is_lowercase());
        counts[3] += u32::from(class.is_whitespace() != c.is_whitespace());
    }
    assert_eq!(
        counts, [0; 4],
        "alphabetic/uppercase/lowercase/whitespace disagree with std"
    );
}

#[test]
fn the_decimal_digit_lane_is_nd_not_every_numeric() {
    // std exposes no `Nd` predicate, so the claim is pinned from both sides:
    // Nd is a strict subset of `is_numeric`, and over ASCII the two agree.
    let mut only_numeric = 0u32;
    for c in scalars() {
        let class = class_of(c);
        assert!(
            !class.is_decimal_digit() || c.is_numeric(),
            "U+{:04X} is Nd but not numeric",
            c as u32
        );
        assert_eq!(
            class.is_decimal_digit() && c.is_ascii(),
            c.is_ascii_digit(),
            "U+{:04X} disagrees with is_ascii_digit",
            c as u32
        );
        only_numeric += u32::from(c.is_numeric() && !class.is_decimal_digit());
    }
    // Nl/No exist and stay out of the digit lane (charter invariant 7).
    assert!(only_numeric > 0);
}

#[test]
fn the_two_index_paths_agree_over_every_scalar() {
    let mut buf = [0u8; 4];
    for c in scalars() {
        let (trie, width) = trie_at(c.encode_utf8(&mut buf).as_bytes());
        assert_eq!(
            class_of(c),
            trie,
            "UTF-8 trie disagrees at U+{:04X}",
            c as u32
        );
        assert_eq!(
            width,
            c.len_utf8(),
            "trie width disagrees at U+{:04X}",
            c as u32
        );
    }
}

#[test]
fn the_bit_list_is_the_one_the_charter_authorizes() {
    let a = class_of('A');
    assert!(a.is_alphabetic() && a.is_uppercase() && !a.is_lowercase());
    assert!(class_of('a').is_lowercase());
    assert!(class_of(' ').is_whitespace() && class_of('\u{a0}').is_whitespace());
    assert!(class_of('7').is_decimal_digit() && class_of('\u{966}').is_decimal_digit());
    assert!(!class_of('\u{2160}').is_decimal_digit()); // ROMAN NUMERAL ONE is Nl
    assert!(class_of('\u{301}').is_mark() && class_of('\u{301}').is_glue());
    assert!(class_of(',').is_punctuation() && class_of('+').is_symbol());
    assert!(class_of('\0').is_control() && class_of('\u{200d}').is_format());
    assert!(class_of('\u{200d}').is_extender() && class_of('\u{200d}').is_glue());
    assert!(class_of('\u{fdd0}').is_noncharacter() && class_of('\u{10ffff}').is_noncharacter());
    assert!(!class_of('\u{fffd}').is_noncharacter());
    assert!(class_of('\u{1f6d1}').is_complex()); // Extended_Pictographic
    assert!(class_of('\n').is_gcb_control() && class_of('\r').is_gcb_control());
    assert!(class_of('\u{600}').is_prepend() && class_of('\u{94d}').is_linker());
    // U+0378 is unassigned: no bit in the authorized list applies.
    assert_eq!(class_of('\u{378}'), Class::default());
    assert!(class_of('\u{378}').is_empty());
}
