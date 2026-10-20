/**
 * Turning the element under the cursor into something a reader can find again.
 *
 * In order of usefulness:
 *
 *   1. `data-loc` — the file, line and column the markup was written at, put
 *      there by the JSX-location Vite transform. This is the whole point. A
 *      comment carrying `src/app/ui/review/UnitCard.tsx:42` is a place to
 *      go; one carrying `.flex.items-center.gap-2` is a grep.
 *   2. A short CSS-ish path, for a project without that transform — this
 *      module must still be useful when dropped somewhere that has not set one
 *      up.
 *   3. The visible text nearby, because "the Find Project card" is how a
 *      person will describe it anyway.
 */

/** Written by `tools/vite/jsxLocation.ts`. Kept as a literal so this file depends on nothing. */
const LOCATION_ATTRIBUTE = "data-loc";

/** The nearest element that knows where it came from, including the target itself. */
export const sourceOf = (element: Element): string | null =>
  element.closest(`[${LOCATION_ATTRIBUTE}]`)?.getAttribute(LOCATION_ATTRIBUTE) ?? null;

const nth = (element: Element): string => {
  const parent = element.parentElement;
  if (parent === null) return "";
  const siblings = [...parent.children].filter((child) => child.tagName === element.tagName);
  if (siblings.length < 2) return "";
  return `:nth-of-type(${String(siblings.indexOf(element) + 1)})`;
};

/**
 * A path short enough to read. Three levels, ids win outright, and at most two
 * classes per step — a full Tailwind class list is longer than the comment and
 * identifies nothing.
 */
export const selectorOf = (element: Element): string => {
  const steps: string[] = [];
  let at: Element | null = element;
  for (let depth = 0; depth < 3 && at !== null && at.tagName !== "BODY"; depth += 1) {
    if (at.id !== "") {
      steps.unshift(`#${at.id}`);
      break;
    }
    const classes = [...at.classList]
      .slice(0, 2)
      .map((name) => `.${name}`)
      .join("");
    steps.unshift(`${at.tagName.toLowerCase()}${classes}${nth(at)}`);
    at = at.parentElement;
  }
  return steps.join(" > ");
};

const MAX_NEARBY = 60;

/**
 * What this element itself says — its OWN text, not its descendants'.
 *
 * `textContent` was the obvious first answer and it is wrong, because it
 * concatenates every descendant with no separator at all. Clicking a card
 * produced
 *
 *   "ButtonsVariantsprimarysecondarytertiarydangerPressedprimarysecondary…"
 *
 * which is not a label, is not readable, and buries the one useful line of the
 * comment under eighty characters of run-together nouns.
 *
 * So: only DIRECT child text nodes. A `<button>primary</button>` still says
 * "primary"; a container of other elements says nothing, and falls through to
 * its accessible name if it has one. Saying nothing is the right answer there —
 * the source location already identifies the element, and an empty quote is
 * better than a misleading one.
 */
export const nearbyTextOf = (element: Element): string => {
  const own = [...element.childNodes]
    .filter((node) => node.nodeType === Node.TEXT_NODE)
    .map((node) => node.textContent ?? "")
    .join(" ")
    .replace(/\s+/gu, " ")
    .trim();
  if (own !== "") return own.length > MAX_NEARBY ? `${own.slice(0, MAX_NEARBY)}…` : own;
  const labelled =
    element.getAttribute("aria-label") ??
    element.getAttribute("placeholder") ??
    element.getAttribute("alt") ??
    element.getAttribute("title") ??
    "";
  return labelled.trim();
};

/** Ours, and already reported on its own line — repeating them is noise. */
const SKIPPED_DATA = new Set(["data-loc", "data-ref", "data-sefer-annotate", "data-sefer-annotate-mode"]);

const MAX_DATA_VALUE = 32;

/**
 * The element's own `data-*` attributes, which in a hand-written codebase are
 * usually the most identifying thing on it — `data-screen`, `data-testid`,
 * `data-state`. Cheap to carry and often the difference between "some row" and
 * "the row for John".
 *
 * Capped at three and truncated, because a serialised blob in a data attribute
 * would otherwise take the whole comment.
 */
export const dataAttributesOf = (element: Element): string => {
  // `dataset` is an HTMLElement convenience; an SVG or MathML node reached by a
  // click has none, and reading the attributes directly works for all three.
  const parts: string[] = [];
  for (const attribute of element.attributes) {
    if (!attribute.name.startsWith("data-")) continue;
    if (SKIPPED_DATA.has(attribute.name)) continue;
    const name = attribute.name.slice("data-".length);
    const value = attribute.value;
    if (value === "") continue;
    const short = value.length > MAX_DATA_VALUE ? `${value.slice(0, MAX_DATA_VALUE)}…` : value;
    parts.push(`${name}=${short}`);
    if (parts.length === 3) break;
  }
  return parts.join(" ");
};

/**
 * Dark or light, as the page actually resolved it rather than as it was asked
 * for — a comment about contrast is useless without knowing which one it was
 * about. Reads the stamped attribute first (an explicit choice), then the
 * media query (the "system" default).
 */
export const themeOf = (): string => {
  const stamped =
    document.documentElement.dataset["theme"] ??
    (document.documentElement.classList.contains("dark") ? "dark" : null);
  if (stamped !== null && stamped !== undefined) return stamped;
  return globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches === true
    ? "dark"
    : "light";
};

export const viewportOf = (): string =>
  `${String(globalThis.innerWidth)}×${String(globalThis.innerHeight)}`;

/** Written by the page's own components. One short name per meaningful place. */
const REF_ATTRIBUTE = "data-ref";

/**
 * Every `data-ref` from the outermost to the element itself:
 * `item:planning-site › calls:respond-flow › row:claimDue()`.
 *
 * The element alone is often a reused component — "a button" — and says
 * nothing about which list or which flow it belongs to. The chain does.
 */
export const anchorsOf = (element: Element): string => {
  const names: string[] = [];
  for (let at: Element | null = element; at !== null; at = at.parentElement) {
    const ref = at.getAttribute(REF_ATTRIBUTE);
    if (ref !== null && ref !== "") names.unshift(ref);
  }
  return names.join(" › ");
};

const HEADINGS = "h1, h2, h3, h4";

/**
 * The heading the element sits under: the nearest h1–h4 before it in document
 * order, or the one it is inside. "Under 'Storage'" is how a person locates a
 * paragraph, and it survives a line number going stale.
 */
export const sectionOf = (element: Element): string => {
  const own = element.closest(HEADINGS);
  if (own !== null) return (own.textContent ?? "").trim();
  const all = [...document.querySelectorAll(HEADINGS)];
  let found: Element | null = null;
  for (const heading of all) {
    // DOCUMENT_POSITION_FOLLOWING: `element` comes after `heading`.
    if (heading.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING) found = heading;
    else break;
  }
  return (found?.textContent ?? "").replace(/\s+/gu, " ").trim();
};
