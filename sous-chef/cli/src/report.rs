//! The `--report` page: one self-contained HTML file, no dependencies.
//!
//! ```text
//! sous --report debug/d3-en_ulb.html testData/exampleCorpora/en_ulb
//!   pattern[7] U+002C ',' placement next=Digit  12/9,812  0.12% band 4  3/66 books
//!     MRK 7:21  … he said , 12 men …          PlacementAfter
//!     LUK 2:4   … came , 40 days …            PlacementAfter
//! ```
//!
//! The visual check on "sites agree with counts": the pattern table at the top,
//! every row expanding to its sites, each site in ~40 scalars of context with
//! the span highlighted. Plain string templating — an HTML crate would be a
//! dependency for a debug view.

use sous_core::{
    Corpus, FindingKind, PackedFinding, Pattern, PatternKey, ProjectedBook, Reasons, ScalarKey,
    TextRange,
};
use usfm_galley::sous::OnionBook;

/// Scalars of context either side of a site's span.
const CONTEXT: usize = 40;
/// Sites written per pattern; the row says how many more there were.
const PER_PATTERN: usize = 200;

/// The whole page for one target corpus.
pub fn render(
    corpus: &Corpus<'_, OnionBook>,
    findings: &[PackedFinding],
    patterns: &[Pattern],
) -> String {
    let mut sites: Vec<Vec<&PackedFinding>> = vec![Vec::new(); patterns.len()];
    for finding in findings {
        if let FindingKind::Convention(digest) = finding.kind() {
            let at = usize::from(digest.pattern().get());
            if let Some(rows) = sites.get_mut(at) {
                rows.push(finding);
            }
        }
    }
    let total: usize = sites.iter().map(Vec::len).sum();

    let mut out = String::with_capacity(1 << 20);
    out.push_str(HEAD);
    out.push_str(&format!(
        "<h1>Sous Chef sites</h1><p class=\"sum\">{} books · {} patterns · {} sites</p>\
         <p class=\"hint\">j / k or \u{2193} / \u{2191} move between sites; o opens or closes a pattern.</p>",
        corpus.len(),
        patterns.len(),
        total,
    ));

    for (index, pattern) in patterns.iter().enumerate() {
        let rows = &sites[index];
        out.push_str(&format!(
            "<details class=\"p\"{}><summary><span class=\"i\">#{index}</span> \
             <span class=\"g\">{}</span> <span class=\"c\">{}</span> \
             <span class=\"e\">{}</span> <span class=\"f\">{}/{} · {:.2}%{}</span> \
             <span class=\"d\">{}/{} books</span> \
             <span class=\"n\">{} site{}</span></summary>",
            if rows.is_empty() { "" } else { " open" },
            escape(&glyph(pattern.glyph)),
            pattern.channel.name(),
            escape(&evidence(pattern)),
            pattern.numerator,
            pattern.denominator,
            f64::from(pattern.share_bp) / 100.0,
            match pattern.band {
                Some(step) => format!(" · band {step}"),
                None => String::new(),
            },
            pattern.books,
            corpus.len(),
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
        ));
        for finding in rows.iter().take(PER_PATTERN) {
            out.push_str(&site(corpus, finding));
        }
        if rows.len() > PER_PATTERN {
            out.push_str(&format!(
                "<div class=\"more\">\u{2026} and {} more</div>",
                rows.len() - PER_PATTERN
            ));
        }
        out.push_str("</details>");
    }
    out.push_str(TAIL);
    out
}

/// One site row: book, reference, reasons, and its context.
fn site(corpus: &Corpus<'_, OnionBook>, finding: &PackedFinding) -> String {
    let FindingKind::Convention(digest) = finding.kind() else {
        return String::new();
    };
    let book = corpus
        .get(finding.book_idx())
        .expect("a finding names a corpus book");
    let span = TextRange::new(finding.from(), finding.to()).expect("packed spans are ordered");
    let address = match book.locate(span) {
        Some(located) => format!(
            "{}:{}-{}:{}",
            located.first.chapter, located.first.first, located.last.chapter, located.last.last
        ),
        None => "?".to_string(),
    };
    let (before, hit, after) = context(book.text(), span);
    format!(
        "<div class=\"s\" tabindex=\"-1\"><div class=\"a\">{} {address} \
         <span class=\"o\">{}..{}</span> <span class=\"r\">{}</span></div>\
         <div class=\"t\">{}<mark>{}</mark>{}</div></div>",
        book.key(),
        finding.from(),
        finding.to(),
        escape(&reasons(digest.reasons())),
        escape(&before),
        escape(&hit),
        escape(&after),
    )
}

/// `CONTEXT` scalars either side of the span, newlines shown as pilcrows so a
/// site never spills over several lines of the page.
fn context(text: &str, span: TextRange) -> (String, String, String) {
    let head: String = text[..span.from() as usize]
        .chars()
        .rev()
        .take(CONTEXT)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let body = &text[span.from() as usize..span.to() as usize];
    let tail: String = text[span.to() as usize..].chars().take(CONTEXT).collect();
    (flatten(&head), flatten(body), flatten(&tail))
}

fn flatten(text: &str) -> String {
    text.chars()
        .map(|scalar| match scalar {
            '\n' => '\u{b6}',
            '\t' | '\r' => '\u{b7}',
            other => other,
        })
        .collect()
}

fn reasons(bits: Reasons) -> String {
    let named: Vec<&str> = Reasons::NAMES
        .iter()
        .enumerate()
        .filter(|(at, _)| bits.bits() & (1 << at) != 0)
        .map(|(_, name)| *name)
        .collect();
    named.join(" + ")
}

fn evidence(pattern: &Pattern) -> String {
    match pattern.key {
        PatternKey::Rarity => "rarity".to_string(),
        PatternKey::Placement { side, class } => {
            format!("{}={}", side.name(), class.name())
        }
        PatternKey::RunShape { pure, bucket } => format!(
            "{} len {bucket}{}",
            if pure { "pure" } else { "mixed" },
            if bucket == 6 { "+" } else { "" }
        ),
        PatternKey::ExactNeighbor(neighbor) => format!("followed by {}", glyph(neighbor)),
        PatternKey::PooledNeighbor(pool) => format!("followed by a {}", pool.name().to_lowercase()),
    }
}

/// `U+002C ','`, or the pooled digit lane.
pub fn glyph(key: ScalarKey) -> String {
    match key.scalar() {
        Some(scalar) => format!("U+{:04X} {scalar:?}", scalar as u32),
        None => "digits".to_string(),
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for scalar in text.chars() {
        match scalar {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

const HEAD: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Sous Chef sites</title><style>
:root { color-scheme: light dark; --line: #8883; --hit: #ffd54f; }
body { margin: 0 auto; padding: 1.5rem; max-width: 62rem; font: 14px/1.5 system-ui, sans-serif; }
h1 { font-size: 1.2rem; margin: 0 0 .25rem; }
.sum, .hint { margin: 0 0 .25rem; opacity: .7; }
.hint { margin-bottom: 1.25rem; }
details.p { border-top: 1px solid var(--line); padding: .35rem 0; }
summary { cursor: pointer; display: flex; gap: .6rem; flex-wrap: wrap; align-items: baseline; }
summary::marker { color: #8887; }
.i { opacity: .5; font-variant-numeric: tabular-nums; }
.g { font-weight: 600; }
.c { opacity: .8; }
.e { opacity: .8; font-style: italic; }
.f { opacity: .6; font-variant-numeric: tabular-nums; }
.d { opacity: .6; font-variant-numeric: tabular-nums; }
.n { margin-left: auto; opacity: .6; }
.s { margin: .35rem 0 .35rem 1.5rem; padding: .2rem .4rem; border-left: 2px solid transparent; }
.s:focus, .s.on { outline: none; border-left-color: currentColor; background: #8881; }
.a { font-size: .82rem; opacity: .65; display: flex; gap: .5rem; }
.o { font-variant-numeric: tabular-nums; }
.r { margin-left: auto; }
.t { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; white-space: pre-wrap;
     overflow-wrap: anywhere; }
mark { background: var(--hit); color: #000; border-radius: 2px; }
.more { margin-left: 1.5rem; opacity: .6; font-style: italic; }
</style></head><body>
"#;

const TAIL: &str = r#"
<script>
const sites = () => [...document.querySelectorAll('details.p[open] .s')];
let at = -1;
function go(step) {
  const all = sites();
  if (!all.length) return;
  at = Math.min(Math.max(at + step, 0), all.length - 1);
  all.forEach(s => s.classList.remove('on'));
  const here = all[at];
  here.classList.add('on');
  here.scrollIntoView({ block: 'center' });
}
addEventListener('keydown', event => {
  if (event.metaKey || event.ctrlKey || event.altKey) return;
  if (event.key === 'j' || event.key === 'ArrowDown') { go(1); event.preventDefault(); }
  else if (event.key === 'k' || event.key === 'ArrowUp') { go(-1); event.preventDefault(); }
  else if (event.key === 'o') {
    const open = document.querySelectorAll('details.p[open]').length;
    document.querySelectorAll('details.p').forEach(d => { d.open = open === 0; });
    at = -1;
    event.preventDefault();
  }
});
</script>
</body></html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_shows_the_span_and_flattens_its_newlines() {
        let text = "one two\nthree, four five";
        let span = TextRange::new(13, 14).unwrap();
        let (before, hit, after) = context(text, span);
        assert_eq!(before, "one two\u{b6}three");
        assert_eq!(hit, ",");
        assert_eq!(after, " four five");
    }

    #[test]
    fn a_long_context_is_clipped_to_forty_scalars_a_side() {
        let text = format!("{}X{}", "a".repeat(100), "b".repeat(100));
        let span = TextRange::new(100, 101).unwrap();
        let (before, hit, after) = context(&text, span);
        assert_eq!(before.chars().count(), CONTEXT);
        assert_eq!(after.chars().count(), CONTEXT);
        assert_eq!(hit, "X");
    }

    #[test]
    fn markup_in_the_text_cannot_escape_the_page() {
        assert_eq!(escape("<b>&\"</b>"), "&lt;b&gt;&amp;&quot;&lt;/b&gt;");
    }

    #[test]
    fn every_reason_bit_is_named() {
        let both = Reasons::PLACEMENT_BEFORE.union(Reasons::RARITY);
        assert_eq!(reasons(both), "PlacementBefore + Rarity");
    }
}
