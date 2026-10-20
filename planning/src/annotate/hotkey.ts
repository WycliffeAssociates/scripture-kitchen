/**
 * Which keystroke turns comment mode on, and how somebody changes it without
 * editing code.
 *
 * ## Why a chord rather than a letter
 *
 * A bare `c` is the right shortcut on a prototyping surface and the wrong one
 * over a real editor, so the annotator shipped with `hotkey: "c"` on `/design`
 * and `null` everywhere else. That is the correct default and a poor ceiling:
 * `⌥C` is safe over CodeMirror, over a text field, over anything, because no
 * application binds it. Supporting a modifier is what lets the hotkey be ON in
 * the places it was previously turned off.
 *
 * ## Why `code` and not `key`
 *
 * On macOS, Option+C produces `event.key === "ç"`. Matching on `key` therefore
 * fails for exactly the chords worth recording. `event.code` is the physical
 * key and is unaffected by modifiers or layout remapping, so the chord is
 * identified by code where there is one, and falls back to `key` for the keys
 * that have no stable code worth naming.
 *
 * ## Why localStorage
 *
 * A hotkey is a preference about a person's hands, not about the page. It
 * should survive a reload and apply on every screen, which rules out the URL
 * (a shareable link is about the design, not about you) and sessionStorage
 * (where the comment batch lives, and where forgetting is a feature).
 */

/** One keystroke, modifiers included. */
export interface Hotkey {
  /** `event.code`, e.g. `"KeyC"`. Null for a key identified by `key` alone. */
  readonly code: string | null;
  /** `event.key`, lowercased. The fallback identity, and the display label. */
  readonly key: string;
  readonly alt: boolean;
  readonly ctrl: boolean;
  readonly meta: boolean;
  readonly shift: boolean;
}

const STORAGE = "sefer.annotate.hotkey";
/** Written when somebody records "no hotkey", which must outrank the default. */
const NONE = "none";

const MODIFIER_KEYS = new Set(["Alt", "Control", "Meta", "Shift", "AltGraph", "CapsLock"]);

/** The code a plain letter or digit would have arrived with, so `"c"` still works. */
const codeFor = (key: string): string | null => {
  if (/^[a-z]$/u.test(key)) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/u.test(key)) return `Digit${key}`;
  return null;
};

/** Accepts the option as written — a bare key, a chord, or nothing. */
export const asHotkey = (value: string | Hotkey | null | undefined): Hotkey | null => {
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") return value;
  const key = value.toLowerCase();
  return { code: codeFor(key), key, alt: false, ctrl: false, meta: false, shift: false };
};

/** The keystroke that just happened, or null if it was only a modifier. */
export const hotkeyFrom = (event: KeyboardEvent): Hotkey | null => {
  if (MODIFIER_KEYS.has(event.key)) return null;
  return {
    code: event.code === "" ? null : event.code,
    key: event.key.toLowerCase(),
    alt: event.altKey,
    ctrl: event.ctrlKey,
    meta: event.metaKey,
    shift: event.shiftKey,
  };
};

export const hotkeyMatches = (event: KeyboardEvent, hotkey: Hotkey): boolean => {
  if (
    event.altKey !== hotkey.alt ||
    event.ctrlKey !== hotkey.ctrl ||
    event.metaKey !== hotkey.meta ||
    event.shiftKey !== hotkey.shift
  ) {
    return false;
  }
  return hotkey.code === null ? event.key.toLowerCase() === hotkey.key : event.code === hotkey.code;
};

const LABELS: Readonly<Record<string, string>> = {
  Escape: "Esc",
  " ": "Space",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

export const describeHotkey = (hotkey: Hotkey | null): string => {
  if (hotkey === null) return "none";
  const base =
    hotkey.code !== null && /^Key[A-Z]$/u.test(hotkey.code)
      ? hotkey.code.slice(3)
      : hotkey.code !== null && /^Digit[0-9]$/u.test(hotkey.code)
        ? hotkey.code.slice(5)
        : (LABELS[hotkey.key] ?? (hotkey.key.length === 1 ? hotkey.key.toUpperCase() : hotkey.key));
  return (
    (hotkey.ctrl ? "⌃" : "") +
    (hotkey.alt ? "⌥" : "") +
    (hotkey.shift ? "⇧" : "") +
    (hotkey.meta ? "⌘" : "") +
    base
  );
};

/**
 * What this browser has been told to use, if anything.
 *
 * Three states, and they are all distinct: a recorded chord, a recorded
 * "nothing" (which must beat the host's default, or turning the hotkey off
 * would not stick), and no answer at all (use the host's default).
 */
export const readSavedHotkey = (): Hotkey | null | undefined => {
  try {
    const held = localStorage.getItem(STORAGE);
    if (held === null) return undefined;
    if (held === NONE) return null;
    const parsed: unknown = JSON.parse(held);
    if (typeof parsed !== "object" || parsed === null) return undefined;
    if (!("key" in parsed) || typeof parsed.key !== "string") return undefined;
    // SAFETY: this is our own write from `saveHotkey`, and `key` — the one
    // field every match falls back on — has just been checked. A missing
    // modifier from an older shape reads as `undefined`, which is falsy, so
    // the worst outcome is a chord that matches without it.
    return parsed as Hotkey;
  } catch {
    // Private browsing, disabled storage, a shape from an older version: the
    // default is always a correct answer, so none of these are worth failing on.
    return undefined;
  }
};

export const saveHotkey = (hotkey: Hotkey | null): void => {
  try {
    localStorage.setItem(STORAGE, hotkey === null ? NONE : JSON.stringify(hotkey));
  } catch {
    // Forgetting beats failing; the recorded chord still applies to this page.
  }
};
