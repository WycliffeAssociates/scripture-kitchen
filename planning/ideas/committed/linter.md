# Lint & diagnostics — decided + discussed so far

Sketch of everything settled or firmly leaning about lint, diagnostics, and
recovery. Sources: QUESTIONS.md (E section, Q9, L ruling), TRANSITIONS.md,
NEXT-STEPS step 2 rulings, onion scar tissue. Items marked (leaning) are
direction, not law.

## The core contract

- **Lint must be correct over `Vec<Token>` alone.** Anything context-shaped
  it needs arrives as a property of the token stream or an explicitly
  attached emission — never a reach into live parser state. The editor's
  entire contract is `Token { id, kind, text }`; it never round-trips
  through usfm text to answer a context question.
- **(leaning) Context facts are a derivable lane.** Effective context /
  open scopes / allowed-here are a PURE FOLD over (tokens × marker table) —
  position-free, source-free. One fold function, two call sites:
  - the fused pass runs it live (the walker) and emits the facts as an
    optional parallel lane keyed by token index;
  - standalone lint over bare tokens runs the SAME function as a pre-pass.
  No duplicated logic, only duplicated execution when the lane is absent.
  The lane is a cache of a pure function, so it is oracle-testable:
  `lane == recompute(tokens)`. This is onion's `Some()` parser-hints smell
  made honest — the hint stops being privileged knowledge.

## The emit-funnel pattern (how listeners attach — Will, 2026-08-10)

`push_token` is already the single funnel every arm flows through, so
it IS the feed point: `emit(t)` = push + `listener.feed(t, src)`. Arms
never manually invoke the walker/lint — emission and observation are
one atomic act, structurally impossible to forget.

- **Zero-cost when off**: `Scanner<L: Listener>`, monomorphized;
  `NoopListener` compiles the feed away entirely. The stateless path
  pays nothing.
- **One feed, two drivers**: the fused pass calls feed inline; the
  bare-tokens path is `tokens.iter().for_each(feed)`. Same trait, same
  fold — this is what makes fused-vs-second-pass an execution knob
  (measure it), not an architecture choice.
- **State discipline ([L] enforcement)**: `feed(token, source)` ONLY.
  The listener never sees ScanMode or any scanner internals — anything
  context-shaped is the LISTENER's own state built from fed tokens.
  That's what keeps walk(tokens) provably equivalent to the fused run.
- **The one-token delay**: the ws fold mutates the last token's len
  AFTER push, so the funnel buffers exactly one token — feed N when
  N+1 arrives (or EOF). Listeners only ever see final tokens, and get
  one token of free lookbehind. Listener-internal buffering for deeper
  lookahead stays allowed on top.

## Where lint runs

- **One structure driver, lint as listener — and structural findings are
  the walker's EXHAUST (ruled 2026-08-10).** The walker asks "is this
  allowed here" FIRST and NECESSARILY — recovery IS a legality check
  (`pop_while(frame forbids marker)` can't run without it). So structural
  lint never re-asks: findings are the walker's transitions annotated
  with codes (recovery pop = not-allowed-here finding; unclosed-at-EOF,
  implicit close, mismatched `\X*` likewise). Diagnostic-only checks the
  walker doesn't need (e.g. `v`-in-otherpara) co-locate in its arm —
  frame data is in hand, one extra AND, gated on a lint listener.
  This is the honest version of onion's ad-hoc Some() hints: don't
  re-ask what an earlier stage answered, but the surfaced answer must be
  DERIVABLE — `walk(tokens)` regenerates the event stream identically,
  so it's a cache of a pure function, not privileged parser knowledge.
  Lint's three tiers: (1) structural = walker exhaust; (2) per-token
  rules = plain listeners, no stack; (3) counting/aggregate = the
  per-slot fold. Only tier 1 touches the walker.
- **Tier 3's canonical example: numbering consistency** (2026-08-10).
  Bare `\q` and `\q1` are each fine per-token; the finding exists only
  in aggregate ("this book mixes bare and numbered spellings" — the
  spec's own bare-only-when-single-level rule is unevaluable until the
  book is seen). State: per-family levels-seen bitmask (~few hundred
  bytes total); finding anchors to the token that revealed the mix.
  These are CONSISTENCY observations, not violations — the distinction
  is the rules table's severity column (info/warning vs error), never
  separate machinery. Fits the slot fold: the bitmask IS the small
  incoming/outgoing state, and it almost never changes on edit.
- **Non-structural lint stays fold-only**: designator spelling, whitespace
  style, anything per-token with no stack. No drift risk, no listener
  needed.
- **The single-pass vision [M]**: the fused pass produces a
  valid/recovered/diagnostics-aware result in ONE traversal. Layering
  lives INSIDE the pass — arms own position, walker owns policy, walker is
  listener-gated so the plain-tokens path remains the always-works oracle.

## The finding (Observation) shape

- `{ code, anchor token, optional second token }`. Severity, category, and
  message template live in a RULES TABLE keyed by code; message rendering
  and localization happen on the consumer's side. (Onion lesson: ICU
  message params baked into findings were a mistake — audit onion's
  messageParams to confirm nothing non-derivable is lost.)
- Findings anchor to token ids / (slot, row) — never absolute byte
  offsets (offsets shift; ids and slot-relative rows don't).
- Retained findings are the ONE legitimate ride-along beyond columns: a
  stamped, refusable, RE-DERIVABLE cache (GLOSSARY "Cache"). Never
  promoted into the format.

## Recovery & errors

- **No tree repair, because no tree.** The partition is lossless by
  construction and the scan never stops; "a new block should have begun
  here" is an OBSERVATION pointing at a span, not a fixup. Refuse-never-
  invent holds because there is nothing to invent into.
- **Unknown/illegal markers (`\s5`, unconfigured `\z*`)**: recovery = pop
  all scopes, start fresh (ruled [F]). Lint raises the finding; the walker
  just re-stabilizes.
- **Note recovery is not a special mechanism [I]**: frames carry
  `effective_context` stamped at push time; a marker illegal in the
  current context pops through the note via the same generic
  `pop_while(predicate)` driver as everything else.

## Context legality (the cheap check)

- Per marker token: `row.allowed_contexts & (1 << current_context) != 0` —
  one AND against the 20-bit SpecContext mask, riding the walker's stack.
  Listener-gated: no lint listener attached → no ANDs.

## Rules already known to be lint's business (not the matcher's/scanner's)

- Bare-vs-numbered: bare `\s` is always a valid SPELLING; "bare only when a
  single level exists in the text" is a lint rule.
- Numbering out of range (`\q7`) — reads the row's Numbering cap.
- Deprecated syntax (`h#`, `addpn`, ...) — reads the deprecated flag.
- Redundancies: empty paragraphs, duplicate chapter labels (occurrence
  ordinals are derived, duplicates are DATA — lint comments, never blocks).
- Delimiter-whitespace violations — reads ws_after_name.
- Conditional attribute cardinality — `AttrStatus` is per-attribute and
  cannot express dependencies BETWEEN attributes, so these are lint rules:
  `eid` is required *if* `sid` was used (3.2 ms/qt.html); `sid`/`eid` are
  optional standalone but required when the milestone is paired
  (ms/ts.html). The rows say `Optional`; lint owns the "required-if".

## Incremental lint (the braid tie-in — see braidv2.md)

- **Lint is a fold over slots**: each slot's lint =
  `(incoming state) → findings + (outgoing state)`. Edit a slot →
  recompute it → propagate downstream ONLY if outgoing state changed
  (it almost never does). Same math as the parallel-stitch recipe.
- Per the spec rail, chapter content starts its own element collection —
  NO block state legitimately crosses `\c`. Cross-chapter residue is
  small carried state (milestone pairing, duplicate chapter labels,
  book-level tallies). The incoming state is a small, explicit,
  serializable PARAMETER — which is what keeps the "no reach into live
  parser state" constraint intact at slot granularity.

## Open

- The exact lane encoding (per-token context values vs event stream) and
  the rules-table shape — sit on it until the walker exists, then test
  against real rules.
- Onion messageParams audit (anything non-derivable?).
- How much of onion's lint rule inventory ports vs dies — not started.
