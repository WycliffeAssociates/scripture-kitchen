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
rows, format_edits/format), the DIFF port (pass 6, anchor-cut skeleton,
SpliceEdit replay), and ANALYZE + the wasm bindings crate (passes 7–9:
seven-read editor wire, diagnostics side-table, TS wrapper). The
CodeMirror spike at ../onion-2-spike consumes it end to end — 16/16
driver checks, keystroke medians ~2–7 ms with the engine at <1 ms of it.
Perf: perf-notes §6–§8 (incl. MEASURED wasm: en_ulb PSA all-reads
3.36 ms, 1.09x native). Rulings ledger: choices.md through pass 9.

NAMING CORRECTED (pass 10, 2026-08-25): the bindings crate is
`onion-wasm/` — piece 2 of the five-crate layout (see
ideas/committed/galley.md). `galley` is reserved for the workflows
crate over onion + sous. `analyze` now returns a plain JS object, the
per-change `wants` set is the app's, and the two distributed builds
(pkg-web, pkg-bundler) are committed for GitHub-tag installs.

## Next code: the designator gate

Sketch: sketches/designator-gate.md (ruled sound by Will 2026-08-25).
A Designator token requires a leading ASCII digit; `\v Then` becomes
Marker + Text, unifying with the already-handled designator-less case
(`\v \p`). Full grammar stays the interpreter's. Ripples: toc anchor
for the absent case, a verse-without-designator lint lane, export/diff/
analyze verification, corpus pins re-pinned, spike re-sync (16 checks).

## Queued after it (small, ruled)

- `format_edits_in(range)` — apply formats cleanly within $scope
  (spike-gaps ask 11, accepted as a candidate).

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
