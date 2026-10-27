/**
 * The atlas's three inputs, validated in one place: a concept page's
 * frontmatter, the canvas map, and a wire layout. The content collection, the
 * pages and `pnpm plans check` all parse with these.
 */
import { z } from "astro/zod";

export const CONCEPT_KINDS = ["primitive", "product", "crate", "wire", "workflow", "concept"] as const;
export type ConceptKind = (typeof CONCEPT_KINDS)[number];

export const conceptSchema = z.object({
  term: z.string().min(1),
  slug: z.string().regex(/^[a-z0-9][a-z0-9-]*$/u),
  aliases: z.array(z.string()).default([]),
  kind: z.enum(CONCEPT_KINDS),
  crate: z.string().min(1),
  files: z.array(z.string()).default([]),
  related: z.array(z.string()).default([]),
  level: z.number().int().min(0).max(3),
  summary: z.string().min(1),
});
export type Concept = z.infer<typeof conceptSchema>;

/** Absent and null mean the same: map.json is written by hand and by scripts. */
const opt = z.string().nullish().transform((v) => v ?? undefined);
const num = z.number().nullish().transform((v) => v ?? undefined);
const size = z.number().positive().nullish().transform((v) => v ?? undefined);

export const mapSchema = z.object({
  nodes: z.array(z.object({
    id: z.string(),
    label: z.string(),
    level: z.number().int().min(0).max(3),
    group: opt,
    kind: z.string().default("concept"),
    slug: opt,
    weight: z.number().default(1),
    /** World position of the node's CENTRE; without it the node is placed by ELK. */
    x: num,
    y: num,
    /** Size override; by default the label and level size the box. */
    w: size,
    h: size,
  })),
  groups: z.array(z.object({
    id: z.string(),
    label: z.string(),
    level: z.number().int().min(0).max(3),
    parent: opt,
    slug: opt,
    /** The frame's TOP-LEFT corner and size; without them it wraps its members. */
    x: num,
    y: num,
    w: size,
    h: size,
  })).default([]),
  edges: z.array(z.object({
    from: z.string(),
    to: z.string(),
    label: opt,
    level: z.number().int().min(0).max(3),
    kind: z.string().default("data"),
    /** Pushes the curve's middle this many world units sideways (left of travel when positive). */
    bend: num,
  })).default([]),
});
export type AtlasMap = z.infer<typeof mapSchema>;

const field = z.looseObject({
  name: z.string(),
  offset: z.number().int().min(0),
  width: z.number().int().min(0),
  space: z.string().default("plain"),
  doc: z.string().default(""),
});
export type LayoutField = z.infer<typeof field>;

export const layoutSchema = z.looseObject({
  format: z.string(),
  magic: z.string().nullable().optional(),
  version: z.number().nullable().optional(),
  declared_in: z.string().optional(),
  reader: z.string().nullable().optional(),
  generated: z.boolean().default(false),
  doc: z.string().optional(),
  /**
   * `at`, `count` and `bytes` are numbers or expressions over earlier regions
   * (`"directory[3].offset"`, `"directory[0].length / 12"`,
   * `"header.sectionCount"`). `kind`: fixed | table | rows | words | blob.
   */
  envelope: z.array(z.looseObject({
    region: z.string(),
    at: z.union([z.number(), z.string()]).optional(),
    kind: z.string().optional(),
    of: z.string().optional(),
    count: z.union([z.number(), z.string()]).optional(),
    entry_bytes: z.number().int().min(0).optional(),
    width: z.number().int().min(0).optional(),
    bytes: z.union([z.number().int().min(0), z.string()]).nullable().optional(),
    doc: z.string().optional(),
    fields: z.array(field).default([]),
  })).default([]),
  records: z.array(z.looseObject({
    name: z.string(),
    doc: z.string().default(""),
    stride: z.number().int().min(0),
    fields: z.array(field).default([]),
    tail: z.unknown().nullable().optional(),
  })).default([]),
  sample: z.looseObject({ usfm: z.string(), bin: z.string() }).nullable().optional(),
});
export type Layout = z.infer<typeof layoutSchema>;
