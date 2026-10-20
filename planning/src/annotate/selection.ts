/**
 * Highlight text, press the chip (or `⌥C`), say something about it.
 *
 * Works in the annotator's ordinary `interact` mode, because comment mode
 * swallows the mouse and you cannot select text in it. The comment is anchored
 * to the block the selection started in — its `data-loc` is the line — and
 * carries the highlighted words as a quote.
 */

import { el } from "./dom.ts";
import type { Annotator } from "./types.ts";

const BLOCK = "[data-loc]";

export const mountSelection = (annotator: Annotator, scope: Element): (() => void) => {
  const chip = el("button", { type: "button", class: "plans-select-chip" }, ["Comment"]);
  chip.style.display = "none";
  document.body.append(chip);

  let held: { element: Element; quote: string } | null = null;

  const current = (): { element: Element; quote: string; rect: DOMRect } | null => {
    const selection = document.getSelection();
    if (selection === null || selection.isCollapsed || selection.rangeCount === 0) return null;
    const quote = selection.toString().trim();
    if (quote === "") return null;
    const range = selection.getRangeAt(0);
    const start = range.startContainer;
    const node = start instanceof Element ? start : start.parentElement;
    if (node === null || !scope.contains(node)) return null;
    return { element: node.closest(BLOCK) ?? node, quote, rect: range.getBoundingClientRect() };
  };

  const show = (): void => {
    if (annotator.mode() === "comment") return;
    const found = current();
    if (found === null) {
      chip.style.display = "none";
      held = null;
      return;
    }
    held = { element: found.element, quote: found.quote };
    chip.style.display = "block";
    chip.style.left = `${String(found.rect.left + window.scrollX)}px`;
    chip.style.top = `${String(found.rect.bottom + window.scrollY + 6)}px`;
  };

  const open = (): void => {
    if (held === null) return;
    annotator.compose(held.element, held.quote);
    chip.style.display = "none";
    held = null;
  };

  const onUp = (): void => {
    // After the browser has settled the selection.
    globalThis.setTimeout(show, 0);
  };
  const onKey = (event: KeyboardEvent): void => {
    if (event.altKey && event.code === "KeyC" && held !== null) {
      event.preventDefault();
      open();
    }
  };
  // mousedown, not click: a click would collapse the selection first.
  chip.addEventListener("mousedown", (event) => {
    event.preventDefault();
    open();
  });

  document.addEventListener("mouseup", onUp);
  document.addEventListener("keyup", onUp);
  document.addEventListener("keydown", onKey);
  return () => {
    document.removeEventListener("mouseup", onUp);
    document.removeEventListener("keyup", onUp);
    document.removeEventListener("keydown", onKey);
    chip.remove();
  };
};
