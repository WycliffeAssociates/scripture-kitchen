# Lint closeout (next after the attr interpreter — one window, lint owes nothing after)

The collector sketch: every owed lint item in one place. Two of the
four have their own detail docs (referenced, not duplicated); the two
attribute rules are fully specified HERE because they had no sketch of
their own (they lived as lines in lint-sketch.md's code list). After
this window the lint subsystem is CLOSED — no deferred rules, no
parked lanes, no open audits.

## 1. `attr-unknown-name` (new code, Attributes, Hint)

For each AttrList: owner = nearest preceding opener (the machinery
Flat already has); for each `Attr` from `attributes::attrs()`, run
`attributes::resolve(name, owner_row)`:

- `Defined(_)` → silent. `UserNamespace` (x-/z-) → silent (legal on
  any character marker — en_ult's ~1M zaln/w lists must stay quiet).
- `Unknown` → finding. Anchor = the AttrList TOKEN (see "sub-token
  anchoring" below). `second` = owner token.
- Bare default form (`name == ""`) resolves through
  `row.default_attribute`; None + bare value = this code too? NO —
  that is a different fact ("this marker has no default attribute"),
  same code family though: fold it in as the same code with aux
  discriminating, or a sibling `attr-no-default`? PROPOSED: same code,
  aux = 1 for the bare-with-no-default case (AuxKind::Count reads as a
  flag), template covers both. React if that feels muddy.
- A `Malformed` event → `attr-malformed` (one NEW code, Warning —
  the interpreter's UnterminatedQuote/EmptyName/MissingValue/BareJunk;
  aux = MalformedAttr discriminant, AuxKind gains a variant or reuses
  Count). One finding per broken tail by construction (iteration ends).

## 2. `attr-required-if` (new code, Attributes, Warning)

The conditional cardinality the rows cannot express (rows say
Optional; lint owns the required-if):

- `eid` required IF `sid` was used — per MILESTONE ELEMENT: when a
  milestone node's list carries `sid`, the matching end-point's list
  must carry `eid`. v1 SIMPLIFICATION (pairing is unmodeled in the
  CST, deliberately): check per POINT — a `-s`/start point with `sid`
  is fine alone; an `-e` point missing `eid` when its row defines it
  AND the book used `sid` on that family… that is pairing by another
  name. PROPOSED v1: the honest per-token rule only — an `-e`-spelled
  point whose row defines `eid` but whose list lacks it, when the SAME
  row appeared earlier with `sid` (one per-row bit, like
  numbering-mix's bitmask). No cross-referencing which sid matches
  which eid — that is vref/pairing territory, later.
- `ta`'s "one or more of the family" — the row's family-cardinality
  fact; fires when a `\ta` list has none of its family's attributes.
- Anchor = the AttrList token (or the milestone token when there is
  no list at all); second = the earlier sid-carrying token where one
  exists.

## 3. Version family — detail: sketches/version-lint.md

`VersionRow { marker, deprecated_in, removed_in, replacement }`
authored in lint/rows.rs (never a MarkerRow column, per the law);
seed = the five `deprecated: true` rows (addpn, fdc, ph, pro, xdc);
gated on declared `\usfm` exactly like attr-trailing-form (no
declaration → silent); fix = mechanical marker rename where
`replacement` exists (opener AND closer — two edits, one fix).

## 4. Positional-context lane — detail: sketches/positional-context.md

RULED KEEP (2026-08-20). The judge for the mask's positional half
(today: zero consumers — `\mt`/`\ip`/`\toc1` after `\c 1` lint clean).
One u8 of band state, the two-instruction transition, one new code
(`out-of-band` / name TBD at build). Landing spot: fifth pocket
machine or a Flat extension — decide at build against real rules
(mt#/cl/ip dual-context resolution is the test that decides).

## 5. The messageParams audit (lint-sketch's last open)

Mechanical, done DURING this window: walk onion's lint message params
(planning/onion_reference or ../usfm_onion) and confirm every param is
either a SPAN (reachable via anchor/second) or a small integer (fits
aux). Any counterexample = a design event, stop and surface it. Then
strike the open from lint-sketch.md.

## Sub-token anchoring (the one real design decision in this window)

Attr findings point INSIDE an AttrList token (the interpreter hands
back absolute `name_span`/`value_span`), but `Observation` is 16 bytes
of token indices. Options:
(a) anchor the whole AttrList token — diagnostic underlines the whole
    list. Simplest, v1-honest, loses precision on a 6-attribute fig.
(b) aux = the byte offset of the offending name (AuxKind::ByteOffset);
    the session narrows the range. Costs the aux slot (fine — these
    codes don't use it otherwise… except the attr-unknown-name
    bare-default proposal above wants aux too. Collision — resolve by
    splitting that case into its own code, or by (a)).
PROPOSED: (a) for v1 across all attr codes; aux stays semantic. The
session can already narrow client-side by re-running the interpreter
over the list span if it ever wants character-precise underlines (it
holds the text; the interpreter is pure). React.

## Corpus expectations (pins that may move)

- `attr-unknown-name`: expect ~0 — x-/z- covers the aligned corpora;
  bdf_reg/bsb have few lists. Investigate any hit at the bytes.
- `attr-malformed`: expect 0 (the interpreter sweep in the attr
  module's own corpus test will have pinned this first — reconcile).
- `attr-required-if`: expect 0 (en_ult zaln pairs are x- namespace,
  not sid/eid; check \qt-s usage — likely absent).
- Version family: expect 0 (no \usfm 3.2+ declarations in the corpora
  except en_ult, which declares 3.0 — verify).
- Positional lane: UNKNOWN, the interesting one — en_ulb's \s5 chunks
  and bdf_reg's assembly may hold real out-of-band material. Every
  nonzero investigated and storied, per the house rule.

## Done-when

All four rule sets live with per-code unit tests + corpus pins; the
messageParams audit struck; lint-sketch.md's "Still owed" lines and
NEXT-STEPS' leftover paragraph DELETED (nothing owed); fix oracle
green over the new fixes (the version-rename fix specifically);
clippy/fmt at baseline. Then exports.
