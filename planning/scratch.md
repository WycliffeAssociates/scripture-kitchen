galley: 
Yes, it's all galley, and yes, onion's toc is enough to build on — the recipe is chapter_chunk_starts (the memmem pre-scan, already written in experiments/chapter_par.rs) + xxh3-128 per chunk + whatever per-chapter product the frontend caches against it. Onion needs nothing new except promoting chapter_chunk_starts out of experiments/.


Ordering I'd propose:

1. Workspace-ify this repo — [workspace] in the root Cargo.toml, members = onion (the current lib), onion-wasm, galley. Mechanical, no code moves beyond maybe paths.
2. galley crate v0 — deps onion; pub use onion; (the whole crate as a module, so nothing is hidden and galley::onion::… always works) plus galley's own curated names on top. First real exports: chapter_chunks(text) and chunk_checksums(text) -> Vec<u128> (xxh3-128-v1, the §13.4 deferral finally pulled by a real consumer — your frontend <checksum, work> cache).
3. Doorway — don't stand up a third wasm crate: the wasm sketch already ruled ONE combined bindings crate. onion-wasm grows a dep on galley and tags chunkChecksums; when sous joins, onion-wasm is renamed to the combined crate it was always destined to be.
4. Sous, when started, lands as sous/ + workspace member; galley adds the proofread recipe.
