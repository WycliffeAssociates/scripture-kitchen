# Next steps

ONE next step at a time, trimmed. Implementation history is not kept
here — code and doc comments are the record, git is the log. The
ordered queue of everything after this step: ideas/committed/roadmap.md,
one design sketch per item in sketches/. Rulings that constrain later
phases: settled-facts.md. Vocabulary: GLOSSARY.md. Unproven perf leads:
investigate-later.md.

## Where we are (2026-08-20)

Scanner COMPLETE (~1.7 GiB/s prose). `cst::build` COMPLETE (~6 ns/token,
flat lossless CST). The ATTRIBUTE INTERPRETER is COMPLETE
(src/attributes.rs — borrowed spans, no allocation, zero malformed over
1.25M corpus lists). LINT IS CLOSED (src/lint/ — 43 codes in six
families, NOTHING OWED: one in-order walk feeding four machines at ~9.4
ns/token on unaligned scripture and ~15.5 on word-aligned en_ult, where
the k/v attribute rules read 31 MB of list interiors nothing read before;
15 codes offer a fix, with `check_fixes` as the oracle over every corpus
fix; splice primitives in src/edit.rs). Full staged pipeline ~25 ns/token
(en_ulb), ~38 on en_ult. The single-pass fusion experiment
is measured and PARKED (experiments/fused.rs is the record); the UTF-16
wire shape is measured and amended (experiments/utf16.rs: stride-256 +
SWAR — settled-facts).

## Next code: exports

Sketch: sketches/usj-export.md (USX/HTML follow it —
sketches/usx-html-export.md). The first CONSUMER of the whole stack: the
CST plus the interpreter's k/v reading, splatted into USJ's JSON shape.
It is where the attribute interpreter's second consumer arrives (lint was
the first), and where the LOSSY step lives — the token stays
byte-identical, the export is what interprets.

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
