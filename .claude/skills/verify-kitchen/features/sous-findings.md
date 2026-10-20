# Sous findings

## Sub-features

- Conventions: Placement, ExactNeighbor, RunShape, Rarity, SentenceStart,
  Casing, Doubled, LetterRun, BookRate.
- Hygiene, length proportionality, presence, source copy (needs a reference).
- Messages: `describe()` turns each finding into an id plus params.

## How to get to it (user POV)

Sefer's review panel: open a project, every finding is a row with a headline
built from the message id and params.

## Driving it with findings.mjs

```sh
ev=debug/verify/$(date +%F)-<topic>; mkdir -p $ev/corpus
cp testData/exampleCorpora/en_ulb/*.usfm $ev/corpus/
# edit $ev/corpus/44-JHN.usfm to hold the case
node .claude/skills/verify-kitchen/findings.mjs $ev/before.jsonl $ev/corpus
# rebuild pkg-node with the change, run doctor, then
node .claude/skills/verify-kitchen/findings.mjs $ev/after.jsonl $ev/corpus
diff $ev/before.jsonl $ev/after.jsonl > $ev/findings.diff
grep convention.letterRun $ev/after.jsonl
```

Proved 2026-10-20: `lovvved`, `tyyypo` and a 23-`s` run seeded into JHN
3:16 publish nothing; the only letterRun row is `joyfullly` (PSA 81:1).

## Gotchas

- en_ulb's file names carry a number (`44-JHN.usfm`, not `43-`).
- A reference (source) corpus needs `update_reference`; `findings.mjs`
  registers targets only, so length, presence and source-copy rows never
  appear through it.
- `from`/`to` index the LF-normalized text; the helper normalizes CRLF.
