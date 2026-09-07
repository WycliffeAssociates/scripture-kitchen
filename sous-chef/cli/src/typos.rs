//! The `--typos` lane: one word's spellings against the corpus's own.
//!
//! ```text
//! typo  "recieve" 3 / "receive" 297   MRK GEN
//! ```

use crate::*;

/// Rare words one edit from a frequent word, grouped by target: an on-demand
/// review action (`sous --typos <corpus>`), not a channel and not a wire row.
/// `sous_core::typos` owns the pure algorithm; this owns the corpus walk that
/// feeds it, the rayon parallel sweep over rare words, and the printed
/// report — the parallel loop lives here because `sous-core` stays
/// dependency-light and carries `rayon` as a dev-dependency only.
pub(crate) fn run_typos(corpus: &Corpus<'_, OnionBook>) -> Result<(), Box<dyn std::error::Error>> {
    let words = typo_word_counts(corpus);
    let config = sous_core::typos::TypoConfig::default();
    let alphabet = sous_core::typos::alphabet_of(&words);
    let index = sous_core::typos::WordIndex::build(&words);
    let rare: Vec<&sous_core::typos::WordEntry> = words
        .words
        .iter()
        .filter(|entry| sous_core::typos::is_rare_candidate(entry, &config))
        .collect();
    let matches: Vec<Vec<usize>> = rare
        .par_iter()
        .map(|word| {
            sous_core::typos::rare_word_candidates(word, &words, &index, &alphabet, &config)
        })
        .collect();
    let groups = sous_core::typos::group_candidates(&words, &rare, &matches);
    print_typo_report(&groups);
    Ok(())
}

/// Every case-folded word the corpus holds, counted, with each word's Title
/// occurrences flagged and up to three references to its earliest
/// occurrences kept while walking — the word's TEXT is recovered here, from
/// the CLI's own copy of the projected text, because the pure algorithm
/// never sees a hash it could not turn back into scalars.
pub(crate) fn typo_word_counts(corpus: &Corpus<'_, OnionBook>) -> sous_core::typos::WordCounts {
    const MAX_REFS: usize = 3;
    let mut index: FxHashMap<String, usize> = FxHashMap::default();
    let mut entries: Vec<sous_core::typos::WordEntry> = Vec::new();
    for (_, book) in corpus.iter() {
        let text = book.text();
        sous_core::words::for_each_word(text, &[], |occurrence| {
            let folded: String = text[occurrence.from as usize..occurrence.to as usize]
                .chars()
                .map(|scalar| scalar.to_lowercase().next().unwrap_or(scalar))
                .collect();
            let slot = *index.entry(folded.clone()).or_insert_with(|| {
                entries.push(sous_core::typos::WordEntry {
                    text: folded,
                    count: 0,
                    title_seen: false,
                    refs: Vec::new(),
                });
                entries.len() - 1
            });
            let entry = &mut entries[slot];
            entry.count += 1;
            entry.title_seen |= occurrence.form == sous_core::words::Form::Title;
            if entry.refs.len() < MAX_REFS
                && let Ok(span) = TextRange::new(occurrence.from, occurrence.to)
                && let Some(located) = book.locate(span)
            {
                entry.refs.push(located.first.to_string());
            }
        });
    }
    sous_core::typos::WordCounts { words: entries }
}

/// ```text
/// katika (8367)   ← 12 possible slips
///     katiki (1)   GEN 3:1
///     kutika (1)   EXO 12:5
/// ```
pub(crate) fn print_typo_report(groups: &[sous_core::typos::TypoGroup]) {
    print!("{}", format_typo_report(groups));
}

/// Built as a string so a test can check it without capturing stdout.
pub(crate) fn format_typo_report(groups: &[sous_core::typos::TypoGroup]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "Possible typos: rare words one edit from a common word. Review, do not \
         trust \u{2014} real on agglutinative languages, mostly noise on short-word \
         languages like English."
    )
    .expect("String writes never fail");
    if groups.is_empty() {
        writeln!(out, "(no candidates)").expect("String writes never fail");
        return out;
    }
    for group in groups {
        writeln!(
            out,
            "{} ({})   \u{2190} {} possible slip{}",
            group.target.0,
            group.target.1,
            group.candidates.len(),
            if group.candidates.len() == 1 { "" } else { "s" }
        )
        .expect("String writes never fail");
        for (text, count, refs) in &group.candidates {
            writeln!(out, "    {text} ({count})   {}", refs.join(", "))
                .expect("String writes never fail");
        }
    }
    out
}
