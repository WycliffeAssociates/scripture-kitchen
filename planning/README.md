# Plans

A small static site for the leftovers of "talk to the agent": open questions, follow-ups,
trade-offs and plans, kept as MDX next to the code they are about. Copy it into a repo once;
it does not need to stay in sync with this template.

## Add it to a repo

```sh
npx degit wkelly17/llm-repo-planning-tool planning     # or copy this folder to planning/
```

1. **Install.** In a pnpm workspace, add `planning` to `pnpm-workspace.yaml` and run
   `pnpm install` at the root. Otherwise `cd planning && pnpm install`.
2. **Keep the repo's tools out of it.** Add `planning/` to the formatter's and linter's
   ignores. It has its own `tsconfig.json`; keep it out of the root one.
3. **Link the skill** so agents load it on demand:
   `ln -s ../../planning/skill .claude/skills/plans` (or wherever the repo keeps skills).
4. **One line in the repo's AGENTS.md / CLAUDE.md:** "Plans, open questions and follow-ups
   live in `planning/items/`; see the `plans` skill (`planning/skill/SKILL.md`)."
5. Optional root script: `"plans": "pnpm --filter planning run plans"`.

## Use it

```sh
pnpm dev            # http://localhost:4321 — the inbox, with every item in the sidebar
pnpm plans open     # the same inbox in the terminal
pnpm plans new "Title" --kind idea
pnpm build          # static site in dist/, to share read-only
```

`pnpm plans` is the `plans` script in this folder's `package.json` (`node scripts/plans.ts`).

**The roadmap** (`roadmap.mdx`, shown at `/roadmap`) is the order of work at a glance:
phases of numbered steps that point at items, side-by-side steps, and gates where a person
reviews before work goes on. `pnpm plans roadmap` prints it.

**The sidebar** lists every item by stage. Filter by stage (closed stages start off) or by
title and `#tag`, sort by priority, updated, created or title, and drag its edge to resize.
Choices are remembered per browser.

**Commenting.** Every page has the annotator (bottom right). Press `c` and click anything,
or highlight text and press the chip (`⌥C`). Shift-click to gather several places into one
comment. Enter adds the comment to the batch; copy the batch with the "Copy N" pill or
`⇧⌘C` and paste it into the agent. Each comment carries the item's file and line, the chain
of named places (`data-ref`), and the heading it sits under. Change the shortcut with
`copyHotkey` on `mountAnnotator` in `src/layouts/Base.astro` (`null` turns it off).

**Components** for richer plans: proposed file trees, diffs against the real code (split or
unified), state machines you can step through (XState-shaped), import graphs scanned from
the repo, call stacks, code with highlighted lines, sandboxed UI mocks, Mermaid. Everything
renders at build time to HTML and SVG: ELK lays out graphs and Shiki colours code; the only
browser libraries are d3-zoom (pan and zoom on graphs) and Mermaid.

- `skill/SKILL.md` — the item format, components, CLI and workflow (for agents and people).
- `examples/components.mdx` — every component, written against this repo's own files.

## Where things came from

`src/styles/tokens.css` is Sefer's design tokens (`src/app/ui/tokens.css`) with only the
`@source` globs changed. `src/annotate/` is Sefer's `src/dev/annotate/`, plus the `data-ref`
chain, the section heading, comments on highlighted text (`selection.ts`), Enter-to-batch
and copy-all.
