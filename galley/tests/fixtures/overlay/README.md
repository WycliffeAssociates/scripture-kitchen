# Overlay fixtures

Hand-written, `include_str!`'d by `galley/tests/overlay.rs` — SHAPES, never a
corpus read. Each pair is the smallest book that states one claim.

| pair | the claim |
| --- | --- |
| `gen-source` / `gen-target` | the worked example: `\p` before v21, four poetry lines inside v23, `\p` before v24; footnotes and `\nd` do not cross |
| `stray-target` / `plain-source` | a target block the source lacks is removed and its text joins the block before it |
| `runs-source` / `runs-target` | `\m \p` and `\p \p` fold to one; `\b` is empty by design and crosses |
| `titles-source` / `titles-target` | `\s` stays home by default and crosses when the host lists it |
| `bridge-source` / `bridge-target` | a bridge pairs with the same bridge; `GEN 1:4` and `GEN 1:5` pair with nothing and take no edits |
| `runon-source` / `runon-target` | verses sharing one line: a leading marker opens the line it needs rather than landing mid-line |

The targets are Swahili so that "the source's words did not cross" is visible
rather than argued, and both sides carry non-ASCII quotation marks so the
`utf16` opt-in has something to convert.
