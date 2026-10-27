/**
 * Drag to pan, ⌘/Ctrl + scroll (or pinch) to zoom, double-click to reset, on
 * every `[data-graph]` host: an SVG whose drawing sits in `<g data-viewport>`.
 * `data-graph="fit"` only zooms when the drawing is wider or taller than its
 * box; one that fits stays a plain picture.
 */
import { select } from "d3-selection";
import { zoom, zoomIdentity } from "d3-zoom";

const overflows = (host: HTMLElement, svg: SVGSVGElement): boolean => {
  const width = Number(svg.getAttribute("width") ?? 0);
  const height = Number(svg.getAttribute("height") ?? 0);
  return width > host.clientWidth + 1 || height > host.clientHeight + 1;
};

export const mountPanZoom = (): void => {
  for (const host of document.querySelectorAll<HTMLElement>("[data-graph]:not([data-zoom])")) {
    const element = host.querySelector("svg");
    if (element === null) continue;
    if (host.dataset.graph === "fit" && !overflows(host, element)) continue;
    host.dataset.zoom = "";
    const svg = select(element);
    const viewport = svg.select<SVGGElement>("[data-viewport]");
    const behaviour = zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.4, 4])
      // Plain wheel scrolls the page; a modifier zooms. Drags always pan.
      .filter((event: Event) => event.type !== "wheel" || (event as WheelEvent).ctrlKey || (event as WheelEvent).metaKey)
      .on("zoom", (event) => viewport.attr("transform", String(event.transform)));
    svg.call(behaviour).on("dblclick.zoom", () => svg.call(behaviour.transform, zoomIdentity));
  }
};
