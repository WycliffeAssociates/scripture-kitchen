---
name: verify-kitchen
description: Prove a scripture-kitchen change the way Sefer sees it. Builds galley's node package, registers real USFM books on one Galley handle, publishes, and records each finding as the message Sefer renders. Use after any change to sous, onion or galley behaviour, before calling it done, and to reproduce a reported finding bug.
---

# Verify kitchen

Sefer consumes kitchen as the galley wasm package: a `Galley` handle that
holds books by id, `publish()` for Sous findings, plus the onion doors
(`lint`, `toc`, `find`, `mask`). Proof is a run through that package from
node, on real USFM, read back with the shipped readers. Unit tests are the
gate (`cargo nextest run`, `cargo clippy --all-targets`); this is the proof.

## Launch

```sh
cd galley && wasm-pack build --target nodejs --release --out-dir pkg-node -- --features wasm
```

About 35 s. Ready when it prints `Your wasm pkg is ready`. `pkg-node` is
gitignored scratch. Never rebuild `pkg-web`/`pkg-bundler` for a proof: those
are committed, byte-gated on x86_64 Linux, and built only by `galley/build.sh`.
Nothing stays running; there is no server.

## Doctor

```sh
.claude/skills/verify-kitchen/doctor.sh
```

Fails when `pkg-node` is missing or older than any `.rs` under `galley`,
`onion`, `sous-chef/core` or `mise`. Run it before every drive; a stale
package proves the old code.

## Drive

Findings, as Sefer renders them:

```sh
node .claude/skills/verify-kitchen/findings.mjs <out.jsonl> <file.usfm | dir>...
```

One JSON line per finding: `book`, `from`/`to` (UTF-16 into that book's text),
`id` (the `galley/sous-messages.ts` message id), `siteText`, `params`. Books
are registered under their file names. Corpora live in
`testData/exampleCorpora/` (`en_ulb` is the reference: 66 books, 172 findings
at 0.1.10).

To prove a behaviour, copy the corpus into the evidence folder, edit the copy
so it holds the case, and drive both before and after the change. A finding
should appear or disappear on the edited text and nothing else should move:
compare the two `findings.jsonl` with `diff`.

Hygiene groups and their fixes, as Sefer's "Remove all" would apply them:

```sh
node .claude/skills/verify-kitchen/fix-safety.mjs [--write <dir>] <file.usfm | dir>...
```

Prints rows → groups and the counts before and after, and fails unless every
fixed row is gone, no group gains a row, and every deleted scalar is of its
row's class. `--write` saves the fixed books, to drive `findings.mjs` on.

Other doors (lint, toc, find, mask, settings) have no helper yet;
`galley/tests/sous_conformance.mjs` shows each one called from node and read
back. Per feature: [features/README.md](features/README.md).

The `sous` CLI is the native path, for a whole-corpus look without wasm:
`cargo run -q --release -p sous-cli -- --findings <dir>` or `--report <out.html>`.

## Evidence

`debug/verify/<yyyy-mm-dd>-<topic>/` (gitignored): the edited corpus copy,
`before.jsonl`, `after.jsonl`, and the `diff` between them. Proof standards:

- Drive the package, not a Rust unit: the bytes must cross the wasm wall and
  be read by the shipped reader.
- Capture before and after on the same input.
- A finding that should appear is named by its `id`, `book` and `siteText`; a
  change that should move nothing else shows an otherwise empty diff.

## Cleanup

Delete the corpus copy, keep the jsonl files and the diff:

```sh
rm -r debug/verify/<run>/corpus
```

`galley/pkg-node` may stay; doctor tells you when it is stale.
