/**
 * The atlas canvas in the browser: pan and zoom (d3-zoom; pinch on a phone),
 * the level the zoom implies, labels that hide rather than shrink, the level
 * buttons, the keyboard, and the side panel a click opens.
 *
 *   +/=  zoom in     -  zoom out     0  whole map     arrows  pan     Esc  close panel
 *   L    every wire's label (otherwise only the hovered or selected box's)
 *   /atlas#cst  opens on the CST, at its level, with its panel open.
 */
import { select } from "d3-selection";
import { zoom, zoomIdentity, type D3ZoomEvent, type ZoomTransform } from "d3-zoom";
import { url } from "../lib/url.ts";

interface Info {
  label: string; level: number; kind: string; crate: string | null; slug: string | null;
  summary: string; problem: string; why: string; group: boolean; x: number; y: number; w: number; h: number;
}
interface Data { threshold: number[]; hub: string | null; width: number; height: number; info: Record<string, Info> }

const LEVELS = ["products", "crates", "primitives", "wires"];
/** Screen size of each label class, px. Matches site.css. */
const FONT: Record<string, number> = { "tile-label": 15, "head-label": 12, "node-label": 13, "wire-label": 11 };

export const mountAtlas = (): void => {
  const host = document.querySelector<HTMLElement>("[data-atlas]");
  if (host === null || host.dataset.mounted !== undefined) return;
  host.dataset.mounted = "";
  const data = JSON.parse(document.getElementById("atlas-data")!.textContent!) as Data;
  const svgEl = host.querySelector<SVGSVGElement>("svg")!;
  const svg = select(svgEl);
  const viewport = svg.select<SVGGElement>("[data-viewport]");
  const labels = [...host.querySelectorAll<SVGTextElement>("text.label")].map((el) => ({
    el,
    w: Number(el.dataset.w),
    h: Number(el.dataset.h ?? Infinity),
    level: el.dataset.level === undefined ? null : Number(el.dataset.level),
    px: FONT[[...el.classList].find((c) => c in FONT) ?? "node-label"] ?? 13,
    chars: el.textContent?.length ?? 0,
    /** The label's width on screen, measured once at --k: 1 (its font is a fixed screen size). */
    len: 0,
  }));
  host.style.setProperty("--k", "1");
  for (const label of labels) {
    try { label.len = label.el.getComputedTextLength(); } catch { label.len = 0; }
    if (!(label.len > 0)) label.len = label.chars * label.px * 0.6;
  }
  const buttons = [...host.querySelectorAll<HTMLButtonElement>(".atlas-levels [data-level]")];

  // The SVG's viewBox maps the whole map onto the box at k = 1; d3's k is on top of that.
  const base = (): number => {
    const box = svgEl.getBoundingClientRect();
    return Math.min(box.width / (data.width + 80), box.height / (data.height + 80));
  };
  const levelAt = (k: number): number => {
    let level = 0;
    data.threshold.forEach((t, l) => { if (k >= t) level = l; });
    return level;
  };

  let current = -1;
  let pending = 0;
  const paint = (t: ZoomTransform): void => {
    // `k` here is screen pixels per world unit.
    const k = t.k * base();
    const level = levelAt(k);
    host.style.setProperty("--k", String(k));
    if (level !== current || host.dataset.z !== String(level)) {
      current = level;
      host.dataset.z = String(level);
      for (const b of buttons) b.setAttribute("aria-pressed", String(Number(b.dataset.level) === level));
    }
    for (const label of labels) {
      // A wire label shows on demand (hover, selection, the toggle), not when it fits.
      if (label.level !== null) {
        label.el.toggleAttribute("data-hide", label.level < level - 1);
        continue;
      }
      const need = label.len + 6;
      const fits = label.w * k >= need && label.h * k >= label.px + 2;
      label.el.toggleAttribute("data-hide", !fits);
    }
  };

  const minK = (): number => Math.min(1, (data.threshold[1]! * 0.5) / base());
  const behaviour = zoom<SVGSVGElement, unknown>()
    .scaleExtent([0.5, 40])
    .filter((event: Event) => !(event.type === "dblclick") && (!(event instanceof MouseEvent) || event.button === 0 || event.type === "wheel"))
    .on("zoom", (event: D3ZoomEvent<SVGSVGElement, unknown>) => {
      viewport.attr("transform", String(event.transform));
      cancelAnimationFrame(pending);
      pending = requestAnimationFrame(() => paint(event.transform));
    });
  svg.call(behaviour);
  const syncExtent = (): void => void behaviour.scaleExtent([minK(), 12 / base()]);
  syncExtent();

  // A short glide between transforms; d3-transition is not a dependency, so this is it.
  let gliding = 0;
  const still = matchMedia("(prefers-reduced-motion: reduce)");
  const tween = (to: ZoomTransform, ms: number): void => {
    cancelAnimationFrame(gliding);
    const from = svgEl.__zoom ?? zoomIdentity;
    if (ms === 0 || still.matches) return void svg.call(behaviour.transform as never, to);
    const begin = performance.now();
    const step = (now: number): void => {
      const p = Math.min(1, (now - begin) / ms);
      const e = p < 0.5 ? 4 * p * p * p : 1 - (-2 * p + 2) ** 3 / 2;
      const k = from.k * (to.k / from.k) ** e;
      // Keep the screen centre on a straight path while the scale changes geometrically.
      const fx = (cx - from.x) / from.k, fy = (cy - from.y) / from.k;
      const tx = (cx - to.x) / to.k, ty = (cy - to.y) / to.k;
      const wx = fx + (tx - fx) * e, wy = fy + (ty - fy) * e;
      svg.call(behaviour.transform as never, zoomIdentity.translate(cx - wx * k, cy - wy * k).scale(k));
      if (p < 1) gliding = requestAnimationFrame(step);
    };
    gliding = requestAnimationFrame(step);
  };
  // d3 works in the viewBox's user units, whose visible centre is the viewBox's centre.
  const cx = data.width / 2;
  const cy = data.height / 2;
  /** Centre world point (x, y) at screen scale `k` (pixels per world unit). */
  const go = (x: number, y: number, k: number, animate = true): void => {
    const t = k / base();
    const target = zoomIdentity.translate(cx - x * t, cy - y * t).scale(t);
    tween(target, animate ? 450 : 0);
  };
  /** Level 0: the level-0 boxes filling the view, at a scale where crates are not yet showing. */
  const overview = (): { x: number; y: number; k: number } => {
    const tops = Object.values(data.info).filter((n) => n.level === 0);
    const all = tops.length > 0 ? tops : Object.values(data.info);
    const x0 = Math.min(...all.map((n) => n.x)), y0 = Math.min(...all.map((n) => n.y));
    const x1 = Math.max(...all.map((n) => n.x + n.w)), y1 = Math.max(...all.map((n) => n.y + n.h));
    const box = svgEl.getBoundingClientRect();
    const k = Math.min((box.width - 32) / (x1 - x0), (box.height - 96) / (y1 - y0), data.threshold[1]! * 0.9);
    return { x: (x0 + x1) / 2, y: (y0 + y1) / 2, k };
  };
  const fit = (animate = true): void => {
    const view = overview();
    go(view.x, view.y, view.k, animate);
  };
  const focus = (id: string, extra = 1.5, animate = true): void => {
    const node = data.info[id];
    if (node === undefined) return;
    // Show the node at the scale its level reads at; a group, big enough to see inside.
    const level = node.group ? Math.min(3, node.level + 1) : node.level;
    const box = svgEl.getBoundingClientRect();
    const k = node.group
      ? Math.max(data.threshold[level]! * 1.1, Math.min(box.width / node.w, box.height / node.h) * 0.85)
      : data.threshold[level]! * extra;
    go(node.x + node.w / 2, node.y + node.h / 2, k, animate);
  };
  const jump = (level: number, animate = true): void => {
    if (level === 0) return fit(animate);
    const t = svgEl.__zoom as ZoomTransform | undefined;
    const box = svgEl.getBoundingClientRect();
    const k = data.threshold[level]! * 1.3;
    if (level === 1 || level === 2) {
      // Level 1: the crate cards filling the view. Level 2: the primitives, as many as fit at a readable zoom.
      const cards = Object.values(data.info).filter((n) => !n.group && n.level === level);
      if (cards.length > 0) {
        const x0 = Math.min(...cards.map((n) => n.x)), y0 = Math.min(...cards.map((n) => n.y));
        const x1 = Math.max(...cards.map((n) => n.x + n.w)), y1 = Math.max(...cards.map((n) => n.y + n.h));
        const fitK = Math.min((box.width - 48) / (x1 - x0), (box.height - 120) / (y1 - y0));
        const ceiling = level === 1 ? data.threshold[2]! * 0.95 : data.threshold[3]! * 0.95;
        return go((x0 + x1) / 2, (y0 + y1) / 2 - 20 / fitK, Math.min(ceiling, Math.max(data.threshold[level]! * 1.05, fitK)), animate);
      }
      return go(data.width / 2, data.height / 2, Math.max(k, Math.min(box.width / data.width, box.height / data.height) * 0.9), animate);
    }
    // Level 3 zooms in where the reader already is.
    const now = t ?? zoomIdentity;
    go((cx - now.x) / now.k, (cy - now.y) / now.k, k, animate);
  };

  for (const b of buttons) b.addEventListener("click", () => jump(Number(b.dataset.level)));
  const scaleBy = (f: number): void => {
    const now = svgEl.__zoom ?? zoomIdentity;
    const k = Math.max(minK(), Math.min(12 / base(), now.k * f));
    tween(zoomIdentity.translate(cx - ((cx - now.x) / now.k) * k, cy - ((cy - now.y) / now.k) * k).scale(k), 200);
  };
  host.querySelector("[data-zoom=in]")!.addEventListener("click", () => scaleBy(1.5));
  host.querySelector("[data-zoom=out]")!.addEventListener("click", () => scaleBy(1 / 1.5));
  host.querySelector("[data-zoom=fit]")!.addEventListener("click", () => fit());

  // Focus and context: the hovered box, else the selected one, lights itself,
  // its wires and the boxes at their other ends; the rest dims.
  const wires = [...host.querySelectorAll<SVGElement>(".wire, .wire-label")];
  const boxesById = new Map([...host.querySelectorAll<SVGElement>(".box")].map((el) => [el.dataset.id!, el]));
  let hovered: string | null = null;
  let opened: string | null = null;
  const light = (): void => {
    const id = hovered ?? opened;
    for (const el of host.querySelectorAll("[data-lit], [data-near]")) {
      el.removeAttribute("data-lit");
      el.removeAttribute("data-near");
    }
    if (id === null) return void delete host.dataset.focus;
    host.dataset.focus = id;
    boxesById.get(id)?.setAttribute("data-lit", "");
    for (const wire of wires) {
      const { from, to } = wire.dataset;
      if (from !== id && to !== id) continue;
      wire.setAttribute("data-near", "");
      boxesById.get(from === id ? to! : from!)?.setAttribute("data-lit", "");
    }
  };
  svgEl.addEventListener("pointerover", (event) => {
    if ((event as PointerEvent).pointerType === "touch") return;
    const box = (event.target as Element).closest<SVGElement>(".box.node");
    const id = box?.dataset.id ?? null;
    if (id !== hovered) { hovered = id; light(); }
  });
  svgEl.addEventListener("pointerleave", () => { hovered = null; light(); });

  const toggle = host.querySelector<HTMLButtonElement>("[data-labels-toggle]")!;
  const allLabels = (on: boolean): void => {
    host.toggleAttribute("data-labels", on);
    toggle.setAttribute("aria-pressed", String(on));
  };
  toggle.addEventListener("click", () => allLabels(!host.hasAttribute("data-labels")));

  // The side panel.
  const panel = host.querySelector<HTMLElement>(".atlas-panel")!;
  const part = (name: string): HTMLElement => panel.querySelector<HTMLElement>(`.atlas-panel-${name}`)!;
  let returnTo: HTMLElement | SVGElement | null = null;
  const open = (id: string, from: SVGElement | null): void => {
    const node = data.info[id];
    if (node === undefined) return;
    opened = id;
    returnTo = from;
    panel.dataset.crate = node.crate ?? "";
    if (node.crate === null) delete panel.dataset.crate;
    part("eyebrow").textContent = [node.group ? "group" : node.kind, node.crate, `level ${node.level} · ${LEVELS[node.level]}`].filter(Boolean).join(" · ");
    part("title").textContent = node.label;
    part("what").textContent = node.summary || (node.slug === null ? "No concept page yet." : "");
    for (const name of ["problem", "why"] as const) {
      const text = node[name] ?? "";
      part(name).textContent = text;
      panel.querySelector<HTMLElement>(`[data-block="${name}"]`)!.hidden = text === "";
    }
    const link = part("link") as HTMLAnchorElement;
    link.hidden = node.slug === null;
    if (node.slug !== null) link.href = url(`/atlas/${node.slug}`);
    part("zoom").hidden = !node.group && node.level >= current;
    panel.hidden = false;
    for (const el of host.querySelectorAll("[data-selected]")) el.removeAttribute("data-selected");
    host.querySelector(`.box[data-id="${CSS.escape(id)}"]`)?.setAttribute("data-selected", "");
    light();
    part("close").focus({ preventScroll: true });
  };
  const close = (): void => {
    panel.hidden = true;
    opened = null;
    for (const el of host.querySelectorAll("[data-selected]")) el.removeAttribute("data-selected");
    light();
    (returnTo as HTMLElement | null)?.focus?.({ preventScroll: true });
  };
  part("close").addEventListener("click", close);
  part("zoom").addEventListener("click", () => { if (opened !== null) focus(opened); });

  // A click opens the panel instead of leaving; a modified click still follows the link.
  let downAt: { x: number; y: number } | null = null;
  svgEl.addEventListener("pointerdown", (event) => { downAt = { x: event.clientX, y: event.clientY }; });
  svgEl.addEventListener("click", (event) => {
    const box = (event.target as Element).closest<SVGElement>(".box");
    const moved = downAt !== null && Math.hypot(event.clientX - downAt.x, event.clientY - downAt.y) > 4;
    if (box === null) {
      if (!moved) close();
      return;
    }
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) return;
    event.preventDefault();
    if (!moved) open(box.dataset.id!, box);
  });
  svgEl.addEventListener("dblclick", (event) => {
    const box = (event.target as Element).closest<SVGElement>(".box");
    if (box !== null) {
      event.preventDefault();
      focus(box.dataset.id!);
    }
  });

  // Keyboard: the canvas has focus, or nothing in particular does.
  document.addEventListener("keydown", (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const target = event.target as HTMLElement;
    if (target.closest("input, textarea, select, [contenteditable]")) return;
    if (!(target === document.body || host.contains(target))) return;
    const PAN = 80;
    switch (event.key) {
      case "+": case "=": scaleBy(1.5); break;
      case "-": case "_": scaleBy(1 / 1.5); break;
      case "0": fit(); break;
      case "ArrowLeft": svg.call(behaviour.translateBy as never, PAN / ((svgEl.__zoom as ZoomTransform).k * base()) * 1, 0); break;
      case "ArrowRight": svg.call(behaviour.translateBy as never, -PAN / ((svgEl.__zoom as ZoomTransform).k * base()), 0); break;
      case "ArrowUp": svg.call(behaviour.translateBy as never, 0, PAN / ((svgEl.__zoom as ZoomTransform).k * base())); break;
      case "ArrowDown": svg.call(behaviour.translateBy as never, 0, -PAN / ((svgEl.__zoom as ZoomTransform).k * base())); break;
      case "l": case "L": allLabels(!host.hasAttribute("data-labels")); break;
      case "Escape": if (!panel.hidden) close(); else return; break;
      case "Enter": {
        const box = target.closest<SVGElement>(".box");
        if (box === null) return;
        open(box.dataset.id!, box);
        break;
      }
      default: return;
    }
    event.preventDefault();
  });

  addEventListener("resize", () => { syncExtent(); paint((svgEl.__zoom as ZoomTransform | undefined) ?? zoomIdentity); });

  // Start where the hash says, else on the whole map.
  const start = (): void => {
    const slug = decodeURIComponent(location.hash.slice(1));
    const id = slug === "" ? undefined : Object.keys(data.info).find((key) => data.info[key]!.slug === slug || key === slug);
    fit(false);
    paint((svgEl.__zoom as ZoomTransform | undefined) ?? zoomIdentity);
    const level = /^level-([0-3])$/u.exec(slug)?.[1];
    if (level !== undefined) jump(Number(level), false);
    if (id !== undefined) {
      focus(id, 1.5, false);
      open(id, host.querySelector<SVGElement>(`.box[data-id="${CSS.escape(id)}"]`));
    }
  };
  start();
  addEventListener("hashchange", start);
};

declare global {
  interface SVGSVGElement { __zoom?: ZoomTransform }
}
