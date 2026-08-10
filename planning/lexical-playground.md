Handoff: drive & discuss the Tab D virtualization prototype (onion_editor_chef)

Goal. Help Will drive the prototype, interrogate the Tab C vs D findings, and settle which residency model (if either) should shape the real editor's data model. Done = Will has positions on Q22–Q27 worth carrying back into the parent design docs.

Context. This repo is a disposable Lexical prototype for a clean-slate redesign of a USFM editor library (usfm_onion → rethought in ../usfm_onion_2). Core model under test: chapters as slots (positional edit containers, session-ephemeral ids), a SlotStore as the working document, Lexical as a disposable lens, one-way reconciliation (keystrokes → updateSlot → store rescans → editor reconciles containers). Four tabs: A (single chapter), B (whole book, no virtualization — 23,559 DOM nodes), C (swap-window ±1: only mounted containers exist in the tree), D (resident collapsed: all containers stay in Lexical's tree forever; far ones evict their children and render as fixed-height non-editable stubs, needing only canBeEmpty() → true).

The last agent's verdict was D, on caret mechanics and self-healing — not speed. Key findings to pressure-test: chapter-targeting accuracy was a wash (0-slot error both C and D — the presumed geometry win mostly isn't one; D only buys an exact scrollbar); residency downgrades the caret-across-split failure (node exists but collapsed → fix is "clear a flag") rather than fixing it; select-all attribution (Q18) is differently broken in both (stubs act as selection barriers; typing destroyed 3 containers with byte-identical self-heal — luck, not design); collapsed containers must be built collapsed, never built-then-evicted (46.8ms vs 1.1ms in editor.update).

Pointers.
- planning/QUESTIONS-PROTO.md — all 27 questions; Q22–Q27 are the new tab-d area, each marked observed vs reasoned. This is the main discussion artifact.
- README.md — C vs D comparison table + the 151-container caveat (whole Bible ≈ 8× that, untested).
- src/SlotStore.ts — scanBook/updateSlot/moveSlot/removeSlot/checkOracle (the store-is-truth reconciler).
- git log -3 on master — the tab D commits, unpushed.
- Parent design context (read-only): ../usfm_onion_2/planning/QUESTIONS.md (esp. "E — Statefulness and granularity"), ../usfm_onion_2/planning/GLOSSARY.md.

Steps. npm run dev, open tabs C and D side by side on Psalms; the inspector panel shows DOM counts, oracle status, reconcile timings.

Constraints & non-goals. This is a prototype — no tests, no polish, don't refactor it. Don't touch ../usfm_onion_2 (that's the parent session's territory). Nothing is decided: Will is in evaluate mode — do NOT declare directions settled or close questions without his explicit agreement. The store/slot model itself is also still under evaluation, not just the virtualization strategy.

Open questions. Q22–Q27 in QUESTIONS-PROTO.md, plus the untriaged Q1–Q21; the >151-container scale question; whether the container-set diff (needed under both C and D for Q18) changes the C-vs-D calculus.

Expected return. Positions/answers on the tab-d questions, distilled so they can be triaged back into ../usfm_onion_2/planning/QUESTIONS.md.

Suggested for the receiving session: just discussion + driving the dev server; /tools agent-browser if you want scripted probes of the running app.
