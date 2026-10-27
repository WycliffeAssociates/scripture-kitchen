/**
 * Hover and tap for a wire sample. The byte map is built at build time
 * (`lib/atlas/layouts.ts`); this only lights bytes and source.
 */
import type { Span } from "../lib/atlas/layouts.ts";

interface Data {
  spans: Span[];
  owner: number[];
}

const escape = (s: string): string =>
  s.replace(/[&<>"]/gu, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c);

export const mountWireSample = (root: HTMLElement): void => {
  const json = root.querySelector("[data-wsample-json]")?.textContent;
  const hex = root.querySelector<HTMLElement>("[data-hex]");
  const readout = root.querySelector<HTMLElement>("[data-readout]");
  const usfm = root.querySelector<HTMLElement>("[data-usfm]");
  if (!json || !hex || !readout || !usfm) return;
  const data = JSON.parse(json) as Data;
  const cells = [...hex.querySelectorAll<HTMLElement>("i[data-i]")];
  const blobCells = [...root.querySelectorAll<HTMLElement>(".wblob-cells [data-i]")];
  const segments = [...usfm.querySelectorAll<HTMLElement>("span[data-b0]")].map((el) => ({
    el, from: Number(el.dataset["b0"]), to: Number(el.dataset["b1"]),
  }));
  const blank = readout.innerHTML;
  let lit: HTMLElement[] = [];
  let pinned = -1;

  const clear = (): void => {
    for (const el of lit) el.removeAttribute("data-on");
    lit = [];
  };
  const light = (els: Iterable<HTMLElement>): void => {
    for (const el of els) {
      el.setAttribute("data-on", "");
      lit.push(el);
    }
  };

  const show = (index: number): void => {
    clear();
    const span = data.spans[index];
    if (!span) {
      readout.innerHTML = blank;
      return;
    }
    light(cells.slice(span.start, span.end));
    light(blobCells.filter((el) => Number(el.dataset["i"]) === span.start));
    const where = span.record
      ? `${span.region} · ${span.rowName ?? `${span.record} row ${span.row}`}`
      : span.rowName
        ? `${span.region}[${span.row}] (${span.rowName})`
        : `${span.region}${span.row ? `[${span.row}]` : ""}`;
    let html = `<span class="font-mono">${escape(where)}</span> · <b class="font-mono">${escape(span.field)}</b> = <span class="font-mono">${escape(span.value)}</span> <span class="text-on-surface-tertiary">bytes ${span.start}–${span.end - 1}</span>`;
    if (span.src) {
      const [from, to] = span.src;
      const hits = segments.filter((s) => s.from >= from && s.to <= Math.max(to, from + 1) && s.from < Math.max(to, from + 1));
      light(hits.map((s) => s.el));
      hits[0]?.el.scrollIntoView({ block: "nearest", inline: "nearest" });
      html += ` <span class="wsrc">→ source ${from}–${to}: <q>${escape(span.text ?? "")}</q></span>`;
    }
    readout.innerHTML = html;
  };

  const indexOf = (target: EventTarget | null): number => {
    const el = (target as HTMLElement | null)?.closest<HTMLElement>("[data-i]");
    if (!el) return -1;
    return data.owner[Number(el.dataset["i"])] ?? -1;
  };

  for (const box of [hex, root.querySelector<HTMLElement>(".wblob-cells")]) {
    if (!box) continue;
    box.addEventListener("pointerover", (e) => {
      if (pinned < 0 && (e as PointerEvent).pointerType !== "touch") show(indexOf(e.target));
    });
    box.addEventListener("pointerleave", () => {
      if (pinned < 0) show(-1);
    });
    box.addEventListener("click", (e) => {
      const i = indexOf(e.target);
      pinned = i === pinned ? -1 : i;
      show(i);
    });
  }

  // A region chip lights its whole extent.
  for (const chip of root.querySelectorAll<HTMLElement>(".wregions [data-from]")) {
    const on = (): void => {
      if (pinned >= 0) return;
      clear();
      light(cells.slice(Number(chip.dataset["from"]), Number(chip.dataset["to"])));
    };
    chip.addEventListener("pointerenter", on);
    chip.addEventListener("focus", on);
    chip.addEventListener("pointerleave", () => pinned < 0 && clear());
    chip.addEventListener("blur", () => pinned < 0 && clear());
  }
};
