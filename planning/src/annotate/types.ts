/**
 * The annotator's public shape — what a host hands it, and what it hands back.
 *
 * This folder is a plain DOM module. No Solid, no router, no Sefer: a design
 * tool that only works in one application is a design tool you rewrite for the
 * next one. `pnpm boundaries` runs `src/core`'s own rule set over it, so
 * `solid-js` and friends are not merely discouraged here, they fail the build.
 *
 * The seam that makes that possible is `StateAdapter`. The annotator never
 * decides where a tweak's value lives; it asks. In Sefer that is the router's
 * search params, so every knob is in a URL somebody can send. In a project with
 * no router it could be `localStorage` or a plain object, and nothing in here
 * changes.
 */

import type { Hotkey } from "./hotkey.ts";

/** Where the panel sits. A kebab menu moves it; a designer works in a corner. */
export type Corner = "bottom-right" | "bottom-left" | "top-right" | "top-left";

export const CORNERS: readonly Corner[] = ["bottom-right", "bottom-left", "top-right", "top-left"];

/**
 * What the pointer is currently for.
 *
 * `comment` is a MODE in the strong sense: while it is on, the application
 * cannot be operated at all. Every event is taken in the capture phase, so a
 * click selects an element instead of pressing the button under it. That is
 * deliberate — a half-mode where some clicks fall through is how somebody ends
 * up filing a bug about the application being broken.
 */
export type Mode = "interact" | "comment";

/** A small named change: one knob, one value. */
export interface Tweak {
  readonly key: string;
  readonly label: string;
  readonly kind: "toggle" | "choice";
  /** `choice` only. The first is the default when `initial` is absent. */
  readonly options?: readonly string[];
  readonly initial?: string | boolean;
}

/**
 * A whole alternative take on the page or feature — the thing a segmented
 * control at the top of the panel switches between.
 *
 * A variant owns its own tweaks, because "table layout" and "card layout" do
 * not usually have the same knobs. Tweaks that DO apply whichever variant is
 * showing go in `AnnotatorOptions.tweaks` instead, and stay put as you switch.
 */
export interface Variant {
  readonly id: string;
  readonly label: string;
  readonly tweaks?: readonly Tweak[];
}

/**
 * Where values live. The host owns this, which is what keeps the annotator
 * free of any particular framework's idea of state.
 *
 * `read` must return the CURRENT values every time it is called — the panel
 * calls it while rendering. `subscribe` is optional: without it the host is
 * expected to call `sync()` after it changes state itself.
 */
export interface StateAdapter {
  readonly read: () => Readonly<Record<string, string>>;
  readonly write: (next: Readonly<Record<string, string>>) => void;
  readonly subscribe?: (onChange: () => void) => () => void;
}

/**
 * One element somebody pointed at.
 *
 * Split out of `Comment` because a comment can be about MORE than one place.
 * "These two should swap" and "move 1 to where 2 is" are the most natural
 * things to say about a layout, and saying them should not cost two comments
 * that each describe half a thought.
 */
export interface Target {
  readonly id: string;
  /** From the `data-loc` stamp, when the JSX-location transform is running. */
  readonly source: string | null;
  /** A short CSS-ish path, for when there is no source location. */
  readonly selector: string;
  /** The element's own text, when it has any of its own. */
  readonly nearby: string;
  /** Its `data-*` attributes, which are usually what identifies it. */
  readonly data: string;
  /** The `data-ref` chain from the page down to the element. */
  readonly anchors: string;
  /** The heading the element sits under. */
  readonly section: string;
  /** Text the reader highlighted, when the comment started from a selection. */
  readonly quote: string;
}

/** One thing somebody said, about one or more places. */
export interface Comment {
  readonly id: string;
  readonly text: string;
  /** At least one, in the order they were clicked. */
  readonly targets: readonly Target[];
  readonly url: string;
  readonly viewport: string;
  readonly theme: string;
  readonly at: number;
}

/** How much a copied batch says. Two levels, because three is a menu nobody reads. */
export type Verbosity = "brief" | "full";

export interface AnnotatorOptions {
  /** Prefixes every state key, so two screens' tweaks cannot collide. */
  readonly namespace?: string;
  readonly variants?: readonly Variant[];
  /** Tweaks that apply whichever variant is showing. */
  readonly tweaks?: readonly Tweak[];
  readonly state: StateAdapter;
  /**
   * Extra facts for a copied batch's header — a build id, which project is
   * open. A function because they change; the annotator reads it when copying.
   */
  readonly context?: () => Readonly<Record<string, string>>;
  readonly corner?: Corner;
  /**
   * How much a copied batch says. `brief` by default.
   *
   * An option rather than a fixed default because the right answer depends on
   * who reads the paste. On a laptop, the reader has the checkout open and a
   * source location is the whole payload. On a DEPLOYED prototype the reader
   * usually cannot look at anything — so the viewport, the theme and the
   * selector stop being noise and start being the only way to reconstruct
   * what was on screen.
   *
   * The annotator does not decide which of those it is in; it has no way to
   * know, and guessing would weld it to one host's idea of a build.
   */
  readonly verbosity?: Verbosity;
  /**
   * The key that toggles comment mode. `"c"` by default, `null` for no hotkey
   * at all — click the segmented control instead. A `Hotkey` for a chord.
   *
   * Configurable rather than fixed because a bare letter is a different
   * proposition depending on what it is floating over. On a design surface,
   * where the page is a prototype and nothing else is listening, `c` is worth
   * having. Over a real editor — Sefer's CodeMirror, say — a single letter
   * competes with the application's own bindings, and `null` is the honest
   * setting. `Escape` always leaves comment mode regardless, because an exit
   * you cannot find is worse than no shortcut at all.
   *
   * Whatever the host passes, a hotkey RECORDED in the panel wins — including
   * a recorded "none". That is the point of recording one: `⌥C` is safe over
   * an editor in a way a bare letter is not, so the person at the keyboard can
   * turn the shortcut back on in the places the host had to disable it.
   */
  readonly hotkey?: string | Hotkey | null;
  /**
   * Start as a puck rather than an open panel. True when the annotator is
   * floating over a real screen, where somebody is using the application
   * rather than designing it; false on a prototyping surface, where the panel
   * IS the reason the page is open.
   */
  readonly minimised?: boolean;
  /**
   * What Enter does in the composer. `copy` (the default) saves and copies at
   * once: one thought, one paste. `batch` adds to the batch and waits — for
   * pages read end to end, where answers come in a sweep and go in one paste.
   */
  readonly enter?: "copy" | "batch";
  /**
   * Copies the whole batch from anywhere, in any mode, even while typing
   * (it has modifiers). `⇧⌘C` by default; `null` for none.
   */
  readonly copyHotkey?: string | Hotkey | null;
  /**
   * An optional "which page am I on" control at the top of the panel.
   *
   * Kept deliberately generic — a list of labels and a callback — because the
   * annotator must not learn what a Sefer design screen is. It exists at all
   * because the alternative is a navigation bar pinned across the top of the
   * page, and a prototyping tool that covers the thing being judged has got
   * the wrong end of its own job.
   */
  readonly nav?: {
    readonly label: string;
    readonly items: readonly { readonly value: string; readonly label: string }[];
    readonly active: string;
    readonly choose: (value: string) => void;
  };
}

export interface Annotator {
  /** Re-read state and redraw the panel. Call after the host changes state. */
  readonly sync: () => void;
  /** Swap variants/tweaks when the host moves to another screen. */
  readonly update: (options: Partial<AnnotatorOptions>) => void;
  readonly destroy: () => void;

  // The rest is what an agent attached over CDP needs, and what the host
  // re-exports on its own debug handle. Reading values needs none of this —
  // they are in the state adapter — but reading the BATCH does, and so does
  // acting on the mode.
  readonly comments: () => readonly Comment[];
  /** Read the batch and clear it: the Copy button, minus the clipboard. */
  readonly drain: () => readonly Comment[];
  readonly mode: () => Mode;
  /** Open the composer on an element, carrying the text the reader highlighted. */
  readonly compose: (element: Element, quote: string) => void;
  readonly setMode: (next: Mode) => void;
}
