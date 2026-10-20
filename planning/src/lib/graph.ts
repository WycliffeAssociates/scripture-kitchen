/**
 * Lays out a graph at build time with ELK (layered), for <Machine> and
 * <Modules>. The page gets plain SVG: no layout runs in the browser.
 *
 * Nodes may sit inside a group (`parent`), which ELK sizes around them.
 * Coordinates come back absolute (`edgeCoords`/`shapeCoords: ROOT`), so the
 * renderer never adds offsets up.
 */

import ELK from "elkjs/lib/elk.bundled.js";

export type Tone = "new" | "changed" | "deleted" | "moved" | "same";

export interface GraphNode {
  readonly id: string;
  readonly label: string;
  /** A second, smaller line: a note, a file count. */
  readonly sub?: string | undefined;
  readonly tone?: Tone | undefined;
  /** Drawn with a dot before it (a machine's initial state). */
  readonly initial?: boolean | undefined;
  /** Drawn with a double border (a final state). */
  readonly final?: boolean | undefined;
  readonly parent?: string | undefined;
  /** The `data-ref` the annotator reports, e.g. `state:idle`. */
  readonly ref: string;
}

export interface GraphEdge {
  readonly from: string;
  readonly to: string;
  readonly label?: string | undefined;
  readonly ref?: string | undefined;
}

export interface Point { readonly x: number; readonly y: number }

export interface PlacedNode extends GraphNode {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
  readonly group: boolean;
}

export interface PlacedEdge extends GraphEdge {
  readonly points: readonly Point[];
  readonly labelBox?: { x: number; y: number; w: number; h: number } | undefined;
}

export interface Laid {
  readonly width: number;
  readonly height: number;
  readonly nodes: readonly PlacedNode[];
  readonly edges: readonly PlacedEdge[];
}

export type Direction = "RIGHT" | "DOWN" | "LEFT" | "UP";

// Build-time text sizing: close enough for 13px sans and 11px secondary text.
const LABEL_PX = 7.4;
const SUB_PX = 6.4;
const widthOf = (node: GraphNode): number =>
  Math.ceil(Math.max(node.label.length * LABEL_PX, (node.sub?.length ?? 0) * SUB_PX) + (node.initial ? 34 : 24));

interface ElkShape {
  id: string;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  children?: ElkShape[];
  edges?: ElkEdge[];
  labels?: { text: string; width: number; height: number; x?: number; y?: number }[];
  layoutOptions?: Record<string, string>;
}
interface ElkEdge {
  id: string;
  sources: string[];
  targets: string[];
  labels?: { text: string; width: number; height: number; x?: number; y?: number }[];
  sections?: { startPoint: Point; endPoint: Point; bendPoints?: Point[] }[];
}

const elk = new ELK();

export const layout = async (
  nodes: readonly GraphNode[],
  edges: readonly GraphEdge[],
  direction: Direction = "RIGHT",
): Promise<Laid> => {
  const parents = new Set(nodes.map((n) => n.parent).filter((p): p is string => p !== undefined));
  const shapes = new Map<string, ElkShape>();
  for (const node of nodes) {
    const group = parents.has(node.id);
    shapes.set(node.id, {
      id: node.id,
      ...(group
        ? {
            children: [],
            // The label is drawn by the renderer in the top padding; ELK only
            // has to leave room for it.
            layoutOptions: {
              "elk.padding": "[top=34,left=14,bottom=14,right=14]",
              "elk.nodeSize.constraints": "MINIMUM_SIZE",
              "elk.nodeSize.minimum": `(${String(widthOf(node))}, 40)`,
            },
          }
        : { width: widthOf(node), height: node.sub ? 46 : 32 }),
    });
  }
  const root: ElkShape = {
    id: "__root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": direction,
      "elk.hierarchyHandling": "INCLUDE_CHILDREN",
      "elk.edgeRouting": "ORTHOGONAL",
      "elk.layered.spacing.nodeNodeBetweenLayers": "56",
      "elk.spacing.nodeNode": "26",
      "elk.spacing.edgeLabel": "4",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
      "elk.json.edgeCoords": "ROOT",
      "elk.json.shapeCoords": "ROOT",
    },
    children: [],
    edges: edges.map((edge, i) => ({
      id: `e${String(i)}`,
      sources: [edge.from],
      targets: [edge.to],
      ...(edge.label
        ? { labels: [{ text: edge.label, width: Math.ceil(edge.label.length * SUB_PX + 10), height: 16 }] }
        : {}),
    })),
  };
  for (const node of nodes) {
    const shape = shapes.get(node.id)!;
    const parent = node.parent === undefined ? root : shapes.get(node.parent);
    (parent?.children ?? root.children!).push(shape);
  }

  const out = (await elk.layout(root as never)) as unknown as ElkShape;

  const placed = new Map<string, ElkShape>();
  const walk = (shape: ElkShape): void => {
    for (const child of shape.children ?? []) {
      placed.set(child.id, child);
      walk(child);
    }
  };
  walk(out);

  const laidNodes: PlacedNode[] = nodes.map((node) => {
    const shape = placed.get(node.id);
    return {
      ...node,
      x: shape?.x ?? 0,
      y: shape?.y ?? 0,
      w: shape?.width ?? 0,
      h: shape?.height ?? 0,
      group: parents.has(node.id),
    };
  });
  // Groups first, so they paint under their children.
  laidNodes.sort((a, b) => Number(b.group) - Number(a.group));

  const laidEdges: PlacedEdge[] = edges.map((edge, i) => {
    const found = (out.edges ?? []).find((e) => e.id === `e${String(i)}`);
    const section = found?.sections?.[0];
    const label = found?.labels?.[0];
    return {
      ...edge,
      points: section ? [section.startPoint, ...(section.bendPoints ?? []), section.endPoint] : [],
      labelBox: label && label.x !== undefined && label.y !== undefined
        ? { x: label.x, y: label.y, w: label.width, h: label.height }
        : undefined,
    };
  });

  return { width: Math.ceil(out.width ?? 0), height: Math.ceil(out.height ?? 0), nodes: laidNodes, edges: laidEdges };
};

/** An orthogonal polyline with softened corners. */
export const pathOf = (points: readonly Point[], radius = 6): string => {
  if (points.length === 0) return "";
  const [first, ...rest] = points;
  let d = `M${first!.x},${first!.y}`;
  for (let i = 0; i < rest.length; i += 1) {
    const here = rest[i]!;
    const next = rest[i + 1];
    const prev = i === 0 ? first! : rest[i - 1]!;
    if (next === undefined) {
      d += ` L${here.x},${here.y}`;
      break;
    }
    const inLen = Math.hypot(here.x - prev.x, here.y - prev.y);
    const outLen = Math.hypot(next.x - here.x, next.y - here.y);
    const r = Math.min(radius, inLen / 2, outLen / 2);
    const ax = here.x - ((here.x - prev.x) / (inLen || 1)) * r;
    const ay = here.y - ((here.y - prev.y) / (inLen || 1)) * r;
    const bx = here.x + ((next.x - here.x) / (outLen || 1)) * r;
    const by = here.y + ((next.y - here.y) / (outLen || 1)) * r;
    d += ` L${ax},${ay} Q${here.x},${here.y} ${bx},${by}`;
  }
  return d;
};
