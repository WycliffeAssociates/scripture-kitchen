import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";
import { itemSchema } from "./lib/schema.ts";

/** Where items live. `PLANS_ITEMS` points the site at another folder. */
const base = process.env["PLANS_ITEMS"] ?? "./items";

export const collections = {
  items: defineCollection({
    loader: glob({ pattern: "**/*.mdx", base }),
    schema: itemSchema,
  }),
};
