//! Every Form row on its own snippet, the axes against each other, and the
//! invariants that hold whatever the options say.
//!
//! Every assertion here goes through [`formatted`], which also runs THE
//! IDEMPOTENCE ORACLE — apply the whole transaction, re-walk, and demand zero
//! findings among the Form rows, the formatter-bit rows and the opted repairs.
//! So each test is also a convergence test for the shape it names.

use super::*;
use crate::lint::{LINT_ROWS, Severity};

fn keep_verses() -> FormatOptions<'static> {
    FormatOptions {
        verse_breaks: VerseBreaks::Keep,
        ..FormatOptions::default()
    }
}

/// Format one snippet and prove the result SETTLED: a second pass finds nothing
/// left to do, which is stronger than a fixpoint in two.
fn formatted(usfm: &str, opts: &FormatOptions) -> String {
    let out = format(usfm.as_bytes(), opts);
    let text = String::from_utf8(out).expect("format keeps UTF-8");
    let left = format_edits(text.as_bytes(), opts);
    assert!(
        left.is_empty(),
        "{usfm:?} formatted to {text:?}, which is still not settled: {left:?}"
    );
    text
}

/// The rows that claimed bytes, in document order.
fn rows(usfm: &str, opts: &FormatOptions) -> Vec<Code> {
    let mut codes: Vec<Code> = format_claims(usfm.as_bytes(), opts)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    codes.dedup();
    codes
}

// ---------------------------------------------------------------------------
// One row at a time
// ---------------------------------------------------------------------------

#[test]
fn every_form_row_fires_on_its_own_snippet() {
    let s5 = FormatOptions {
        remove_markers: &["s5"],
        ..FormatOptions::default()
    };
    let bridge = FormatOptions {
        bridge_empty_verses: true,
        ..FormatOptions::default()
    };
    let dedupe = FormatOptions {
        dedupe_verse_number: true,
        ..FormatOptions::default()
    };
    let join = FormatOptions {
        char_marker_breaks: CharBreaks::Join,
        ..FormatOptions::default()
    };
    let crlf = FormatOptions {
        newline: Newline::CrLf,
        ..FormatOptions::default()
    };

    // (row, options, before, after) — one MINIMAL snippet per Form row, whose
    // claim set is exactly that row.
    let cases: [(Code, &FormatOptions, &str, &str); 11] = [
        (
            Code::RemoveMarker,
            &s5,
            "\\p a\n\\s5\n\\p b\n",
            "\\p a\n\\p b\n",
        ),
        (
            Code::BridgeEmptyVerses,
            &bridge,
            "\\p \\v 1\\v 2\\v 3 asdf",
            "\\p \\v 1-3 asdf",
        ),
        (
            Code::DedupeVerseNumber,
            &dedupe,
            "\\p \\v 2 2 men went",
            "\\p \\v 2 men went",
        ),
        (
            Code::BlockMarkerOwnLine,
            &FormatOptions::default(),
            "\\c 1 \\c 2",
            "\\c 1\n\\c 2",
        ),
        (
            Code::CharMarkerLineJoin,
            &join,
            "\\p \\w In\\w*\n\\w the\\w* rest",
            "\\p \\w In\\w* \\w the\\w* rest",
        ),
        (
            Code::CollapseBlankLines,
            &FormatOptions::default(),
            "\\c 1\n\n\n\\c 2",
            "\\c 1\n\\c 2",
        ),
        (
            Code::NormalizeNewlines,
            &crlf,
            "\\c 1\n\\c 2",
            "\\c 1\r\n\\c 2",
        ),
        (
            Code::TrimTextEdges,
            &FormatOptions::default(),
            "\\p \\add x\\add*   more",
            "\\p \\add x\\add* more",
        ),
        (
            Code::DelimiterSingle,
            &FormatOptions::default(),
            "\\p   Text",
            "\\p Text",
        ),
        (
            Code::DesignatorWsSingle,
            &FormatOptions::default(),
            "\\p \\v 12   Text",
            "\\p \\v 12 Text",
        ),
        (
            Code::MarkerWsAtLineStart,
            &FormatOptions::default(),
            "\\p a\n   \\p b",
            "\\p a\n\\p b",
        ),
    ];

    let mut demonstrated: Vec<Code> = Vec::new();
    for (code, opts, before, after) in cases {
        assert_eq!(rows(before, opts), vec![code], "{before:?}");
        assert_eq!(formatted(before, opts), after, "{before:?}");
        demonstrated.push(code);
    }
    for row in LINT_ROWS.iter().filter(|row| row.is_form()) {
        assert!(
            demonstrated.contains(&row.code),
            "{} has no snippet above",
            row.name
        );
    }
}

/// The Form channel's shape, as data: no Form row is a diagnostic, every one of
/// them repairs something, and none of them is also a dual citizen (the channel
/// IS the membership).
#[test]
fn the_form_channel_is_declared_consistently() {
    for row in LINT_ROWS.iter().filter(|row| row.is_form()) {
        assert_eq!(row.severity, Some(Severity::Form), "{}", row.name);
        assert!(row.escalation.is_empty(), "{}", row.name);
        assert!(row.fix_label.is_some(), "{} repairs nothing", row.name);
        assert!(
            !row.formatter,
            "{} is a Form row AND a dual citizen",
            row.name
        );
        assert!(row.formats(), "{}", row.name);
        // A Form row's "severity" is not on the diagnostic ladder at any
        // declared version.
        for version in [None, Some(crate::lint::UsfmVersion::V4_0)] {
            assert_eq!(row.severity_at(version), Some(Severity::Form));
        }
    }
    // The three DUAL CITIZENS keep their real severity and carry the bit.
    for code in [
        Code::MissingParagraph,
        Code::MarkerNotWsPreceded,
        Code::EmptyParagraph,
    ] {
        let row = code.row();
        assert!(row.formatter && !row.is_form(), "{}", row.name);
        assert!(row.fix_label.is_some(), "{}", row.name);
    }
}

// ---------------------------------------------------------------------------
// The rules of thumb
// ---------------------------------------------------------------------------

/// The load-bearing one: whitespace INSIDE a text run is content.
#[test]
fn interior_verse_text_is_untouched_and_edges_are_not() {
    assert_eq!(
        formatted("\\c 1\n\\p \\v 1 In  the beginning\n", &keep_verses()),
        "\\c 1\n\\p\n\\v 1 In  the beginning\n"
    );
    // NBSP is content, never structural whitespace.
    assert_eq!(
        formatted(
            "\\c 1\n\\p \\v 1 In\u{00A0}\u{00A0}the beginning\n",
            &keep_verses()
        ),
        "\\c 1\n\\p\n\\v 1 In\u{00A0}\u{00A0}the beginning\n"
    );
    // The EDGES are another matter — and `\p   Text` has no leading run at all,
    // the scanner having folded it into the marker's own span.
    assert_eq!(
        formatted("\\p   Text\n", &FormatOptions::default()),
        "\\p Text\n"
    );
    assert_eq!(
        formatted("\\p \\add x\\add*   more   \n", &FormatOptions::default()),
        "\\p \\add x\\add* more\n"
    );
}

/// Poetry is not special-cased: `\q#` is a Paragraph row, so the newline in
/// front of each one IS the block-marker newline. Removing verse breaks does not
/// touch it.
#[test]
fn poetry_falls_out_of_the_block_rule() {
    let stanza = "\\c 1\n\\q1 a \\q2 b \\q1 c\n\\p \\v 1 x\n";
    assert_eq!(
        formatted(stanza, &FormatOptions::default()),
        "\\c 1\n\\q1 a\n\\q2 b\n\\q1 c\n\\p \\v 1 x\n"
    );
    assert_eq!(
        formatted(stanza, &keep_verses()),
        "\\c 1\n\\q1 a\n\\q2 b\n\\q1 c\n\\p\n\\v 1 x\n"
    );
}

/// Never insert a separator at a character/note/milestone boundary: a closing
/// `\X*` glues to what follows it, legitimately.
#[test]
fn a_closing_char_marker_glues_to_the_text_after_it() {
    let glued = "\\c 1\n\\p \\v 1 \\nd Lord\\nd*'s Battles\n";
    assert_eq!(formatted(glued, &FormatOptions::default()), glued);
    assert_eq!(
        formatted(glued, &keep_verses()),
        "\\c 1\n\\p\n\\v 1 \\nd Lord\\nd*'s Battles\n"
    );
    // Aligned USFM is built out of hugging, and none of it moves.
    let aligned = "\\c 1\n\\p \\v 1 \\w In\\w*\\w the\\w*\\add x\\add*\n";
    assert_eq!(formatted(aligned, &FormatOptions::default()), aligned);
}

// ---------------------------------------------------------------------------
// The axes
// ---------------------------------------------------------------------------

#[test]
fn the_two_verse_break_settings_differ_only_in_verse_breaks() {
    let usfm = "\\c 1\n\\p\n\\v 1 a\n\\v 2 b\n\\q1 poetry\n";
    assert_eq!(
        formatted(usfm, &keep_verses()),
        "\\c 1\n\\p\n\\v 1 a\n\\v 2 b\n\\q1 poetry\n"
    );
    assert_eq!(
        formatted(usfm, &FormatOptions::default()),
        "\\c 1\n\\p \\v 1 a \\v 2 b\n\\q1 poetry\n"
    );
    // Same bytes modulo the break form: strip the whitespace and the two
    // outputs are identical.
    let squeeze = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        squeeze(&formatted(usfm, &keep_verses())),
        squeeze(&formatted(usfm, &FormatOptions::default()))
    );
}

#[test]
fn every_ending_in_the_output_is_the_configured_one() {
    let mixed = "\\id GEN\r\n\\c 1\n\\p a\r\n\\p b\n";
    let lf = formatted(mixed, &FormatOptions::default());
    assert_eq!(lf, "\\id GEN\n\\c 1\n\\p a\n\\p b\n");
    assert!(!lf.contains('\r'));

    let crlf = FormatOptions {
        newline: Newline::CrLf,
        ..FormatOptions::default()
    };
    let out = formatted(mixed, &crlf);
    assert_eq!(out, "\\id GEN\r\n\\c 1\r\n\\p a\r\n\\p b\r\n");
    // INSERTED breaks use the same form as normalized ones.
    assert_eq!(
        formatted("\\id GEN\n\\c 1\\v 1 Text\n", &crlf),
        "\\id GEN\r\n\\c 1\r\n\\p \\v 1 Text\r\n"
    );
    assert!(
        out.match_indices('\n')
            .all(|(at, _)| at > 0 && out.as_bytes()[at - 1] == b'\r')
    );
}

#[test]
fn joining_char_marker_lines_reads_as_flowing_text() {
    let join = FormatOptions {
        char_marker_breaks: CharBreaks::Join,
        ..FormatOptions::default()
    };
    // The en_ult shape: one word per line, each wrapped in its own `\w`.
    let aligned = "\\c 1\n\\p \\v 1 \\w In\\w*\n\\w the\\w*\n\\w beginning\\w*\n";
    assert_eq!(
        formatted(aligned, &FormatOptions::default()),
        "\\c 1\n\\p \\v 1 \\w In\\w*\n\\w the\\w*\n\\w beginning\\w*\n"
    );
    let joined = formatted(aligned, &join);
    assert_eq!(
        joined,
        "\\c 1\n\\p \\v 1 \\w In\\w* \\w the\\w* \\w beginning\\w*\n"
    );

    // …and the verse text a mask reads off the RESULT is clean prose.
    let tokens = crate::lex(&joined);
    let cst = crate::cst::build(&tokens);
    let mask = crate::mask(
        joined.as_bytes(),
        &tokens,
        &cst,
        &crate::Filter::verse_text(),
    );
    // The line endings OUTSIDE any verse extent are still Text the mask keeps,
    // which is why this reads the verse's own bytes and not the whole string.
    assert_eq!(mask.text(joined.as_bytes()).trim(), "In the beginning");
}

// ---------------------------------------------------------------------------
// The parameterized rows
// ---------------------------------------------------------------------------

#[test]
fn remove_markers_takes_the_marker_and_the_line_it_stood_on() {
    let ulb = "\\id GEN\n\\c 1\n\\p\n\\v 1 a\n\n\\s5\n\\p\n\\v 2 b\n";
    // Empty list: not one finding, and the `\s5` survives untouched.
    assert!(
        rows(ulb, &FormatOptions::default())
            .iter()
            .all(|code| *code != Code::RemoveMarker)
    );
    assert!(formatted(ulb, &FormatOptions::default()).contains("\\s5"));

    let s5 = FormatOptions {
        remove_markers: &["s5"],
        ..FormatOptions::default()
    };
    // THE en_ulb STRESS BLOCK: a removed chunk marker, an empty paragraph and a
    // blank line, all in one transaction, converging in one pass.
    assert_eq!(
        formatted(ulb, &s5),
        "\\id GEN\n\\c 1\n\\p \\v 1 a\n\\p \\v 2 b\n"
    );
    // A marker that OPENS a scope takes its subtree — the caller asked, and
    // linting the output is the only rail.
    assert_eq!(
        formatted(
            "\\id GEN\n\\c 1\n\\p a\n\\q1 poetry\n\\p b\n",
            &FormatOptions {
                remove_markers: &["q1"],
                ..FormatOptions::default()
            }
        ),
        "\\id GEN\n\\c 1\n\\p a\n\\p b\n"
    );
}

#[test]
fn the_repeated_verse_number_goes_only_on_a_number_boundary() {
    let usfm = "\\id GEN\n\\c 1\n\\p\n\\v 2 2 men went\n\\v 3 2000 men\n";
    // Default OFF: both lines are left exactly as written.
    assert_eq!(
        formatted(usfm, &keep_verses()),
        "\\id GEN\n\\c 1\n\\p\n\\v 2 2 men went\n\\v 3 2000 men\n"
    );
    let dedupe = FormatOptions {
        dedupe_verse_number: true,
        ..keep_verses()
    };
    assert_eq!(
        formatted(usfm, &dedupe),
        "\\id GEN\n\\c 1\n\\p\n\\v 2 men went\n\\v 3 2000 men\n"
    );
}

#[test]
fn a_run_of_empty_verses_bridges_into_the_one_that_has_text() {
    let usfm = "\\id GEN\n\\c 1\n\\p \\v 1\\v 2\\v 3 asdf\n";
    // Default OFF — lint still flags the shape, format leaves it.
    assert_eq!(formatted(usfm, &FormatOptions::default()), usfm);

    let bridge = FormatOptions {
        bridge_empty_verses: true,
        ..FormatOptions::default()
    };
    assert_eq!(
        formatted(usfm, &bridge),
        "\\id GEN\n\\c 1\n\\p \\v 1-3 asdf\n"
    );
    // A verse with content of its own is not empty and ends no run. (The glued
    // `\v` stays glued: `Remove` deletes verse breaks, it never invents one.)
    let full = "\\id GEN\n\\c 1\n\\p \\v 1 a\\v 2 b\n";
    assert_eq!(formatted(full, &bridge), full);
}

// ---------------------------------------------------------------------------
// The dual citizens
// ---------------------------------------------------------------------------

#[test]
fn only_the_unambiguous_empty_paragraph_is_deleted() {
    assert_eq!(
        formatted(
            "\\id GEN\n\\c 1\n\\p\n\\p text\n",
            &FormatOptions::default()
        ),
        "\\id GEN\n\\c 1\n\\p text\n"
    );
    // A MIXED pair says nothing about which was meant: the diagnostic stands and
    // format changes nothing here.
    let mixed = "\\id GEN\n\\c 1\n\\m\n\\p text\n";
    assert_eq!(formatted(mixed, &FormatOptions::default()), mixed);

    // A RUN of identical empties collapses in ONE run — the fix is chain-aware,
    // so N repeats need N-of-nothing passes, not N.
    let chain = "\\id GEN\n\\c 1\n\\p a\n\\p\n\\p\n\\p\n\\p b\n";
    let once = formatted(chain, &FormatOptions::default());
    assert_eq!(once, "\\id GEN\n\\c 1\n\\p a\n\\p b\n");
    assert_eq!(formatted(&once, &FormatOptions::default()), once);

    // A chain whose survivor is empty at EOF is left alone.
    let dangling = "\\id GEN\n\\c 1\n\\p a\n\\p\n\\p\n\\p\n";
    assert_eq!(formatted(dangling, &FormatOptions::default()), dangling);
}

#[test]
fn the_missing_paragraph_fix_rides_the_verse_break_axis() {
    // Keep: the `\p` gets its own line and so does the verse.
    assert_eq!(
        formatted("\\id GEN\n\\c 1\\v 1 In the beginning\n", &keep_verses()),
        "\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning\n"
    );
    // Remove: the same `\p`, with the verse flowing after it — the fix's own
    // trailing break IS a verse break, so the axis decides it.
    assert_eq!(
        formatted(
            "\\id GEN\n\\c 1\\v 1 In the beginning\n",
            &FormatOptions::default()
        ),
        "\\id GEN\n\\c 1\n\\p \\v 1 In the beginning\n"
    );
}

#[test]
fn a_repair_joins_the_transaction_only_when_it_is_asked_for() {
    let truncated = "\\id GEN\n\\c 1\n\\p \\v 1 a\\f + \\ft note\\c 2\n\\p b\n";
    // Without the allowlist the note stays open — format is not a linter.
    assert_eq!(
        formatted(truncated, &keep_verses()),
        "\\id GEN\n\\c 1\n\\p\n\\v 1 a\\f + \\ft note\n\\c 2\n\\p b\n"
    );
    let repairs = FormatOptions {
        repairs: &[Code::UnclosedNote],
        ..keep_verses()
    };
    assert_eq!(
        formatted(truncated, &repairs),
        "\\id GEN\n\\c 1\n\\p\n\\v 1 a\\f + \\ft note\\f*\n\\c 2\n\\p b\n"
    );
}

// ---------------------------------------------------------------------------
// The invariants
// ---------------------------------------------------------------------------

/// The fixtures every invariant below is checked over — one per shape the rules
/// have a reason to fear.
const FIXTURES: [&str; 12] = [
    "",
    "just prose with no markers at all\n",
    "\\id GEN\n\\c 1\\v 1 Text",
    "\\id GEN\r\n\\c 1\r\n\\p a\r\n",
    "\\c 1\n\n\n\\p   \\v 1  In  the beginning \n\\v 2 b\n",
    "\\p \\w In\\w*\n\\w the\\w*\n\\w beginning\\w*\n",
    "\\id GEN\n\\c 1\n\\p\n\\p\n\\q1\n\\v 3 poetry\n",
    "\\p a\n\\s5\n\\p \\v 1\\v 2\\v 3 asdf\n",
    "\\p \\nd Lord\\nd*'s Battles\\f + \\ft n\\f*\n",
    "\\id GEN\n\\c 1\n\\p \\v 2 2 men went\n",
    "\\zfoo |k=\"v\"\\* \\qt-s |who=\"Levi\"\n",
    "   \\p\t\tindented\t\n\t\n",
];

/// Every combination of every axis and every switch, over every fixture: no
/// panic, a valid transaction, and a result that settles in one pass. The option
/// space is finite and small, so this ENUMERATES it rather than sampling.
#[test]
fn options_are_total() {
    let mut checked = 0u32;
    for verse_breaks in [VerseBreaks::Keep, VerseBreaks::Remove] {
        for char_marker_breaks in [CharBreaks::Keep, CharBreaks::Join] {
            for newline in [Newline::Lf, Newline::CrLf] {
                for bits in 0u32..1 << 9 {
                    let opts = FormatOptions {
                        verse_breaks,
                        char_marker_breaks,
                        newline,
                        remove_markers: &["s5"],
                        repairs: &[Code::UnclosedNote, Code::UnclosedAtEof],
                        block_marker_own_line: bits & 1 != 0,
                        collapse_blank_lines: bits & 2 != 0,
                        normalize_newlines: bits & 4 != 0,
                        trim_text_edges: bits & 8 != 0,
                        delimiter_single: bits & 16 != 0,
                        designator_ws_single: bits & 32 != 0,
                        marker_ws_at_line_start: bits & 64 != 0,
                        dedupe_verse_number: bits & 128 != 0,
                        bridge_empty_verses: bits & 256 != 0,
                    };
                    for fixture in FIXTURES {
                        // `formatted` proves the transaction valid (a debug
                        // assertion inside `format_edits`) and settled.
                        formatted(fixture, &opts);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 8 * 512 * FIXTURES.len() as u32);
}

/// FORMAT NEVER INVENTS CONTENT. Every inserted byte is whitespace, or an engine
/// `FixStr` from a lint fix (a `\p`, a closer), or the digits of a bridge range.
#[test]
fn every_inserted_byte_is_whitespace_or_an_engine_string() {
    let bundles = [
        FormatOptions::default(),
        keep_verses(),
        FormatOptions {
            newline: Newline::CrLf,
            char_marker_breaks: CharBreaks::Join,
            remove_markers: &["s5"],
            repairs: &[Code::UnclosedNote, Code::UnclosedAtEof],
            bridge_empty_verses: true,
            dedupe_verse_number: true,
            ..FormatOptions::default()
        },
    ];
    for opts in &bundles {
        for fixture in FIXTURES {
            for (code, edit) in format_claims(fixture.as_bytes(), opts) {
                let insert = edit.insert.as_bytes();
                let allowed = insert.iter().all(|byte| byte.is_ascii_whitespace())
                    // A marker or a closer the engine wrote: `\p`, `\f*`.
                    || insert.starts_with(b"\\")
                    || insert.starts_with(b"\n\\")
                    || insert.starts_with(b"\r\n\\")
                    // A bridged verse range.
                    || (code == Code::BridgeEmptyVerses
                        && insert
                            .iter()
                            .all(|byte| byte.is_ascii_digit() || *byte == b'-'));
                assert!(
                    allowed,
                    "{} inserted {:?} into {fixture:?}",
                    code.row().name,
                    edit.insert
                );
            }
        }
    }
}

/// Non-UTF-8 bytes are not an error and not a guess: no edits at all.
#[test]
fn malformed_bytes_format_to_themselves() {
    let broken = b"\\id GEN\n\\p \xff\xfe not utf8\n";
    assert_eq!(format_edits(broken, &FormatOptions::default()), vec![]);
    assert_eq!(format(broken, &FormatOptions::default()), broken.to_vec());
}

/// A CrLf rewrite that outgrows one `FixStr` stacks adjacent edits instead of
/// silently keeping the un-normalized fix.
#[test]
fn an_overflowing_retarget_stacks_adjacent_edits() {
    let opts = FormatOptions {
        newline: Newline::CrLf,
        ..FormatOptions::default()
    };
    // 8 breaks = 8 bytes in, 16 out — one past the 15-byte cap.
    let fix = Edit {
        from: 10,
        to: 12,
        insert: FixStr::new(b"\n\n\n\n\n\n\n\n"),
    };
    let mut out = Vec::new();
    retarget(fix, Code::UnclosedNote, &opts, &mut out);
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].from, out[0].to), (10, 12));
    assert_eq!((out[1].from, out[1].to), (12, 12));
    let stacked: Vec<u8> = out
        .iter()
        .flat_map(|edit| edit.insert.as_bytes().to_vec())
        .collect();
    assert_eq!(stacked, b"\r\n".repeat(8));
}
