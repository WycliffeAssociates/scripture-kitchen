Yes—this example clarifies the distinction nicely. The retained state is the project inventory; the finding is a small pointer into one surprising part of it.

Suppose the source contains:

```text
This is a wo,rd with an unusual comma.
```

The rule classifies the comma’s immediate topology:

```text
left neighbor:  regular letter
glyph:          ,
right neighbor: regular letter

profile: Letter–Comma–Letter
```

During analysis, each chapter contributes aggregate punctuation counts:

```rust
PunctuationChapterSummary {
    comma: {
        eligible_total: 742,
        profiles: {
            LetterCommaSpace: 711,
            DigitCommaDigit: 22,
            LetterCommaLetter: 1,
            // ...
        },
    },
}
```

The reduce pass combines those chapter summaries into the project inventory:

```text
Comma inventory
Eligible occurrences: 50,000

Profile                         Count    Rate
letter , whitespace            47,900   95.80%
digit  , digit                   1,500    3.00%
quote/other adjacency              595    1.19%
letter , letter                      5    0.01%
```

Given configuration such as:

```text
Flag boundary profiles occurring below 0.5%
Require at least 1,000 eligible uses of the glyph
```

`Letter–Comma–Letter` qualifies:

```text
5 / 50,000 = 0.01%
0.01% < 0.5%
```

That produces a squiggle under the comma—or perhaps `wo,rd`, depending on the presentation policy.

### What the compact diagnostic displays

The ordinary editor diagnostic could say:

> Comma between letters is unusually rare: 5 of 50,000 comma uses in this project (0.01%; configured threshold: 0.5%).

The future code-specific packed payload could naturally be:

```rust
FindingKind::PunctuationBoundary {
    matching_profile_count: 5,
    eligible_glyph_count: 50_000,
}
```

On the wire, hypothetically:

```text
from/to    location of "," or "wo,rd"
book_idx   containing book
code       PunctuationBoundary
flags      count saturation, if applicable
lane A     5
lane B     50,000
```

The record does not need to encode “comma” or `Letter–Comma–Letter` explicitly:

- `book_idx + from..to` recovers `","` from the immutable projected text.
- The rule classifier examines the neighboring bytes and recovers `Letter–Comma–Letter`.
- `code` selects the punctuation-boundary inventory and its detail resolver.

So within a valid snapshot, your proposed combination is nearly right:

```text
rule kind + located text/context
    ↓
inventory bucket
```

But the complete address is really:

```text
SnapshotId
  + BookIndex
  + from..to
  + FindingKind
```

The snapshot identity guarantees that the offsets, text, and inventory all belong together.

### What happens when the user clicks

Conceptually:

```rust
fn detail(
    snapshot: &AnalysisSnapshot,
    handle: FindingHandle,
) -> PunctuationBoundaryDetail {
    let finding = snapshot.findings[handle.row];
    let glyph = snapshot.text(finding.book_idx)[finding.from..finding.to];
    let profile = classify_neighbors(snapshot, &finding);

    let inventory = snapshot
        .rule_summaries
        .punctuation
        .glyph(glyph);

    PunctuationBoundaryDetail {
        glyph,
        profile,
        inventory,
    }
}
```

The detail panel can then show:

> **Comma usage in this project**
>
> 50,000 eligible occurrences across 66 books.  
> `letter , letter` occurs 5 times (0.01%).

It can display the complete profile table, threshold explanation, per-book distribution, and example actions:

- Find all 50,000 commas
- Find the 5 commas between letters
- Show distribution by book
- Exclude this profile
- Disable this rule
- Adjust the rarity threshold

### What is retained versus re-derived

To keep retained state minimal, Galley need not store 50,000 occurrence addresses.

It retains aggregate inventory:

```rust
struct GlyphInventory {
    eligible_total: u32,
    profiles: Map<BoundaryProfile, u32>,
    by_book: Vec<BookProfileCounts>,
}
```

When the user selects “Find the 5 occurrences,” Sous searches the current projected corpus for comma bytes and reclassifies their neighbors:

```text
memchr(',')
    → classify each comma’s boundary profile
    → retain only Letter–Comma–Letter
    → return the 5 current locations
```

That follows the earlier decision: retain counts and distributions, but re-derive sites on demand unless measurement demonstrates that retaining site lists is necessary.

### The actual lookup chain

I would describe it as:

```text
FindingHandle
    │
    ▼
PackedFinding
  code + book_idx + range + compact counts
    │
    ├── slice current text → ","
    ├── classify context → Letter–Comma–Letter
    │
    ▼
Punctuation inventory[","][Letter–Comma–Letter]
    │
    ├── full project distribution
    ├── per-book counts
    └── rescan command for matching sites
```

So there probably is no universal stored `ObservationKey` in every 16-byte finding. Each `FindingKind` defines how to turn its compact record plus current text into its inventory selector.

If a future rule cannot reconstruct that selector from its span—for example, a finding representing a normalized family not recoverable from one local spelling—then that rule must spend some of its code-specific payload on a snapshot-local bucket ID. That should be a rule-by-rule decision, not a universal tax on every finding.

This example also validates the discriminated payload decision: proportionality uses its lanes for signed deviations, while punctuation rarity can use the same physical lanes for `matching / eligible` counts. The full inventory remains typed retained state behind the rule.

### Publication scope and coordinates

The inventory and the finding rows have different reuse laws. Galley retains
chapter observations, then performs an ordered whole-corpus reduction to
derive current rule summaries and judgments. An edit in one chapter may change
a denominator used to judge findings in another book, so the published result
is one complete corpus findings snapshot rather than independently reusable
book buffers.

The buffer has a book directory, allowing the UI to seek without materializing
the rest of the corpus:

```ts
const snapshot = FindingsSnapshot.open(buffer);
const mark = snapshot.book("MRK");
const first = mark.at(0);
```

Sous's analysis range is projected-book UTF-8. Before the invocation releases
its owned input strings, Galley uses the matching producer projection to map it
back to raw book coordinates, then converts those raw UTF-8 boundaries to
UTF-16 for the JS/editor wire. A byte-identical book may reuse detached
projection and UTF-16 index data keyed by its raw checksum; a borrow-bearing
cursor is recreated against the current invocation's string. Thus the hot path
is:

```text
chapter observations
    -> whole-corpus reduction
    -> projected UTF-8 findings
    -> producer source-map rebasing
    -> raw-book UTF-16 findings
    -> complete corpus ArrayBuffer
```

The `ArrayBuffer` remains only the compact findings publication. The resident
chapter observations and rule inventories are not serialized into it; detail,
inventory, and site-search APIs query the matching live Galley snapshot.


| Kind | Byte 11 | Bytes 12–13 | Bytes 14–15 | Lazy detail |
|---|---|---|---|---|
| Proportionality | representation flags | signed book deviation | signed project deviation | target/source lengths, ratio, median, side MAD, fallback |
| Casing | representation flags | focal-form count | eligible-family count | complete form distribution, dominant form, exclusions |
