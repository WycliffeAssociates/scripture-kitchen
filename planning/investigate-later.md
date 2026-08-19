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

**(2) IS DONE, 2026-08-19.** `cst::Builder` is `pub(crate)`, driven by
`with_capacity(tokens, scope_openers)` / `new()` → `feed(token_idx: u32,
&Token)` per token → `finish() -> Cst`; `build(&[Token])` is that loop plus
the exact pre-count, and the public API is otherwise untouched. All 151 tests
green (cst oracle, lint corpus, fix oracle, partition, header, fast-path all
unchanged), clippy warning set identical to HEAD, `--cst-stats` identical.
Three findings that bear on step 3:

- **The feed contract came out STRONGER than "may re-read earlier tokens".**
  The Builder holds NO slice at all: the walker's only look-back was
  `frame_marker_idx`, re-reading an open frame's opening token for its row,
  and that row is now COPIED into `Frame::marker_idx` at push time. Verified
  by exhaustion that nothing else indexed a neighbouring token — the
  bare-spelling milestone clause, `same_kind_evict`, the container searches
  and `milestone_point` all read FRAMES, never tokens. So the sink never
  needs the scanner's growing vec in hand, which also dodges the borrow
  fight (a `Builder<'a>` borrowing the vec the scanner is pushing into is not
  expressible without passing the slice back in on every feed).
- **The one-token delay is the DRIVER's rule, and it is now written on
  `Builder`'s doc.** The Builder reads `kind()` and `marker_idx` only, never
  `len`, so `whitespace_arm`'s late mutation of the previous token cannot
  reach it — but its downstream consumers are not so lucky, so a `push_token`
  sink must still feed N only once N+1 exists (or at EOF).
- **Capacity split.** `build` keeps its exact pre-count (now
  `exact_scope_openers`) and so allocates identically to before;
  `with_capacity` takes those two counts as HINTS for a driver that only has
  the source length (measured: tokens ≈ bytes/16, openers ≈ tokens/8 on prose
  and tokens/4 on aligned text), and `new()` reserves nothing.

**Perf (min-of-8, `--cst-only`, both binaries kept side by side and
interleaved):** en_ulb 1.376 → 1.344 ms (**−2.3%**), en_ult 39.09 → 39.53 ms
(**+1.1%**, 5.95 → 6.02 ns/token). Isolated by building a third variant that
keeps `feed`/`finish` but re-reads the row off a held slice: that one lands ON
the baseline, so **the feed/finish split itself is free** and the whole ±1% is
the `Frame::marker_idx` byte — one extra store per frame push, which the
aligned corpus pays (a point frame per word, its row rarely re-read) and prose
recovers with interest (deeper displacement searches, each of which now skips
two loads). `Frame` is still 12 bytes, so nothing regressed in layout. Judged
a wash and taken for the streaming contract. `#[inline(always)]` on `feed` was
tried and is WORSE (40.0 ms); plain `#[inline]` is right.

**(3) IS BUILT AND MEASURED, 2026-08-19 — and the answer is NO for lint.**
`src/experiments/fused.rs` is the whole pipeline in ONE traversal: a copy of
the scanner's ARMS (every pure piece — boundary finders, `classify_marker`,
`resolve_marker_idx`, `escape_len`, `attr_list_end`, `folds_delimiter`,
`HotIdx`, `ScanState` — reused from `scanner` via `pub(crate)`, so the copy can
only diverge in EMISSION) whose `push_token` feeds a `Sink` holding a COPIED
`cst::Builder` with the lint machine calls inlined at its push/pop/leaf sites,
plus the four REAL machines, `Emit` and `header_scan`, reused verbatim. The
production diff is visibility-only. `tests/fused_identity.rs` is the oracle:
`analyze_fused(src) == (lex, build, lint)` — tokens, `Cst`, and every field of
`LintReport` — over all **226 corpus books** and a 50-snippet zoo. Green.

Four things learned building it:

- **The delay is TWO tokens, not one.** `whitespace_arm`'s late `len` extension
  is only half of it: `push_marker`/`marker_arm` stamp `marker_idx` AFTER
  `push_token` returns, and `Flat`'s attr-terminator-mismatch rule reads
  `tokens[idx + 1]`'s row. One token of delay hands that rule an unstamped
  successor and it reports a mismatch on every well-formed `\w …|…\w*` in
  en_ult NAM. Feed N once N+2 exists and both N and its lookahead are settled.
- **`header_scan` needs no divergence at all** — a WARM-UP BUFFER beats judging
  with a half-known version. The sink delivers no events until it has settled
  the first `Chapter` marker, then runs the REAL `header_scan` over exactly
  that prefix (identical to the full slice, since it breaks there anyway) and
  flushes. An `AttrList` ahead of the `\usfm` line is judged exactly as staged
  judges it. Cost: a buffered book header; a document with no `\c` buffers whole.
- **`next_number` is the one thing that genuinely cannot run in a single pass.**
  Lint's only unbounded lookahead walks FORWARD to the next `\c`/`\v` to refuse
  a renumber that would just move the problem along (bdf_reg ROM 3). Fused is a
  strict SUPERSET there (a short slice returns `None`, which always permits), so
  it is corrected at `finish` over the FINDINGS — never the tokens — withdrawing
  the fixes staged refuses. Removing the correction fails the oracle on both the
  corpus and the zoo, so it is load-bearing, not defensive.
- The debug leaf counter transfers unchanged: the sink asserts every token index
  is delivered exactly once, in order.

**Perf (min-of-8, all six modes interleaved in one window, one binary):**

| ns/token | lex | lex+cst | lex+cst+lint |
|---|---|---|---|
| en_ult staged | 15.65 | 21.94 | **30.90** |
| en_ult fused | 16.10 (noop sink) | 21.44 | **36.64** |
| en_ulb staged | 10.16 | 15.57 | **24.59** |
| en_ulb fused | 12.67 (noop sink) | 17.24 | **31.91** |

(en_ult 102.9 / 144.2 / 203.1 ms vs 105.8 / 140.9 / 240.8; en_ulb 2.592 /
3.970 / 6.272 ms vs 3.232 / 4.397 / 8.138.)

**The verdict splits cleanly at the CST/lint line.** Fusing the BUILDER into
the scan is free-to-slightly-positive — en_ult 144.2 → 140.9 ms (**−2.3%**),
en_ulb 3.97 → 4.40 (+11%, and most of that is the hook's per-document setup
showing up over 66 small books). Fusing LINT is a straight loss: the lint step
alone goes 58.9 → 99.9 ms on en_ult (**+70%**) and 2.30 → 3.74 on en_ulb
(**+63%**), for **+18.6% / +29.8%** end to end. The predicted 8-10 ns/token of
deleted iteration overhead is real but is dwarfed by what the machines lose:
`lint::walk` keeps its frame in LOCALS, its four machines as LOCAL structs
whose hot fields live in registers, and builds `Doc` ONCE outside the loop —
fused, every event re-derives `Doc` from `self` and reaches each machine through
`&mut self` on a ~1 KB struct that also carries the scanner's mode flags, the
memmem finder and the Builder's three vecs. That is exactly the effect
scanner.rs already documents for its own `ScanState` (6% just from `&mut self`
vs threaded parameters), paid four times per leaf, and the scanner's flags spill
across the call to boot. **Hanging the sink off `push_token` costs the LEX
almost nothing** (en_ult +0.45 ns/token, 102.9 → 105.8 ms;
en_ulb's +2.5 ns/token is a 66-small-book per-document setup artifact), so the
hook is not the problem — the machines' working set is.

**Recommendation: PARK the lint fusion, and note that streaming lex→cst is
viable on its own** if a future editor session wants a tree without a token
slice. The staged path stays the only public API either way.
