/**
 * Lays out the atlas map once, at build, in ONE coordinate space: crates sit
 * inside products, primitives inside crates, wires beside the primitives they
 * plate. Zooming in is then literally zooming in: the client only decides
 * which levels to show at the current scale.
 *
 * Each level is sized for the scale it is read at. A level-L thing is drawn
 * `SCALE[L]` world units per screen pixel, so a product tile is six times the
 * size of a primitive and reads at a sixth of the zoom.
 *
 *   level 0  products, consumers   read at k ≈ 1/6
 *   level 1  crates                read at k ≈ 1/2.5
 *   level 2  primitives            read at k ≈ 1
 *   level 3  wires                 read at k ≈ 2
 */
import ELK from "elkjs/lib/elk.bundled.js";
import { pathOf } from "../graph.ts";
import type { AtlasMap } from "./schema.ts";

/** World units per screen pixel for each level's own drawing. */
export const SCALE = [6, 2.5, 1, 0.5] as const;
/** The zoom at which a level's things are big enough to show. */
export const THRESHOLD = SCALE.map((s) => 0.7 / s);
export const LEVEL_NAMES = ["products", "crates", "primitives", "wires"] as const;

const CHAR = 7.6;
const PAD_X = 28;
const NODE_H = 36;

export interface Info {
  readonly id: string;
  readonly label: string;
  readonly level: number;
  readonly kind: string;
  readonly crate: string | null;
  readonly slug: string | null;
  readonly summary: string;
  readonly why: string;
}

export interface Box extends Info {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
  readonly group: boolean;
  /** Most weighty node of its level: drawn as the hub. */
  readonly hub: boolean;
  /** Group header height, in world units. */
  readonly head: number;
  /** A group opens from a tile into a frame at the level its first child shows. */
  readonly opens: number;
}

export interface Wire {
  readonly from: string;
  readonly to: string;
  readonly label: string | null;
  readonly level: number;
  /** The level at which both ends and the edge itself are on screen. */
  readonly shows: number;
  readonly kind: string;
  readonly d: string;
  readonly mid: { x: number; y: number; len: number } | null;
}

export interface Canvas {
  readonly width: number;
  readonly height: number;
  readonly boxes: readonly Box[];
  readonly wires: readonly Wire[];
  /** The id the level-2 view centres on: the heaviest primitive (the CST). */
  readonly hub: string | null;
}

export interface ConceptInfo { readonly crate: string; readonly summary: string; readonly why: string; readonly kind: string }

interface ElkShape {
  id: string;
  x?: number; y?: number; width?: number; height?: number;
  children?: ElkShape[];
  edges?: ElkEdge[];
  layoutOptions?: Record<string, string>;
}
interface Point { x: number; y: number }
interface ElkEdge { id: string; sources: string[]; targets: string[]; sections?: { startPoint: Point; endPoint: Point; bendPoints?: Point[] }[] }

const elk = new ELK();

export const layoutCanvas = async (raw: AtlasMap, concepts: ReadonlyMap<string, ConceptInfo>): Promise<Canvas> => {
  // A crate is both a group and the node inside it, under one id. Groups are
  // keyed `group:<id>` so the two never collide; an edge names the node when
  // there is one, else the group.
  const nodeIds = new Set(raw.nodes.map((n) => n.id));
  const groupIds = new Set(raw.groups.map((g) => g.id));
  const gk = (id: string): string => `group:${id}`;
  const end = (id: string): string | undefined => (nodeIds.has(id) ? id : groupIds.has(id) ? gk(id) : undefined);
  const map = {
    groups: raw.groups.map((g) => ({ ...g, id: gk(g.id), name: g.id, parent: g.parent === undefined ? undefined : gk(g.parent) })),
    nodes: raw.nodes.map((n) => ({ ...n, group: n.group === undefined ? undefined : gk(n.group) })),
    edges: raw.edges.flatMap((e) => {
      const from = end(e.from), to = end(e.to);
      return from === undefined || to === undefined ? [] : [{ ...e, from, to }];
    }),
  };
  const groups = new Map(map.groups.map((g) => [g.id, g]));
  const parentOf = new Map<string, string | undefined>();
  for (const g of map.groups) parentOf.set(g.id, g.parent !== undefined && groups.has(g.parent) && g.parent !== g.id ? g.parent : undefined);
  for (const n of map.nodes) parentOf.set(n.id, n.group !== undefined && groups.has(n.group) ? n.group : undefined);
  const ancestors = (id: string): string[] => {
    const out: string[] = [];
    for (let at = parentOf.get(id); at !== undefined && !out.includes(at); at = parentOf.get(at)) out.push(at);
    return out;
  };
  const levelOf = new Map<string, number>([
    ...map.groups.map((g) => [g.id, g.level] as const),
    ...map.nodes.map((n) => [n.id, n.level] as const),
  ]);

  // A crate's hue: the concept's own crate, else the nearest crate-level group.
  const crateOf = (id: string, slug: string | undefined): string | null => {
    const concept = slug === undefined ? undefined : concepts.get(slug);
    if (concept) return concept.crate;
    const chain = [id, ...ancestors(id)];
    const crate = chain.find((at) => groups.get(at)?.level === 1 || concepts.get(groups.get(at)?.slug ?? at)?.kind === "crate");
    return crate === undefined ? null : (groups.get(crate)?.name ?? crate);
  };
  const info = (id: string, label: string, level: number, kind: string, slug: string | undefined): Info => {
    const concept = slug === undefined ? undefined : concepts.get(slug);
    return {
      id, label, level, kind,
      crate: crateOf(id, slug),
      slug: concept ? slug! : null,
      summary: concept?.summary ?? "",
      why: concept?.why ?? "",
    };
  };

  const children = new Map<string | undefined, string[]>();
  for (const id of [...map.groups.map((g) => g.id), ...map.nodes.map((n) => n.id)]) {
    const parent = parentOf.get(id);
    children.set(parent, [...(children.get(parent) ?? []), id]);
  }

  // Within a level-L container, things are spaced for the level below it.
  const inner = (level: number): number => SCALE[Math.min(3, level + 1)];
  const nodes = new Map(map.nodes.map((n) => [n.id, n]));
  const weightiest = new Map<number, string>();
  for (const n of map.nodes) {
    const best = weightiest.get(n.level);
    if (best === undefined || nodes.get(best)!.weight < n.weight) weightiest.set(n.level, n.id);
  }

  const shape = (id: string): ElkShape => {
    const group = groups.get(id);
    if (group === undefined) {
      const n = nodes.get(id)!;
      const s = SCALE[n.level] * (1 + Math.max(0, n.weight - 1) * 0.18);
      return { id, width: Math.ceil((n.label.length * CHAR + PAD_X) * s), height: Math.ceil(NODE_H * s) };
    }
    const s = inner(group.level);
    const kids = children.get(id) ?? [];
    const tile = SCALE[group.level];
    return {
      id,
      children: kids.map(shape),
      layoutOptions: {
        "elk.direction": "RIGHT",
        "elk.padding": `[top=${32 * s},left=${18 * s},bottom=${18 * s},right=${18 * s}]`,
        "elk.nodeSize.constraints": "MINIMUM_SIZE",
        "elk.nodeSize.minimum": `(${Math.ceil((group.label.length * CHAR + PAD_X) * tile)}, ${Math.ceil(NODE_H * 2 * tile)})`,
        "elk.spacing.nodeNode": String(30 * s),
        "elk.layered.spacing.nodeNodeBetweenLayers": String(90 * s),
        "elk.layered.spacing.edgeNodeBetweenLayers": String(20 * s),
        "elk.spacing.edgeNode": String(16 * s),
      },
    };
  };

  // An edge from a group to something inside it cannot be routed; it is left out.
  const edges = map.edges.filter((e) => levelOf.has(e.from) && levelOf.has(e.to) && e.from !== e.to
    && !ancestors(e.from).includes(e.to) && !ancestors(e.to).includes(e.from));

  // Two passes. Each top-level group is laid out alone, left to right, with
  // the edges inside it routed. Then the groups are placed as boxes, top to
  // bottom by who depends on whom, and an edge between groups is drawn as a
  // curve from node to node. One pass over everything makes a ribbon.
  const top = children.get(undefined) ?? [];
  const topOf = (id: string): string => [id, ...ancestors(id)].at(-1)!;
  const options = {
    "elk.algorithm": "layered",
    "elk.direction": "RIGHT",
    "elk.hierarchyHandling": "INCLUDE_CHILDREN",
    "elk.edgeRouting": "ORTHOGONAL",
    "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
    "elk.json.edgeCoords": "ROOT",
    "elk.json.shapeCoords": "ROOT",
  };
  const placed = new Map<string, ElkShape>();
  const routed = new Map<number, ElkEdge>();
  const parts = await Promise.all(top.map(async (id) => {
    const inside = edges.flatMap((e, i) => (topOf(e.from) === id && topOf(e.to) === id ? [{ id: `e${i}`, sources: [e.from], targets: [e.to] }] : []));
    const out = (await elk.layout({ id: "__part", layoutOptions: { ...options, "elk.padding": "[top=0,left=0,bottom=0,right=0]" }, children: [shape(id)], edges: inside } as never)) as unknown as ElkShape;
    return { id, out };
  }));
  const sizes = new Map(parts.map(({ id, out }) => {
    const s = out.children![0]!;
    return [id, { w: s.width ?? 0, h: s.height ?? 0 }];
  }));
  const between = new Map<string, ElkEdge>();
  for (const e of edges) {
    const from = topOf(e.from), to = topOf(e.to);
    if (from !== to) between.set(`${from}>${to}`, { id: `${from}>${to}`, sources: [from], targets: [to] });
  }
  const gap = 40 * SCALE[1];
  const root = (await elk.layout({
    id: "__root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.separateConnectedComponents": "false",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
      "elk.spacing.nodeNode": String(gap),
      "elk.layered.spacing.nodeNodeBetweenLayers": String(gap * 1.5),
      "elk.padding": `[top=${gap / 2},left=${gap / 2},bottom=${gap / 2},right=${gap / 2}]`,
    },
    // Products and consumers sit on top: the first thing a reader sees.
    children: top.map((id) => ({
      id, width: sizes.get(id)!.w, height: sizes.get(id)!.h,
      ...(levelOf.get(id) === 0 ? { layoutOptions: { "elk.layered.layering.layerConstraint": "FIRST" } } : {}),
    })),
    edges: [...between.values()],
  } as never)) as unknown as ElkShape;
  const origin = new Map((root.children ?? []).map((c) => [c.id, { x: c.x ?? 0, y: c.y ?? 0 }]));
  const shift = (p: Point, o: Point): Point => ({ x: p.x + o.x, y: p.y + o.y });
  for (const { id, out } of parts) {
    const o = origin.get(id)!;
    const walk = (sh: ElkShape): void => {
      for (const c of sh.children ?? []) {
        placed.set(c.id, { ...c, x: (c.x ?? 0) + o.x, y: (c.y ?? 0) + o.y });
        walk(c);
      }
      for (const e of sh.edges ?? []) {
        routed.set(Number(e.id.slice(1)), {
          ...e,
          sections: (e.sections ?? []).map((sec) => ({
            startPoint: shift(sec.startPoint, o), endPoint: shift(sec.endPoint, o), bendPoints: (sec.bendPoints ?? []).map((p) => shift(p, o)),
          })),
        });
      }
    };
    walk(out);
  }
  const out = root;

  const depth = (id: string): number => ancestors(id).length;
  const opens = (id: string): number =>
    Math.min(4, ...(children.get(id) ?? []).map((kid) => (groups.has(kid) ? groups.get(kid)!.level : levelOf.get(kid)!)));
  const boxes: Box[] = [
    ...map.groups.map((g) => ({ ...info(g.id, g.label, g.level, "group", g.slug ?? (concepts.has(g.name) && !nodeIds.has(g.name) ? g.name : undefined)), group: true, hub: false, head: 32 * inner(g.level), opens: opens(g.id) })),
    ...map.nodes.map((n) => ({ ...info(n.id, n.label, n.level, n.kind, n.slug), group: false, hub: weightiest.get(n.level) === n.id && n.weight > 1, head: 0, opens: 4 })),
  ].map((b) => {
    const s = placed.get(b.id);
    return { ...b, x: s?.x ?? 0, y: s?.y ?? 0, w: s?.width ?? 0, h: s?.height ?? 0 };
  // Outer groups first, so inner ones paint over them.
  }).sort((a, b) => Number(b.group) - Number(a.group) || depth(a.id) - depth(b.id));

  const centre = (id: string): Point & { w: number; h: number } => {
    const p = placed.get(id)!;
    return { x: (p.x ?? 0) + (p.width ?? 0) / 2, y: (p.y ?? 0) + (p.height ?? 0) / 2, w: p.width ?? 0, h: p.height ?? 0 };
  };
  /** Where the ray from a box's centre towards `to` leaves the box. */
  const rim = (c: ReturnType<typeof centre>, to: Point): Point => {
    const dx = to.x - c.x, dy = to.y - c.y;
    const t = Math.min(Math.abs(c.w / 2 / (dx || 1e-9)), Math.abs(c.h / 2 / (dy || 1e-9)));
    return { x: c.x + dx * t, y: c.y + dy * t };
  };

  const wires: Wire[] = edges.map((e, i) => {
    const section = routed.get(i)?.sections?.[0];
    let points: Point[];
    let d: string;
    let mid: Wire["mid"] = null;
    if (section) {
      points = [section.startPoint, ...(section.bendPoints ?? []), section.endPoint];
      d = pathOf(points, 8 * SCALE[Math.min(3, e.level)]);
      for (let j = 1; j < points.length; j += 1) {
        const a = points[j - 1]!, b = points[j]!;
        const len = Math.hypot(b.x - a.x, b.y - a.y);
        if (mid === null || len > mid.len) mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2, len };
      }
    } else {
      // Between groups: a curve, leaving and arriving vertically.
      const from = centre(e.from), to = centre(e.to);
      const a = rim(from, to), b = rim(to, from);
      const bend = Math.max(40, Math.abs(b.y - a.y) * 0.45);
      const down = b.y >= a.y ? 1 : -1;
      d = `M${a.x},${a.y} C${a.x},${a.y + bend * down} ${b.x},${b.y - bend * down} ${b.x},${b.y}`;
      mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2, len: Math.hypot(b.x - a.x, b.y - a.y) * 0.6 };
    }
    return {
      from: e.from, to: e.to, label: e.label ?? null, level: e.level, kind: e.kind,
      shows: Math.max(e.level, levelOf.get(e.from)!, levelOf.get(e.to)!),
      d, mid,
    };
  });

  return {
    width: Math.ceil(out.width ?? 0),
    height: Math.ceil(out.height ?? 0),
    boxes,
    wires,
    hub: weightiest.get(2) ?? null,
  };
};
