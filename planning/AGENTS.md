# Plans — instructions for agents

Read `skill/SKILL.md`. It covers the item format, the components, the `pnpm plans` CLI,
and how to handle comment batches and transcripts. Component examples are in
`examples/components.mdx`.

When editing the site itself (not items): components are in `src/components/` and are
registered in `src/components/index.ts`; anything that reads items goes through
`src/lib/lift.ts` and `src/lib/schema.ts`; run `pnpm check` and `pnpm build`.
