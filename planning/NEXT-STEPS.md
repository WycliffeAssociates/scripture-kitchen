# Next steps

ONE next step at a time, trimmed. Implementation history is not kept
here — code and doc comments are the record, git is the log. The
ordered queue of everything after this step: ideas/committed/roadmap.md,
one design sketch per item in sketches/. Rulings that constrain later
phases: settled-facts.md. Vocabulary: GLOSSARY.md. Unproven perf leads:
investigate-later.md.

## Where we are (2026-08-20)

Scanner COMPLETE (~1.7 GiB/s prose). `cst::build` COMPLETE (~6 ns/token,
flat lossless CST). LINT COMPLETE and read by Will (src/lint/ — 37
codes, one in-order walk feeding four machines at ~9 ns/token, the fix
model with `check_fixes` as the oracle over every corpus fix; splice
primitives in src/edit.rs). Full staged pipeline ~31 ns/token. The
single-pass fusion experiment is measured and PARKED
(experiments/fused.rs is the record); the UTF-16 wire shape is measured
and amended (experiments/utf16.rs: stride-256 + SWAR — settled-facts).

Lint leftovers, each parked against its roadmap item: the two k/v
attribute rules (item 1), the Version family (item 5), and the
POSITIONAL-CONTEXT LANE (sketches/positional-context.md — NOT on the
roadmap yet; slot it when ready).

## Next code: the attribute k/v interpreter

Sketch: sketches/attr-interpreter.md. The designator interpreter's
sibling — pure span → judgment over an AttrList token's interior:
quoted/unquoted values, the default attribute (the row's
`default_attribute`), `defined_attributes` matching, the `a-*` prefix
wildcard, "later definition wins" merge. Judged, never repaired; no
allocation on the read path.

Unblocks two consumers at once: exports' k/v splatting (the lossy step)
and the two owed lint rules (`attr-unknown-name`, `attr-required-if`).
Built standalone, both consumers arrive with it already tested.

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
