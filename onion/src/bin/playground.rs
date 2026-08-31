// AGENT: USE THIS FILE FOR EYEBALL DUMPS OF THE ENGINE'S PASSES
//
// Timings live in `onion/benches/` — `cargo bench -p usfm_onion`. Every mode
// here is untimed on purpose: the dumps under debug/ change only when the
// bytes change.
//
// Usage:
//   cargo run --release --bin playground -- --toc-trace <file>  // a readable Toc listing on stdout
//   cargo run --release --bin playground -- --mask-trace <file>            // the masked text of a book
//   cargo run --release --bin playground -- --mask-trace <file> --mask-chapter 3  // …one chapter of it
//   cargo run --release --bin playground -- --mask-trace <file> --mask-recipe structure  // …one recipe only
//   cargo run --release --bin playground -- --vref <file>                   // one line per verse ("GEN 1:1\ttext")
//   cargo run --release --bin playground -- --vref <file> --vref-chapter 1  // …one chapter of it
//   cargo run --release --bin playground -- --format-trace <file> --format-variant join-chars  // one book formatted
//   cargo run --release --bin playground -- --diff-trace <a> <b>          // the decision-unit listing of two books
//   cargo run --release --bin playground -- --cst-stats        // CloseReason distribution over the corpus
//   cargo run --release --bin playground -- --lint-stats       // per-code finding counts (and fix counts)
//   cargo run --release --bin playground -- --codes            // the diagnostics side-table, browsable
//   cargo run --release --bin playground -- --fix-preview unclosed-note  // …plus before/after windows for one code
//
// A bare path (file, or dir of *.usfm) picks the corpus; the default is
// en_ulb. The trace modes name their own file instead.

use std::fs;
use std::path::{Path, PathBuf};

use usfm_onion::mask::{Filter, Mask};

const DEFAULT_CORPUS: &str = "../testData/exampleCorpora/en_ulb";

fn main() {
    let mut path: Option<PathBuf> = None;
    let mut cst_stats = false;
    let mut lint_stats = false;
    let mut codes = false;
    let mut fix_preview: Option<String> = None;
    let mut toc_trace = false;
    let mut mask_trace: Option<PathBuf> = None;
    let mut mask_chapter: Option<u16> = None;
    let mut mask_recipe: Option<String> = None;
    let mut vref_trace: Option<PathBuf> = None;
    let mut vref_chapter: Option<u16> = None;
    let mut format_trace: Option<PathBuf> = None;
    let mut format_chapter: Option<u16> = None;
    let mut format_variant: Option<String> = None;
    let mut diff_trace: Option<(PathBuf, PathBuf)> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--toc-trace" => toc_trace = true,
            "--mask-trace" => mask_trace = args.next().map(PathBuf::from),
            "--mask-chapter" => {
                mask_chapter = Some(
                    args.next()
                        .and_then(|n| n.parse().ok())
                        .expect("--mask-chapter takes a chapter number"),
                );
            }
            "--mask-recipe" => mask_recipe = args.next(),
            "--vref" => vref_trace = args.next().map(PathBuf::from),
            "--vref-chapter" => {
                vref_chapter = Some(
                    args.next()
                        .and_then(|n| n.parse().ok())
                        .expect("--vref-chapter takes a chapter number"),
                );
            }
            "--format-trace" => format_trace = args.next().map(PathBuf::from),
            "--format-chapter" => {
                format_chapter = Some(
                    args.next()
                        .and_then(|n| n.parse().ok())
                        .expect("--format-chapter takes a chapter number"),
                );
            }
            "--format-variant" => format_variant = args.next(),
            "--diff-trace" => {
                let baseline = args.next().expect("--diff-trace takes two paths");
                let current = args.next().expect("--diff-trace takes two paths");
                diff_trace = Some((PathBuf::from(baseline), PathBuf::from(current)));
            }
            "--cst-stats" => cst_stats = true,
            "--lint-stats" => lint_stats = true,
            // The side-table, made browsable — the `{anchor}` conventions are
            // only discoverable by rendering one.
            "--codes" => codes = true,
            // Implies --lint-stats: the same sweep, plus before/after windows
            // for the first few fixes of ONE code.
            "--fix-preview" => {
                lint_stats = true;
                fix_preview = args.next();
            }
            other => path = Some(PathBuf::from(other)),
        }
    }
    // Before the corpus load: --vref and --mask-trace name their own one file.
    if let Some(path) = &vref_trace {
        print!("{}", vref_listing(&read_source(path), vref_chapter));
        return;
    }
    // Same reason: --format-trace names its own one file.
    if let Some(path) = &format_trace {
        print!(
            "{}",
            format_listing(
                &read_source(path),
                format_chapter,
                format_variant.as_deref().unwrap_or("default"),
            )
        );
        return;
    }
    // Same reason: --diff-trace names its own two files.
    if let Some((baseline, current)) = &diff_trace {
        print!(
            "{}",
            diff_listing(&read_source(baseline), &read_source(current))
        );
        return;
    }
    // Same reason: --mask-trace names its own one file.
    if let Some(path) = &mask_trace {
        print!(
            "{}",
            mask_listing(&read_source(path), mask_chapter, mask_recipe.as_deref())
        );
        return;
    }

    let path = path.unwrap_or_else(|| PathBuf::from(DEFAULT_CORPUS));

    let sources: Vec<String> = if path.is_dir() {
        let mut paths = Vec::new();
        collect_usfm_paths(&path, &mut paths);
        paths.sort();
        paths.iter().map(|p| read_source(p)).collect()
    } else {
        vec![read_source(&path)]
    };

    let bytes: usize = sources.iter().map(|s| s.len()).sum();
    eprintln!(
        "playground: loaded {} source(s), {bytes} bytes total",
        sources.len()
    );

    if cst_stats {
        report_cst_stats(&sources);
        return;
    }
    if codes {
        report_codes(&sources);
        return;
    }
    if lint_stats {
        report_lint_stats(&sources, &path, fix_preview.as_deref());
        return;
    }
    if toc_trace {
        for source in &sources {
            print!("{}", toc_listing(source));
        }
        return;
    }

    eprintln!("playground: no mode named — see the usage block at the top of this file");
}

fn collect_usfm_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read dir entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            collect_usfm_paths(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "usfm") {
            paths.push(path);
        }
    }
}

/// Two documents diffed: the timing, the unit census, and every changed unit.
/// One decision unit per line, the changed ones only — the census header, then
/// each unit's status, kind, addresses and flags.
fn diff_listing(baseline: &str, current: &str) -> String {
    use usfm_onion::diff::{Decisions, MergeSide, Status, diff, to_edits};

    let skeleton = diff(baseline, current);
    let edits = to_edits(&skeleton, &Decisions::new(), MergeSide::Current).expect("no decisions");
    let census = |status: Status| {
        skeleton
            .units
            .iter()
            .filter(|unit| unit.status == status)
            .count()
    };

    let mut out = format!(
        "=== diff {} bytes vs {} bytes\n\
         === {} slots, {} units: {} unchanged, {} modified, {} added, {} deleted, {} moved\n\
         === {} replay splices\n\n",
        baseline.len(),
        current.len(),
        skeleton.slots.len(),
        skeleton.units.len(),
        census(Status::Unchanged),
        census(Status::Modified),
        census(Status::Added),
        census(Status::Deleted),
        census(Status::Moved),
        edits.len(),
    );

    for unit in &skeleton.units {
        if unit.status == Status::Unchanged {
            continue;
        }
        let address = |addr: Option<usfm_onion::diff::Addr>| {
            addr.map_or_else(|| "-".to_string(), |addr| addr.to_string())
        };
        let mut flags = Vec::new();
        if unit.is_whitespace_change {
            flags.push("ws");
        }
        if unit.is_usfm_structure_change {
            flags.push("usfm");
        }
        if unit.displaced {
            flags.push("displaced");
        }
        if unit.relabeled {
            flags.push("relabeled");
        }
        if unit.dup_context.is_dup() {
            flags.push("dup");
        }
        if unit.covered_by.is_some() {
            flags.push("covered");
        }
        out.push_str(&format!(
            "{:<9?} {:<9?} {:<24} -> {:<24} {}\n",
            unit.status,
            unit.kind,
            address(unit.baseline_addr),
            address(unit.current_addr),
            flags.join(","),
        ));
    }
    out
}

fn read_source(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// The Toc as a human reads it: the chapter table, then the first few verse
/// anchors of each chapter with the sid `locate` gives at that byte.
fn toc_listing(source: &str) -> String {
    const ANCHORS_PER_CHAPTER: usize = 6;

    let tokens = usfm_onion::lex(source);
    let toc = usfm_onion::toc(source.as_bytes(), &tokens);
    let mut out = String::new();
    out.push_str(&format!(
        "{} — {} chapter rows, {} verse anchors, {} bytes\n\n",
        toc.locate(0),
        toc.chapters.len(),
        toc.verses.len(),
        source.len(),
    ));
    out.push_str("  ch            bytes  verses  first anchors\n");
    for row in &toc.chapters {
        let anchors: Vec<&usfm_onion::VerseAnchor> = toc
            .verses
            .iter()
            .filter(|v| v.at >= row.start && v.at < row.end)
            .collect();
        let mut listed: Vec<String> = anchors
            .iter()
            .take(ANCHORS_PER_CHAPTER)
            .map(|v| format!("{}@{}", toc.locate(v.at), v.at))
            .collect();
        if anchors.len() > ANCHORS_PER_CHAPTER {
            listed.push(format!("… {} more", anchors.len() - ANCHORS_PER_CHAPTER));
        }
        out.push_str(&format!(
            "{:>4}  {:>7}..{:<7}  {:>5}  {}\n",
            row.number,
            row.start,
            row.end,
            anchors.len(),
            listed.join("  "),
        ));
    }
    out
}

/// One book's masked text, both recipes, optionally narrowed to one chapter
/// through the Toc's own `chapter_span` — the dumps a human reads to decide
/// whether a recipe is right.
/// One book (or one chapter of it, sliced by the Toc) formatted under a named
/// option variant, with the options and a wall-clock cost in the header. The
/// regenerator for debug/formatting/*.txt.
fn format_listing(source: &str, chapter: Option<u16>, variant: &str) -> String {
    use usfm_onion::{CharBreaks, FormatOptions, VerseBreaks, format, format_edits};

    let opts = match variant {
        "default" => FormatOptions::default(),
        "keep-verse-breaks" => FormatOptions {
            verse_breaks: VerseBreaks::Keep,
            ..FormatOptions::default()
        },
        "remove-s5" => FormatOptions {
            remove_markers: &["s5"],
            ..FormatOptions::default()
        },
        "join-chars" => FormatOptions {
            char_marker_breaks: CharBreaks::Join,
            ..FormatOptions::default()
        },
        other => panic!("unknown --format-variant {other}"),
    };

    // A chapter window slices the SOURCE first — the fragment formats on its
    // own, so the dump stays small enough to read by eye.
    let bytes = source.as_bytes();
    let window = match chapter {
        Some(n) => {
            let tokens = usfm_onion::lex(source);
            let toc = usfm_onion::toc(bytes, &tokens);
            toc.chapter_span(n)
                .unwrap_or_else(|| panic!("no chapter {n} in this book"))
        }
        None => 0..bytes.len() as u32,
    };
    let input = &bytes[window.start as usize..window.end as usize];

    let edits = format_edits(input, &opts);
    let mut out = format!(
        "=== format {} — variant {variant}\n=== {opts:?}\n=== {} edits over {} bytes\n\n",
        chapter.map_or_else(|| "whole book".to_string(), |n| format!("chapter {n}")),
        edits.len(),
        input.len(),
    );
    out.push_str(&String::from_utf8(format(input, &opts)).expect("formatted output is UTF-8"));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn mask_listing(source: &str, chapter: Option<u16>, only: Option<&str>) -> String {
    let bytes = source.as_bytes();
    let tokens = usfm_onion::lex(source);
    let cst = usfm_onion::cst::build(&tokens);
    let toc = usfm_onion::toc(bytes, &tokens);
    let window = match chapter {
        Some(n) => toc
            .chapter_span(n)
            .unwrap_or_else(|| panic!("no chapter {n} in this book")),
        None => 0..bytes.len() as u32,
    };

    let mut out = String::new();
    for (recipe, filter) in [
        ("verse_text", Filter::verse_text()),
        ("structure", Filter::structure()),
    ] {
        if only.is_some_and(|name| name != recipe) {
            continue;
        }
        let m = usfm_onion::mask(bytes, &tokens, &cst, &filter);
        let text = window_text(bytes, &m, &window);
        out.push_str(&format!(
            "=== {} {} — {recipe}: {} of {} window bytes kept, {} ranges whole-book\n\n",
            toc.locate(window.start),
            chapter.map_or_else(|| "whole book".to_string(), |n| format!("chapter {n}")),
            text.len(),
            window.end - window.start,
            m.ranges.len(),
        ));
        out.push_str(&text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// One book as vref lines — `sid`, a tab, the verse's text — optionally
/// narrowed to one chapter. The regenerator for debug/vref.*.txt.
fn vref_listing(source: &str, chapter: Option<u16>) -> String {
    let bytes = source.as_bytes();
    let tokens = usfm_onion::lex(source);
    let cst = usfm_onion::cst::build(&tokens);
    let toc = usfm_onion::toc(bytes, &tokens);
    let m = usfm_onion::mask(bytes, &tokens, &cst, &Filter::verse_text());

    let mut out = String::new();
    for (sid, text) in usfm_onion::verses(&toc, &m, bytes) {
        if chapter.is_some_and(|n| n != sid.chapter) {
            continue;
        }
        out.push_str(&format!("{sid}\t{}\n", text.trim()));
    }
    out
}

/// The masked bytes that fall inside one source window.
fn window_text(source: &[u8], m: &Mask, window: &std::ops::Range<u32>) -> String {
    let mut out = String::new();
    for range in &m.ranges {
        let start = range.start.max(window.start) as usize;
        let end = range.end.min(window.end) as usize;
        if start < end {
            out.push_str(std::str::from_utf8(&source[start..end]).expect("UTF-8 source"));
        }
    }
    out
}

/// Untimed corpus sweep: what does lint actually FIND in the wild? Per-code
/// counts, then a sample of each code's sites (book, code, anchor offset) —
/// enough to eyeball a class before pinning its count in tests/lint_corpus.rs.
/// Every lint code as a UI would meet it: identity, ladder, template, fix
/// label, and ONE rendered message off real bytes.
///
/// The rendering is the point. `{anchor}` is a token span the consumer slices
/// out of its own document, and what that slice actually contains — the
/// marker's backslash, the delimiter the scanner folded on — is not something a
/// template reads like.
fn report_codes(sources: &[String]) {
    use usfm_onion::lint::{LINT_ROWS, NO_TOKEN, Severity, UsfmVersion};

    // The first real occurrence of each code, rendered the way `reader.ts`'s
    // `message()` renders it: template plus two document slices. Straight off
    // the lint walk — the wire adds nothing a dump wants, and byte spans read
    // the same as the UTF-16 ones a browser would get.
    let mut example: Vec<Option<String>> = vec![None; LINT_ROWS.len()];
    for source in sources {
        if example.iter().all(Option::is_some) {
            break;
        }
        let bytes = source.as_bytes();
        let tokens = usfm_onion::lex(source);
        let cst = usfm_onion::cst::build(&tokens);
        let report = usfm_onion::lint::lint(bytes, &tokens, &cst);
        let slice = |token: u32| match tokens.get(token as usize) {
            Some(t) => String::from_utf8_lossy(
                &bytes[t.start as usize..(t.start + t.trimmed_len(bytes)) as usize],
            )
            .into_owned(),
            None => String::new(),
        };
        for obs in &report.observations {
            let slot = obs.code as usize;
            if example[slot].is_some() {
                continue;
            }
            let second = if obs.second == NO_TOKEN {
                String::new()
            } else {
                slice(obs.second)
            };
            example[slot] = Some(
                LINT_ROWS[slot]
                    .template
                    .replace("{anchor}", &slice(obs.anchor))
                    .replace("{second}", &second)
                    .replace("{aux}", &obs.aux.to_string()),
            );
        }
    }

    let name = |severity: Option<Severity>| match severity {
        None => "gated",
        Some(Severity::Error) => "error",
        Some(Severity::Warning) => "warning",
        Some(Severity::Info) => "info",
        Some(Severity::Hint) => "hint",
        Some(Severity::Form) => "form",
    };
    let version = |v: UsfmVersion| match v {
        UsfmVersion::V3_0 => "3.0",
        UsfmVersion::V3_2 => "3.2",
        UsfmVersion::V4_0 => "4.0",
    };

    println!(
        "codes n={} schema={} (examples from {} loaded source(s))",
        LINT_ROWS.len(),
        usfm_onion::lint::catalog::SCHEMA,
        sources.len()
    );
    for (code, row) in LINT_ROWS.iter().enumerate() {
        let mut ladder = String::from(name(row.severity));
        for (rung, at) in row.escalation {
            ladder.push_str(&format!(" → {}@{}", name(Some(*at)), version(*rung)));
        }
        println!();
        println!("[{code:>2}] {}  ({:?})", row.name, row.category);
        println!("     severity  {ladder}");
        println!("     aux       {:?}", row.aux);
        println!("     template  {:?}", row.template);
        match row.fix_label {
            Some(label) => println!(
                "     fix       {label:?}{}",
                if row.formatter {
                    "  (also a formatting action)"
                } else {
                    ""
                }
            ),
            None => println!("     fix       —"),
        }
        match &example[code] {
            Some(message) => println!("     example   {message:?}"),
            None if row.is_form() => {
                println!(
                    "     example   — (a Form row: the formatter's channel, never a diagnostic)"
                )
            }
            None => println!("     example   — (not triggered by the loaded corpus)"),
        }
    }
}

fn report_lint_stats(sources: &[String], root: &Path, preview_of: Option<&str>) {
    use usfm_onion::lint::LINT_ROWS;

    const SAMPLES_PER_CODE: usize = 12;
    const PREVIEWS: usize = 6;

    let names: Vec<String> = if root.is_dir() {
        let mut paths = Vec::new();
        collect_usfm_paths(root, &mut paths);
        paths.sort();
        paths
            .iter()
            .map(|p| {
                p.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    } else {
        vec![
            root.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ]
    };

    let mut counts = [0u64; LINT_ROWS.len()];
    let mut fixed = [0u64; LINT_ROWS.len()];
    let mut samples: Vec<Vec<(String, u32)>> = vec![Vec::new(); LINT_ROWS.len()];
    let mut previews: Vec<(String, &'static str, String, String)> = Vec::new();
    let mut books_without_id = 0u64;
    let mut tokens_total = 0u64;

    for (i, source) in sources.iter().enumerate() {
        let tokens = usfm_onion::lex(source);
        let cst = usfm_onion::cst::build(&tokens);
        let report = usfm_onion::lint::lint(source.as_bytes(), &tokens, &cst);
        tokens_total += tokens.len() as u64;

        let book = match report.book {
            Some(idx) => {
                let token = tokens[idx as usize];
                source[token.start as usize..token.end() as usize].to_string()
            }
            None => {
                books_without_id += 1;
                names.get(i).cloned().unwrap_or_else(|| format!("doc#{i}"))
            }
        };
        for (index, obs) in report.observations.iter().enumerate() {
            let slot = obs.code as usize;
            counts[slot] += 1;
            if samples[slot].len() < SAMPLES_PER_CODE {
                samples[slot].push((book.clone(), tokens[obs.anchor as usize].start));
            }
            let Some(fix) = report.fix(index) else {
                continue;
            };
            fixed[slot] += 1;
            // Repaired text beside the original: the only way to see whether a
            // proposal reads sanely in real scripture, not in a test snippet.
            if previews.len() < PREVIEWS
                && preview_of.is_some_and(|name| name == LINT_ROWS[slot].name)
            {
                let edits = report.edits(fix);
                let at = edits[0].from as usize;
                let window = at.saturating_sub(60)..(at + 60).min(source.len());
                let after = String::from_utf8(usfm_onion::lint::apply(source.as_bytes(), edits))
                    .expect("fixes are ASCII");
                let shift = window.start..(window.end + 8).min(after.len());
                previews.push((
                    book.clone(),
                    fix.label,
                    source[window].to_string(),
                    after[shift].to_string(),
                ));
            }
        }
    }

    let total: u64 = counts.iter().sum();
    let fixable: u64 = fixed.iter().sum();
    println!(
        "lint-stats docs={} tokens={tokens_total} findings={total} with-fix={fixable} books-without-id={books_without_id}",
        sources.len()
    );
    for (book, label, before, after) in &previews {
        println!("  fix-preview {book} [{label}]");
        println!("    before {before:?}");
        println!("    after  {after:?}");
    }
    for (slot, row) in LINT_ROWS.iter().enumerate() {
        if counts[slot] == 0 {
            continue;
        }
        println!(
            "  {} x{} ({} with a fix)",
            row.name, counts[slot], fixed[slot]
        );
        for (book, offset) in &samples[slot] {
            println!("    {book} {} @{offset}", row.name);
        }
        if counts[slot] > SAMPLES_PER_CODE as u64 {
            println!("    … {} more", counts[slot] - SAMPLES_PER_CODE as u64);
        }
    }
    for row in LINT_ROWS.iter() {
        if counts[row.code as usize] == 0 {
            println!("  {} x0", row.name);
        }
    }
}

/// Untimed corpus sweep: how do frames actually CLOSE in the wild? Clean books
/// are nearly all Explicit/Implicit; a Recovery cluster is either real data
/// damage or a walker/table gap — eyeball them.
fn report_cst_stats(sources: &[String]) {
    use usfm_onion::cst::CloseReason;
    let mut totals = [0u64; 4];
    let mut nodes_total = 0u64;
    let mut tokens_total = 0u64;
    let mut worst: Vec<(u64, usize)> = Vec::new();
    let mut by_marker: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for (i, source) in sources.iter().enumerate() {
        let tokens = usfm_onion::lex(source);
        let cst = usfm_onion::cst::build(&tokens);
        tokens_total += tokens.len() as u64;
        nodes_total += cst.nodes.len() as u64;
        let mut recoveries = 0u64;
        for node in &cst.nodes[1..] {
            let reason = node.close_reason();
            totals[reason as usize] += 1;
            if reason == CloseReason::Recovery {
                recoveries += 1;
            }
        }
        if recoveries > 0 {
            worst.push((recoveries, i));
        }
        for node in &cst.nodes[1..] {
            if node.close_reason() == CloseReason::Recovery {
                let idx = tokens[node.token as usize].marker_idx;
                let name = usfm_onion::tables::generated::name(idx);
                *by_marker.entry(name).or_insert(0u64) += 1;
            }
        }
    }
    println!(
        "cst-stats docs={} tokens={tokens_total} nodes={nodes_total} explicit={} implicit={} recovery={} eof={}",
        sources.len(),
        totals[CloseReason::Explicit as usize],
        totals[CloseReason::Implicit as usize],
        totals[CloseReason::Recovery as usize],
        totals[CloseReason::Eof as usize],
    );
    let mut by_marker: Vec<_> = by_marker.into_iter().collect();
    by_marker.sort_unstable_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (name, count) in by_marker.iter().take(10) {
        println!("  recovery marker \\{name} x{count}");
    }
    worst.sort_unstable_by_key(|(count, _)| std::cmp::Reverse(*count));
    for (count, doc) in worst.iter().take(10) {
        println!("  recovery x{count} in doc #{doc}");
    }
}
