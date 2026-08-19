# Next steps

The OPEN QUEUE only: what to write next and the settled design it embodies.
Implementation history is not kept here — code and doc comments are the
record, git is the log. Vocabulary: GLOSSARY.md. Rulings that constrain
LATER phases (exports, wasm, editor session, sous, braid — plus the parked
list and the large-piece order): settled-facts.md. Lint/braid designs:
ideas/committed/. Unproven perf leads: investigate-later.md.

## Where we are (2026-08-19)

The scanner is COMPLETE (~1.7 GiB/s prose, ~1.0 aligned): all token kinds,
the marker table + codegen, attribute lists, the three carved payloads,
and `ParseHeader` as its own pass. `cst::build` is COMPLETE (src/cst.rs —
flat lossless CST, ~6 ns/token; design and walker rules live in its doc
comments and tests). Green: `tests/partition_oracle.rs`,
`tests/fast_path_identity.rs`, `tests/parse_header_oracle.rs`,
`tests/cst_oracle.rs` (lifted partition over 226 books). Corpus health via
`playground --cst-stats`: zero Recovery except three genuinely unclosed
`\f` (en_ulb ISA/MRK, bsb GEN) — lint's first three real findings, waiting.

## Next code: lint

Shapes, fix model, and the full code list: lint-sketch.md (marked up
2026-08-19 — fix model A ruled, severity ladder ruled, three small opens
left at its bottom). What follows is the settled contract.

ONE entry point (ruled 2026-08-19): lint ALWAYS gets a CST — the library
is always three lines (lex → cst::build → lint); no fused pass, no
listener/funnel, no lint-over-bare-tokens door (the editor sends TEXT and
every transaction re-lexes). The CST is cheap and without it lint would
recompute forced closures the walker already judged.

    pub fn lint(source: &[u8], tokens: &[Token], cst: &Cst) -> LintReport

    struct LintReport {
        book: Option<u32>,  // BookCode token idx; None IS the missing-\id
                            //   finding (real: BSB Ecclesiastes), never a crash
        observations: Vec<Observation>,
    }
    // Observation: { code, anchor: u32, second: Option<u32> } — anchors are
    // token indices, per-build; severity/category/template live in a rules
    // table; message rendering is the consumer's. Audit onion's
    // messageParams before finalizing.

Internal invariant to document on `lint`: it never reorders, inserts, or
drops tokens (the session's token→span→UTF-16 mapping relies on it).

Two subsystems:

1. **Structural** — largely a linear `match` over `Node.reason` (the CST
   is a flat vec; no recursion): `Recovery` always a finding, `Eof` a
   finding iff the row wanted a closer, `Explicit`/`Implicit` silent —
   lint READS the walker's verdict, never re-derives it (non-negotiable).
   The full code list (attributes, adjacency, payload, form, the
   flag-never-repair rules) lives in lint-sketch.md. Lint is a PASS —
   the scanner's `push_token` funnel/listener idea is dead (linter.md
   deleted 2026-08-19, still-true rulings folded into the sketch).
   Escape hatch: a finding proven un-re-derivable rides the scanner
   individually; the general funnel does not come back.
2. **Ordering** — tokens only, ignores the CST: filter
   `Designator`/`BookCode` kinds, run the verse-designator interpreter
   per span, compare across the sequence. Needs the interpreter (spec
   `VERSE` pattern `/[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*/`
   — pure text rules, zero table; its doc comment IS the comparison
   rules vref needs later) and the books aux table (valid codes only,
   membership — copy the list from onion). Ruling reversal 2026-08-18:
   non-contiguous, out-of-order, and duplicate verses ARE findings.

Neither lint nor `cst::build` takes `ParseHeader` — it is a
consumer-side index; if ordering lint wants chapter runs it computes
them. Suppressions: NONE in v1 (ruled 2026-08-19) — the only future
knob is per-rule off/severity config, and only when a consumer asks.

### The positional-context lane (deferred here from cst::build)

- The positional band (`Scripture → … → ChapterContent`) is MONOTONIC,
  no new data: ordering = enum declaration order, transitions =
  `allowed_contexts`. Stay if current allowed, else advance to the lowest
  allowed context above current, else it's behind us → lint, don't move.
  Two instructions: `mask & !((1 << (cur+1)) - 1)`, `trailing_zeros()`.
  No marker "enables" regions; `\id` needs no special case.
- Markers listing two positional contexts (`mt#`, `cl`, `ip`) resolve by
  lowest-above-current; `cl`'s dual semantics falls out.
- `ca`/`cp`/`va`/`vp` are adjacency lint rules, not context questions —
  empty context slice, the machine abstains.
- The lane's exact encoding (per-token sidecar vs stamped on nodes) and
  the rules-table shape get tested against real rules when built. Node
  has spare layout room.

## Standing laws

- **Partition oracle is not negotiable.** A change that wants to break
  `concat(spans) == source` is a design event — stop and log it.
- **Rows change only through the spec-diff**
  (`planning/spec_contexts_diff.py`, needs a tcdocs clone). The MARKER
  PAGE is the referee; spec fuzziness is absorbed by LINT SEVERITY,
  never by inventing table values.
- **Never synthesize tokens.** Flag, never repair. Every place the
  reference implementation normalizes is a place we lint.
- **Normalization is never the lexer's.** Spans keep their bytes exactly;
  trimming happens when a consumer asks for values.
- **Go slow** (Will, 2026-08-10): one behavior at a time, each behind the
  oracle, each with its perf delta read before the next. Deltas under
  ~15% need MAX-of-8 runs and a re-measured baseline in the same window.

Large-piece order (Will, 2026-08-17): CST + lint → exports → diff port →
wasm/BookSession (pure Rust until then) → braid last.
