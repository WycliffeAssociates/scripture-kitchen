# The designator gate (ruled sound by Will 2026-08-25, ready to build)

## The defect

The scanner's general path carves a Designator token POSITIONALLY —
"the run up to the first structural stop" — with no shape test (the
`\v `+digits fast path bails to it on anything non-digit, so the slow
path catches everything else). Consequence: `\v Then He declared`
tokenizes "Then " as a Designator, the interpreter judges it
Malformed, and every consumer downstream (toc rows, analyze anchors,
diff addressing, the editor) has to UN-treat it — the NUMBER_SHAPED
flag exists mostly to patch this. An oversight, not a ruling: the
token kind should be honest.

## The rule

**A Designator token requires a leading ASCII digit; everything else
after `\c `/`\v ` is ordinary Text.** The full grammar stays the
interpreter's — the gate is one byte:

```text
\v 1 text     Marker  Designator("1 ")   Text        (unchanged)
\v 2b text    Marker  Designator("2b ")  Text        (unchanged — wellformed verse)
\v 012 text   Marker  Designator("012 ") Text        (token yes; interpreter says
                                                      Malformed — VERSE starts [1-9])
\c 12b        Marker  Designator("12b")              (token yes; Malformed — chapters
                                                      are bare integers; SHAPED clear)
\v Then He…   Marker  Text("Then He…")               (CHANGED — no designator token)
\v \p         Marker  Marker                          (unchanged — already no token)
```

The gate is `is_ascii_digit(first byte)`, NOT `[1-9]`: `\v 012` must
still tokenize so the interpreter can judge it (degrade at the
interpretation layer, where the judgment is richer). Will's [1-9]
observation is the INTERPRETER's law and already lives there
(leading-zero test in designator.rs).

## Why this is a unification, not a new state

`\v \p` and `\v ⏎text` ALREADY produce no designator token today (the
expectation dies at a structural stop) — with handling everywhere
downstream. The gate makes `\v Then` collapse into that existing
designator-less case instead of being a third state that every
consumer patches around.

## Ripples (each is part of the pass)

1. **Scanner**: the gate in the general path's designator carve; the
   fast path is untouched (already digit-only). `payload_end`/
   `payload_label` untouched (callers/book codes keep their own
   shapes).
2. **Toc**: the banked "malformed `\v` still gets a row" SURVIVES —
   the row for a designator-less verse anchors at the MARKER token
   (number 0), matching what `\v \p` should already do (verify it
   does; if `\v \p` currently gets no row, THAT is the behavior to
   unify on and the banked ruling needs Will's re-read).
   VerseAnchor.token points at the marker in the absent case;
   designator_span answers empty.
3. **Lint**: chapter-without-designator exists; add/extend the VERSE
   counterpart — a `\v` whose designator is absent (which now
   includes the was-`Then` case). Ordering machine resyncs the same
   way it does for malformed today. Corpus counts will move; pin the
   new numbers honestly.
4. **Exports**: a designator-less verse follows the existing bare-\v
   path (no number="Then" style output anywhere). Verify USJ/USX
   fixtures; the 187/195 oracles are the gate.
5. **Analyze**: an absent designator emits marker_from = the marker,
   num_from = num_to = content_from (the propped-open slot, pass-9
   behavior kept). NUMBER_SHAPED survives ONLY for
   digit-start-but-malformed (`\c 12b`, `\v 012`) — the flag's
   meaning sharpens to "the interpreter accepted it".
6. **Diff**: the `\v 2"` ZEC case — "2\"" starts with a digit so it
   still tokenizes and still cuts a block (pass-6 divergence ruling
   stands). A was-`Then` verse becomes designator-less: block still
   cut at the marker anchor, addressed c:0, same `@N` tiebreak.
7. **Format**: designator-ws-single and dedupe-verse-number read
   Designator tokens; both simply never fire on the absent case
   (verify no pin moves).
8. **Editor/spike**: scan.ts's anchor-authority patch (pass 9 item 1)
   becomes mostly moot — the lines read and the anchors agree by
   construction once "Then" is Text. Re-sync the pkg and re-run the
   16 checks; the unshaped-designator probe check must still pass
   (it now exercises the absent-designator path instead of the
   flag-clear path).

## Tests

- Scanner units: the table above, byte-for-byte.
- Toc: `\v Then` row anchored at marker, number 0; `\v \p` identical
  shape (the unification assert).
- Lint: the new/extended verse-without-designator code fires once per
  absence; corpus pins re-pinned with the diff explained in the
  ledger.
- Full gate: `cargo test -- --include-ignored` + the spike's 16.

## Perf

The gate is one byte-compare in an already-branchy path; expect
noise. Measure per go-slow anyway (playground --serial, max-of-8).
