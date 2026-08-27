galley: 
Yes, it's all galley, and yes, onion's toc is enough to build on — the recipe is chapter_chunk_starts (the memmem pre-scan, already written in experiments/chapter_par.rs) + xxh3-128 per chunk + whatever per-chapter product the frontend caches against it. Onion needs nothing new except promoting chapter_chunk_starts out of experiments/.


Ordering I'd propose:

1. Workspace-ify this repo — [workspace] in the root Cargo.toml, members = onion (the current lib), onion-wasm, galley. Mechanical, no code moves beyond maybe paths.
2. galley crate v0 — deps onion; pub use onion; (the whole crate as a module, so nothing is hidden and galley::onion::… always works) plus galley's own curated names on top. First real exports: chapter_chunks(text) and chunk_checksums(text) -> Vec<u128> (xxh3-128-v1, the §13.4 deferral finally pulled by a real consumer — your frontend <checksum, work> cache).
3. Doorway — don't stand up a third wasm crate: the wasm sketch already ruled ONE combined bindings crate. onion-wasm grows a dep on galley and tags chunkChecksums; when sous joins, onion-wasm is renamed to the combined crate it was always destined to be.
4. Sous, when started, lands as sous/ + workspace member; galley adds the proofread recipe.



Getting mind clear for tomorrow:

# Truths:

1. Usfm is by design a book format

## Per chapter
What does that mean?

### Case: Galley holds a copy of the string:
#### cons
- You have to ingest splices[]
- This could be a point of failure
- serialization costs on a string is memcpy speed (i.e textencoder. You're not gonna speed it up much)

#### pros
- You save on encodeInto/serialization/ipc costs

Alternative: You always have to pass the full book:

But if you do always pass the book:
What would a per chapter mode mean?
Some stuff can be per chapter (valid marker or not), but operating as such means  any subsytems need to implment some version of Carry, if possible

So let's you say what chapter level interaction.
you passed the whole book (not worth maintaingin splice edits here), and you scan/cst/lint (cause caching those is more use of memory than time maybe?) So what  does it mean to do chapter level work at that point  other than to say, we'll return from the fn call only chapter level diagnostics instead of all (but then if you have a broader sink where you collect them, you have to know to merge in per <book,chap>).   

So what is the return? only Analyze products with chapter relative outputs and that's it? All the work is still full file? But it begs the question. What's the addressing space for that? Like, the chapter designator from toc? I guess the toc designator and ordinal would have to be the contract? I.e. I'm working from a toc designator + oridinal, and when I call udpate I have to give it back so you know what it is I'm scoped to? 
A delete or added chapter does what though? Even all this is kinda complex.   
Is any of these worth doing? The clear win in this world is rendering for eidtor side I guess in that it doesn't have to protect off bytes and literally just draw evertyhing in a toc/ordinal section?
