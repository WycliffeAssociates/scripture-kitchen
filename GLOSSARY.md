# Glossary — ubiquitous language

One meaning per term, one term per concept, each mapped to at most ONE type
when it becomes code. If a sentence about the domain can't be written in
planning/; the definition records the current soft direction.

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
  markers resolve to an entry in the marker table. A custom (`\z…`) marker
  resolves too once it is REGISTERED — to the template row its `\category`
  behaves as, so a registered `\zfoot` is a footnote with a different name and
  gets every behaviour `\f` has. Unregistered, it lands on row 0 and falls
  back to its span.
- **Extension** — a user `\z` marker the spec's `\category` field classifies.
  A registration is a name plus a category; the engine maps it to a template
  row and then knows nothing else about it. `onion::set_extensions` installs a
  list; `mise::extensions::parse_markers_ext` reads one out of a `markers.ext`.
- **Template row** — one marker-table row per behaviour-bearing `\category`
  word, copied from the spec row that category behaves as. Appended after
  every spec row, unreachable by name from a document, and the ONE thing it
  cannot carry is the name the author spelled (`generated::is_extension`).
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
- **Pad** — the token kind for a delimiter run's REDUCIBLE SURPLUS: every
  horizontal-whitespace code unit past the one a chrome token keeps
  (RFC-Lexer-change-8-27). Visible and editable in the editor (no paint
  stands in for it), dropped whole by text views (mask/vref/USJ/USX/HTML),
  flagged by `delimiter-surplus`, deleted by format. Exists only at
  delimiter positions — whitespace inside a text run is never Pad.

- **Scope** — an open region on the stack: it opens, it takes Content, it
  closes. THE stack mechanism, and the reason a `\p` cannot be swallowed by an
  unclosed `\f`. Type: `ScopeKind`. Columns: `opens_scope`, `closes_scope`.
- **Frame** — one live occurrence of a Scope on the stack. A Scope is the kind;
  a Frame is the instance. Frames carry the Context stamped at push time [I].
- **Context** — a legality region: the answer to "may this marker appear here?"
  `Scripture`, `BookHeaders`, `ChapterContent`, `Para`, `Footnote`, … Type:
  `SpecContext`. Column: `allowed_contexts`.
- **Positional context** — a Context established by document POSITION rather
  than by an open Scope: `BookIdentification → BookHeaders → BookTitles →
  BookIntroduction → ChapterContent`, in sequence, advanced by the markers that
  initialize each. Will, 2026-08-12: *"you know you're in bookheaders sort of by
  knowing you're pre-chapter, and one of these markers initializes that context
  as well."*

  **An empty Scope stack does not mean no Context.** Confirmed by Will on the
  `\q1` case: with nothing open, the legality question is still live — *"are we
  in ChapterContent, i.e. not BookHeaders or something."* This is what makes the
  positional mechanism load-bearing rather than tidy-up. NEXT-STEPS §5 (positional context).
- **Contributes** — the relation between the two: an open Scope contributes a
  Context to its children (`\f` contributes `Footnote`, which is what makes
  `\ft` legal). Derived from `kind` × `category`, never stored:
  `schema::contributes_context`. `None` means the Frame is TRANSPARENT — children
  are judged against the Frame below.
- **Content** — what flows inside an open Scope; in a tree, the children array.
  A marker that takes Content is a **Container**; one that does not is a
  **Point**.
- **Children** — the Scopes and Tokens directly inside one Container's Content.
  Tree-side word only; the Scanner has no children.
- **Point** — a marker that occupies a position and takes no Content: `\c`,
  `\v`, `\pb`, milestones. Q16, ruled 2026-08-12: `\c`/`\v` are Points, so
  paragraphs are their SIBLINGS, not their children. A Point may still DISPLACE
  (see Precedence) — pushing nothing and popping something are independent.
- **Precedence** — how many frames to pop (Will's definition, 2026-08-12).
  Usfmtc calls its integer form `node_depths`. **Precedence is a SCOPE fact and
  has nothing to do with Context** — it is displacement, not legality. The term
  deliberately does NOT commit to being an integer, because the encoding stays a
  mask (`PARENTS`): an integer cannot express "Character may contain Character".
  Implemented as `MarkerRow::precedence()`, baked into the packed row.

  Three things to keep straight:

  1. **Precedence cannot do Context's job.** It is a total order, so it permits
     everything strictly deeper, unconditionally — it cannot say "`\v` may not
     sit directly in a chapter", cannot allow `\add` inside `\add`, and cannot
     tell `\p` from `\qa` (same rank) for the `\v`-forbidden rule. Contexts carry
     information Precedence structurally cannot. usfmtc agrees by construction:
     `node_depths` is referenced exactly ONCE, inside `removeType`, and never in
     `validating/` — legality is a separate grammar pass there.
  2. **Only DISPLACING markers consult it.** A `\nd` just pushes; it is closed
     explicitly, so it never pops a peer. usfmtc's ordinary `char` handler does
     not call `removeType` at all. So "do I displace?" is a per-marker fact —
     and `ClosingBehavior` already almost IS it (`None` → displaces,
     `RequiredExplicit` → nests), read together with kind (`\pb` is `None` but a
     Character, and must not close its paragraph).
  3. **Points have Precedence without opening a Scope.** `\c` pushes nothing yet
     must still pop, so it is a separate fact from `opens_scope`. DONE 2026-08-12
     (item L): `opens_scope` now means PUSHING only, and `precedence()` is two
     arms — `MarkerKind::Chapter`/`Verse` displace at their own rank, everything
     else displaces at whatever it pushes. `\pb` and the milestones get `None`
     for free, which is exactly the bug onion has.

  Precedence must never CONTRADICT Context (if a Para may contain a Character then
  `rank(Para) < rank(Character)`), but it carries strictly less information.
- **Displacement** — closing an open Frame because an incoming marker outranks
  it (see Precedence), with no explicit closer in the source. Always a Lint event, never silent:
  Will, 2026-08-12 — *"let them know we closed the footnote, but it's supposed to
  close explicitly."*

  Displacement is for TREE SANITY, and is not the same as illegality. A marker
  that is merely in the wrong Context but opens nothing (`\cp` outside a
  chapter) has nothing to pop, so it is lint-only. That is the concrete answer to
  NEXT-STEPS §5 for this marker class.
- **Unclosed vs implicitly closed** — an `\f` that never gets `\f*` is
  **unclosed** (a finding anchored at the `\f`); a `\ft` that legitimately ends
  at its note's end is **implicitly closed** (no finding). The row column that
  distinguishes them is `closing: ClosingBehavior`.

## Trees and passes

- **CST** — the lossless Concrete Syntax Tree over the token stream: every byte
  of Source is reachable, Contexts nest, Points sit as siblings. The tree the
  walker builds. "Flat CST" is the same thing before any Context has opened.
- **Linter** — the pass that emits Observations over `Vec<Token>` alone (never
  reaching into live parser state, per [L]). Consumes Displacement events,
  Context legality, and the row's own flags.
- **Scanner** — canonical, defined above. **"Lexer" is a synonym to AVOID** —
  one term per concept, and `scanner.rs` is the module.
- **Formatter** — the pass that emits `Edit`s rather than Observations, over
  the same rows table: `format_edits`. Opt-in, and the ONE pass allowed to
  mutate, delete and invent bytes.
- **Form channel** — the rows the formatter alone evaluates, marked
  `Severity::Form`. Never a diagnostic: the Linter does not reach them. Rows
  that are BOTH (a real finding whose fix is also a formatting action) are
  **dual citizens** and carry `LintRow::formatter` instead.

## Naming collisions — RULED 2026-08-12

- **Walker** is the name for the structural loop (frame stack, push/pop,
  displacement, legality) — "the tree the walker builds", the thing lint
  listens to. **"Driver" is a synonym to AVOID for it**: this glossary
  already uses "driver" for the engine-host sense ("drivers may vary, the
  engine never forks"), and one word may not mean both.
- **`common_marker_checks`** is the name (NOT `marker_fast_paths`). The fused
  hot-marker checks, NEXT-STEPS step 4.4.
- **Designator** is the name for the `\c`/`\v` payload — the glossary already had
  the word ("Verse designator", "Chapter designator", below), so
  `Payload::NumberRange` becomes `Payload::Designator` and the step-4.3
  `TokenKind` is `Designator`. Neither `NumberRange` nor `NumberKind` survives:
  one term per concept, and the column and the TokenKind must not disagree.

  The spec pattern the INTERPRETER implements, verbatim (spec name `VERSE`,
  "Verse number, including ranges and sequences"):

  ```
  /[1-9][0-9]*[\p{L}\p{Mn}]*(‏?[\-,][0-9]+[\p{L}\p{Mn}]*)*/
  ```

  It belongs to the designator interpreter, never to the Scanner: the Scanner
  emits ONE `Designator` token spanning the whole thing and never looks inside.
  Its fast path recognizes only the pure-digit happy shape and falls back for
  suffixes, ranges, sequences, the U+200F RLM, and junk.

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
