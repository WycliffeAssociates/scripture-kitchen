import { getCollection, type CollectionEntry } from "astro:content";
import { relative, resolve } from "node:path";
import { repoRoot as root } from "./root.ts";
import { parseFile, type Lifted } from "./lift.ts";
import { refersTo } from "./roadmap.ts";
import { CLOSED } from "./schema.ts";

export interface Loaded {
  readonly entry: CollectionEntry<"items">;
  /** URL segment: the filename without `.mdx`. */
  readonly slug: string;
  /** Path from the repo root, as an agent would open it. */
  readonly path: string;
  readonly lifted: readonly Lifted[];
  readonly closed: boolean;
}


let cached: Promise<Loaded[]> | null = null;

/** Cached for a build (the roadmap asks once per step); fresh on every call in dev. */
export const loadItems = (): Promise<Loaded[]> =>
  import.meta.env.DEV ? load() : (cached ??= load());

const load = async (): Promise<Loaded[]> => {
  const entries = await getCollection("items");
  return entries
    .map((entry) => {
      const file = resolve(entry.filePath ?? "");
      return {
        entry,
        slug: entry.id,
        path: relative(root, file),
        lifted: parseFile(file).lifted,
        closed: CLOSED.includes(entry.data.status),
      };
    })
    .sort((a, b) => b.entry.data.updated.getTime() - a.entry.data.updated.getTime());
};

export const day = (date: Date): string => date.toISOString().slice(0, 10);

/** The item a roadmap step points at: by id, filename, or filename without its date. */
export const findItem = (items: readonly Loaded[], ref: string): Loaded | undefined =>
  items.find((item) => refersTo(ref, { id: item.entry.data.id, slug: item.slug }));
