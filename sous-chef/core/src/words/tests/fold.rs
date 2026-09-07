//! The fold: chapters merged into one book, seams included.

use super::*;

#[test]
fn the_fold_merges_by_hash_and_carries_no_seam() {
    let joined = fold(&["David went", "David wept"]);
    let david = joined.rows_for(hash_of("david"));
    // Both chapters' first words stand at their own chapter's start.
    assert_eq!(david.len(), 1);
    assert_eq!(david[0].before(), Before::Start);
    assert_eq!(david[0].count_of(Form::Title), 2);
    assert!(joined.cased());

    // A word is never split across a masked `\c`, so order cannot matter.
    let reversed = fold(&["David wept", "David went"]);
    assert_eq!(joined.words(), reversed.words());
    assert!(!fold(&["\u{5d0}\u{5d1}", "\u{5d2}"]).cased());
}

/// The doubles lane merges by hash, and the seam carries nothing: a pair the
/// chapter edge split was never counted, so there is nothing to fold.
#[test]
fn the_fold_merges_the_doubles_lane_and_the_seam_ends_a_pair() {
    let joined = fold(&["go go on", "go, go on"]);
    let go = joined.doubles_for(hash_of("go")).expect("one row");
    assert_eq!((u64::from(go.bare), go.count_of(true)), (1, 1));

    // A cased word that never doubles is in no doubles row at all, which is
    // what keeps the lane cheap in a cased script.
    let split = fold(&["and go", "go and"]);
    assert!(split.doubles().is_empty());
}
