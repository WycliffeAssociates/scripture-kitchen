# Open Questions / Design Log

Running log of gnarly design questions hit while building this lexer, so they
don't get lost or re-litigated from scratch. Newest entries at the top.
Status is `open` (still deciding) or `resolved` (decision made, with the why).

Ubiquitous language lives in GLOSSARY.md — questions here are written in its
terms, and resolving a question updates the glossary entry it pends on.

---

## [direction] E — Statefulness and granularity (the session/slot model)

### First response

**The problem this answers (2026-08-07):** retained state. Lazy
materialization + byte spans means one keystroke shifts every downstream
offset; onion's answer (eager owned tokens + a braid-in-a-worker synced by
MutationEffect) traded that for two stores negotiating truth across a thread
boundary, which was its own pain. Also: sids are addresses, so they must
never be the keys a stateful system updates by — something else has to be.

**The model (leaning — the only sane one found so far):**

- **Chapters are SLOTS, not numbers.** A book session is a list of slots in
  source order — `[slot, slot, slot]` — where a slot is one `\c` run (plus
  slot 0 for front matter). The chapter LABEL is display data derived from
  the slot's own tokens; duplicates are just two slots whose labels match
  (UI may ordinal them). Slots get session-minted ids, because slots split
  (typing a new `\c` mid-slot) and merge (deleting one) — array position is
  too fragile to be the write-back key.
- **Spans are slot-relative.** Each slot's base offset lives in the run
  table. A keystroke in slot 3 re-scans slot 3 only (tens of µs); every
  other slot's columns are byte-identical; base offsets adjust lazily.
  Nothing downstream holds absolute offsets — findings anchor to
  (slot, row); absolute positions exist only at serialization
  (`concat(slots)` = the file, lossless under partition).
- **The library IS the working-files store** (this is what braid should
  have been): the editor pushes edits in — `updateSlot(slotId, text or
  tokens)` — and re-renders that slot from what comes back. The lib owns
  checksums, dirty flags, serialization, retained lint state. The editor
  owns pixels. One store, no thread boundary, no mirror.
- **Incremental lint is a fold over slots.** Most rules are slot-local.
  The cross-chapter residue (duplicate chapter labels, milestone pairing)
  is small carried state — and per the spec rail, chapter content starts
  its own element collection, so NO block state legitimately crosses `\c`;
  onion's "verse inherits the open block" was a messy-text tolerance
  (allowImplicitChapterContentVerse), living inside one optional rule, not
  in the design's spine. Each slot's
  lint = (incoming state) → findings + (outgoing state); edit a slot →
  recompute it → propagate downstream ONLY if its outgoing state changed
  (it almost never does). Same math as the parallel-stitch recipe, reused
  for incrementality. Book-level tallies (chapter-number counts) are the
  easy degenerate case.
- **Main thread is plausible again**: per-keystroke work = rescan one slot
  + relint one slot + refold. Sync wasm calls (colorless functions — the
  editor has real scars from mutating across async boundaries) IF a latency
  measurement says it holds; the worker was only ever needed because
  whole-book relint was 100ms+.
- **Braid becomes an opt-in high layer over a stateless core.** The
  stateless path (send usfm/tokens, get results) remains the always-works
  fallback AND the oracle the stateful layer is checked against — scan/
  interpret/lint of a slot are the same pure functions in both.
- **The stateless core already starts here:** `scan(source) -> (slots,
  header)` — the run table IS the slot list; the 200-line parser's next
  shape emits per-slot columns with slot-relative spans from day one.
  `to_tokens(slot)` does the allocation; the future braid layer retains the
  same slots and re-scans one on edit. Same types, no migration.

**This layer's own Q1 (open, the real remaining architecture decision):
who owns a live slot's text?** (a) the lib owns it — Lexical edits are
transactions pushed in, editor re-renders from returned tokens; single
source of truth, every keystroke round-trips. (b) Lexical owns live text;
the lib's slot is a trailing index synced on idle — no keystroke coupling,
but a small two-truths window (onion did (b) at book grain across a worker
and it hurt; (a) at slot grain on main thread may be cheap enough to be
right). This is a latency measurement, not an argument.

**Also open here:** payload-values inventory (what `to_tokens` attaches to
owned tokens vs leaves as spans): designators, attr lists (+ per-marker
default-attr resolution), book code, note callers, milestone sid/eid,
legacy `\fig` payloads. Attr lists are their own ROW lexically (partition,
render-sufficient kinds) but ATTACH to their owning marker in object space
during materialization — the driver knows which attr-bearing marker is
open.

### Second response — editor integration (2026-08-07, leanings)

- **The document lives in the STORE; Lexical is a disposable lens.** This is
  the inversion that makes windowing a rendering parameter instead of a
  data-model commitment: today's one-chapter editor is window=1; a sliding
  3-chapter window and whole-book-with-collapsed-placeholders render the
  same store. Ship N=1 now, choose scroll mechanics later. Protect the
  inversion, not the window.
- **Chapter = Lexical container node; the tree IS the map.** Every node
  sits inside a ChapterNode (front matter = slot 0's container). Which
  slot = ancestor lookup; membership is containment; the only mapping is
  container id → slot id (one entry per chapter). Node keys are Lexical
  internals — anything persistent uses our minted ids.
- **One-way data flow kills the jerkiness question.** Keystrokes never
  create/destroy containers — they edit text → `updateSlot(slotId,
  serialized)` → store re-scans → returns deltas → editor RECONCILES
  containers to the store's slot list (create/remove/split/merge),
  React-style. Typing `\c 24` mid-chapter honestly pops a new chapter —
  soften presentation, never the mechanics.
- **Attribution is a hint, never correctness.** Resolution ladder: node in
  container → that slot; ambiguous (cross-boundary paste) → anchor/focus
  bracket both slots → re-scan both; lost → re-scan the book. Every rung
  degrades to "re-scan wider," which is why none of it is delicate. Bytes
  determine position, always; containers are performance furniture.
- **Deleted vs emptied falls out of bytes**: `\c` line gone → slot gone
  (retire id); `\c` survives childless → empty slot (lint's business).
  Slot ids are EPHEMERAL granularity handles — nothing durable keys on
  them; undo resurrecting a chapter may mint a fresh id, reconciliation
  copes.
- **Undo/redo is store-level** (already true in the editor for domain
  reasons — Lexical history never matched what counts as history). The
  slot model gives the existing stack its natural entry:
  (slotId, before, after); undo is just another updateSlot.
- **The updateSlot effect shape is small and closed**: re-scanned tokens
  for touched slot(s) + findings delta + slot-list delta
  (split/merged/created/removed). That is the entire sync surface —
  contrast MutationEffect's book-grain sprawl.
- **Slot API sketch**: `updateSlot(slotId, text|tokens)`,
  `moveSlots([ids], beforeId)` (token-space free — slot-relative spans
  untouched; serialization order + lint fold re-chain), deletion =
  updateSlot with empty text / container removal.
- **Bounds**: the BOOK is the rescan bound and the scroll bound — virtual
  scroll across chapters within a book, never across books (matches
  USFM's file grain, the session grain, and the scan unit).
- **Standing oracle for the whole layer**: after any edit sequence
  (updates/splits/merges/reorders), `serialize(slots)` re-scanned from
  scratch must equal retained state — tokens, slots, findings.
  Retained-vs-fresh equivalence over edit-sequence fixtures; fast stays
  honest against correct.

---

## [direction] Where we've landed so far (2026-08-06 discussion — leanings, not law)

Plain-language summary of the working direction. Everything here is
"evaluating, current best answer" unless marked resolved.

**The core shape.** Store what the scanner discovered while finding
boundaries; derive the rest. Token = one compact 8-byte row
(`start u32 · len u16 · kind u8 · markerIdx u8`). Text is always a slice of
the source — the binary is an index over the source, never a replacement;
`(source, columns)` travel together. Onion's sidecars (attribute rows,
number records, book-code records, sid dictionaries) were derivable facts
promoted into stored format — each grew the format AND the reader. Don't
repeat that.

**Token is the first cacheable layer** — delimiter boundaries plus
span-to-kind. Everything above it is a function of (stream order, marker
table data). Stream facts live in pass state; table facts live in columns;
nothing lives in a third place.

**One fused pass** (this is the onion "single-pass engine" idea made real):
scan + walk run as one streaming pass over the source, emitting token
columns + header (book-code slice, chapter run table) + optional
events/diagnostics for listeners (editor tree, lint). The walker pattern —
same as onion's walker, but its knowledge moves into table columns and its
event stream becomes the public interface.

**Kinds must be enough to render from.** A consumer reading only `kind`
never needs USFM trivia. So: delimiter whitespace folds into the marker span
(per marker CLASS, from the table — not unconditional; closing markers'
trailing space is content), and an attribute list becomes ONE AttrList token
once the pass's stack exists (a bare pipe in plain text stays content).

**Addresses (the old "sid") are derived, never stored** — see resolved-ish
Q1 below. Callers never supply addresses; they are outputs only.

**Ids are for a lifecycle, not cold storage.** Positional
(`bookcode-rowIdx`) for immutable snapshots — zero bytes stored, exact ids
for single-chapter materialization via run-table arithmetic. Session-owned
ids once a book is live in an editor. An id column exists only when
caller-supplied ids need persisting.

**Lint finding (Observation) shape**: `{code u16, anchor u32, aux? u32}` —
severity/category/template in a generated rules table; message rendering
and localization on the consumer's side, derived from the anchor. Onion's
messageParams (strings assembled in Rust) was the wrong choice — audit it
before deleting to confirm nothing non-derivable is lost.

**Dual implementation (JS)**: only where JS-scale demands it, bounded, and
proven by a byte-level oracle (onion's packed-equivalence pattern worked).
Tables/schema are GENERATED for JS, never hand-written; only behavior may be
twinned, and the twin must never contain a marker name (greppable tripwire).
Measure the wasm route FIRST — consumer-sufficient kinds may have shrunk
what JS even needs to know.

---

## Open queue (in rough order)

- **Q1 — address: stored fact or derived view? LEANING STRONGLY DERIVED**
  (not final). Nothing address-shaped per token. Book = first-`\id` rule
  (later `\id`s are lint findings, bytes kept — never truncate, it's a
  slice). Chapter = structural (run table in the header). Verse =
  slice-local within a run (occurrence counters reset per `\c`; `\c` closes
  everything and milestones are standalone, so chapter starts are clean
  states — single-chapter walks need no incoming state). Duplicates =
  positional ordinal in the DERIVED value only (never typed, never stored,
  never displayed by default; plural lookups return sets like search
  results). Display always shows the typed bytes.
- **Q2 — designator comparison semantics.** Display never interprets
  designators (typed bytes shown verbatim). But lint ("verse out of
  order"), vref ("does 15-17 contain 16?"), and diff pairing must COMPARE
  them. One function, defined once:
  `designator text → (start, end?, suffix?, exact)` + ordering/containment
  rules — what's junk, does `13a` sort inside or after `13`, does a range
  contain its endpoints. Small, contained, unwritten. Lands as the verse
  designator interpreter's doc-comment (NEXT-STEPS "later").
- **Q3 — occurrence counting rule** for duplicates (soft: onion's
  `derive_canonical_sids` counting, promoted to THE single definition).
- **Q4 — the one output address spelling.** Addresses are outputs only, so
  exactly one canonical string spelling exists (findings' human address, nav
  labels, merge-decision keys shown to UI) — defined once, second spellings
  never minted. This is the exact door String sids re-entered through in
  onion. Related: comparing/ordering sids for consumers is Q2's function,
  not a new spelling.
- **Q5 — partition (leaning yes, oracle pending).** Does the token stream
  cover every source byte exactly once, in order? If yes, lossless =
  `concat(spans)` by construction. The standing test in NEXT-STEPS step 2
  answers this.
- **Q6 — granularity cuts log.** Onion's cuts by default (marker and number
  separate — wysi shows them separately). Log each cut as it's made:
  - delimiter ws folds into the marker span — per marker class via table
    (correcting the current unconditional hand-rolled version).
  - AttrList as one token via the fused pass's stack (leaning).
  - kind carries shape subtypes (Closing/Nested variants) rather than
    flat-kind+flags — deliberate for now.
- **Q7 — structure events → editor tree: what does JS actually need?**
  The knowledge is table columns (role, closure, closes-on); the residual
  algorithm is a small table-driven stack machine fused into the scan,
  emitting balanced open/close/text events with recovery (unclosed `\f`
  force-closed at `\p`/`\c` with a diagnostic, never a throw) — the
  consumer just pushes/pops, zero lookups. Open: whether JS needs a twin of
  that small driver at all (measure wasm first), and if so, twin vs a
  stored depth-u8/parent-u32 cache column (re-derivable, stale on edit).
  Per-token bit flags (nested?/closed?) are OFF the table — insufficient to
  pair opens with closes.
- **Q8 — marker table compile shape.** Plain static array first; codegen /
  perfect-hash only if measurement says lookup is hot (onion's
  double-HashMap-per-marker `fast_lookup` is the known smell). Every column
  added to the table is an `if` deleted from some driver.

---

## Parked — onion scar tissue, not yet reached here

One-liners so the lesson isn't lost; expand only when the topic becomes
live in THIS repo. (Full write-ups exist in onion's plans/ and the old
braid ledger.)

- **Boundary crossing (wasm/JS)**: the boundary was always the cost, never
  the parse. Bytes cross as Uint8Array + offsets; maps stay plain objects;
  nullish means absent; pin shapes with runtime gates. Becomes live when
  this repo grows a wasm boundary.
- **Generated .d.ts coherence**: nothing reads the generated declarations
  as an artifact — duplicate exports / lying types shipped three times in
  one day. Becomes live with the first generated d.ts.
- **Content identity**: one equality definition (source + tokens + line
  ending); every cache/no-op predicate calls THE one — hand-rolled
  predicates were wrong 3 of 4 times. Becomes live with the first cache.
- **Cache invalidation currency**: crate version (blunt, safe) vs semantic
  rules version. Decide before the first shipped cache.
- **Produce/consume pairs ship together**: best version — the producer's
  output type IS the consumer's input type verbatim. Becomes live with the
  first publish/restore pair.
- **Save-point (baseline) semantics**: name the state slots first, derive
  the verbs from the slots; whole-book atomic; ambiguity refused. Becomes
  live with the session/edit layer.
- **Errors at the boundary**: native errors carry typed facts; boundary
  mirrors are mechanical string projections; never both derive the boundary
  macro. Live with the wasm boundary.
- **Chapter-parallel needs an explicit stitch** — though note the new
  design weakens this: `\c` closes everything structurally, so structure is
  chapter-clean; only cross-chapter LINT state (block-supports-verse) still
  needs the stitch. Becomes live with any parallel lane.
- **Lossless edges**: attributes needed TWO facts to round-trip in onion
  (verbatim bytes AND placement); unclosed-note repair belongs to lossy
  exports only. Partially live already via Q5/partition; the attribute
  placement lesson becomes live with the attr interpreter.

---

## Scratch (raw notes)

## Lossless invariant is most important
- attributes are hard. chars with explicit/implicit closing. Notes act differently.
## Binary (object creation is expensive and the editor deals in objects (Lexical))
## Sids - fiedlity, u64,


# Statefulness and granularity
 ## Rambling prompt
Ok, so usfm is a per book file format, but even if we keep things per book, I realized a bit of an issue with how braid had been desinged likely. 

First fact: 
usfm is a per book file format

Second (not fact but sure):
nothing we designed to today is incompatible with still primarily working in a "token" space for the wysi editor that is chpater per time. 

Succiently put the problem is with retained state. 

Example A. 
We've discussed and editor using a columnar format / lazy object materialization (wasm or js twin file) to product Tokens with actual Sids. (and I'm guessing, I guess there should technially be a 3rd type? Lexeme? Token? And?? Cause I don't think our operator arms we've discussed are going to just output the exact same things as Token, else you don't need them at all, which might be, IF, you did granularize tokens more, but not sure. I guess kind attrbitues are really teh main ones you might not want to leave as string parsing to end user, but that's a digression atm). 

The point is with retained state, we've disucssed materializing lazily.   Well, Let's say we show the first 200 lines only. Someone types, in that file.  Now the byte boundaries have shifted entierly and would need full recalc. You can't just how what's in viewport withotu recalculating. Which is fast in rust and wasm both, but like, we're not the only subsytem wanting to do work. You could I guess diff the update and then just adjust the start/end for that subsytem? This is why eager materialization fo tokens didn't have that problem. Unless you go chapter relative in a token, but that feels, probably finicky as well? Same for lintFindings, if they are just pointing at spans, a type means you have to do offset math for that chapter to everything downstream of it, which is maybe fast? 

And speaking of retained state, I severly underestiamte the awkwardness of onions mutationEffect sort of api to sync the 2 and then coordiante the 2 bc one is in a webworker.   I suppose the only pattern more natural would have been to wrap braid itself as the workignFiles store and use its intenrals as the mutable ref for editing/changing usfm inside the main thread.

But yeah, retaining state could buy you potentially enough speed to live on the main thread.  Model things in a such a way as you can pass in a single chapter to relint (i.e. most lint errs a property of a chapter, and what's not is retained via state for a book such as chapNums mapped to spans, (duplicate chaps) or chapter label inconsistency (when a chapter updates, you just bumpt tallies)) For milestone pairing, just trackign if any closed before etc;.   For fresh parse/lex, book level usfm Speaking of, how do you expose and update the itnernals of retrained state in braid was just, yeah, I didn't see these complexities coing.

Mostly meaning, we've said sids are for addressing. Meaning, we we shouldn't use them in any braid like system to update stuff.  And I know the simplest answer to start is you always just materialize all the tokens eagerly and call lint (but don't want to have to trigger manually, it's an editor), and run fresh, but tht'at slwo. Either slow in one main ui thread that blocks for 200ms of wasm, ro slow on transport.  

I gues you could actually think of chapters not by number (which is the display), but by positional idx after parse?

I.e. 
\c 1 
\v 1 thing
\v 2 thing 2
\c 2 \v 1 thing 3
\v 2 thing 4
\c 1 \v 1 thing 5 duplicate chpater, 
\v 2 thing 6


And we handle duplicates by saying that chapters aren't ordinal (and sending orindals feels fragile / weird), but rather just hunks or slots for a workign session. I.e. above becomes. 

[[first c1 slot of tokens], [second one], [duplicate one]];

If you're nto trying to coordinate state, ui renders a list taht is a pointer into that slice (idk how you'd do somethign like a single file virtualized scroll);
So ui renderes
[Chapter your C number 1]
[Chapter your C number 2]
[Chapter your C number 1 (maybe a ui ordinal?)]

And when you click, you know which array idx we're writing tokens back into.  

Idk. I realize none of this is 100% needed bc you can just have a stateless library like onion was, and send / receive usfm or tokens and be done.   But that still leaves some coordination in the client that I just theoretically would love not to have to do. 

A dumb client that can just say, oh heres a new chunk of usfm, or a new chunk of the Token contract for this section, 

And the lib handles checksumming, dirty file indicators, serialization, etc;  There's several questions here, and I realize that it's not 100% needed, but I wanted to build braid for a reason to thin our the editor for sure, but mirroring the Tokens, main thread, handling duplicate chpaters for updates if we are mirroring, lazy init of objects.. Not sure what to do


## First response
