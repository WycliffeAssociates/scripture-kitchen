import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";
import { conceptsDir } from "./lib/atlas/paths.ts";
import { conceptSchema } from "./lib/atlas/schema.ts";
import { itemSchema } from "./lib/schema.ts";

/** Where items live. `PLANS_ITEMS` points the site at another folder. */
const base = process.env["PLANS_ITEMS"] ?? "./items";

export const collections = {
  items: defineCollection({
    loader: glob({ pattern: "**/*.mdx", base }),
    schema: itemSchema,
  }),
  /** The atlas's concept pages; the id is the filename, which must equal `slug`. */
  concepts: defineCollection({
    loader: glob({ pattern: "*.mdx", base: conceptsDir }),
    schema: conceptSchema,
  }),
};
