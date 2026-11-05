//! When a save happened, when the device's clock can't be right.
//!
//! ```text
//! device said   1,600,000,000   946,684,800 (2000-01-01)   946,684,900   1,600,010,000
//! clocks()      1,600,000,000   1,600,009,899              1,600,009,999 1,600,010,000
//!                               device_at: 946,684,800     device_at: 946,684,900
//! ```
//!
//! A reset battery dates saves 1970-01-01 or 2000-01-01 (Tesaka, Ɓille). A
//! save dated before Writer existed, or after `latest`, gets a time between
//! the good saves around it; the device's time is kept so a replayed commit can
//! say so (`Writer-Clock:`). Writer's own message is the same clock, so only
//! the neighbouring saves say when the work happened.
//!
//! Ported from the hub's `estimate_bad_clocks` (hub `65cd350`). It's a pure
//! function of the saves: the history keeps what git says, and replay and
//! progress ask for the estimate.

use crate::walk::Save;

/// 2015-01-01: before translationStudio, so before any Writer save.
pub const FIRST_POSSIBLE_SAVE: i64 = 1_420_070_400;

/// One save's times, as replay writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub author_at: i64,
    pub committer_at: i64,
    /// The device's author time, when it couldn't be right and `author_at`
    /// is an estimate.
    pub device_at: Option<i64>,
}

/// Each save's times, estimated where the device's clock can't be right:
/// before [`FIRST_POSSIBLE_SAVE`], or after `latest` (a day past now).
///
/// A bad clock is usually one device's run of saves, the clock counting on
/// from its reset, so a run keeps its own spacing and ends a second before the
/// next good save. When that spacing doesn't fit after the good save before
/// it, the run is spread evenly between the two. A run with good saves on one
/// side only sits a second apart beside them; a history with no good save is
/// left as it is. An estimated committer time keeps its lag behind the author,
/// up to a minute.
pub fn clocks(saves: &[Save], latest: i64) -> Vec<Clock> {
    let mut out: Vec<Clock> = saves
        .iter()
        .map(|s| Clock {
            author_at: s.author.at,
            committer_at: s.committer.at,
            device_at: None,
        })
        .collect();
    let bad = |at: i64| !(FIRST_POSSIBLE_SAVE..=latest).contains(&at);
    let mut i = 0;
    while i < saves.len() {
        if !bad(saves[i].author.at) {
            i += 1;
            continue;
        }
        let end = i + saves[i..].iter().take_while(|s| bad(s.author.at)).count();
        let before = i.checked_sub(1).map(|j| saves[j].author.at);
        let after = saves.get(end).map(|s| s.author.at);
        let run: Vec<i64> = saves[i..end].iter().map(|s| s.author.at).collect();
        let n = run.len() as i64;
        // Times are the repo's, so anything i64 holds: saturate, never wrap.
        let times: Vec<i64> = match (before, after) {
            (None, None) => return out,
            (Some(b), None) => (1..=n).map(|k| b.saturating_add(k)).collect(),
            (None, Some(a)) => (0..n).map(|k| a.saturating_sub(n - k)).collect(),
            (Some(b), Some(a)) => {
                let kept: Option<Vec<i64>> = a
                    .saturating_sub(1)
                    .checked_sub(run[run.len() - 1])
                    .and_then(|shift| run.iter().map(|t| t.checked_add(shift)).collect());
                match kept {
                    Some(kept) if kept.is_sorted() && kept[0] > b => kept,
                    // Evenly between the two good saves, never before the first.
                    _ => {
                        let gap = (i128::from(a) - i128::from(b)).max(0);
                        let n = i128::from(n);
                        (1..=n)
                            .map(|k| (i128::from(b) + gap * k / (n + 1)) as i64)
                            .collect()
                    }
                }
            }
        };
        for ((clock, save), at) in out[i..end].iter_mut().zip(&saves[i..end]).zip(times) {
            let lag = save.committer.at.saturating_sub(save.author.at);
            *clock = Clock {
                author_at: at,
                committer_at: at.saturating_add(lag.clamp(0, 60)),
                device_at: Some(save.author.at),
            };
        }
        i = end;
    }
    out
}

/// `2000-01-01`: the UTC day of a Unix time (days to civil, Howard Hinnant's).
pub fn day_of(unix: i64) -> String {
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests;
