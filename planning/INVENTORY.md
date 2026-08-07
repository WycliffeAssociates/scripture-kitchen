# Inventory — onion's lexeme/token kinds → where each fact lives in the new shape

Source of truth audited: `usfm_onion/src/lexer.rs` (RawTokenKind, lex modes)
and `usfm_onion/src/token.rs` (TokenKind, TokenData payloads), 2026-08-07.
"New home" reflects the current direction (QUESTIONS.md), not law.

## Onion's lexer-level kinds (RawTokenKind, logos-generated)

| onion raw kind | what it was | new home |
|---|---|---|
| `Whitespace` | space/tab runs; absorbed as marker delimiter post-lex or folded to Text | same fold, but per marker CLASS from the table (not unconditional); never a public kind |
| `Newline` | `\r?\n` / `\r` | row kind `Newline`, unchanged |
| `OptBreak` | `//` | row kind `OptBreak`, unchanged |
| `Pipe` | `\|` — consumed by the lexer's attr-run mode | no public kind; fused stack opens an `AttrList` row when an attr-bearing marker is open; bare pipe in text = content |
| `MilestoneEnd` | `\*` | row kind, unchanged |
| `NestedClosingMarker` / `NestedMarker` | `\+w`, `\+w*` | OPEN (Q6): testbed has these as kind subtypes; onion made `nested` a bool FIELD on Marker/EndMarker instead. Pick one — subtypes cost kind values, field costs a bit |
| `ClosingMarker` | `\w*`, `\it*` | row kind `ClosingMarker` (onion: `EndMarker`) |
| `Marker` | `\p`, `\v`, `\zaln-s` | row kind `Marker` + `markerIdx` |
| `Text` | everything else | row kind `Text`, unchanged |

Lexer mode state onion already used (precedent for classify-with-mode):
- `in_attribute_run` — attrs lexed inside the lexer. Same mechanism as the
  proposed fused stack, just book-keeping a bool instead of a stack.
- `PendingPayload::BookCode` — lexeme after `\id` promoted to BookCode.
- `PendingPayload::NumberRange` — lexeme after `\v`/`\c` promoted to Number.

## Onion's public TokenKind + TokenData payloads

| onion kind | payload fields | new home for each fact |
|---|---|---|
| `Marker` | `name` | `markerIdx` into the table spine; custom `\z*` = escape value + read the span |
| | `metadata.canonical` | table column (was a PER-TOKEN COPY of static table data — the sidecar disease in memory; don't repeat) |
| | `metadata.kind` (MarkerDefKind) | table column (role class) |
| | `metadata.family` | table column |
| | `metadata.index` (WS2B perf handle) | this IS `markerIdx` — onion retrofitted it late; the new shape makes it the primary key from day one |
| | `structural` (StructuralMarkerInfo) | derived by the driver per occurrence (verify what it holds before porting anything) |
| | `nested: bool` | Q6: field vs kind-subtype, see above |
| | `attrs: Option<Box<MarkerAttrs>>` | lexically: own `AttrList` row (partition). Object-space: attached to the owning marker's owned Token during `to_tokens`, via the `attrList` interpreter |
| `EndMarker` | name/metadata/structural/nested | same as Marker minus attrs |
| `Milestone` | name/metadata/structural + attrs | same treatment; standalone in the driver (never a stack entry) |
| `MilestoneEnd` | — | row kind |
| `BookCode` | `code`, `is_valid` | header book-fact SLICE (first `\id`); `is_valid` = derived (spec-list lookup → lint), never stored |
| `Number` | `start`, `end?`, `kind` (NumberRangeKind) | designator interpreter output (Q2) — computed on comparison demand, never stored |
| `Text` | — | row kind |
| `Newline` / `OptBreak` | — | row kinds |

## Onion's per-token envelope (Token struct)

| field | new home |
|---|---|
| `id: TokenId` | positional (`bookcode-rowIdx`) for immutable snapshots; session-minted once live; id column only when caller-supplied |
| `sid: Option<Sid>` | GONE from tokens — address is derived (Q1); outputs only |
| `span` | `start u32 (slot-relative) · len u16` |
| `source: &str` | the slot's slice of Source — tokens never carry text |

## Attribute payload detail (AttributeItem / MarkerAttrs)

| onion fact | new home |
|---|---|
| `key`, `value`, `is_default` per item | `attrList` interpreter output (payload value, not a stream type) |
| default-attr expansion (`marker_default_attribute`) | marker-table column, consulted by the interpreter (the ONE table fact any interpreter needs) |
| `attribute_source` (verbatim `\|...` bytes) | the AttrList row's span IS the verbatim — free under partition |
| `attribute_offset` (placement memory) | dissolves: placement is the row's position in the stream — partition keeps it |

## Observations from the audit

1. Onion's late perf fixes (MarkerIndex handle, delimiter-absorption via
   dense index) are the new design's day-one defaults — the retrofits
   point at the right primitives.
2. Onion's lexer already ran mode-state classification (attr runs, pending
   payloads). The fused-pass direction is a generalization of what worked,
   not a new bet.
3. The per-token `metadata` block is the strongest argument for
   `markerIdx`-only tokens: canonical/kind/family were copied onto every
   marker token from static data, and serialized outward too.
4. `attribute_offset` — onion's two-fact attribute round-trip scar —
   simply doesn't exist under partition: the AttrList row sits where the
   bytes sat.
