/**
 * Where each box of the atlas map sits, from map.json alone: the sizes, the
 * authored positions, the frames that wrap them, and which boxes are on
 * screen at each zoom level. The canvas draws from it and `pnpm atlas check`
 * checks it, so the two can never disagree about an overlap.
 *
 * Each level is sized for the scale it is read at. A level-L thing is drawn
 * `SCALE[L]` world units per screen pixel, so a product tile is six times the
 * size of a primitive and reads at a sixth of the zoom.
 *
 *   level 0  products, consumers   read at k ≈ 1/6
 *   level 1  crates                read at k ≈ 1/2.5
 *   level 2  primitives            read at k ≈ 1
 *   level 3  wires                 read at k ≈ 2
 *
 * A crate is a group and a node under one id (a twin): at level 1 it is a
 * compact card in the dependency picture; from level 2 the card gives way to
 * the crate's frame around its primitives, which sits wherever its members do.
 */
import type { AtlasMap } from "./schema.ts";

/** World units per screen pixel for each level's own drawing. */
export const SCALE = [6, 2.5, 1, 0.5] as const;
/** The zoom at which a level's things are big enough to show. */
export const THRESHOLD = SCALE.map((s) => 0.7 / s);
export const LEVEL_NAMES = ["products", "crates", "primitives", "wires"] as const;
/** The level from which a crate's frame replaces its card. */
export const FRAME_FROM = 2;

const CHAR = 8.4;
const PAD_X = 34;
const NODE_H = 36;

export interface Rect { readonly x: number; readonly y: number; readonly w: number; readonly h: number }
type MapNode = AtlasMap["nodes"][number];
type MapGroup = AtlasMap["groups"][number];

export const sizeOf = (n: MapNode): { w: number; h: number } => {
  const s = SCALE[n.level] * (1 + Math.max(0, n.weight - 1) * 0.18);
  return { w: n.w ?? Math.ceil((n.label.length * CHAR + PAD_X) * s), h: n.h ?? Math.ceil(NODE_H * s) };
};

/** Within a level-L group, things are spaced for the level below it. */
export const inner = (level: number): number => SCALE[Math.min(3, level + 1)];
export const frameHead = (level: number): number => 32 * inner(level);
const framePad = (level: number): number => 18 * inner(level);

export const twinsOf = (map: AtlasMap): Set<string> =>
  new Set(map.groups.filter((g) => map.nodes.some((n) => n.id === g.id)).map((g) => g.id));

export const nodeShows = (n: MapNode, z: number, twins: ReadonlySet<string>): boolean =>
  n.level <= z && !(twins.has(n.id) && z >= FRAME_FROM);
export const groupShows = (g: MapGroup, z: number, twins: ReadonlySet<string>): boolean =>
  g.level <= z && (!twins.has(g.id) || z >= FRAME_FROM);

export interface Placed {
  /** Keyed by node id. */
  readonly nodes: Map<string, Rect>;
  /** Keyed by group id. */
  readonly groups: Map<string, Rect>;
  /** Nodes with no authored position. */
  readonly missing: readonly string[];
}

/**
 * Authored nodes where map.json puts them; the rest from `fallback` (ELK, keyed
 * by node id and `group:<id>`). A group without an authored frame wraps its
 * members, never its own twin card.
 */
export const place = (map: AtlasMap, fallback?: ReadonlyMap<string, Rect>): Placed => {
  const nodes = new Map<string, Rect>();
  const missing: string[] = [];
  for (const n of map.nodes) {
    const { w, h } = sizeOf(n);
    if (n.x !== undefined && n.y !== undefined) nodes.set(n.id, { x: n.x - w / 2, y: n.y - h / 2, w, h });
    else {
      missing.push(n.id);
      const f = fallback?.get(n.id);
      if (f !== undefined) nodes.set(n.id, f);
    }
  }
  const groups = new Map<string, Rect>();
  const byId = new Map(map.groups.map((g) => [g.id, g]));
  const busy = new Set<string>();
  const frame = (g: MapGroup): Rect | undefined => {
    if (groups.has(g.id)) return groups.get(g.id);
    if (busy.has(g.id)) return undefined;
    busy.add(g.id);
    let out: Rect | undefined;
    if (g.x !== undefined && g.y !== undefined && g.w !== undefined && g.h !== undefined) out = { x: g.x, y: g.y, w: g.w, h: g.h };
    else {
      const members = [
        ...map.nodes.filter((n) => n.group === g.id && n.id !== g.id).map((n) => nodes.get(n.id)),
        ...map.groups.filter((c) => c.parent === g.id && c.id !== g.id).map((c) => frame(c)),
      ].filter((r): r is Rect => r !== undefined);
      if (members.length > 0) {
        const pad = framePad(g.level);
        const x0 = Math.min(...members.map((r) => r.x)) - pad, y0 = Math.min(...members.map((r) => r.y)) - frameHead(g.level);
        const x1 = Math.max(...members.map((r) => r.x + r.w)) + pad, y1 = Math.max(...members.map((r) => r.y + r.h)) + pad;
        out = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
      } else out = fallback?.get(`group:${g.id}`);
    }
    busy.delete(g.id);
    if (out !== undefined) groups.set(g.id, out);
    return out;
  };
  for (const g of byId.values()) frame(g);
  return { nodes, groups, missing };
};

const hit = (a: Rect, b: Rect): boolean => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
const inside = (a: Rect, b: Rect): boolean => a.x >= b.x && a.y >= b.y && a.x + a.w <= b.x + b.w && a.y + a.h <= b.y + b.h;

/** Every pair of boxes that are on screen together and overlap, and every member outside its frame. */
export const overlaps = (map: AtlasMap, placed: Placed): string[] => {
  const twins = twinsOf(map);
  const parent = new Map<string, string | undefined>(map.groups.map((g) => [g.id, g.parent]));
  const within = (group: string | undefined, of: string): boolean => {
    for (let at = group, n = 0; at !== undefined && n < 32; at = parent.get(at), n += 1) if (at === of) return true;
    return false;
  };
  const out = new Set<string>();
  for (let z = 0; z <= 3; z += 1) {
    const ns = map.nodes.filter((n) => nodeShows(n, z, twins) && placed.nodes.has(n.id));
    const gs = map.groups.filter((g) => groupShows(g, z, twins) && placed.groups.has(g.id));
    for (let i = 0; i < ns.length; i += 1) {
      for (let j = i + 1; j < ns.length; j += 1) {
        if (hit(placed.nodes.get(ns[i]!.id)!, placed.nodes.get(ns[j]!.id)!)) out.add(`"${ns[i]!.id}" overlaps "${ns[j]!.id}"`);
      }
    }
    for (let i = 0; i < gs.length; i += 1) {
      for (let j = i + 1; j < gs.length; j += 1) {
        const a = gs[i]!, b = gs[j]!;
        if (within(a.parent, b.id) || within(b.parent, a.id)) continue;
        if (hit(placed.groups.get(a.id)!, placed.groups.get(b.id)!)) out.add(`frame "${a.id}" overlaps frame "${b.id}"`);
      }
    }
    for (const n of ns) {
      const r = placed.nodes.get(n.id)!;
      for (const g of gs) {
        const f = placed.groups.get(g.id)!;
        const member = within(n.group, g.id);
        if (member && !twins.has(n.id) && !inside(r, f)) out.add(`"${n.id}" sticks out of its frame "${g.id}"`);
        if (!member && hit(r, f)) out.add(`"${n.id}" overlaps frame "${g.id}"`);
      }
    }
  }
  return [...out];
};
