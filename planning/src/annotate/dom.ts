/**
 * Six lines of DOM convenience, so the panel reads as markup rather than as
 * two hundred `createElement` calls.
 *
 * No framework, by rule — `pnpm boundaries` fails this folder for importing
 * one. The panel is small enough that a render-everything-on-change approach
 * is correct, and correct is worth more here than clever.
 */

type Attributes = Readonly<Record<string, string | boolean | undefined>>;

export const el = <K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attributes: Attributes = {},
  children: readonly (Node | string)[] = [],
): HTMLElementTagNameMap[K] => {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(attributes)) {
    if (value === undefined || value === false) continue;
    node.setAttribute(name, value === true ? "" : value);
  }
  node.append(...children);
  return node;
};

export const on = <K extends keyof HTMLElementEventMap>(
  node: HTMLElement,
  type: K,
  handler: (event: HTMLElementEventMap[K]) => void,
): HTMLElement => {
  node.addEventListener(type, handler);
  return node;
};

/** A stable-enough id for a comment in one session. */
export const newId = (): string =>
  `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
