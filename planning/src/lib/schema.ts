/**
 * The item frontmatter, in one place. `content.config.ts` validates with it
 * and `scripts/plans.ts` checks with it, so a bad file fails the same way in
 * both.
 */

import { z } from "astro/zod";

export const KINDS = ["idea", "plan", "followup", "note"] as const;

/** Left to right is the usual path. The last three are where things stop. */
export const STATUSES = [
  "idea",
  "discussing",
  "planned",
  "in-progress",
  "done",
  "parked",
  "dropped",
] as const;
export type Status = (typeof STATUSES)[number];

/** Left out of the inbox; hidden in the sidebar until their stage filter is on. */
export const CLOSED: readonly Status[] = ["done", "parked", "dropped"];

export const itemSchema = z.object({
  id: z.uuid(),
  title: z.string().min(1),
  kind: z.enum(KINDS),
  status: z.enum(STATUSES),
  created: z.coerce.date(),
  updated: z.coerce.date(),
  /** The code commit this was written against. */
  sha: z.string().optional(),
  /** Lower is sooner: 0 is "now". Any whole number; unset sorts last. */
  priority: z.number().int().min(0).optional(),
  tags: z.array(z.string()).default([]),
  /** Other items, by id. */
  related: z.array(z.uuid()).default([]),
  /** A transcript or conversation this came from. */
  source: z.string().optional(),
});

export type Item = z.infer<typeof itemSchema>;
