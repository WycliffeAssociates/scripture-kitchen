//! What counts as a verse number in running text. A document picks one way of
//! writing them; the mode names it, and only numbers written that way, in
//! sequence, become verses. Everything else stays text.
//!
//! ```text
//! "¹In the beginning"   superscript "1"       a typeset draft
//! "v1 In", "verse 2 …"  marked                an editor's typed numbers
//! "3 He said"           lineStart             a number opening a line
//! "and 3 sons"          inline                only in `any` mode
//! "1,000 men"           thousands             a quantity, never a verse
//! ```
//!
//! The matchers are `@sefer/document-import`'s regular expressions written
//! out by hand, so the module needs no regex engine:
//!
//! ```text
//! NUMBER      (\d{1,3})(?:\s?[-–]\s?(\d{1,3}))?(?!\d)\.?
//! WHOLE_RUN   ^\s*NUMBER\s*$
//! LINE_START  ^\s*NUMBER(?=\s|$)
//! MARKED      (?<=^|\s)(?:verse\s*|v\.?\s*)NUMBER(?=\s|$)      case-insensitive
//! INLINE      (?<=^|[\s(])NUMBER(?=[\s,;:!?)]|$)
//! ```

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerseMode {
    /// A superscript number, a number set apart in its own run, or one that
    /// starts a paragraph or line.
    Superscript,
    /// "v1", "v 1", "verse 1".
    Marked,
    /// A number at the start of a paragraph or line.
    #[default]
    LineStart,
    /// No numbers at all; every paragraph is the next verse. Only chosen.
    PerLine,
    /// Any free-standing number. Only chosen.
    Any,
}

impl VerseMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Superscript => "superscript",
            Self::Marked => "marked",
            Self::LineStart => "lineStart",
            Self::PerLine => "perLine",
            Self::Any => "any",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "superscript" => Self::Superscript,
            "marked" => Self::Marked,
            "lineStart" => Self::LineStart,
            "perLine" => Self::PerLine,
            "any" => Self::Any,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Superscript,
    Alone,
    Marked,
    LineStart,
    Inline,
}

pub fn accepts(mode: VerseMode, kind: Kind) -> bool {
    match mode {
        VerseMode::PerLine => false,
        VerseMode::Superscript => matches!(kind, Kind::Superscript | Kind::Alone | Kind::LineStart),
        VerseMode::Marked => kind == Kind::Marked,
        VerseMode::LineStart => matches!(kind, Kind::LineStart | Kind::Alone),
        VerseMode::Any => true,
    }
}

/// How many numbers of each kind a document holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub superscript: usize,
    pub alone: usize,
    pub marked: usize,
    pub line_start: usize,
    pub inline: usize,
}

impl Counts {
    pub fn add(&mut self, kind: Kind) {
        match kind {
            Kind::Superscript => self.superscript += 1,
            Kind::Alone => self.alone += 1,
            Kind::Marked => self.marked += 1,
            Kind::LineStart => self.line_start += 1,
            Kind::Inline => self.inline += 1,
        }
    }
}

/// A number written in a way that could be a verse number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub kind: Kind,
    /// Byte range of the run the number occupies, prefix and period included.
    pub start: usize,
    pub end: usize,
    pub from: u32,
    /// The last verse of a bridge such as "1-2"; `from` otherwise.
    pub to: u32,
    /// "1,000", "1.000" or "1 000": a quantity, never a verse.
    pub thousands: bool,
}

/// One way NUMBER can match at a position, in the order a backtracking regex
/// would try them.
#[derive(Debug, Clone, Copy)]
struct Number {
    end: usize,
    from: u32,
    to: u32,
}

fn digits(s: &str, at: usize) -> usize {
    s[at..].bytes().take_while(u8::is_ascii_digit).count()
}

fn space_at(s: &str, at: usize) -> Option<usize> {
    s[at..].chars().next().filter(|c| c.is_whitespace()).map(char::len_utf8)
}

/// NUMBER at `at`: with the bridge before without, the period before none.
fn numbers(s: &str, at: usize) -> Vec<Number> {
    let n = digits(s, at);
    if n == 0 || n > 3 {
        return Vec::new();
    }
    let from: u32 = s[at..at + n].parse().unwrap_or(0);
    let mut bases = Vec::new();
    // The bridge: \s?[-–]\s?(\d{1,3})(?!\d)
    let mut j = at + n;
    if let Some(w) = space_at(s, j)
        && s[j + w..].starts_with(['-', '–'])
    {
        j += w;
    }
    if let Some(dash) = s[j..].chars().next().filter(|c| matches!(c, '-' | '–')) {
        let mut k = j + dash.len_utf8();
        if let Some(w) = space_at(s, k)
            && s[k + w..].starts_with(|c: char| c.is_ascii_digit())
        {
            k += w;
        }
        let m = digits(s, k);
        if (1..=3).contains(&m) {
            let to = s[k..k + m].parse().unwrap_or(0);
            bases.push(Number { end: k + m, from, to });
        }
    }
    bases.push(Number { end: at + n, from, to: from });
    let mut out = Vec::new();
    for b in bases {
        if s[b.end..].starts_with('.') {
            out.push(Number { end: b.end + 1, ..b });
        }
        out.push(b);
    }
    out
}

fn ws_or_end(s: &str, at: usize) -> bool {
    at == s.len() || space_at(s, at).is_some()
}

fn skip_ws(s: &str, mut at: usize) -> usize {
    while let Some(w) = space_at(s, at) {
        at += w;
    }
    at
}

fn thousands(s: &str, start: usize, end: usize) -> bool {
    let after = &s[end..];
    let three_digits = |t: &str| t.len() >= 3 && t.as_bytes()[..3].iter().all(u8::is_ascii_digit);
    let after_hit = match after.chars().next() {
        Some(',' | '.') => three_digits(&after[1..]),
        Some(c @ (' ' | '\u{A0}' | '\u{202F}')) => {
            let rest = &after[c.len_utf8()..];
            three_digits(rest) && !rest[3..].starts_with(|c: char| c.is_ascii_digit())
        }
        _ => false,
    };
    let before = &s[..start];
    let mut back = before.chars().rev();
    let before_hit = matches!(back.next(), Some(',' | '.' | ' ' | '\u{A0}' | '\u{202F}'))
        && back.next().is_some_and(|c| c.is_ascii_digit());
    after_hit || before_hit
}

/// Every way the run could hold a verse number, earliest first.
pub fn candidates(text: &str, superscript: bool, at_line_start: bool) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut add = |kind: Kind, start: usize, n: Number| {
        if out.iter().any(|c| c.start < n.end && start < c.end) {
            return;
        }
        out.push(Candidate {
            kind,
            start,
            end: n.end,
            from: n.from,
            to: n.to,
            thousands: thousands(text, start, n.end),
        });
    };

    // WHOLE_RUN: the match is the whole run, its spaces included.
    let lead = skip_ws(text, 0);
    if let Some(n) = numbers(text, lead).into_iter().find(|n| skip_ws(text, n.end) == text.len()) {
        add(if superscript { Kind::Superscript } else { Kind::Alone }, 0, Number { end: text.len(), ..n });
        return out;
    }
    if superscript {
        return out;
    }
    // LINE_START: the match starts at the run's start, leading spaces included.
    if at_line_start && let Some(n) = numbers(text, lead).into_iter().find(|n| ws_or_end(text, n.end)) {
        add(Kind::LineStart, 0, n);
    }
    // MARKED, global
    let mut at = 0;
    while at < text.len() {
        let behind_ok = at == 0 || text[..at].chars().next_back().is_some_and(char::is_whitespace);
        let found = behind_ok.then(|| marked_at(text, at)).flatten();
        match found {
            Some(n) => {
                add(Kind::Marked, at, n);
                at = n.end.max(at + 1);
            }
            None => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    // INLINE, global
    let mut at = 0;
    while at < text.len() {
        let behind_ok = at == 0 || text[..at].chars().next_back().is_some_and(|c| c.is_whitespace() || c == '(');
        let found = behind_ok
            .then(|| {
                numbers(text, at).into_iter().find(|n| {
                    n.end == text.len()
                        || text[n.end..].starts_with(|c: char| c.is_whitespace() || ",;:!?)".contains(c))
                })
            })
            .flatten();
        match found {
            Some(n) => {
                add(Kind::Inline, at, n);
                at = n.end.max(at + 1);
            }
            None => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    out.sort_by_key(|c| c.start);
    out
}

/// `(?:verse\s*|v\.?\s*)NUMBER(?=\s|$)` at `at`, case-insensitive.
fn marked_at(text: &str, at: usize) -> Option<Number> {
    let rest = &text[at..];
    let lower = |n: usize| rest.get(..n).map(str::to_ascii_lowercase);
    let mut prefixes = Vec::new();
    if lower(5).as_deref() == Some("verse") {
        prefixes.push(at + 5);
    }
    if lower(1).as_deref() == Some("v") {
        if rest[1..].starts_with('.') {
            prefixes.push(at + 2);
        }
        prefixes.push(at + 1);
    }
    prefixes
        .into_iter()
        .find_map(|p| numbers(text, skip_ws(text, p)).into_iter().find(|n| ws_or_end(text, n.end)))
}

/// Picks the document's convention from how its numbers are written.
pub fn detect(counts: &Counts) -> VerseMode {
    let superscript = counts.superscript as f64;
    let marked = counts.marked as f64;
    let line_start = (counts.line_start + counts.alone) as f64;
    if superscript >= 3.0 && superscript >= marked && superscript >= line_start / 2.0 {
        return VerseMode::Superscript;
    }
    if marked >= 3.0 && marked >= line_start / 2.0 {
        return VerseMode::Marked;
    }
    VerseMode::LineStart
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Next,
    Restart,
}

/// Whether a number may be the next verse: ahead of the last one by at most
/// `gap`, or 1 again (a new chapter) when `restart` is allowed. `max` is the
/// chapter's verse count from a versification, when there is one.
pub fn next_verse(previous: u32, from: u32, to: u32, gap: u32, restart: bool, max: Option<u32>) -> Option<Step> {
    if to < from {
        return None;
    }
    if from > previous && from <= previous + gap {
        return if max.is_some_and(|m| to > m) { None } else { Some(Step::Next) };
    }
    if restart && from == 1 && previous >= 1 {
        return Some(Step::Restart);
    }
    None
}

/// How far ahead a number may jump: strong signals tolerate a skipped verse or two more.
pub fn gap_for(kind: Kind) -> u32 {
    if matches!(kind, Kind::Superscript | Kind::Marked | Kind::Alone) { 10 } else { 3 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, sup: bool, line: bool) -> Vec<(Kind, String, u32, u32, bool)> {
        candidates(text, sup, line)
            .into_iter()
            .map(|c| (c.kind, text[c.start..c.end].to_owned(), c.from, c.to, c.thousands))
            .collect()
    }

    #[test]
    fn a_run_that_is_only_a_number() {
        assert_eq!(found(" 12 ", true, false), vec![(Kind::Superscript, " 12 ".to_owned(), 12, 12, false)]);
        assert_eq!(found("3-4", false, false), vec![(Kind::Alone, "3-4".to_owned(), 3, 4, false)]);
        assert_eq!(found("1000", true, false), vec![]);
    }

    #[test]
    fn marked_line_start_and_inline() {
        let got = found("v1 One. verse 2 Two. v 3 Three in 4 parts.", false, true);
        let kinds: Vec<(Kind, u32)> = got.iter().map(|g| (g.0, g.2)).collect();
        assert_eq!(kinds, vec![(Kind::Marked, 1), (Kind::Marked, 2), (Kind::Marked, 3), (Kind::Inline, 4)]);
        let got = found("1 One, 3 days.", false, true);
        assert_eq!(got[0].0, Kind::LineStart);
        assert_eq!(got[1], (Kind::Inline, "3".to_owned(), 3, 3, false));
    }

    #[test]
    fn quantities_are_marked() {
        let got = found("The army was 1,000 men", false, false);
        assert!(got.iter().all(|g| g.4), "{got:?}");
        assert!(found("and 3 sons", false, false).iter().all(|g| !g.4));
    }

    #[test]
    fn sequence() {
        assert_eq!(next_verse(1, 2, 2, 3, true, None), Some(Step::Next));
        assert_eq!(next_verse(2, 9, 9, 10, true, Some(5)), None);
        assert_eq!(next_verse(9, 1, 1, 10, true, None), Some(Step::Restart));
        assert_eq!(next_verse(9, 1, 1, 3, false, None), None);
    }
}
