# usfm_onion

Rust workspace: `onion` is a USFM engine; `sous-chef/` is a separate product
that finds reviewable inconsistencies in scripture from counts alone.

- Commit freely on a work branch; merge to master on Will's ok. No PRs
  needed.
- Gate for any pass: `cargo nextest run` green (~5s; `cargo test` also works,
  ~9s) plus `cargo clippy --all-targets` clean. Run the whole workspace.
- Design authority: onion's is `planning/` and `GLOSSARY.md`; sous-chef's is
  `sous-chef/charter.md`, `rules/` and `evidence.md`.
- Plans, open questions, follow-ups and the decision ledger for the whole
  workspace live in `planning/items/`; see the `plans` skill
  (`planning/skill/SKILL.md`). Order of work: `planning/roadmap.mdx`.

Read when relevant:

- `agents/architecture.md`: the crates, who may depend on whom, the seam.
- `agents/testing.md`: corpus tiers, the ignore rule, the silent-skip trap.
- `agents/conventions.md`: comment and doc style, debug/ dumps, the ledger.
