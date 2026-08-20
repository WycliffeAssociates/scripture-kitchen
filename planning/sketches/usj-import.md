# USJ import sketch (USJ → USFM — follow-on to usj-export, NOT SCHEDULED)

RULED 2026-08-20 ("drop it as own sketch/idea somewhere"): wanted, but
its own item AFTER the export lands. Interop is the reason — platform
tools (Paratext ecosystem, scripture-editors) hand USJ back; our world
stays USFM bytes, so USJ arriving at the door needs a way in.

## Shape (PROPOSED)

```rust
pub fn usfm_from_usj(json: &str) -> Result<String, UsjError>
```

Two halves, both new:
- A JSON READER. The export needed only a writer; reading means
  parsing. Either serde_json graduates from dev-dep to a real dep
  behind the same `usj` feature, or a hand-rolled reader (~150 lines
  for USJ's closed shape). Decide when scheduled — the no-deps lean
  says hand-rolled, the boring-well-maintained lean says serde. Feature
  gating keeps either out of consumers who don't ask.
- A USFM PRINTER: walk the USJ tree, emit markers. The export's
  mapping table read RIGHT-TO-LEFT: para→`\marker `, chapter number
  (sid ignored — derivable), lifted attrs put back as `\ca …\ca*`
  etc., attrs re-joined `|name="value"`, table wrapper unwrapped to
  bare `\tr` rows, ms as-spelled.

## Losslessness, stated honestly

Output is CANONICAL USFM. The export dropped seam newlines, quote
styles, attr order, delimiter whitespace — none of it comes back.
So usfm→usj→usfm is NOT byte-identity and never claims to be. The
identity that DOES hold (and is the test): usj→usfm→(our pipeline)→usj
is a FIXED POINT — the second trip changes nothing.

## Tests (plain english — testData funds all of it)

- Fixed point over the 207 validated-pass fixtures: read origin.json,
  print USFM, lex+cst+usj it, compare `Value ==` against origin.json.
- Canonical-form spot checks against origin.usfm where the fixture is
  already canonical (many are) — a diff, not a pin.
- Zoo: one inverse case per export mapping row, reusing the export
  zoo's fixtures backwards.
- Damage: a USJ tree we'd never emit (unknown `type`) → UsjError, not
  a guess. The never-synthesize spirit applies inbound too.

## Open

1. serde_json as real dep vs hand-rolled reader (above).
2. Does the printer emit lint-clean USFM by construction (one newline
   per paragraph marker, space discipline), and do we pin `lint == 0`
   on printed output as an oracle? (Lean: yes — cheap and sharp.)
