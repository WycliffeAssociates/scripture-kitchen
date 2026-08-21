# Masks + TOC sketch (roadmap item 1 — NOT APPROVED, plan to react to)

Written 2026-08-21 off the voice-memo reframing. Supersedes
toc-vref-slab.md (vref is now a renderer over the TOC, and the "slab"
IS the TOC). Two artifacts answering different questions, and (ruled
2026-08-21) they walk DIFFERENT things:

- **Mask**: WHICH bytes survive a filter (a range set + offset maps).
  Walks the CST — "text is not verse text" is a scope fact, and
  dropping a note subtree is O(1) at its node.
- **Toc**: WHERE things are (chapter/verse anchors + locate()). A pure
  pass over TOKENS — `\c` and `\v`+designator are flat facts, so this
  is ParseHeader grown, needs no tree, and the editor can have a Toc
  without ever building a CST. (Not inside the lexer itself: the fused
  experiment priced hot-loop hooks; the scanner stays single-purpose.)

Both stateless, built per call, no caching (a whole-book walk is tens
of microseconds — perf-notes.md).

## Entry points (PROPOSED)

```rust
pub fn mask(source: &[u8], tokens: &[Token], cst: &Cst, filter: &Filter) -> Mask
pub fn toc(source: &[u8], tokens: &[Token]) -> Toc     // no CST — a token pass
```

## Filter — use cases first, few options (ruled direction 2026-08-21)

NO trim option anywhere: trimming is invention. And no whitespace
anxiety needed: the scanner already folds a marker's delimiter
whitespace INTO the marker token, so dropping a marker drops exactly
its own whitespace; Newline tokens are their own tokens and the
recipes below keep them — nothing smushes.

ONE API for Rust and wasm, config only — NO predicate/callback
anywhere (ruled 2026-08-21: a JS callback per node is a boundary call
× ~100k nodes, and two APIs is two APIs). The load-bearing find: a
scope-opening marker has TWO different drops, so the action is named:

```rust
pub enum Action { Keep, Unwrap, Remove }
// Keep   = marker tokens + subtree survive
// Unwrap = children survive, the marker's own tokens (and attr list) drop
// Remove = the whole subtree drops

pub enum TextRule { All, VerseExtent, None }
// VerseExtent = Text survives only inside an open verse (the vid
// machinery's definition) — the rule that makes verse_text honest.

pub struct Filter {
    pub kinds: [Action; MarkerKind::COUNT], // COMPLETE map — no "unlisted" case
    pub markers: Vec<(String, Action)>,     // marker NAMES ("f"), resolved to
                                            // MarkerIdx once at construction;
                                            // unknown name = loud error. The one
                                            // precedence rule: marker > kind.
    pub unknowns: Action,                   // every row-0 marker
    pub text: TextRule,
    pub newlines: bool,
    pub attr_lists: bool,
}
```

No recipe field, no Options (ruled 2026-08-21): recipes are
CONSTRUCTORS returning a fully-populated Filter — every field
concrete and inspectable, overriding = mutate the struct you were
handed (`let mut f = Filter::structure(); f.markers.push(("f", Keep))`),
no merge semantics beyond marker-beats-kind. A designator rides its
marker (nobody keeps `\v` and drops its `1`). The constructors ARE
this table:

| kind | verse_text() | structure() |
|---|---|---|
| Note / Milestone / Sidebar | Remove | Remove |
| Character | Unwrap (text survives) | Remove |
| Paragraph | Unwrap | Keep |
| Chapter/Verse, Identification, Titles | Remove | Keep |
| text | VerseExtent | None |
| newlines | keep | keep |
| unknowns | Remove | Remove |

"Structure but keep footnotes" = `markers: [("f", Keep)]`; "verse text
including footnote text" = `kinds: [(Note, Unwrap)]`.

Why VerseExtent exists (Will's catch): naive keep-all-Text also keeps
intro text, front matter, and text before `\v 1`. A verse runs from
its `\v` to the next `\v`/`\c`/EOF, sidebars are their own scope —
exactly usx.rs's vid machinery reused. The kind table's Removes stack
on top (a mid-verse `\s1`'s text dies as Titles/Sections, not by
extent).

- **verse_text()** (sous): KEEP Text + Newline. Drop markers,
  designators, attr lists, and note/milestone/sidebar SUBTREES
  (subtree drop is why the mask walks the CST — `Text` inside a note
  dies because the note dies).
- **structure()** (copy-a-Bible scaffolding): KEEP `\id`, `\h`/`\toc*`,
  `\mt*`, `\c` + designator, paragraph markers, `\v` + designator, and
  their Newlines. DROP all Text, notes, character markup, unknowns.
  Yields a `\c 1 \p \v 1 \v 2 …` skeleton. (Exact keep-list is open
  question 1 — one worked example from Will settles it.)


## Mask — the range set IS the offset map

```rust
pub struct Mask {
    ranges: Vec<Range<u32>>,  // kept SOURCE bytes, sorted, non-overlapping
    starts: Vec<u32>,         // zip-mate of ranges: ranges[i]'s bytes sit at
                              // starts[i].. in the mask string (running sum of
                              // prior range lengths) — the to_source math below
}
impl Mask {
    pub fn text(&self, source: &[u8]) -> String;        // ONE alloc: concat(ranges)
    pub fn iter<'s>(&self, source: &'s [u8]) -> impl Iterator<Item = &'s str>; // zero-alloc view
    pub fn to_source(&self, mask_off: u32) -> u32;      // binary search starts, O(log n)
    pub fn from_source(&self, src_off: u32) -> Option<u32>; // None = that byte is masked out
    pub fn len(&self) -> u32;                           // mask-space length
}
```

Worked example (`·` marks dropped bytes):

```text
source   \v 1 Jesus wept.\f + \ft why\f* Then…
         ·····Jesus wept.················ Then…
ranges   [5..17, 33..38]        starts  [0, 12]
text()   "Jesus wept. Then…"
to_source(3)  = 5  + (3−0)  = 8      ("u" maps to the real "u")
to_source(13) = 33 + (13−12) = 34    (past the note, still exact)
from_source(20) = None               (inside the dropped \f — no home in this view)
```

The map costs one u32 per RANGE (≈ per dropped region), not per byte:
thousands per book, tens of KB, built by the same walk. A finding sous
reports against `text()` round-trips to real document bytes through
`to_source` — that round trip is the product; the string alone would
be a dead end.

sous is Rust/wasm: it takes `text()` (or the zero-alloc `iter`),
finds "doubled word at 12..17 of the mask", calls `to_source` twice,
and files a diagnostic in SOURCE bytes. No UTF-16 anywhere in that
path.

CONVENTION (ruled 2026-08-21): **public offsets are always source
bytes.** A mask's consumer converts via `to_source` BEFORE emitting
any diagnostic — mask space never escapes its maker, so every
downstream tool speaks one space. `from_source` serves only the
reverse "show a source diagnostic inside this masked view" direction,
where `None` honestly means "not in this view."

## The UTF-16 question, un-dizzied

The rule that dissolves it: **every index belongs to exactly ONE
string, and UTF-16 exists only at a JS boundary.** There is no
combined mask+utf16 map — there are two tiny single-purpose maps you
compose, and you only ever build the one for the string that actually
crosses the wire.

- `Utf16Index(s)` = the stride-256 index (experiments/utf16.rs) built
  over string `s`: `byte↔utf16` in O(1), ~1.6% of `s` in size. Works
  for ANY string — the source, or a mask's text. (str_indices is the
  approved crate if we'd rather not own it.)

### How the stride index works, worked in Greek

The counting fact first. UTF-16 units per char: 1 for everything
below U+10000, 2 above. In UTF-8 bytes that is:

```text
char      utf8 bytes            utf16 units    units = bytes − continuations + (lead ≥ 0xF0)
e         1  (65)               1              1 − 0 + 0 = 1
λ         2  (CE BB)            1              2 − 1 + 0 = 1
ἦ         3  (E1 BC A6)         1              3 − 2 + 0 = 1
𝕽 U+1D57D 4  (F0 9D 95 BD)      2              4 − 3 + 1 = 2
```

One formula covers every char, and it only needs BYTE CLASSES
(continuation `10xxxxxx`, lead `≥ 0xF0`) — countable 8-at-a-time with
SWAR, no decoding. So over `\v 1 λόγος ἦν`:

```text
bytes   \  v  ␠  1  ␠  CE BB CF 8C CE B3 CE BF CF 82 ␠  E1 BC A6 CE BD
byte#   0  1  2  3  4  5     7     9     11    13    15 16       19
utf16#  0  1  2  3  4  5     6     7     8     9     10 11       12
                       λ     ό     γ     ο     ς        ἦ        ν
```

"λόγος" = 10 bytes, 5 continuations, 0 four-byte leads → 5 units.
The drift (byte 16 = unit 11) is exactly what the index absorbs.

The INDEX is one u32 per 256 bytes: the utf16 count at each boundary,
filled by one pass of that formula over the whole string:

```text
stride:  [0, 214, 431, 645, …]      // utf16 units before byte 0, 256, 512, 768…
byte→utf16(517) = stride[2]         // 431, precomputed
                + swar(bytes 512..517)  // ≤256 bytes counted on the spot, ~ns
utf16→byte: binary search stride for the segment, then the same
            ≤256-byte scan forward until the unit count matches.
```

Greek text ≈ 2 bytes/char so a boundary count of 214 units per 256
bytes is typical; pure ASCII would read [0, 256, 512, …]; the formula
never cares which. Size: 4 bytes per 256 = ~1.6% of the string.
Building it for a whole book is one linear SWAR pass (~µs); queries
never decode anything.
- `Mask` = `mask-byte ↔ source-byte` (above).
- `Toc` = `source-byte → sid` (below).

Each hop is exact; chains compose left to right:

```text
sous finding on mask text (Rust, bytes)
  12..17 ──Mask::to_source──▶ source bytes 33..38            done (pure Rust — no utf16 hop at all)

that same diagnostic handed to a JS editor holding the SOURCE
  source bytes 33..38 ──Utf16Index(source)──▶ utf16 33..38′  one index, built over source, at the wire

JS editor asks "what verse is the cursor in?" (utf16 in, sid out)
  utf16 41,203 ──Utf16Index(source)──▶ byte 41,911 ──Toc::locate──▶ "MRK 6:3"

JS consumer that was handed the MASK STRING itself (rare — e.g. a proofread view in a webview)
  utf16 in mask text ──Utf16Index(mask_text)──▶ mask byte ──Mask::to_source──▶ source byte
```

Notes on the chains:
- Multibyte text changes NUMBERS, never mechanics: in `\v 1 नमस्ते…`,
  byte 15 ≠ utf16 15, and the Utf16Index eats exactly that drift.
- `Utf16Index(source)` and `Utf16Index(mask_text)` are DIFFERENT
  artifacts. Never point one string's index at another string — the
  composition rule exists so that mistake is unrepresentable in the
  API (each index is owned by / borrowed from the thing it indexes).
- Nothing here is stored: each map is built on demand from bytes it
  can see, so no invalidation story exists to get wrong.

## Toc

Layout (RULED 2026-08-21): fixed-width `#[repr(C)]` rows, offsets not
bytes — the source outlives every API here, so a designator's raw
spelling ("6a", a malformed run) is reachable through its token span;
rendering uses the NUMBERS the designator interpreter yields, and a
bridge is `first..last` as two u16s, never text. No heap strings
anywhere; the one inline byte array is `book`, fixed by the spec.

Fixed width is what buys the WIRE: a `Vec` of POD rows is already one
contiguous buffer, so wasm→JS is `(ptr, len)` and a zero-copy
`DataView` over wasm memory — no serde, no object creation. Two
consumption modes off the same bytes: query API through wasm
(`locate()` returns scalars, nothing streams) or bulk view (JS reads
fields at documented byte offsets, ~20 lines of accessors). Rust-side,
par_iter over 66 books → 66 buffers → memcpy-concatenable stream.
Discipline it costs: field order avoids padding,
`static_assert(size_of::<VerseAnchor>() == 16)`, byte layout
documented next to the struct so JS offsets can never drift.

```rust
pub struct Toc {
    book:     [u8; 3],           // the BookCode token's first 3 bytes, AS-IS
                                 // (short/missing → zero-padded; an invalid
                                 // code renders an ugly sid — lint owns that
                                 // complaint, the Toc never judges)
    // chapter rows TILE the source: gaps (front matter) belong to chapter 0
    chapters: Vec<ChapterRow>,   // number:u16 + span:2×u32 = 10 → 12 B/row
    verses:   Vec<VerseAnchor>,  // at:u32 + token:u32 + chapter:u16
                                 // + first:u16 + last:u16 = 14 → 16 B/row
}
// whole big book ≈ 50×12 + 1,200×16 + header ≈ 20 KB, built in the
// ParseHeader pass. The fixed layout is INTERNAL hygiene (#[repr(C)] +
// size assert), not a versioned wire format — the wire decision waits
// for wasm; fixing the width now just keeps both doors open.
impl Toc {
    pub fn locate(&self, src_byte: u32) -> Sid;          // binary search ×2 → "MRK 6:3"
    pub fn chapter_span(&self, n: u16) -> Range<u32>;    // the CodeMirror window/clamp
    pub fn verse_at(&self, src_byte: u32) -> Option<&VerseAnchor>;
}
```

This is ParseHeader grown to verse anchors (the long-standing
unification question — header_scan/ParseHeader/Toc become one thing
here). Diagnostics get human labels (`Toc::locate` at the anchor
byte); scroll-sync is `locate` on one doc + `chapter_span`/anchor on
the other; the chapter-window editor gets its clamp ranges for free.

## vref — a renderer, not an artifact

ebible's own shape is two parallel files (vref.txt = references only;
each translation = line-per-verse aligned by line number). So the
core is an iterator and the formats are one-liners over it:

```rust
pub fn verses<'s>(toc: &Toc, mask: &Mask /* verse_text() */, source: &'s [u8])
    -> impl Iterator<Item = (Sid, String)>  // ("GEN 1:1", "In the beginning…")
```

- keys file  = `sids.join("\n")`
- lines file = `texts.join("\n")`
- joined     = `"GEN 1:1\t…"` per line
- BRIDGES (`\v 1-3`): policy, not machinery (the designator
  interpreter already yields first/last). The difference is whether
  line-number-equals-verse-slot survives — ebible's whole design is
  that line N of EVERY translation is the same verse, which is what
  lets one vref.txt key them all:

```text
\v 1-3 One two three. \v 4 Four.

ebible <range>:              omit:
GEN 1:1  One two three.      GEN 1:1  One two three.
GEN 1:2  <range>             GEN 1:4  Four.
GEN 1:3  <range>
GEN 1:4  Four.               (every verse after the bridge is now on
                              the wrong line for alignment tooling)
```

  RULED 2026-08-21: `<range>` lines, ebible precedent kept. (Full
  alignment to ebible's master vref.txt also needs blank lines for
  verses this book never had — that requires a versification table we
  don't own; v1 renders what the file contains and leaves master-list
  padding to the consumer.) Line TRIM is a vref-RENDERER option only —
  it never exists on the Mask (trimming is invention; sous reads real
  bytes).

## Invariants (checkable, stated once — the tests below enforce them)

Mask:
- `ranges` strictly ascending, disjoint, every range within `0..source.len()`.
- `starts[i] == sum(len(ranges[..i]))`; `text().len() == starts.last + len(last)`.
- `text()[i] == source[to_source(i)]` for every mask byte i.
- `to_source(from_source(b)) == b` for every kept source byte b.
- `from_source(b) == None` exactly when b is dropped.
- Filter construction FAILS on an unknown marker name (no silent no-op).

Toc:
- `chapters` spans tile `0..source.len()` exactly: no gap, no overlap.
- `verses` sorted by `at`; every verse's `at` inside its chapter's span.
- `locate()` is total — every byte resolves to some sid.
- `size_of::<VerseAnchor>() == 16`, `size_of::<ChapterRow>() == 12` (static asserts).

Utf16Index:
- `utf16_at(b)` equals a naive char-walk count, every char boundary, every corpus.
- `byte_at(utf16_at(b)) == b` at every char boundary.

Chain: on kept char-boundary bytes, every hop mask↔source↔utf16 is
invertible; composed with `Toc::locate`, the sid agrees with a direct
locate of the source byte.

## Traces — debug/ (ruled 2026-08-21: readable artifacts, not just pins)

Passes dump REAL output to debug/ for eyeballing, plain strings:
- Pass 1: a human-readable Toc listing for one book (chapter table +
  first verse anchors, en_ulb PSA).
- Pass 3: masked output files — verse_text() and structure() for a
  representative poetry chapter (PSA with line breaks) from en_ulb,
  en_ult, and bsb (footnote-rich): `debug/psa.<corpus>.<recipe>.txt`.
- Pass 4: a vref keys+lines sample including a bridge.

## Tests (plain english)

- Mask invariants over the whole corpus, every preset: ranges sorted +
  disjoint; `text() == concat(iter())`; `to_source(from_source(b)) ==
  b` for every kept byte; `from_source` is None exactly on dropped
  bytes; mask-space length == sum of range lengths.
- verse_text() zoo: markers/designators/notes/milestones gone,
  `\add`'s TEXT kept while its markers drop, `~` and `//` policy
  stated and pinned.
- Toc vs lint: verse/chapter counts agree with the machines' counts
  over all 226 corpus books; `locate` round-trips every verse anchor.
- Chain test: for a non-ASCII book (Hindi IRV via testData), pick 1k
  random kept bytes, run mask→source→utf16→byte→sid and check each
  hop's inverse — the composition rule made executable.
- vref: a small book renders keys+lines; line counts equal; a bridge
  case shows the `<range>` policy.

## Open questions

1. `Filter::structure()`'s exact recipe — what does copy-a-Bible keep?
   (markers + designators, drop notes and verse TEXT? or keep \p text
   too? Needs Will's use case spelled once.)
2. Utf16Index: promote experiments/utf16.rs to src/ now, or take
   str_indices? (Approved-in-principle earlier; decide at build.)

RULED 2026-08-21: bridges = ebible `<range>`; `Toc` SUBSUMES
ParseHeader (one token pass, one struct — header fields become Toc
fields); trim is a vref-renderer option only; Filter = predicate in
Rust + data struct over wasm.
