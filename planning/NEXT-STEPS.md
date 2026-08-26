# Next steps

ONE next step at a time, trimmed. Implementation history is not kept
here — code and doc comments are the record, git is the log. The
ordered queue of everything after this step: ideas/committed/roadmap.md,
one design sketch per item in sketches/. Rulings that constrain later
phases: settled-facts.md. Vocabulary: GLOSSARY.md. Unproven perf leads:
investigate-later.md.

## Where we are (2026-08-25)

Everything through the wasm milestone is BUILT: scanner, CST, attribute
interpreter, lint (54 codes incl. the Form channel), exports (USJ/USX/
HTML), masks + Toc + vref, utf16 index, FORMAT (pass 5, Form-channel
rows, format_edits/format, ranged in pass 14), the DIFF port (pass 6,
anchor-cut skeleton, SpliceEdit replay), and ANALYZE + the wasm bindings crate (passes 7–9:
seven-read editor wire, diagnostics side-table, TS wrapper). The
CodeMirror spike at ../onion-2-spike consumes it end to end — 16/16
driver checks, keystroke medians ~2–7 ms with the engine at <1 ms of it.
Perf: perf-notes §6–§8 (incl. MEASURED wasm: en_ulb PSA all-reads
3.36 ms, 1.09x native). Rulings ledger: choices.md through pass 14.

NAMING CORRECTED (pass 10, 2026-08-25): the bindings crate is
`onion-wasm/` — piece 2 of the five-crate layout (see
ideas/committed/galley.md). `galley` is reserved for the workflows
crate over onion + sous. `analyze` now returns a plain JS object, the
per-change `wants` set is the app's, and the two distributed builds
(pkg-web, pkg-bundler) are committed for GitHub-tag installs.

DONE (pass 13): the designator gate — a Designator token requires a
leading ASCII digit, `\v Then` is Marker + Text.

DONE (pass 14, 2026-08-25): `format_edits_in(source, range, opts)` —
scoped formatting, spike-gaps ask 11. Whole-book analysis, filtered to
a byte range: an edit straddling the boundary is dropped WHOLE (never
cut), a multi-edit claim is kept only if all of it is inside, and a
pure insertion ON either edge is inside. `formatEditsIn(text, from, to,
opts)` is the UTF-16 export. Chapter scope needs no sugar — the caller
holds the span. Honest limit, documented and tested: in-scope
idempotence holds over a CLEAN boundary (a chapter span); a window edge
cutting through a straddler leaves those bytes in scope, so the scoped
format converges over a few passes instead of settling in one.

## Next code: verse-under-heading + empty-verse-runs

Both candidates are RESOLVED and ruled; build them in ONE pass — they
share the ancestry/emptiness machinery.

- `ideas/candidates/verse-under-heading.md` — lint/ancestry.rs's
  "paragraph above" predicate becomes `MarkerKind::Paragraph &&
  !v_forbidden(marker_idx)`, consuming the already-generated,
  currently-consumerless `V_FORBIDDEN_IN_PARAGRAPHS`. Zero authored
  lists. Corpus movement is unknown — measure and re-pin.
- `ideas/candidates/empty-verse-runs.md` — verse-without-designator
  gains a fix ONLY when the `\v` is EMPTY (ws-only to the next
  verse/chapter/para marker): delete the extent, a consecutive run
  collapsing under ONE fix anchored on the first (the pass-12 chain
  pattern). `\v Then text` stays fixless. Formatter bit on.

One corner is still awaiting Will's nod: `\qa` is v-FORBIDDEN in
usx.rng but verse-valid in his pasted poetry list — lean is the table
wins (an acrostic heading, same disease as `\s1`).

## Standing laws

- **Partition oracle is not negotiable.** A change that wants to break
  `concat(spans) == source` is a design event — stop and log it.
- **Rows change only through the spec-diff**
  (`planning/spec_contexts_diff.py`, needs a tcdocs clone). The MARKER
  PAGE is the referee. WILL MAY OVERRIDE THE REFEREE, and did once
  (2026-08-19, the character-class Footnote/CrossReference curation) —
  an override is recorded on the rows and in the sketch, never left
  implicit. Spec fuzziness is absorbed by LINT SEVERITY, never by
  inventing table values.
- **Never synthesize tokens.** Flag, never repair. Every place the
  reference implementation normalizes is a place we lint. (Fixes are
  proposed TEXT, offered never applied.)
- **Normalization is never the lexer's.** Spans keep their bytes
  exactly; trimming happens when a consumer asks for values.
- **Go slow** (Will, 2026-08-10): one behavior at a time, each behind
  the oracle, each with its perf delta read before the next. Deltas
  under ~15% need MAX-of-8 runs and a re-measured baseline in the same
  window.
