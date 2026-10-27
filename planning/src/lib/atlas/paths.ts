/**
 * Where the atlas reads from. The content (concept pages, the canvas map, the
 * wire layouts) is written by hand or generated elsewhere; until a source
 * exists the site builds against `planning/atlas-fixture/`, so the renderer
 * never waits on the writer.
 *
 *   ATLAS_DIR    default ./atlas       concepts/*.mdx, map.json
 *   LAYOUTS_DIR  default <repo>/docs/layouts   <format>.json, samples/
 */
import { existsSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { repoRoot } from "../root.ts";

const fixture = resolve("atlas-fixture");
const atlas = resolve(process.env["ATLAS_DIR"] ?? "atlas");
const layouts = resolve(process.env["LAYOUTS_DIR"] ?? join(repoRoot, "docs/layouts"));

const has = (dir: string, ext: string): boolean =>
  existsSync(dir) && readdirSync(dir).some((name) => name.endsWith(ext));

export const conceptsDir = has(join(atlas, "concepts"), ".mdx") ? join(atlas, "concepts") : join(fixture, "concepts");
export const mapPath = existsSync(join(atlas, "map.json")) ? join(atlas, "map.json") : join(fixture, "map.json");
export const layoutsDir = has(layouts, ".json") ? layouts : join(fixture, "layouts");
/** True for any source still read from the fixture, so a page can say so. */
export const fixtures = {
  concepts: conceptsDir.startsWith(fixture),
  map: mapPath.startsWith(fixture),
  layouts: layoutsDir.startsWith(fixture),
};
