# Glossary — ubiquitous language

One meaning per term, one term per concept, each mapped to at most ONE type
when it becomes code. If a sentence about the domain can't be written in
these terms, the glossary is missing an entry — add it here BEFORE adding a
type. Terms marked `(pending Qn)` have a live design question in
QUESTIONS.md; the definition records the current soft direction.

## The material

- **Source** — the exact bytes of a USFM document. Never normalized, never
  repaired on ingest. Every other artifact is an index over, or a derivation
  from, Source.
- **Span** — a byte range into Source (`start..end`). Text is always
  source-backed: materializing a token's text is a slice, never a copy of a
  stored string.
- **Document** — one USFM source (conventionally one book). Its book identity
  is whatever its `\id` declares — including invalid, overlong, or missing
  codes, which are real revision-state data, kept verbatim as a per-document
  fact (stated once, never repeated per-address).
- **Book code** — the `\id` declaration. The spec has a fixed list; the
  domain does not. A code outside the list is data, not an error.

## Lexing

- **Marker** — a USFM control word (`\v`, `\p`, `\w`, `\zaln-s`). Spec
  markers resolve to an entry in the marker table; custom (`\z...`) markers
  don't, and fall back to their span.
- **Marker table** — THE single registry mapping marker → semantics:
  structural role (paragraph/character/note/milestone), nesting behavior,
  payload grammar, render class. Defined once in Rust, generated for JS —
  never hand-mirrored.
- **Scanner** — the cursor-owning layer: finds token boundaries in Source,
  emits `(kind, span)` events. The only code that owns position.
- **Interpreter** (a.k.a. combinator) — a pure, position-free function from
  a slice to a structured payload (`attrList(&src[span])`,
  `verseDesignator(&src[span])`). Callable on any bytes from anywhere;
  requires no cursor and no surrounding state.
- **Token** — the smallest lexed unit and the ONE working representation:
  `{ id, kind, markerIdx?, span }`. Everything richer is a derivation.
  (pending Q0: the exact eager field list — each added field is twin tax.)

## Identity and location (the four jobs — never conflated)

- **Identity (Job 1)** — "which token is this?" An opaque per-token id:
  caller-honored when supplied, positional by default. Used by patches,
  finding anchors, undo, DOM ids. The ONLY stored per-token identity.
  Carries no meaning; never parsed.
- **Address (Job 2)** — "where in scripture is this?" for humans: book +
  chapter + verse designator + occurrence. Consumed by lint messages and
  navigation. (pending Q1: stored fact vs derived view — soft direction:
  DERIVED, never stored on tokens, never crossing a boundary as a token
  field.)
- **Alignment (Job 3)** — "is this the same section in both documents?"
  for diff/merge pairing. Requires a derivation that is deterministic and
  identical on both sides, and an occurrence rule for duplicates. Alignment
  keys are internal values, not token fields.
- **Partition (Job 4)** — "which verse/chapter do these tokens belong to?"
  for vref extraction and scoped slicing. A sticky assignment derived from
  marker structure; an index, not a token field.

## Address components

- **Verse designator** — the text after `\v`: TEXT with a conventional
  numeric interpretation (number, range `1-2`, suffix `1a`, or junk).
  (pending Q2: the interpretation rules — numeric fields where parseable +
  an exactness flag where not. Exactness is domain-real, not an
  implementation artifact.)
- **Chapter designator** — the text after `\c`, same character as verse
  designator (numeric interpretation + exactness), plus the reopened-chapter
  reality.
- **Occurrence** — the positional ordinal distinguishing duplicate addresses
  (two `\v 14` in one chapter, one chapter label opened twice). Domain-real:
  messy text under revision legitimately contains duplicates. Falls out of
  the derivation for free; only hard if addresses are stored. (pending Q3:
  the exact counting rule — soft direction: onion's `derive_canonical_sids`
  rule, promoted to THE definition.)

## Derivation and the engine

- **Derivation** — any fact computed from Source + Tokens: addresses,
  numeric interpretations, attribute structure, lint findings, diffs.
  Derivations are owned by the engine, computed on demand, and are never
  promoted into stored format (see Cache for the one exception).
- **Engine** — the single Rust implementation of all derivations and
  operations (interpret, lint, diff, search, publish). One implementation
  per behavior; drivers may vary, the engine never forks.
- **Index** — a cached derivation offering lookup (address → token range,
  verse → tokens). How navigation works: `index.locate("GEN 3:16")`, never
  `tokens.find(...)`.

## Representation and lanes

- **Columns** — the binary form: parallel arrays of the Token fields, an
  INDEX over Source, not a replacement for it. Ships as `(source, columns)`.
  Exists for deferred object creation, not compression. A derivable fact
  promoted into a column is the sidecar disease.
- **Eager Token / lazy payload** — the read lane materializes eager fields
  into objects; payload interiors (attribute lists, designator numerics)
  stay as spans until asked, then go through an interpreter. The eager field
  list is the twin-tax boundary.
- **Read lane** — the complete JS materialization path:
  `materializeBinJs(bin, source) → Token[]` + the interior interpreters, all
  JS, no per-call dispatch to wasm. Proven equivalent to the Rust lane by an
  oracle gate (byte-level, whole corpus). The only sanctioned dual
  implementation; its size is bounded by the eager field list.
- **Engine lane** — operations that never materialize caller objects (lint,
  diff, search, publish). Runs in Rust/wasm over columns.
- **Cache** — the one legitimate ride-along beyond columns: a stamped,
  refusable, RE-DERIVABLE result of an expensive derivation (e.g. findings).
  Test for column candidacy: "stamped cache of expensive derivation" is
  allowed in its own section; "derivable fact sneaking into the format" is
  the disease.

## Invariants

- **Lossless invariant** — Source bytes are preserved exactly; the token
  stream is an index over them, so round-trip is identity by construction.
  Lossy projections (HTML, USJ, vref) are explicitly named as lossy.
- **Refuse, never invent** — input that cannot be represented is refused
  with a precise reason; it is never repaired, defaulted, or silently
  normalized. (Boundary-tolerance exceptions — e.g. nullish-means-absent —
  are documented contracts, decided once.)
