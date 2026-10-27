/**
 * Hover, tap and keys for a wire sample, both ways: a byte lights its field,
 * its row and the source it points at; a piece of source lights every field
 * that points into it. The byte map is built at build time
 * (`lib/atlas/layouts.ts`, `lib/atlas/hex-lines.ts`); this only lights things.
 */
import type { Span } from "../lib/atlas/layouts.ts";

interface Data {
  spans: Span[];
  owner: number[];
}

const VIEW_KEY = "wsample-view";

const escape = (s: string): string =>
  s.replace(/[&<>"]/gu, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c);

const whereOf = (span: Span): string =>
  span.record
    ? `${span.region} · ${span.rowName ?? `${span.record} row ${span.row}`}`
    : span.rowName
      ? `${span.region}[${span.row}] (${span.rowName})`
      : `${span.region}${span.row ? `[${span.row}]` : ""}`;

const shortOf = (span: Span): string =>
  `${span.region}${span.rowName ? `[${span.rowName}]` : `[${span.row}]`}.${span.field}`;

export const mountWireSample = (root: HTMLElement): void => {
  const json = root.querySelector("[data-wsample-json]")?.textContent;
  const grids = [...root.querySelectorAll<HTMLElement>("[data-hex]")];
  const readout = root.querySelector<HTMLElement>("[data-readout]");
  const usfm = root.querySelector<HTMLElement>("[data-usfm]");
  if (!json || grids.length === 0 || !readout || !usfm) return;
  const data = JSON.parse(json) as Data;

  // Every copy of a byte (one per layout), and every byte of a record row.
  const byIndex = new Map<number, HTMLElement[]>();
  const byRow = new Map<string, HTMLElement[]>();
  for (const el of root.querySelectorAll<HTMLElement>(".whex i[data-i]")) {
    const i = Number(el.dataset["i"]);
    byIndex.set(i, [...(byIndex.get(i) ?? []), el]);
    const r = el.dataset["r"];
    if (r) byRow.set(r, [...(byRow.get(r) ?? []), el]);
  }
  const blobCells = [...root.querySelectorAll<HTMLElement>(".wblob-cells [data-i]")];
  const segments = [...usfm.querySelectorAll<HTMLElement>("span[data-b0]")].map((el) => ({
    el, from: Number(el.dataset["b0"]), to: Number(el.dataset["b1"]),
  }));
  const reach = (span: Span): [number, number] | null =>
    span.src ? [span.src[0], Math.max(span.src[1], span.src[0] + 1)] : null;

  const blank = readout.innerHTML;
  let lit: [HTMLElement, string][] = [];
  // What a click or tap holds: a span index, or a segment index as ~n.
  let pinned: number | null = null;

  const clear = (): void => {
    for (const [el, attr] of lit) el.removeAttribute(attr);
    lit = [];
  };
  const light = (els: Iterable<HTMLElement>, attr = "data-on"): void => {
    for (const el of els) {
      el.setAttribute(attr, "");
      lit.push([el, attr]);
    }
  };
  const lightSpan = (span: Span): void => {
    const row = root.querySelector<HTMLElement>(`.whex i[data-i="${span.start}"]`)?.dataset["r"];
    if (row) light(byRow.get(row) ?? [], "data-near");
    for (let b = span.start; b < span.end; b++) light(byIndex.get(b) ?? []);
    light(blobCells.filter((el) => Number(el.dataset["i"]) === span.start));
  };
  const visibleCell = (i: number): HTMLElement | undefined =>
    byIndex.get(i)?.find((el) => !el.closest("[hidden]"));

  const showSpan = (index: number, scroll = false): void => {
    clear();
    const span = data.spans[index];
    if (!span) {
      readout.innerHTML = blank;
      return;
    }
    lightSpan(span);
    let html = `<span class="font-mono">${escape(whereOf(span))}</span> · <b class="font-mono">${escape(span.field)}</b> = <span class="font-mono">${escape(span.value)}</span> <span class="text-on-surface-tertiary">bytes ${span.start}–${span.end - 1}</span>`;
    const range = reach(span);
    if (span.src && range) {
      const [from, to] = range;
      const hits = segments.filter((s) => s.from >= from && s.to <= to && s.from < to);
      light(hits.map((s) => s.el));
      if (scroll) hits[0]?.el.scrollIntoView({ block: "nearest", inline: "nearest" });
      html += ` <span class="wsrc">→ source ${span.src[0]}–${span.src[1]}: <q>${escape(span.text ?? "")}</q></span>`;
    }
    readout.innerHTML = html;
  };

  // The inverse: a piece of source, and every field whose range covers it.
  const showSegment = (n: number, scroll = false): void => {
    clear();
    const seg = segments[n];
    if (!seg) {
      readout.innerHTML = blank;
      return;
    }
    light([seg.el]);
    const hits: Span[] = [];
    for (const span of data.spans) {
      const range = reach(span);
      if (range && range[0] < seg.to && range[1] > seg.from) {
        hits.push(span);
        lightSpan(span);
      }
    }
    if (scroll && hits[0]) visibleCell(hits[0].start)?.scrollIntoView({ block: "nearest", inline: "nearest" });
    const text = seg.el.textContent ?? "";
    const quote = `<q>${escape(text.length > 60 ? `${text.slice(0, 60)}…` : text)}</q>`;
    const names = hits.slice(0, 6).map((s) => `<span class="font-mono">${escape(shortOf(s))}</span>`).join(", ");
    readout.innerHTML = hits.length === 0
      ? `source ${seg.from}–${seg.to}: ${quote} <span class="text-on-surface-tertiary">no field points here</span>`
      : `source ${seg.from}–${seg.to}: ${quote} ← ${hits.length} field${hits.length === 1 ? "" : "s"}: ${names}${hits.length > 6 ? `, <span class="text-on-surface-tertiary">${hits.length - 6} more</span>` : ""}`;
  };

  const spanOf = (target: EventTarget | null): number => {
    const el = (target as HTMLElement | null)?.closest<HTMLElement>("[data-i]");
    if (!el) return -1;
    return data.owner[Number(el.dataset["i"])] ?? -1;
  };
  const segmentOf = (target: EventTarget | null): number => {
    const el = (target as HTMLElement | null)?.closest<HTMLElement>("span[data-b0]");
    return el ? segments.findIndex((s) => s.el === el) : -1;
  };
  const restore = (): void => {
    if (pinned === null) showSpan(-1);
    else if (pinned >= 0) showSpan(pinned);
    else showSegment(~pinned);
  };

  // Hex and blob cells.
  for (const box of [...grids, root.querySelector<HTMLElement>(".wblob-cells")]) {
    if (!box) continue;
    box.addEventListener("pointerover", (e) => {
      if (pinned === null && e.pointerType !== "touch") showSpan(spanOf(e.target));
    });
    box.addEventListener("pointerleave", () => pinned === null && showSpan(-1));
    box.addEventListener("click", (e) => {
      const i = spanOf(e.target);
      if (i < 0) return;
      pinned = i === pinned ? null : i;
      showSpan(i, true);
    });
  }

  // Source.
  usfm.addEventListener("pointerover", (e) => {
    if (pinned === null && e.pointerType !== "touch") showSegment(segmentOf(e.target));
  });
  usfm.addEventListener("pointerleave", () => pinned === null && showSegment(-1));
  usfm.addEventListener("click", (e) => {
    const n = segmentOf(e.target);
    if (n < 0) return;
    pinned = ~n === pinned ? null : ~n;
    showSegment(n, true);
  });

  // Keys: one tab stop per pane; arrows walk fields or pieces of source.
  const rove = (els: HTMLElement[], onFocus: (el: HTMLElement) => void): void => {
    els.forEach((el, k) => el.setAttribute("tabindex", k === 0 ? "0" : "-1"));
    els.forEach((el, k) => {
      el.addEventListener("focus", () => {
        for (const other of els) other.setAttribute("tabindex", "-1");
        el.setAttribute("tabindex", "0");
        onFocus(el);
      });
      el.addEventListener("blur", () => restore());
      el.addEventListener("keydown", (e) => {
        const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
        const to = e.key === "Home" ? 0 : e.key === "End" ? els.length - 1 : step === undefined ? -1 : k + step;
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          el.click();
          return;
        }
        if (to < 0 || to >= els.length) return;
        e.preventDefault();
        els[to]?.focus();
      });
    });
  };
  rove(segments.map((s) => s.el), (el) => showSegment(segmentOf(el), true));
  // In each grid, the first visible byte of every field is a stop.
  for (const grid of grids) {
    const firsts = [...grid.querySelectorAll<HTMLElement>("i[data-i]")].filter((el) => {
      const i = Number(el.dataset["i"]);
      const own = data.owner[i] ?? -1;
      return own >= 0 && data.spans[own]?.start === i;
    });
    rove(firsts, (el) => showSpan(spanOf(el), true));
  }

  // A region chip lights its whole extent.
  for (const chip of root.querySelectorAll<HTMLElement>(".wregions [data-from]")) {
    const on = (): void => {
      if (pinned !== null) return;
      clear();
      for (let b = Number(chip.dataset["from"]); b < Number(chip.dataset["to"]); b++) light(byIndex.get(b) ?? []);
    };
    chip.addEventListener("pointerenter", on);
    chip.addEventListener("focus", on);
    chip.addEventListener("pointerleave", () => pinned === null && clear());
    chip.addEventListener("blur", () => pinned === null && clear());
  }

  // The layout toggle, remembered per viewer when storage allows.
  const buttons = [...root.querySelectorAll<HTMLButtonElement>("[data-view-btn]")];
  const choose = (name: string): void => {
    if (!grids.some((g) => g.dataset["hex"] === name)) return;
    for (const g of grids) g.hidden = g.dataset["hex"] !== name;
    for (const b of buttons) b.setAttribute("aria-pressed", String(b.dataset["viewBtn"] === name));
  };
  for (const b of buttons) {
    b.addEventListener("click", () => {
      const name = b.dataset["viewBtn"] ?? "bytes";
      choose(name);
      try {
        localStorage.setItem(VIEW_KEY, name);
      } catch {
        // Storage is a convenience.
      }
    });
  }
  try {
    const saved = new URLSearchParams(location.search).get("hex") ?? localStorage.getItem(VIEW_KEY);
    if (saved) choose(saved);
  } catch {
    // Storage is a convenience.
  }
};
