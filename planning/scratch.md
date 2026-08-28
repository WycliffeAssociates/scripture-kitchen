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

Thoughts?




EDITOR vs LIBRARIES (ONION / GALLEY)

Chapter View Mode
cons: 2 loops (inner, outter), have to rebase edits into the whole for TOC yourself (though maybe not bad?)
pros; throw it all away if you want, it'll all be sub-ms.

control flow is:
if (chapterViewMode) {
 onKeystroke(analyze(chapter));
 paintChapter
startDebounce
--book.serialize(toc)
--analyzeWhole(wholeBook)
--updateProjectDiagnosticSink()
--paintDiagnosticsForChap()
}
Can you flesh this out using a combo of pseudo call stack like just dash and human namme (and you can gimme the exact codemirror api though as well as such as // e.g. changeSet.map)


Whole View Mode w/ a optional chapter clip
cons: You cna do a chapter mode, but it's a window fold/mask/clip out, and don't allow anything outside what I've currently clipped to be edited/selected. Higher string serialize (but this is tiny, hardly a con)
pros: always absolute. 1 loop. 1 coordinate system. See more context, i.e. +- line or a verse is pretty simple to just bump the clip up N rows of the TOC


control flow is?
onKey() {
if(chapterClipped) {
constrainSelToClipUnlessTrusted()
analyze(wholeBook, wantsAll)
needsState(recomputWholeBookState?)
purelyVisual(drawOnly(visibleRanges))
}
}
Can you flesh this out using a combo of pseudo call stack like just dash and human namme (and you can gimme the exact codemirror api though as well as such as // e.g. changeSet.map


Honestly, either method should probably so likely do set.map(changes) (what's this full api though?) This one? map(other: ChangeDesc, before⁠?: boolean = false) → ChangeSet
Lots of hit for "map" on the codemirror docs



Really those these are editor questions, but the reason I'm asking here is it is we have to build the right foundation and primitives ideally to enable either working method I think.   

Which leads me to this question, cause if we did prefer the latter and the doc was large, then: (transcript incoming)

Okay, so I've been thinking about this, obviously. So couple thoughts. Um for U SFM Onion. You could never do anything stateful. Express. I think the two most significant pieces are the concrete syntax tree and the Lant diagnostics. As of today, there's the potential that does not exist in Lant right now, because you could say it's more of a corpus thing, which is what Galley is gonna do. Um which is that chapter you the consistency in the usage of chapter light bulbs. Maybe at some point we try consistency on how other markers are done um stuff of that nature and the point being is like is it some complexity to pay to make lent and to make the concrete syntax tree a monoid. Yes theoretically you could do one and not the other because because I think lent is derived from the concrete syntax tree. Um But this is pretty easily testable. And respect to the first version is is to say USFM onion today changes lint and concrete from attack tree to be a true map. And then reduce kind of uh piece of work. Which theoretically would also I I don't know if we're gonna do this internally, but theoretically it would open up the avenue for parallelism because if it becomes a true monoid for the concretes in texture and for linting, then you can do it. Alright. And for the average case of you're working inside of a with the stateful handle we know that text encoder is quick into web assembly. We know that even if you did it over a process call such as an interprocess call and and towering or Electron or something. That that's fast. I think it's correct to probably not rebuild splice edits. Especially because there could be foot gunnisms with respect to addressing space and you could have foot guns with respect to um things what this would partially do is allow um What this would partially do is the costume wearing version of we're only working inside of a chapter but everything actually is still full book offsets and then we've just collapsed everything up to that point or after that point is that even that becomes performant on large texts because on keystroke if we have the transfer cost sure that's quick we had a millisecond scan and then a millisecond you know like a little bit of work to only traverse through just the changed checksum chapter. And then Um Also I'm thinking about you know we mentioned those trade-offs earlier of like okay maybe tokens itself is the most trivially testing thing but you know we I'm I'm stress testing on the English VLT and you say oh you shouldn't be worried about the English VOB but the point is like I'm doing all of this work right now in WebAssembly, I'm doing all of this work only in the editor, I don't have a reactive system, I don't have a react out there running that I'm measuring, I'm not measuring save backups I'm not measur checksums I'm not I'm not doing this is all I we have no sous chef work and so I think I would love to have orders of magnitude overhead and a frame budget for anything else that might happen. Cause if I can keep synchronous functions then not blow up RAM. So you say okay, well so you get the English U of E and Um You know, it's generally fast enough just to do everything from scratch for most books. So let's say you are doing a sort of review across a bunch of books kind of workflow where you're you're sending in a bunch of Tagalli like book string. Uh and it and if those of those are cache misses, it's probably fine. I mean you can probably use a least recently used cache of inbooks. You said it of of um whatever, you know, and of size. And for the English ULB, or for New Testament's, it's fine. And granted, for most of our New Testament books, this is wholly unneeded. But expressing onion in these terms opens it up to the potential parallelism to build a concrete syntax tree. It opens it up to Lent to be done in parallel. It opens uh I I think the thing is is we can prove that it's doable today with our tests. So it's the same test. Anything that's done is a collect And I f I feel like the way I would do it in terms of sort of a fake fuzzing is like for every corpus we load it and the way that it is cash today is like the way that you see it's hard to compare because you almost have to build the implementation twice. A refactor in place is a little finicky to express it as internally as a map reduce. So the implementation just stays being or the the interface still says lent or CST. Under the hood that becomes table of contents map reduce And then you have to change the internals to be You'd have to change the internals to maybe not that much. I don't know. The question is how how crazy is it to make these a shape that maps and reduces. Maybe it's not that bad. Maybe everything just gets a singular carry. Some stuff wrong always and so it doesn't have to have the mute carry and then it's just a reduce over only carry. 


Thoughts?



I've previously used Criterion, but I think I'd rather use Divan: 


### Todo fold cache: 

Clean back-to-back numbers, and they correct something I told you:

                        fastest    slowest    median     mean
ALLOC OFF
  analyze::full         28.18 ms   45.45 ms   29.39 ms   30.09 ms
  vref::pass_only       36.54 ms   195.4 ms   37.17 ms   39.22 ms
ALLOC ON
  analyze::full         28.21 ms   46.66 ms   29.12 ms   29.99 ms
  vref::pass_only       35.10 ms   45.08 ms   36.01 ms   36.47 ms

The AllocProfiler overhead is unmeasurable here — analyze is identical (28.18 vs 28.21 fastest), vref marginally faster with it on. So my "counts and timings can't be read in the same run" was overcautious for this workload: vref does ~415k allocator ops in 35 ms, ~85 ns per op, against a counter increment of ~1 ns. I'll soften that comment.

On variance: fastest and median are tight and reproducible in both configs (analyze ~28/29, vref ~35–37). The slowest column is rare environmental spikes — vref took 195.4 ms once with alloc off, and 60.74 ms once with it on. The spikes land wherever a run gets unlucky; they're not a property of either config.

  Ran 1 shell command

Let me get a real number on the memory question rather than guess at it.

  Searched for 1 pattern, ran 2 shell commands

Both answered.

Yes — and it's already cached, and never read

fold::Products holds cst: Cst. It's built by cst::build_chunk, consumed once to produce local/carried, then kept forever and never read again. The field carries #[allow(dead_code)], and grep confirms the only three mentions in the file are its construction (fold.rs:151), its LRU weighting (:162), and the field initializer (:163).

It isn't free — product_bytes charges it against the byte budget. Measured over en_ulb:

nodes=33,777   child_ids=288,370   obs=19,849   fix_of=19,849
fixes=5,462    edit_list=5,462

cst weight  = 1,693,912 B
lint weight =   659,156 B
cst share   = 72.0%

Nearly three quarters of the fold's resident bytes are a CST nothing reads. child_ids is the surprise: 288,370 entries against 33,777 nodes, so child_ids * 4 alone is 1.15 MB — bigger than every lint term combined. Dropping it would cut the fold's memory class ~3.5× at the same hit rate.

But for an analyze fold, the CST isn't the binding constraint — tokens are. The seven reads walk (source, tokens, cst), and Products keeps only token_count: u32; the earlier ruling dropped the token vector as "nothing needs them back," which was true when only lint folded. And from the pipeline benches:

lex          2.764 ms   (whole corpus, fastest)
cst::build   1.679 ms   (from pre-lexed tokens)

Lex is ~1.6× the build. So re-lexing clean chunks to reuse their cached CSTs costs more than the cached CST saves. Input-caching only pays if you cache tokens too — roughly doubling resident bytes on top of the 72% already spent on the CST.

The shape that matches your existing design is to cache analyze's outputs per chunk — the seven read arrays, chunk-relative — and reduce them exactly as local/carried are. Then neither tokens nor the CST need to come back, and the stored CST becomes droppable. Which is the same conclusion the memory finding reaches from the other direction. So I'd revise what I said earlier: analyze isn't un-foldable, it's un-folded, and the cheap path doesn't route through the cached tree.

Variance with alloc on — and a correction

I told you counts and timings couldn't share a run. Measured back-to-back, that's wrong here:

                        fastest    slowest    median     mean
ALLOC OFF
  analyze::full         28.18 ms   45.45 ms   29.39 ms   30.09 ms
  vref::pass_only       36.54 ms   195.4 ms   37.17 ms   39.22 ms
ALLOC ON
  analyze::full         28.21 ms   46.66 ms   29.12 ms   29.99 ms
  vref::pass_only       35.10 ms   45.08 ms   36.01 ms   36.47 ms

No detectable overhead — analyze identical, vref marginally faster with it on. vref does ~415k allocator ops in 35 ms (~85 ns/op) against a ~1 ns counter increment, so it can't show up. I've corrected the comments in both bench files: alloc-counts is off by default because the extra columns are screen clutter, not because the clock is compromised.

On the variance itself: fastest and median are tight and reproducible in both configs (analyze ~28/29, vref ~35–37). The slowest column is rare environmental spikes landing on whichever run is unlucky — vref took 195.4 ms once with counting off and 60.74 ms once with it on. Nothing intrinsic to either. My earlier note blaming analyze's 116 ms outlier on noise holds; vref's is the same phenomenon, not a separate one.

Temp measurement test removed; cargo fmt clean, cargo bench --no-run 0 warnings. Corrections and both findings are appended to the report at …/scratchpad/pass-report-divan.md.



FoldCache::new(n) allocates nothing. It builds an empty HashMap; n is only the threshold insert compares against resident_bytes() when deciding to evict. So 512 << 20 meant "never evict," not "reserve 512 MB." Still a terrible number to leave in a bench someone might read as a default. Measured what it actually needs:

book              source      warm (entries)     +50 keystrokes
en_ult/19-PSA   5,122,298   3,269,856 B (151)   4,616,256 B (201)
en_ulb/19-PSA     272,592     307,280 B (151)     442,080 B (201)
en_ulb/41-MAT     134,023      64,524 B ( 29)     149,524 B ( 79)
en_ulb/66-JUD       3,946       1,860 B (  2)      74,660 B ( 52)

Products run 0.6–1.1× source size. Bench now at 16 MB — 3.5× the worst case, still non-evicting.


#### FOLD CACHE August 28, 2026
For wasm I'd go lower than your 64 MB — 16–32, for a target-specific reason: wasm linear memory grows and never shrinks. A cache budget there isn't a steady-state figure, it's a permanent high-water mark for the page's lifetime. 16 MB already holds the largest book in the corpus 3.5× over, and an editor holds one book open. Add the CST drop and en_ult PSA falls from 4.62 MB to ~1.3 MB, at which point 8 MB does. Also worth noting new has no default at all — onion-wasm should pick a constant rather than pass raw bytes through from JS.
