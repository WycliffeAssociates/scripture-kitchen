//! Instrument: SHAPES — hand-built records, chosen so one JSON field is
//! under test at a time.

use super::*;

/// The bucket a hand-built join classifies into, and how many hits it
/// counts: `"a, b ,c , d,e"`, glyph `,`.
#[test]
fn topo_buckets_split_by_joint_neighbour_class() {
    let text = "a, b ,c , d,e";
    // Verse spans the whole string so every comma sits inside it.
    let chapter =
        Chapter::new(1, sous_core::TextRange::new(0, text.len() as u32).unwrap()).unwrap();
    let cursor = Cursor::new(text, std::slice::from_ref(&chapter));
    let mut counts: FxHashMap<&'static str, usize> = FxHashMap::default();
    for (at, c) in text.char_indices() {
        if c != ',' {
            continue;
        }
        let at = at as u32;
        let prev = cursor.prev_outer(at);
        let next = cursor.next_outer(at);
        let bucket = TOPO
            .iter()
            .find(|(_, p, n)| *p == prev && *n == next)
            .map_or("in-run", |(name, _, _)| name);
        *counts.entry(bucket).or_insert(0) += 1;
    }
    // "a," -> StartOnly (letter before, space after); " ,c" -> EndOnly
    // (space before, letter after); " , " -> Neither; "d,e" -> Both.
    assert_eq!(counts.get("Both"), Some(&1));
    assert_eq!(counts.get("StartOnly"), Some(&1));
    assert_eq!(counts.get("EndOnly"), Some(&1));
    assert_eq!(counts.get("Neither"), Some(&1));
    assert_eq!(counts.get("in-run"), None);
}
