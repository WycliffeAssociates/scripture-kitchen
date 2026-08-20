# Positional-context lane (lint leftover — not yet on the roadmap)

Design preserved verbatim from NEXT-STEPS when it slimmed (2026-08-20).
Deferred from cst::build to lint; never built in lint phases 1-4. The
lane judges the MONOTONIC positional band; it is not the walker's
displacement mask (that is container contexts, already live).

- The positional band (`Scripture → … → ChapterContent`) is MONOTONIC,
  no new data: ordering = enum declaration order, transitions =
  `allowed_contexts`. Stay if current allowed, else advance to the
  lowest allowed context above current, else it's behind us → lint,
  don't move. Two instructions: `mask & !((1 << (cur+1)) - 1)`,
  `trailing_zeros()`. No marker "enables" regions; `\id` needs no
  special case.
- Markers listing two positional contexts (`mt#`, `cl`, `ip`) resolve
  by lowest-above-current; `cl`'s dual semantics falls out.
- `ca`/`cp`/`va`/`vp` are adjacency lint rules, not context questions —
  this POSITIONAL lane abstains on all four. (2026-08-19 amendment:
  `ca`/`va`/`vp` carry the character class's CONTAINER contexts because
  the walker's pop predicate reads the mask — that is displacement, not
  this lane; only `cp` still has an empty slice.)
- The lane's exact encoding (per-token sidecar vs stamped on nodes) and
  the rules-table shape get tested against real rules when built. Node
  has spare layout room.
- Likely landing spot: a fifth lint machine (or a Flat extension)
  carrying one u8 of band state, emitting an out-of-band code — but
  that is unruled; decide at build time against real rules.
