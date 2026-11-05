use super::*;
use crate::pack::{ObjectId, Signature};

fn save(at: i64) -> Save {
    let who = |at| Signature {
        name: String::new(),
        email: String::new(),
        at,
        offset_minutes: 0,
    };
    Save {
        id: ObjectId([0; 20]),
        author: who(at),
        committer: who(at + 5),
        message: String::new(),
        changes: Vec::new(),
    }
}

fn author_times(clocks: &[Clock]) -> Vec<i64> {
    clocks.iter().map(|c| c.author_at).collect()
}

const GOOD: i64 = 1_600_000_000;

// Why: a reset battery dates saves 1970 or 2000, and those dates reached the
// hub's progress charts and consolidated logs (Tesaka, Ɓille). A run of bad
// saves must land between the good ones around it, in order, keeping the
// device's time. The cases below are the hub's, where an estimate goes
// subtly wrong: no room for the run's spacing, good saves on one side only,
// none at all.
#[test]
fn a_reset_clock_keeps_its_spacing_and_ends_before_the_next_good_save() {
    let saves = [
        save(GOOD),
        save(946_684_800),
        save(946_684_900),
        save(GOOD + 10_000),
    ];
    let c = clocks(&saves, i64::MAX);
    assert_eq!(
        author_times(&c),
        [GOOD, GOOD + 9_899, GOOD + 9_999, GOOD + 10_000]
    );
    assert_eq!(c[1].device_at, Some(946_684_800));
    assert_eq!(
        c[1].committer_at,
        GOOD + 9_904,
        "the committer keeps its lag"
    );
    assert_eq!(c[0].device_at, None);
    assert_eq!(c[0].committer_at, GOOD + 5);
}

#[test]
fn spacing_wider_than_the_gap_is_spread_evenly() {
    let saves = [save(GOOD), save(0), save(50_000), save(GOOD + 300)];
    assert_eq!(
        author_times(&clocks(&saves, i64::MAX)),
        [GOOD, GOOD + 100, GOOD + 200, GOOD + 300]
    );
}

#[test]
fn good_saves_on_one_side_only_and_a_future_date_is_as_bad_as_1970() {
    let saves = [save(0), save(GOOD), save(GOOD * 3)];
    assert_eq!(
        author_times(&clocks(&saves, GOOD * 2)),
        [GOOD - 1, GOOD, GOOD + 1]
    );
}

#[test]
fn nothing_to_anchor_to_is_left_as_the_device_said() {
    let saves = [save(0), save(10)];
    let c = clocks(&saves, i64::MAX);
    assert_eq!(author_times(&c), [0, 10]);
    assert!(c.iter().all(|c| c.device_at.is_none()));
}

#[test]
fn an_estimated_committer_lags_at_most_a_minute_and_never_leads() {
    let mut late = save(0);
    late.committer.at = 10_000;
    let mut early = save(5);
    early.committer.at = 0;
    let saves = [save(GOOD), late, early, save(GOOD + 1_000)];
    let c = clocks(&saves, i64::MAX);
    assert_eq!(c[1].committer_at - c[1].author_at, 60);
    assert_eq!(c[2].committer_at, c[2].author_at);
}

#[test]
fn days() {
    assert_eq!(day_of(0), "1970-01-01");
    assert_eq!(day_of(946_684_800), "2000-01-01");
    assert_eq!(day_of(951_782_400), "2000-02-29");
    assert_eq!(day_of(-1), "1969-12-31");
}
