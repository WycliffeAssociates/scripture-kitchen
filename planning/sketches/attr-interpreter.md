# The attribute k/v interpreter (roadmap 1 — next code)

The designator interpreter's sibling: pure span → judgment over an
AttrList token's INTERIOR. Never stores, never repairs; the AttrList
token stays byte-identical (that is what makes attribute passthrough
lossless — token.rs). Consumers: exports' k/v splatting (the lossy
step), lint's `attr-unknown-name` / `attr-required-if`.

## Shape (PROPOSED — mirrors designator.rs: module `src/attributes.rs`)

```rust
/// One parsed attribute. Borrowed spans of the SOURCE, no allocation.
pub struct Attr<'a> {
    pub name: &'a [u8],       // empty + default_attribute = the bare-value form
    pub value: &'a [u8],      // quotes stripped (the one interpretation)
    pub name_span: Range<u32>,   // absolute — lint anchors point INSIDE tokens
    pub value_span: Range<u32>,
}

/// Iterator over one AttrList span's interior. `Malformed` ends iteration —
/// judged, never resynchronized (a broken tail is one finding, not many).
pub fn attrs(source: &[u8], list: &Token) -> AttrIter<'_>;
pub enum AttrEvent<'a> { Attr(Attr<'a>), Malformed { at: u32, why: MalformedAttr } }
pub enum MalformedAttr { UnterminatedQuote, EmptyName, MissingValue, BareJunk }

/// The ONE place that learns naming conventions (schema.rs's ruling):
/// exact match, the `x-`/`z-` user namespace (legal on any character
/// marker), and the `"a-*"` prefix wildcard sentinel from
/// defined_attributes. Returns the matched def for AttrStatus reads.
pub fn resolve<'t>(name: &[u8], row: &'t MarkerRow) -> AttrResolution<'t>;
pub enum AttrResolution<'t> { Defined(&'t (/*name*/ &'static str, AttrStatus)), UserNamespace, Unknown }
```

Call flow: `token_pass sees AttrList → owner = nearest preceding
opener → for ev in attrs(source, list) → resolve(name, owner_row) →
consumer judges`. Exports run the same loop and splat into USJ/USX
attribute objects; "later definition wins" is the CONSUMER's merge
(exports overwrite on duplicate key across one node's lists — two
AttrLists on a node already draw `attr-both-lists`).

## Grammar read (from token.rs + the 3.2 attributes page)

- Interior = span minus delimiting pipe(s): leading `|` always; trailing
  `|` iff node-initial (U25001).
- `name = "value"` pairs, WHITESPACE-separated only (RESOLVED
  2026-08-20 by probe: usfmtc drops everything after a `,` — both
  `"a", strong=` and `"a",strong=` silently lose `strong`; there is no
  comma dialect). A comma between pairs is `Malformed { BareJunk }` at
  the comma — where the reference implementation silently loses data,
  we flag. (The scanner is no constraint either way: `attr_list_end`
  only finds the span's END and never parses the interior, so the
  scanner "blesses" any bytes.)
- Quoted value: `"` … `"`, no escapes defined by the spec — a `\"` case
  is BareJunk, recorded not guessed.
- Unquoted value: legal for the bare default form (`\w grace|strong\w*`)
  → `Attr { name: b"", … }`; caller resolves through
  `row.default_attribute` (NOT always the first def — fig's is `src`).
- default_attribute = None + bare value (`\fig` has no default) =
  lint's `attr-missing-required`-adjacent problem, NOT Malformed — the
  shape parsed fine.

## Tests (plain English)

- Real corpus shapes: `\zaln-s |x-strong="G2532" x-lemma="καί" …\*`
  (user namespace, many pairs, milestone owner); `\w In|in\w*` (bare
  default → lemma); `\w gracious|lemma="grace" strong="G5485"\w*`.
- `\fig` full six-attribute form; bare-value on fig = parsed, resolution
  says no default.
- `\ta` with `a-plus="…"` → wildcard hit; `\ta` with `foo="x"` → Unknown.
- Malformed: `|lemma="grace` (unterminated), `|="x"` (empty name),
  `|lemma=` (missing value) — each yields exactly ONE Malformed event
  and stops.
- Oracle vs usfmtc: splat a corpus book's attributes via usfmtc USJ and
  diff names/values (the exports sketch inherits this; a small probe
  here first catches separator-dialect surprises early).
- Zero-allocation assertion: the iterator is `Copy`-state over borrowed
  bytes; no String anywhere (type-level, no test needed).

## Open

None — both former opens closed 2026-08-20. (1) Separator dialect
RESOLVED: whitespace only, comma = Malformed; see the grammar bullet.
(2) was never a question, just scheduling: `attr-unknown-name` and
`attr-required-if` (eid-required-if-sid, ta's one-or-more) land in
lint's Flat in the same build window as this module. Ready to build.
