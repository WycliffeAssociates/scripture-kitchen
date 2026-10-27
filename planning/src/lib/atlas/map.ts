/** The canvas map, read and validated. A broken map fails the build with the reason. */
import { readFileSync } from "node:fs";
import { mapPath } from "./paths.ts";
import { mapSchema, type AtlasMap } from "./schema.ts";

export const readMap = (): AtlasMap => {
  const parsed = mapSchema.safeParse(JSON.parse(readFileSync(mapPath, "utf8")));
  if (!parsed.success) {
    throw new Error(`${mapPath}: ${parsed.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ")}`);
  }
  return parsed.data;
};
