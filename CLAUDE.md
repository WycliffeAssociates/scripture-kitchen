# usfm_onion

Rust workspace: `onion` is a USFM engine; `sous-chef/` is a separate product
that finds reviewable inconsistencies in scripture from counts alone.

- Do not commit. Leave the tree for Will's review; he commits on explicit ok.
- Gate for any pass: `cargo nextest run` green (~5s; `cargo test` also works,
  ~9s) plus `cargo clippy --all-targets` clean. Run the whole workspace.
- Design authority: onion's is `planning/` and `GLOSSARY.md`; sous-chef's is
  `sous-chef/charter.md`, `rules/`, `roadmap.md`, `evidence.md`.

Read when relevant:

- `agents/architecture.md`: the crates, who may depend on whom, the seam.
- `agents/testing.md`: corpus tiers, the ignore rule, the silent-skip trap.
- `agents/conventions.md`: comment and doc style, debug/ dumps, the ledger.
