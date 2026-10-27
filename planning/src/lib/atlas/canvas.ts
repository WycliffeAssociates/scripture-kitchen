/**
 * Lays out the atlas map once, at build, in ONE coordinate space: the client
 * only decides which levels to show at the current scale. Positions are
 * authored in map.json (src/lib/atlas/place.ts); ELK places only a node that
 * has none. Every wire is a smooth curve between box sides, its ends spread
 * along the side so wires into one box fan out instead of stacking.
 */
import ELK from "elkjs/lib/elk.bundled.js";
import { FRAME_FROM, frameHead, inner, place, SCALE, sizeOf, twinsOf, type Rect } from "./place.ts";
import type { AtlasMap } from "./schema.ts";

export { LEVEL_NAMES, SCALE, THRESHOLD } from "./place.ts";

export interface Info {
  readonly id: string;
  readonly label: string;
  readonly level: number;
  readonly kind: string;
  readonly crate: string | null;
  readonly slug: string | null;
  readonly summary: string;
  readonly problem: string;
  readonly why: string;
}

export interface Box extends Info {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
  readonly group: boolean;
  /** The one weightiest node of its level: drawn as the hub. */
  readonly hub: boolean;
  /** Group header height, in world units. */
  readonly head: number;
  /** A group opens from a tile into a frame at the level its first child shows. */
  readonly opens: number;
  /** Shown only from this level (a crate's frame). */
  readonly from: number | null;
  /** Hidden from this level on (a crate's card). */
  readonly until: number | null;
}

export interface Wire {
  readonly from: string;
  readonly to: string;
  readonly label: string | null;
  readonly level: number;
  /** The level at which both ends and the edge itself are on screen. */
  readonly shows: number;
  /** Hidden from this level on, with the card it leaves or reaches. */
  readonly until: number | null;
  readonly kind: string;
  readonly d: string;
  readonly mid: { x: number; y: number; len: number };
  /** Drawn as a stub towards a far-off product, named at its tip. */
  readonly stub: { name: string; x: number; y: number } | null;
}

export interface Canvas {
  readonly width: number;
  readonly height: number;
  readonly boxes: readonly Box[];
  readonly wires: readonly Wire[];
  /** The id the level-2 view centres on: the heaviest primitive (the CST). */
  readonly hub: string | null;
}

export interface ConceptInfo { readonly crate: string; readonly summary: string; readonly problem: string; readonly why: string; readonly kind: string }

interface ElkShape {
  id: string;
  x?: number; y?: number; width?: number; height?: number;
  children?: ElkShape[];
  edges?: { id: string; sources: string[]; targets: string[] }[];
  layoutOptions?: Record<string, string>;
}
interface Point { x: number; y: number }

const elk = new ELK();

/** Positions for nodes map.json does not place: one layered pass over everything. */
const elkFallback = async (map: AtlasMap): Promise<Map<string, Rect>> => {
  const gk = (id: string): string => `group:${id}`;
  const groupIds = new Set(map.groups.map((g) => g.id));
  const kids = new Map<string | undefined, ElkShape[]>();
  const add = (parent: string | undefined, shape: ElkShape): void => void kids.set(parent, [...(kids.get(parent) ?? []), shape]);
  for (const n of map.nodes) add(n.group !== undefined && groupIds.has(n.group) ? gk(n.group) : undefined, { id: n.id, width: sizeOf(n).w, height: sizeOf(n).h });
  const shape = (g: AtlasMap["groups"][number]): ElkShape => {
    const s = inner(g.level);
    return {
      id: gk(g.id),
      layoutOptions: { "elk.padding": `[top=${frameHead(g.level)},left=${18 * s},bottom=${18 * s},right=${18 * s}]`, "elk.spacing.nodeNode": String(30 * s) },
      children: [...(kids.get(gk(g.id)) ?? []), ...map.groups.filter((c) => c.parent === g.id && c.id !== g.id).map(shape)],
    };
  };
  const top = [...(kids.get(undefined) ?? []), ...map.groups.filter((g) => g.parent === undefined || !groupIds.has(g.parent)).map(shape)];
  const ids = new Set(map.nodes.map((n) => n.id));
  const out = (await elk.layout({
    id: "__root",
    layoutOptions: {
      "elk.algorithm": "layered", "elk.direction": "RIGHT", "elk.hierarchyHandling": "INCLUDE_CHILDREN",
      "elk.json.shapeCoords": "ROOT", "elk.spacing.nodeNode": String(40 * SCALE[1]),
    },
    children: top,
    edges: map.edges.filter((e) => ids.has(e.from) && ids.has(e.to) && e.from !== e.to).map((e, i) => ({ id: `e${i}`, sources: [e.from], targets: [e.to] })),
  } as never)) as unknown as ElkShape;
  const rects = new Map<string, Rect>();
  const walk = (s: ElkShape): void => {
    for (const c of s.children ?? []) {
      rects.set(c.id, { x: c.x ?? 0, y: c.y ?? 0, w: c.width ?? 0, h: c.height ?? 0 });
      walk(c);
    }
  };
  walk(out);
  return rects;
};

type Side = "l" | "r" | "t" | "b";

export const layoutCanvas = async (raw: AtlasMap, concepts: ReadonlyMap<string, ConceptInfo>): Promise<Canvas> => {
  let placed = place(raw);
  if (placed.missing.length > 0) placed = place(raw, await elkFallback(raw));

  const twins = twinsOf(raw);
  const gk = (id: string): string => `group:${id}`;
  const nodeIds = new Set(raw.nodes.map((n) => n.id));
  const groupIds = new Set(raw.groups.map((g) => g.id));
  const groups = new Map(raw.groups.map((g) => [gk(g.id), { ...g, id: gk(g.id), name: g.id, parent: g.parent === undefined || !groupIds.has(g.parent) || g.parent === g.id ? undefined : gk(g.parent) }]));
  const nodes = new Map(raw.nodes.map((n) => [n.id, { ...n, group: n.group === undefined || !groupIds.has(n.group) ? undefined : gk(n.group) }]));
  const parentOf = (id: string): string | undefined => groups.get(id)?.parent ?? nodes.get(id)?.group;
  const ancestors = (id: string): string[] => {
    const out: string[] = [];
    for (let at = parentOf(id); at !== undefined && !out.includes(at); at = parentOf(at)) out.push(at);
    return out;
  };

  // Everything moves so the map's top-left corner is the origin.
  const rawRect = (id: string): Rect | undefined => (groups.has(id) ? placed.groups.get(groups.get(id)!.name) : placed.nodes.get(id));
  const all = [...groups.keys(), ...nodes.keys()].map(rawRect).filter((r): r is Rect => r !== undefined);
  const ox = Math.min(0, ...all.map((r) => r.x)), oy = Math.min(0, ...all.map((r) => r.y));
  const rectOf = (id: string): Rect => {
    const r = rawRect(id) ?? { x: 0, y: 0, w: 0, h: 0 };
    return { x: r.x - ox, y: r.y - oy, w: r.w, h: r.h };
  };

  // A crate's hue: the concept's own crate, else the nearest crate-level group.
  const crateOf = (id: string, slug: string | undefined): string | null => {
    const concept = slug === undefined ? undefined : concepts.get(slug);
    if (concept) return concept.crate;
    const crate = [id, ...ancestors(id)].find((at) => groups.get(at)?.level === 1 || concepts.get(groups.get(at)?.slug ?? at)?.kind === "crate");
    return crate === undefined ? null : (groups.get(crate)?.name ?? crate);
  };
  const info = (id: string, label: string, level: number, kind: string, slug: string | undefined): Info => {
    const concept = slug === undefined ? undefined : concepts.get(slug);
    return { id, label, level, kind, crate: crateOf(id, slug), slug: concept ? slug! : null, summary: concept?.summary ?? "", problem: concept?.problem ?? "", why: concept?.why ?? "" };
  };

  // The hub is the one node strictly heavier than the rest of its level.
  const hubs = new Set<string>();
  for (let level = 0; level <= 3; level += 1) {
    const ranked = raw.nodes.filter((n) => n.level === level).sort((a, b) => b.weight - a.weight);
    if (ranked[0] !== undefined && ranked[0].weight > 1 && (ranked[1] === undefined || ranked[1].weight < ranked[0].weight)) hubs.add(ranked[0].id);
  }
  const depth = (id: string): number => ancestors(id).length;
  const opens = (g: string): number => {
    if (twins.has(groups.get(g)!.name)) return FRAME_FROM;
    const levels = [...groups.values()].filter((c) => c.parent === g).map((c) => c.level)
      .concat([...nodes.values()].filter((n) => n.group === g).map((n) => n.level));
    return Math.min(4, ...levels);
  };
  const boxes: Box[] = [
    ...[...groups.values()].map((g) => ({
      ...info(g.id, g.label, g.level, "group", g.slug ?? (concepts.has(g.name) && !nodeIds.has(g.name) ? g.name : undefined)),
      group: true, hub: false, head: frameHead(g.level), opens: opens(g.id),
      from: twins.has(g.name) ? FRAME_FROM : null, until: null,
    })),
    ...[...nodes.values()].map((n) => ({
      ...info(n.id, n.label, n.level, n.kind, n.slug),
      group: false, hub: hubs.has(n.id), head: 0, opens: 4,
      from: null, until: twins.has(n.id) ? FRAME_FROM : null,
    })),
  ].filter((b) => rawRect(b.id) !== undefined)
    .map((b) => ({ ...b, ...rectOf(b.id) }))
    // Outer groups first, so inner ones paint over them.
    .sort((a, b) => Number(b.group) - Number(a.group) || depth(a.id) - depth(b.id));

  // An edge names a node when there is one, else the group; once a crate's
  // card has given way to its frame, an edge drawn at that level meets the frame.
  const edges = raw.edges.flatMap((e) => {
    const lv = (id: string): number => nodes.get(id)?.level ?? groups.get(gk(id))?.level ?? 0;
    const shows = Math.max(e.level, lv(e.from), lv(e.to));
    const end = (id: string): string | undefined => {
      if (nodeIds.has(id)) return twins.has(id) && shows >= FRAME_FROM ? gk(id) : id;
      return groupIds.has(id) ? gk(id) : undefined;
    };
    const from = end(e.from), to = end(e.to);
    if (from === undefined || to === undefined || from === to || rawRect(from) === undefined || rawRect(to) === undefined) return [];
    // An edge from a group to something inside it has nowhere to go; it is left out.
    if (ancestors(from).includes(to) || ancestors(to).includes(from)) return [];
    return [{ ...e, from, to, shows }];
  });

  // A wire from a primitive to a product (a consumer such as Sefer) would cross
  // the whole map; it is drawn as a short stub towards the product, named at its tip.
  const levelOfEnd = (id: string): number => groups.get(id)?.level ?? nodes.get(id)?.level ?? 0;
  const stubOf = (e: (typeof edges)[number]): "from" | "to" | null =>
    e.shows < 2 ? null : levelOfEnd(e.to) === 0 ? "from" : levelOfEnd(e.from) === 0 ? "to" : null;

  // Each end leaves the side facing the other box, along the axis with the wider gap.
  const centre = (r: Rect): Point => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });
  const flip: Record<Side, Side> = { l: "r", r: "l", t: "b", b: "t" };
  const sides = edges.map((e) => {
    const a = rectOf(e.from), b = rectOf(e.to), ca = centre(a), cb = centre(b);
    if (stubOf(e) !== null) {
      const towards: Side = cb.y >= ca.y ? "b" : "t";
      return { from: towards, to: flip[towards] };
    }
    const gapX = Math.abs(cb.x - ca.x) - (a.w + b.w) / 2, gapY = Math.abs(cb.y - ca.y) - (a.h + b.h) / 2;
    const from: Side = gapX >= gapY ? (cb.x >= ca.x ? "r" : "l") : (cb.y >= ca.y ? "b" : "t");
    return { from, to: flip[from] };
  });
  // Ports: the ends on one side are spread along it, ordered by where the other end is.
  const ports = new Map<string, { edge: number; end: "from" | "to"; along: number }[]>();
  edges.forEach((e, i) => {
    const stub = stubOf(e);
    for (const end of ["from", "to"] as const) {
      if (stub !== null && stub !== end) continue;
      const id = e[end], other = centre(rectOf(e[end === "from" ? "to" : "from"]));
      const side = sides[i]![end];
      const key = `${id}|${side}`;
      ports.set(key, [...(ports.get(key) ?? []), { edge: i, end, along: side === "l" || side === "r" ? other.y : other.x }]);
    }
  });
  const at = new Map<string, Point>();
  for (const [key, list] of ports) {
    const [id, side] = key.split("|") as [string, Side];
    const r = rectOf(id);
    list.sort((p, q) => p.along - q.along);
    const n = list.length;
    list.forEach((p, k) => {
      const f = n === 1 ? 0.5 : 0.5 + 0.7 * ((k + 0.5) / n - 0.5);
      const pt = side === "l" ? { x: r.x, y: r.y + r.h * f }
        : side === "r" ? { x: r.x + r.w, y: r.y + r.h * f }
        : side === "t" ? { x: r.x + r.w * f, y: r.y }
        : { x: r.x + r.w * f, y: r.y + r.h };
      at.set(`${p.edge}|${p.end}`, pt);
    });
  }
  const dir: Record<Side, Point> = { l: { x: -1, y: 0 }, r: { x: 1, y: 0 }, t: { x: 0, y: -1 }, b: { x: 0, y: 1 } };
  const round = (v: number): number => Math.round(v * 10) / 10;
  const pt = (p: Point): string => `${round(p.x)},${round(p.y)}`;

  const wires: Wire[] = edges.map((e, i) => {
    const card = (id: string): boolean => nodeIds.has(id) && twins.has(id);
    const base = {
      from: e.from, to: e.to, label: e.label ?? null, level: e.level, kind: e.kind, shows: e.shows,
      until: card(e.from) || card(e.to) ? FRAME_FROM : null,
    };
    const stub = stubOf(e);
    if (stub !== null) {
      const near = at.get(`${i}|${stub}`)!, d = dir[sides[i]![stub]];
      const len = 56 * SCALE[Math.min(3, e.shows)];
      const far = { x: near.x + d.x * len, y: near.y + d.y * len };
      const [a, b] = stub === "from" ? [near, far] : [far, near];
      const name = nodes.get(stub === "from" ? e.to : e.from)?.label ?? "";
      return { ...base, d: `M${pt(a)} L${pt(b)}`, mid: { x: round(near.x + d.x * len * 0.5), y: round(near.y + d.y * len * 0.5), len }, stub: { name, x: round(far.x), y: round(far.y + d.y * 6 * SCALE[Math.min(3, e.shows)]) } };
    }
    const a = at.get(`${i}|from`)!, b = at.get(`${i}|to`)!;
    const da = dir[sides[i]!.from], db = dir[sides[i]!.to];
    const span = Math.abs(da.x !== 0 ? b.x - a.x : b.y - a.y);
    const pull = Math.max(span * 0.5, 24 * SCALE[Math.min(3, e.shows)]);
    // A bend moves both control points sideways, so the middle moves by `bend`.
    const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    const side = { x: ((b.y - a.y) / len) * (e.bend ?? 0) * (4 / 3), y: (-(b.x - a.x) / len) * (e.bend ?? 0) * (4 / 3) };
    const c1 = { x: a.x + da.x * pull + side.x, y: a.y + da.y * pull + side.y };
    const c2 = { x: b.x + db.x * pull + side.x, y: b.y + db.y * pull + side.y };
    const mid = { x: (a.x + 3 * c1.x + 3 * c2.x + b.x) / 8, y: (a.y + 3 * c1.y + 3 * c2.y + b.y) / 8 };
    return { ...base, d: `M${pt(a)} C${pt(c1)} ${pt(c2)} ${pt(b)}`, mid: { x: round(mid.x), y: round(mid.y), len }, stub: null };
  });

  return {
    width: Math.ceil(Math.max(0, ...boxes.map((b) => b.x + b.w))),
    height: Math.ceil(Math.max(0, ...boxes.map((b) => b.y + b.h))),
    boxes,
    wires,
    hub: [...hubs].find((id) => nodes.get(id)!.level === 2) ?? null,
  };
};

