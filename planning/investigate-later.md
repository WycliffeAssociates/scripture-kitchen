# Investigate later

Simplification leads that look real but haven't been checked. Each one's
GOAL IS NET DELETION — if the investigation ends with more code than it
started, the answer was no. Nothing here is committed to; nothing here
blocks anything.

## Does the scanner need `ws_after_name` at all? (2026-08-13)

**The suspicion:** the delimiter-fold rule may be nothing more than
*openers and milestones fold, end markers don't* — pure SHAPE, zero table.

**Why it looks that way now.** Step 4.6 collapsed the scanner's use of the
column to a single bit: `folds_delimiter` reads six `Ws` variants and
distinguishes exactly one of them (`SingleNewline`) from all the others.
Row 0 is special-cased to "fold" anyway. So of 151 rows the column changes
the scanner's answer for exactly one marker: `\b`.

**What to check, concretely:**

1. Is `\b` even a real counterexample? Its row says `SingleNewline`, so
   today `\b x` leaves the space as content. If it folded instead, does
   anything break? Partition holds either way (the run just rides the
   marker span). `\b` takes no content at all, so a following space is
   already anomalous — which means the question is really "who reports
   it", not "which token owns the byte".
2. If lint is the honest owner of that report, the scanner's read of the
   column is dead weight, and the deletions are: `folds_delimiter`'s row
   lookup, the `Ws` import in scanner.rs, **`Hot.folds` and its resolve
   step entirely**, and the fold branch in `fused_plain` that exists only
   to serve `\b`. That is a genuine net deletion across two files.
3. The column itself STAYS regardless — lint needs required-vs-optional
   for its delimiter-whitespace findings, so this is about deleting a
   scanner READ, not a table column. Do not confuse the two.
4. Check the same question for the codegen side: if the scanner stops
   reading it, does the packed field's bit width still earn its place, or
   does it become a lint-only column that could move out of the hot
   `u128`/row? (Bit budget is `BITS_USED` in generated.rs.)

**Do it when:** any time after 5B. It is independent of the attribute work
and would make `Hot` smaller, so ideally BEFORE any further `HotIdx`
surgery.

**Watch out for:** the `AtLeastOneWhitespace` rows (`\v`) — HS *or*
newline. The scanner already never folds a newline (`ws_run_end` eats
space/tab only), so those rows are not counterexamples; they just make the
enum look more load-bearing to the scanner than it is.

## Unicode whitespace where the spec says `hs` (2026-08-17)

**Current answer, and probably the final one: it's spec, and lint's.** The
delimiter fold takes SPACE and TAB only, which is what `hs` means, so a NBSP
after a marker name stays content and does not fold. Will has not seen real
files with one after a marker, and the terms file doesn't put one there
either — so this is a lint finding, not a scanner behavior.

**Why it needs no scanner change if that holds:** the facts lint needs are
already derivable from the spans. HS beyond the delimiter is inside the
marker span, and span length vs name length recovers it; a NBSP is simply at
the head of the following Text run.

**Evidence, 2026-08-17:** the 226-book corpora contain ZERO occurrences of
NBSP (U+00A0), NNBSP (U+202F), ideographic space (U+3000) or ZWSP (U+200B) —
not in delimiter position, not anywhere. So there is no corpus case to serve
and nothing to weigh against the spec reading.

**What would reopen it:** a real book with a non-`hs` whitespace character
in delimiter position. Until one shows up this is closed.

## A `\w` fused arm — the aligned-corpus equivalent of 4.4 (2026-08-13)

NOT a deletion, so it needs a stronger justification than the item above —
but the number is large enough to record. `common_marker_checks`'s
membership is the top-9 measured on PROSE (en_ulb/bsb). On word-aligned
text the distribution is completely different: `\w` and `\zaln-s` dominate,
`\w` appearing once per WORD, and neither has an arm — so en_ult's hottest
path is entirely general-path (`marker_end` → `classify_marker` →
`resolve_marker_idx` → the fold predicate, three walks over the same ≤4
bytes).

Evidence it would pay: 5B measured en_ult at 1017 MiB/s vs prose's ~1700,
and 5B's own residual cost is concentrated on exactly this shape
(`\w text|attrs\w*`). An arm could fuse marker + delimiter + the
back-position ladder for the whole word in one shape test.

**Before building it:** re-measure the frequency table on an aligned corpus
(planning/marker-frequencies.md is prose-only, which is why `\w` isn't in
the cut), and check this against the "one-load marker path" spike already
banked at the bottom of NEXT-STEPS — they overlap, and the spike should
come first since it helps every marker rather than one.

## Single-pass pipeline: a generic sink on push_token (Will, 2026-08-19)

**The idea:** the whole pipeline could run in ONE traversal — the scanner's
`push_token` feeds a generic sink (`Noop` by default, monomorphized away),
the sink is the CST Builder, and lint's state machines feed on the
Builder's events (leaf token / node-open / node-close(reason)). This is
the linter.md emit funnel RESURRECTED — deliberately: the reasons it was
killed are dead (no external-token door exists; the editor sends text),
and the version that survives is the one where the scanner learns nothing
about what listens.

**Why it's mechanical, verified 2026-08-19:**
- `cst::build` is a forward-only Builder loop, no lookahead — inverts to
  `feed(token)` + `finish()`, with `build(&[Token])` as sugar.
- The Builder's live frame stack IS the ancestry lint's tree walk
  re-derives; close verdicts are stamped at pop, exactly when the
  structural machine wants them; orphan closers get SIMPLER fused (the
  consumed-bitset exists only because the passes are separate).
- Whole-file facts (missing-id, numbering-mix) flush at finish().

**The wrinkles, priced:**
- ONE-TOKEN DELAY, still true: `whitespace_arm` (scanner.rs) extends the
  PREVIOUS token's len on a later iteration, so a sink sees token N only
  when N+1 exists (or EOF). Old linter.md recorded exactly this.
- Two lint rules hold one token of lookahead (attr-terminator-mismatch,
  adjacency windows) — pending-state in the machine, judged on next feed.
- THE STAGED PATH STAYS AS THE ORACLE: fused output identity-tested
  against lex → build → lint run separately (the fast_path_identity
  precedent). The staged functions never go away.

**Honest perf bound:** fusion deletes iteration overhead — ~8-10 ns/token
pipeline-wide at best (~33 → ~23 measured baseline) — but the scanner's
1.7 GiB/s comes from a tight loop; per-token work hanging off push_token
risks the lex itself. Max-of-8, both corpora, believed only when measured.

**Staging (converges with work already queued):** (1) lint one-walk
refactor SHAPED AS feedable state machines, driven by the tree walk —
90% of the extraction as a pure reorganization; (2) invert cst::build to
feed(); (3) the scanner-sink experiment in experiments/, behind the
identity oracle.

**(1) IS DONE, 2026-08-19.** src/lint.rs is one in-order walk feeding
`Structure`/`Ancestry`/`Ordering`/`Flat` — plain structs, `on_node_open` /
`on_leaf` / `on_node_close` / `finish`, no view of the driver's stack. Every
observation, fix label and edit is byte-identical over all 226 books, and
12.1 → 9.1 ns/token on en_ult, 12.8 → 8.4 on en_ulb. Two findings that bear
on step 2: the orphan-closer BITSET IS GONE (a closer that closed something is
the last child of an `Explicit` node, so the very next event settles it — no
side table, which is exactly the simplification this section predicted), and
the one fact a machine needed at both open and close rides a `u8` scratch on
the driver's frame, so the Builder's frames must carry the same byte. The
only lookahead left is `attr-terminator-mismatch`'s one token, still read off
the slice and still owed the pending-state treatment when the Builder drives.
