# Next steps

The OPEN QUEUE only: what to write next, the questions blocking it, and
settled design the code doesn't embody yet. Implementation history is not
kept here — the code and its doc comments are the record, git is the log.
Vocabulary: GLOSSARY.md. Lint and braid designs: ideas/committed/.
Unproven simplification leads: investigate-later.md.

## Where we are (2026-08-13)

The scanner is COMPLETE. Token kinds, the marker table
and its codegen, the fused fast arms, attribute lists (both U25001
node-initial and legacy trailing), all three carved payloads (designator,
note caller, `\id` book code), and the `Scanner` struct all landed;
`tests/partition_oracle.rs` (226 books, `concat(spans) == source`) and
`tests/fast_path_identity.rs` are green. ~1.7 GiB/s prose, ~1.0 aligned.

`\usfm` deliberately carves nothing — its Text is already isolated by the
line ending, so lint reads the version off the adjacent marker.

Nothing consumes tokens yet: no walker, no lint listener, no exports. The
books table lint will need (valid codes only — membership, no ordering or
testament data; onion has the list to copy) does not exist yet either.

## Open questions — decide these before or while coding

1. **Version data has nowhere to live.** `MarkerRow` carries only
   `deprecated: bool`, but the facts keep accumulating: `ta` since 3.1.2,
   trailing attribute lists deprecated 3.2 / removed 4, `\list-s` and
   `\table-s` optional 3.2 / required 4. One of those is decisive — a
   deprecated GRAMMAR FORM is owned by no row — so the lint rules table
   has to be the primary home (`severity(code, declared_version)`, fed by
   `\usfm`), with rows carrying per-marker since/until only if the rules
   table can't. There are 46 spare bits per row if it comes to that.
   **Deliberately unresolved until the rules table exists** (Will,
   2026-08-14), since that may make the column unnecessary. Note the
   scanner takes no runtime version input and has no rejection path — it
   implements a static SUPERSET of forms — so this decision adds no
   scanner branches either way.
2. **Unicode whitespace where the spec says `hs`.** The fold takes SPACE
   and TAB only, which is spec-correct, so a NBSP after a marker name
   stays content and does not fold. Real files contain them. Lint's, we
   think — the facts it needs are derivable (HS beyond the delimiter is
   inside the marker span; span length vs name length recovers it).

## Next code, in order

1. **Header emission** — `Header`/`ChapterRun` are defined but never
   produced. Unblocked: `\id`'s payload is now a `BookCode` token, and
   remember a BOM means `\id` is not always token 0, so FIND it.
2. **The walker** — settled design below, unbuilt. Stack first, with no
   consumers, then the context lane.
3. **Lint listener** — attaches at the `push_token` funnel per
   ideas/committed/linter.md, which also lists the four attribute
   findings owed (deprecated trailing form, both-lists, rung-3 hint,
   mismatched terminator) and the interval where they are missing.

## Standing laws

- **Partition oracle is not negotiable.** If a change wants to break
  `concat(spans) == source`, that's a design event — stop and log it.
  (It has already caught what unit tests could not: a correct token
  emitted in the wrong ORDER.)
- **Rows change only through the spec-diff** (`planning/spec_contexts_diff.py`,
  needs a tcdocs clone). 6 of 7 row edits made by
  inference (2026-08-12) were wrong; the diff caught all of them. The
  MARKER PAGE is the referee — the doc index, the RNG grammar (it
  describes USX), and the pages themselves disagree in spots; spec
  fuzziness is absorbed by LINT SEVERITY, never by inventing table values.
- **Never synthesize tokens.** usfmtc fabricates an implicit `\p` when
  `\v` follows `\c`; we cannot — no span to give it. Flag, never repair.
  Every place the reference implementation normalizes is a place we lint.
- **Normalization is never the lexer's.** Spans keep their bytes exactly
  (`"a, b"` in an attribute value keeps its space); trimming happens when
  a consumer asks for values, and editing a region is how a user opts
  into it.
- **Go slow** (Will, 2026-08-10): one behavior at a time, each behind the
  oracle + playground verify, each with its perf delta read before the
  next. Deltas under ~15% need MAX-of-8 runs and a re-measured baseline in
  the same window — this machine's noise band is ~24% under load.

## The walker (SETTLED design, unbuilt)

Distilled from the retired TRANSITIONS.md, 2026-08-12. Milestone sid/eid
pairing and note callers' full behavior live here, not in the scanner.

- **One generic loop**: frames are stamped with their context at push
  time (`frame.ctx = row.contributes_context() or inherited` — Character
  frames are transparent). On a marker:
  `pop_while(top frame's stamped context ∉ row's context_mask)`, then
  push if `opens_scope`. That single predicate reproduces every
  displacement: `\c` unwinding a note and a paragraph (its
  `[ChapterContent]` forbids both), `\p` closing the previous `\p`,
  `\pb` leaving its paragraph alone, nested `\add`.
- **Dead ends, do not re-derive** (tombstones in schema.rs): a PARENTS
  table and a rank/`precedence()` value were both built and deleted —
  the stamped-context predicate already does their job.
- **Two hand rules** (~10 lines each): `\X*` searches the stack for the
  Note|Character frame with the matching `marker_idx`, pops through;
  `\*`/`\esbe` pops the topmost frame of the kind it closes.
- **Two data-keyed clauses** (predicate arms reading existing columns,
  not marker lists): a `ScopeKind::Sidebar` frame is a POP BARRIER —
  only `\esbe` closes it, a `\c` inside `\esb` stops and lints; an
  incoming `closing == OptionalExplicitUntilNoteEnd` marker first pops
  an open frame of that same class (note peers — `\ft` then `\fq` are
  siblings, and the column marks exactly the 19 real peers).
- **Only rows that DISPLACE run the pop predicate** — a derived boolean
  (`opens_scope.is_some() || kind ∈ {Chapter, Verse}`), not a rank. The
  mask serves legality AND displacement, and they diverge exactly on the
  empty-mask adjacency rows (`ca` must never pop the stack).
- **Milestone spelling overrides the table**: any token the lexer shaped
  as `\name-s/-e` is scope-kind Milestone whatever the row says — that's
  what pairs an unknown `\zaln-s` with its `\*`.
- **Displacement pops are LINT EVENTS**, not silent ("let them know we
  closed the footnote — it's supposed to close explicitly").
- **Unknown/illegal markers recover by popping ALL frames and starting
  fresh** — row 0 is what the walker keys that on (`ScopeKind::Unknown`).
- **`\table-s`/`\list-s` break one stated assumption** (checked against
  U25003 + the shipped ms/list.html, ms/table.html rows, 2026-08-14).
  Lexing needs nothing — they are `-s`/`-e` milestones and their
  attributes are ordinary front-position lists, verified — but:
  `ScopeKind::Table`'s doc says "no marker opens a table, the walker
  SYNTHESIZES the frame when `\tr` arrives," and these markers do open
  one explicitly. So the walker needs BOTH paths and must not nest two
  frames when a `\table-s` is followed by `\tr`. There is also no
  `ScopeKind::List` at all, and both rows currently carry
  `opens_scope: Some(ScopeKind::Milestone)` — decide whether a generic
  milestone frame is enough or lists/tables need their own kinds, and fix
  that doc comment either way. The proposal's closure rule ("a closing
  milestone is required before anything that would otherwise end the
  list/table, e.g. `\p`") is a pop barrier plus a lint event, the same
  shape as the Sidebar barrier; the requirement level is version-keyed
  (optional in 3.2, required in 4), like the trailing-attribute
  deprecation.

### Positional context (the other half of legality)

- The positional band (`Scripture → … → ChapterContent`,
  `SpecContext::is_positional`) is MONOTONIC and needs no new data: the
  ordering is the enum declaration order, the transitions are
  `allowed_contexts`. Rule: stay if current is allowed, else advance to
  the lowest allowed context above current, else it's behind us → lint,
  don't move. Two instructions: `mask & !((1 << (cur+1)) - 1)`, then
  `trailing_zeros()`. No marker "enables" regions — skipping forward is
  what optionality means, so `\id` needs no special case.
- Markers listing TWO positional contexts are the spec saying "I appear
  at two points" (`mt#`, `cl`, `ip`); lowest-above-current resolves all
  three correctly. `cl`'s dual semantics (book-wide label before ch.1,
  that chapter's label after) falls out.
- `ca`/`cp`/`va`/`vp` are NOT context questions: adjacency lint rules of
  shape (lastMarker, token) — see linter.md. Their rows carry an empty
  context slice; the context machine abstains.
- The exact lane encoding (per-token context values vs an event stream)
  and the rules-table shape stay open until the walker exists, then get
  tested against real rules.

## Later

- **Verse designator interpreter** — the spec `VERSE` pattern
  (`/[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[-,][0-9]+[\p{L}\p{Mn}]*)*/`); pure text
  rules, zero table. Writing its doc-comment IS writing the comparison
  rules lint/vref need. The attribute interpreter is its sibling: k/v
  over an `AttrList` span, including the default-attribute binding and
  the comma/colon interior splits.
- **Exports / render surfaces** (from the retired html-elements doc):
  three surfaces only — the marker-keyed `html_element` column, the
  token-KIND-keyed NoteCaller rendering, and SCOPE-DERIVED containers
  (list/table/chapter wrappers are synthesized around scopes, unreachable
  from any row). Heading base levels are an authored aux table; data
  attributes always verbose. USJ types are codegen from Token × the
  table, and MUST use official USJ names (`para`, `char`, `note`, `ms` …)
  — check the USJ JSON schema in usfm-grammar before naming.
- **`\z` custom markers are CONFIG-provided** (markers.ext shape); zero
  behavior unconfigured, which is what row 0 already gives us. Two 3.2
  facts the config shape must handle when it gets designed: attribute
  PATTERNS (`a-*` wildcards, so config attributes are not a plain name
  list) and the `standalone` category (maps onto no-scope + no-payload +
  no-ws — needs no new `Category` variant).
- **Observation (lint finding) shape**: `{ code, anchor token, optional
  second token }` — severity/category/template in a rules table; message
  rendering on the consumer's side. Audit onion's messageParams first.
- **Structure events → editor tree.** Measure the wasm route FIRST before
  building any JS twin. Feature-gate exports when that happens — wasm
  doesn't tree-shake, so combos are separate build artifacts.
- **Perf, what is already known** (rules, not numbers; the numbers live
  at their call sites in scanner.rs and in src/experiments/): stop
  DENSITY is the wall, so speed comes from EMITTING FEWER TOKENS; a new
  scalar loop over hot bytes costs far more than a new stop; the arms
  must stay `#[inline(always)]`; when two scans in sequence want the same
  needle over overlapping bytes, hand the answer forward. Prose full-lex
  already beats its own scan skeleton. Chapter-par only pays on big
  books — a whole-project-open tool, not a default. Criterion when there
  are two real alternatives to compare.
- **Spike candidate: one-load marker path** (2026-08-12, unbuilt). The
  general path walks the same ≤8 bytes three times (`marker_end`,
  `classify_marker`, `resolve_marker_idx`'s stem scan + u64 key build).
  One 8-byte load + one SWAR alnum mask + ctz could feed all three: end
  position, suffix byte (`*`/`-s`/`-e`), and the zero-padded name key
  from a single register. NOT a straight SWAR win — the alnum walk is 1
  iteration for the Zipf-common names, so fixed-cost masking (~12-16 ops)
  loses to the loop there; the win, if any, is DE-DUPLICATING the three
  walks. Spike first, per standing law. See investigate-later.md, which
  notes a `\w` fused arm would overlap this and should come second.

## Parked (do not start)

Editing/session layer (design: ideas/committed/braidv2.md), multi-book
project file format (toc, checksums, caches), diff/merge, publish. Each
gets designed against this base once it holds. Also parked, not lost:
Q-A7/`aid` with U25002; anchors. wasm comes after all of the above.
