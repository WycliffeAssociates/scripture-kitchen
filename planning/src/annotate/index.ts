/**
 * Point and prompt: a floating panel that drives a prototype's variants and
 * tweaks, and turns "this padding is wrong" into something an agent can act on.
 *
 * ```ts
 * const annotator = mountAnnotator({
 *   namespace: "onboarding",
 *   variants: [{ id: "a", label: "Cards", tweaks: [...] }, { id: "b", label: "Table" }],
 *   tweaks: [{ key: "density", label: "Density", kind: "choice", options: ["cosy", "tight"] }],
 *   state: { read: () => search(), write: (next) => navigate({ search: next }) },
 *   context: () => ({ build: buildId }),
 * });
 * ```
 *
 * Everything about it is a corner of the screen and a `StateAdapter`. It knows
 * nothing about Solid, about the router, or about Sefer — `pnpm boundaries`
 * runs `src/core`'s rule set over this folder, so an accidental `solid-js`
 * import fails the build rather than quietly welding the tool to one app.
 *
 * ## The two modes
 *
 * `interact` is the application, untouched. `comment` takes every event in the
 * CAPTURE phase, so while it is on the application cannot be operated at all —
 * a click selects the element under the pointer instead of pressing it. A
 * half-mode where some clicks fall through is how somebody ends up filing a
 * bug about the app being broken, so the mode is total, loudly signposted (a
 * crosshair and a tinted viewport edge), and `Escape` always leaves it.
 *
 * ## What a comment carries
 *
 * The `data-loc` stamp from the JSX-location transform, when one is running —
 * that is the difference between a note an agent can act on and a note it has
 * to grep for. Without it, a short selector and the nearby words, so this
 * module is still useful dropped into a project that has not set that up.
 *
 * ## One sentence, several places
 *
 * A plain click comments on what is under it. SHIFT+click collects the element
 * and waits, so the next plain click opens one composer holding all of them.
 * "These two should swap" and "move 1 to where 2 is" are among the most natural
 * things to say about a layout, and before this they cost two comments that
 * each described half a thought.
 *
 * Every collected place gets a number, painted on a pin over the element and
 * printed as `[2]` in the paste. The same number in both is the whole point:
 * it is what lets the sentence refer to them.
 */

import {
  addComment,
  archiveComments,
  copyText,
  readComments,
  readLastBatch,
  removeComment,
  renderMarkdown,
  restoreLastBatch,
} from "./batch.ts";
import {
  anchorsOf,
  sectionOf,
  dataAttributesOf,
  nearbyTextOf,
  selectorOf,
  sourceOf,
  themeOf,
  viewportOf,
} from "./describe.ts";
import { el, newId, on } from "./dom.ts";
import {
  asHotkey,
  describeHotkey,
  hotkeyFrom,
  hotkeyMatches,
  readSavedHotkey,
  saveHotkey,
  type Hotkey,
} from "./hotkey.ts";
import { ON, OFF, currentVariant, isOn, readTweak, withTweak, withVariant } from "./state.ts";
import { PAGE_STYLES, PANEL_STYLES } from "./styles.ts";
import {
  CORNERS,
  type Annotator,
  type AnnotatorOptions,
  type Comment,
  type Corner,
  type Mode,
  type Target,
  type Tweak,
  type Variant,
  type Verbosity,
} from "./types.ts";

const HOST_ATTRIBUTE = "data-sefer-annotate";

/** ⇧⌘C: copy the whole batch. */
const COPY_ALL: Hotkey = { code: "KeyC", key: "c", alt: false, ctrl: false, meta: true, shift: true };
const MODE_ATTRIBUTE = "data-sefer-annotate-mode";

/**
 * Is somebody typing right now? Asked of `composedPath()[0]`, not of
 * `event.target`, and that distinction is the whole function.
 *
 * A keydown that starts inside a shadow root is RETARGETED on its way out: by
 * the time a listener on `document` sees it, `event.target` is the shadow
 * HOST, not the field being typed into. The annotator's own composer lives in
 * its shadow root, so a guard written against `event.target` cannot ever see
 * it — and the single-letter hotkey fires on the first `c` somebody types into
 * a comment. `composedPath()[0]` is the real element, inside or outside.
 *
 * The same applies to any host that puts an editor in a shadow root, which is
 * why this checks the path rather than special-casing our own panel.
 */
const isTyping = (event: KeyboardEvent): boolean => {
  const actual = event.composedPath()[0] ?? event.target;
  if (!(actual instanceof HTMLElement)) return false;
  return (
    actual.isContentEditable ||
    actual instanceof HTMLInputElement ||
    actual instanceof HTMLTextAreaElement ||
    actual.closest("[contenteditable='true']") !== null
  );
};

/** A described place and the element it was described from. */
interface Placed {
  readonly target: Target;
  readonly element: Element;
}

const place = (element: Element, quote = ""): Placed => ({
  element,
  target: {
    id: newId(),
    source: sourceOf(element),
    selector: selectorOf(element),
    nearby: nearbyTextOf(element),
    data: dataAttributesOf(element),
    anchors: anchorsOf(element),
    section: sectionOf(element),
    quote,
  },
});

export const mountAnnotator = (options: AnnotatorOptions): Annotator => {
  let settings = options;
  let mode: Mode = "interact";
  let corner: Corner = options.corner ?? "bottom-right";
  let verbosity: Verbosity = options.verbosity ?? "brief";
  let minimised = options.minimised ?? false;
  let menuOpen = false;
  let comments = readComments();
  /** Shift-clicked places waiting for the sentence that will be about them. */
  let pending: Placed[] = [];
  let composing: readonly Placed[] | null = null;
  let recording = false;
  let status = "";

  /**
   * Where a pin goes: the live element, kept in memory beside the serialisable
   * target.
   *
   * Deliberately NOT stored coordinates. A click point goes stale the moment
   * anything scrolls, and a pin three inches from the thing it marks is worse
   * than no pin. Holding the element means the pin is recomputed from its rect
   * and therefore tracks reflow, a scrolling container, a resize, everything.
   * The cost is that pins do not survive a reload — the comments do, the pins
   * do not, which is the honest trade and the one that never lies.
   */
  const placed = new Map<string, Element>();

  // ---- the tool's own DOM, sealed off from the page ------------------------

  const host = el("div", { [HOST_ATTRIBUTE]: "" });
  host.style.position = "fixed";
  host.style.inset = "0";
  host.style.pointerEvents = "none";
  host.style.zIndex = "2147483000";
  const shadow = host.attachShadow({ mode: "open" });
  shadow.append(el("style", {}, [PANEL_STYLES]));
  const pins = el("div", { class: "pins" });
  shadow.append(pins);
  const toastBox = el("div", { class: "toast", role: "status", "aria-live": "polite" });
  shadow.append(toastBox);
  let toastTimer: ReturnType<typeof globalThis.setTimeout> | undefined;
  /** Seen whether the panel is open or not: a copy from the hotkey has no other feedback. */
  const toast = (message: string): void => {
    toastBox.textContent = message;
    toastBox.setAttribute("data-show", "");
    globalThis.clearTimeout(toastTimer);
    toastTimer = globalThis.setTimeout(() => {
      toastBox.removeAttribute("data-show");
    }, 2200);
  };
  const root = el("div", { class: "root" });
  root.style.pointerEvents = "auto";
  shadow.append(root);

  const pageStyle = el("style", { [HOST_ATTRIBUTE]: "page" }, [PAGE_STYLES]);
  document.head.append(pageStyle);
  document.body.append(host);

  const outline = el("div", { class: "sefer-annotate-outline" });
  outline.style.display = "none";
  document.body.append(outline);
  const edge = el("div", { class: "sefer-annotate-edge" });
  edge.style.display = "none";
  document.body.append(edge);

  // ---- state helpers -------------------------------------------------------

  const namespace = (): string => settings.namespace ?? "";
  const variants = (): readonly Variant[] => settings.variants ?? [];
  const sharedTweaks = (): readonly Tweak[] => settings.tweaks ?? [];
  const showing = (): Variant | undefined =>
    currentVariant(namespace(), variants(), settings.state.read());

  const setTweak = (variantId: string | null, tweak: Tweak, value: string): void => {
    settings.state.write(withTweak(settings.state.read(), namespace(), variantId, tweak, value));
    render();
  };

  /** Is this event ours? Anything inside the panel must keep working in comment mode. */
  const isOurs = (target: EventTarget | null): boolean =>
    target instanceof Node &&
    (host.contains(target) || target === host || host.shadowRoot?.contains(target) === true);

  // ---- comment mode --------------------------------------------------------

  const hideOutline = (): void => {
    outline.style.display = "none";
  };

  const drawOutline = (element: Element): void => {
    const box = element.getBoundingClientRect();
    outline.style.display = "block";
    outline.style.left = `${String(box.left)}px`;
    outline.style.top = `${String(box.top)}px`;
    outline.style.width = `${String(box.width)}px`;
    outline.style.height = `${String(box.height)}px`;
  };

  const elementAt = (event: MouseEvent): Element | null => {
    // `composedPath` first so a click inside somebody else's shadow tree still
    // resolves to the real element rather than to its host.
    const first = event.composedPath()[0];
    if (first instanceof Element && !isOurs(first)) return first;
    const found = document.elementFromPoint(event.clientX, event.clientY);
    return found !== null && !isOurs(found) ? found : null;
  };

  const onMove = (event: MouseEvent): void => {
    if (mode !== "comment" || composing !== null) return;
    const element = elementAt(event);
    if (element === null) hideOutline();
    else drawOutline(element);
  };

  /**
   * One handler for every interactive event, in the capture phase. Swallowing
   * `pointerdown`/`mousedown` as well as `click` matters: CodeMirror and most
   * drag handles act on the former and would otherwise run before the click
   * they never see.
   */
  const swallow = (event: Event): void => {
    if (mode !== "comment" || isOurs(event.target)) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.type === "click" && event instanceof MouseEvent) {
      const element = elementAt(event);
      if (element === null) return;
      const chosen = place(element);
      placed.set(chosen.target.id, element);

      // Shift collects instead of composing. Holding a modifier to mean "and
      // this one too" is how every list in every file manager works, so it
      // needs no explaining — and it keeps the plain click meaning exactly what
      // it meant before.
      if (event.shiftKey) {
        pending = [...pending, chosen];
        drawOutline(element);
        render();
        return;
      }

      composing = [...pending, chosen];
      pending = [];
      // Opening the composer un-minimises, because the composer IS part of the
      // panel: a puck has nowhere to put a textarea, so a click in comment mode
      // while minimised would swallow the click and then show nothing at all.
      // Somebody who has just pointed at something is about to type.
      minimised = false;
      drawOutline(element);
      render();
    }
  };

  const SWALLOWED = [
    "click",
    "mousedown",
    "mouseup",
    "pointerdown",
    "pointerup",
    "dblclick",
    "submit",
  ] as const;

  /**
   * What actually toggles comment mode here and now.
   *
   * A recorded chord outranks whatever the host passed, INCLUDING a recorded
   * "none" — otherwise turning the shortcut off would last until the next
   * render. `undefined` from storage means nothing was recorded, which is the
   * only case where the host's own setting is consulted.
   */
  const hotkey = (): Hotkey | null => {
    const saved = readSavedHotkey();
    if (saved !== undefined) return saved;
    return asHotkey(settings.hotkey === undefined ? "c" : settings.hotkey);
  };

  const onKey = (event: KeyboardEvent): void => {
    // Recording takes every keystroke, because the chord being recorded might
    // be anything — Escape cancels and Backspace clears, so those two are the
    // only keys that cannot be recorded, which is a price worth paying for an
    // exit that always works.
    if (recording) {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        recording = false;
        render();
        return;
      }
      if (event.key === "Backspace" || event.key === "Delete") {
        saveHotkey(null);
        recording = false;
        flash("Hotkey cleared.");
        return;
      }
      const chord = hotkeyFrom(event);
      // A modifier on its own is somebody part-way through a chord, not a chord.
      if (chord === null) return;
      saveHotkey(chord);
      recording = false;
      flash(`Hotkey is ${describeHotkey(chord)}.`);
      return;
    }

    // The menu first, and in ANY mode: it is the thing most recently opened,
    // and it is the one surface here with no click-away (a click away in
    // comment mode is a comment).
    if (event.key === "Escape" && menuOpen) {
      event.preventDefault();
      menuOpen = false;
      render();
      return;
    }

    if (event.key === "Escape" && mode === "comment") {
      event.preventDefault();
      if (composing !== null) {
        composing = null;
        render();
        return;
      }
      if (pending.length > 0) {
        pending = [];
        hideOutline();
        render();
        return;
      }
      setMode("interact");
      return;
    }

    const copyChord =
      settings.copyHotkey === undefined ? COPY_ALL : asHotkey(settings.copyHotkey);
    if (copyChord !== null && hotkeyMatches(event, copyChord)) {
      event.preventDefault();
      event.stopPropagation();
      if (comments.length === 0) toast("Nothing to copy.");
      else copyBatch();
      return;
    }

    const chord = hotkey();
    if (chord === null || !hotkeyMatches(event, chord)) return;
    // A chord with a modifier is safe in a text field; a bare letter is not.
    const bare = !chord.alt && !chord.ctrl && !chord.meta;
    if (bare && isTyping(event)) return;
    event.preventDefault();
    setMode(mode === "comment" ? "interact" : "comment");
  };

  const setMode = (next: Mode): void => {
    mode = next;
    composing = null;
    // Places collected but never spoken about are not notes; leaving them to
    // reappear on the next visit to comment mode would be a small haunting.
    pending = [];
    if (next === "comment") {
      document.documentElement.setAttribute(MODE_ATTRIBUTE, "comment");
      edge.style.display = "block";
    } else {
      document.documentElement.removeAttribute(MODE_ATTRIBUTE);
      edge.style.display = "none";
      hideOutline();
    }
    render();
  };

  const saveComment = (text: string): void => {
    if (composing === null || composing.length === 0 || text.trim() === "") return;
    const comment: Comment = {
      id: newId(),
      text: text.trim(),
      targets: composing.map((held) => held.target),
      url: location.href,
      viewport: viewportOf(),
      theme: themeOf(),
      at: Date.now(),
    };
    comments = addComment(comment);
    composing = null;
    hideOutline();
    render();
  };

  /**
   * The one-comment path: write it, press Enter, it is on the clipboard.
   *
   * Worth its own function because the ordinary flow — "Add comment", then find
   * and press "Copy" — is two deliberate actions for what is usually one
   * thought. Enter is a genuine user gesture, so the clipboard write is allowed
   * to ride on it, and the whole round trip becomes: click the thing, say what
   * is wrong, Enter, paste.
   *
   * Batching is still there and still deliberate: "Add comment" adds without
   * copying, so a sweep of eight notes is eight Enters' worth of typing and one
   * Copy at the end. Shift+Enter is a newline, because some comments are two
   * sentences.
   */
  const saveAndCopy = (text: string): void => {
    saveComment(text);
    copyBatch();
  };

  let statusTimer: ReturnType<typeof globalThis.setTimeout> | undefined;

  /** A line under the panel that says what just happened, and then stops saying it. */
  const flash = (message: string): void => {
    status = message;
    globalThis.clearTimeout(statusTimer);
    statusTimer = globalThis.setTimeout(() => {
      status = "";
      render();
    }, 2600);
    render();
  };

  const copyBatch = (): void => {
    if (comments.length === 0) return;
    const count = comments.length;
    const text = renderMarkdown(comments, verbosity, settings.context?.() ?? {});
    void copyText(text).then((ok) => {
      if (ok) {
        archiveComments();
        comments = readComments();
        placed.clear();
      }
      const plural = count === 1 ? "" : "s";
      toast(ok ? `Copied ${String(count)} comment${plural}. Paste into Claude.` : "Could not reach the clipboard.");
      flash(ok ? "Copied. Paste into Claude." : "Could not reach the clipboard.");
    });
  };

  // ---- rendering -----------------------------------------------------------

  const segmented = (
    items: readonly { readonly value: string; readonly label: string; readonly title?: string }[],
    active: string,
    choose: (value: string) => void,
  ): HTMLElement => {
    const group = el("div", { class: "segmented", role: "group" });
    for (const item of items) {
      const button = el(
        "button",
        {
          type: "button",
          "aria-pressed": item.value === active ? "true" : "false",
          ...(item.title === undefined ? {} : { title: item.title }),
        },
        [item.label],
      );
      on(button, "click", () => {
        choose(item.value);
      });
      group.append(button);
    }
    return group;
  };

  const tweakField = (variantId: string | null, tweak: Tweak): HTMLElement => {
    const value = readTweak(namespace(), variantId, tweak, settings.state.read());
    if (tweak.kind === "toggle") {
      const box = el("input", { type: "checkbox", ...(isOn(value) ? { checked: true } : {}) });
      on(box, "change", () => {
        setTweak(variantId, tweak, box.checked ? ON : OFF);
      });
      return el("label", { class: "field" }, [el("span", {}, [tweak.label]), box]);
    }
    const select = el("select", {});
    for (const option of tweak.options ?? []) {
      select.append(
        el("option", { value: option, ...(option === value ? { selected: true } : {}) }, [option]),
      );
    }
    on(select, "change", () => {
      setTweak(variantId, tweak, select.value);
    });
    return el("label", { class: "field" }, [el("span", {}, [tweak.label]), select]);
  };

  const cornerMenu = (): HTMLElement => {
    const menu = el("div", { class: "menu" });
    // A way out that is visible. Escape closes it too, but a menu whose only
    // exit is a key you have to guess is a menu people get stuck in — and
    // clicking away is not an option here, because in comment mode a click
    // anywhere else is a comment.
    const close = el("button", { type: "button", class: "ghost icon", title: "Close" }, ["×"]);
    on(close, "click", () => {
      menuOpen = false;
      render();
    });
    menu.append(
      el("div", { class: "menu-head" }, [el("div", { class: "legend title" }, ["Panel"]), close]),
    );
    for (const where of CORNERS) {
      const button = el("button", { type: "button" }, [where.replace("-", " ")]);
      on(button, "click", () => {
        corner = where;
        menuOpen = false;
        render();
      });
      menu.append(button);
    }
    const record = el("button", { type: "button" }, [
      recording ? "press a key…" : `hotkey: ${describeHotkey(hotkey())}`,
    ]);
    on(record, "click", () => {
      recording = true;
      status = "Press the keys you want. Esc cancels, ⌫ clears.";
      globalThis.clearTimeout(statusTimer);
      render();
    });
    menu.append(record);
    if (readLastBatch().length > 0) {
      const restore = el("button", { type: "button" }, ["restore last batch"]);
      on(restore, "click", () => {
        comments = restoreLastBatch();
        menuOpen = false;
        render();
      });
      menu.append(restore);
    }
    return menu;
  };

  const composer = (): HTMLElement => {
    const batching = settings.enter === "batch";
    const area = el("textarea", {
      placeholder: `Describe the issue or suggestion…  ⏎ to ${batching ? "add" : "copy"}, ⇧⏎ for a new line`,
    });
    const copy = el("button", { type: "button", class: batching ? "" : "primary" }, [
      batching ? "Copy now" : "Copy ⏎",
    ]);
    const save = el("button", { type: "button", class: batching ? "primary" : "" }, [
      batching ? "Add ⏎" : "Add to batch",
    ]);
    const cancel = el("button", { type: "button", class: "ghost" }, ["Cancel"]);
    on(copy, "click", () => {
      saveAndCopy(area.value);
    });
    on(save, "click", () => {
      saveComment(area.value);
    });
    on(cancel, "click", () => {
      composing = null;
      render();
    });
    on(area, "keydown", (event) => {
      if (event.key !== "Enter") return;
      // Shift is the newline; everything else finishes the comment. The plain
      // Enter is the point — it is a user gesture, so the clipboard write is
      // permitted to ride on it and the whole loop is click, type, Enter, paste.
      if (event.shiftKey) return;
      event.preventDefault();
      event.stopPropagation();
      if (batching) saveComment(area.value);
      else saveAndCopy(area.value);
    });
    // Every place this sentence is about, numbered to match the pins on the
    // screen, so "1 should go where 2 is" has something to refer to.
    const held = composing ?? [];
    const numbers = numbering();
    const where = el("div", { class: "hint" });
    for (const one of held) {
      where.append(
        el("div", {}, [
          `[${String(numbers.get(one.target.id) ?? 0)}] ${one.target.source ?? one.target.selector}`,
        ]),
      );
    }
    const section = el("div", { class: "section" }, [
      el("div", { class: "legend" }, [
        held.length > 1 ? `Comment on ${String(held.length)}` : "Comment",
      ]),
      where,
      area,
      el("div", { class: "row" }, batching ? [save, copy, cancel] : [copy, save, cancel]),
    ]);
    globalThis.requestAnimationFrame(() => {
      area.focus();
    });
    return section;
  };

  /**
   * Every place in play, in the order it was pointed at: the saved comments'
   * targets first, then whatever is collected but not yet written about.
   *
   * One sequence for the panel, the pins and the paste, computed once. Three
   * places deriving their own numbering is three chances for the number on the
   * screen to disagree with the number in the text, which would make the
   * numbering worse than useless.
   */
  const rows = (): { target: Target; n: number; text: string | null }[] => {
    const all: { target: Target; n: number; text: string | null }[] = [];
    let n = 0;
    for (const comment of comments) {
      for (const target of comment.targets) {
        n += 1;
        all.push({ target, n, text: comment.text });
      }
    }
    for (const held of [...pending, ...(composing ?? [])]) {
      n += 1;
      all.push({ target: held.target, n, text: null });
    }
    return all;
  };

  const numbering = (): Map<string, number> => new Map(rows().map((row) => [row.target.id, row.n]));

  /**
   * The pins, drawn from the live elements rather than from stored coordinates.
   *
   * Only in comment mode, and that restraint is the point of the whole tool:
   * the panel floats in a corner and the pins appear over the design precisely
   * when somebody is pointing at it. Leaving eight numbered dots on top of a
   * screen that is being JUDGED would obstruct the one thing being looked at.
   */
  const renderPins = (): void => {
    pins.replaceChildren();
    if (mode !== "comment") return;
    for (const row of rows()) {
      const element = placed.get(row.target.id);
      if (element === undefined || !element.isConnected) continue;
      const box = element.getBoundingClientRect();
      if (box.width === 0 && box.height === 0) continue;
      const pin = el(
        "div",
        { class: "pin", ...(row.text === null ? { "data-pending": "true" } : {}) },
        [String(row.n)],
      );
      pin.style.left = `${String(box.right)}px`;
      pin.style.top = `${String(box.top)}px`;
      pin.append(
        el("div", { class: "bubble" }, [
          el("div", { class: "where" }, [row.target.source ?? row.target.selector]),
          row.text ?? "Waiting for a sentence.",
        ]),
      );
      pins.append(pin);
    }
  };

  const commentList = (): HTMLElement => {
    const list = el("div", { class: "comments" });
    const numbers = numbering();
    for (const comment of comments) {
      const drop = el("button", { type: "button", class: "ghost icon", title: "Remove" }, ["×"]);
      on(drop, "click", () => {
        for (const target of comment.targets) placed.delete(target.id);
        comments = removeComment(comment.id);
        render();
      });
      const where = el("div", { class: "where" });
      for (const target of comment.targets) {
        where.append(
          el("div", {}, [
            `[${String(numbers.get(target.id) ?? 0)}] ${target.source ?? target.selector}`,
          ]),
        );
      }
      list.append(
        el("div", { class: "comment" }, [
          el("div", {}, [where, el("div", { class: "what" }, [comment.text])]),
          drop,
        ]),
      );
    }
    return list;
  };

  function render(): void {
    root.replaceChildren();
    root.setAttribute("data-corner", corner);
    renderPins();

    if (minimised) {
      const puck = el("div", { class: "puck", "data-mode": mode, title: "Open the comments panel" }, [
        "◆",
      ]);
      on(puck, "click", () => {
        minimised = false;
        render();
      });
      if (comments.length > 0) {
        const chord =
          settings.copyHotkey === undefined ? COPY_ALL : asHotkey(settings.copyHotkey);
        const all = el(
          "button",
          { type: "button", class: "copy-all", title: chord === null ? "Copy all" : `Copy all (${describeHotkey(chord)})` },
          [`Copy ${String(comments.length)}`],
        );
        on(all, "click", copyBatch);
        root.append(el("div", { class: "dock" }, [all, puck]));
      } else root.append(puck);
      return;
    }

    const kebab = el("button", { type: "button", class: "ghost icon", title: "Panel options" }, [
      "⋯",
    ]);
    on(kebab, "click", () => {
      menuOpen = !menuOpen;
      render();
    });
    const shrink = el("button", { type: "button", class: "ghost icon", title: "Minimise" }, ["–"]);
    on(shrink, "click", () => {
      minimised = true;
      menuOpen = false;
      render();
    });

    const body = el("div", { class: "body" });

    const chord = hotkey();
    const press = chord === null ? "" : ` (${describeHotkey(chord)})`;
    body.append(
      el("div", { class: "section" }, [
        segmented(
          [
            { value: "interact", label: "Interact", title: `Use the application${press}` },
            { value: "comment", label: "Comment", title: `Point at things${press}` },
          ],
          mode,
          (value) => {
            setMode(value === "comment" ? "comment" : "interact");
          },
        ),
      ]),
    );

    if (composing !== null) body.append(composer());
    else if (pending.length > 0) {
      const write = el("button", { type: "button", class: "primary" }, [
        `Comment on ${String(pending.length)}`,
      ]);
      on(write, "click", () => {
        composing = pending;
        pending = [];
        render();
      });
      const clear = el("button", { type: "button", class: "ghost" }, ["Clear"]);
      on(clear, "click", () => {
        pending = [];
        hideOutline();
        render();
      });
      body.append(
        el("div", { class: "section" }, [
          el("div", { class: "legend" }, [`${String(pending.length)} selected`]),
          el("div", { class: "hint" }, ["Shift+click to add another, or click one without Shift."]),
          el("div", { class: "row" }, [write, clear]),
        ]),
      );
    }

    const nav = settings.nav;
    if (nav !== undefined && nav.items.length > 1) {
      body.append(
        el("div", { class: "section" }, [
          el("div", { class: "legend" }, [nav.label]),
          segmented(nav.items, nav.active, nav.choose),
        ]),
      );
    }

    const list = variants();
    if (list.length > 1) {
      body.append(
        el("div", { class: "section" }, [
          el("div", { class: "legend" }, ["Variant"]),
          segmented(
            list.map((variant) => ({ value: variant.id, label: variant.label })),
            showing()?.id ?? "",
            (value) => {
              settings.state.write(withVariant(settings.state.read(), namespace(), list, value));
              render();
            },
          ),
        ]),
      );
    }

    const own = showing()?.tweaks ?? [];
    const shared = sharedTweaks();
    if (own.length > 0 || shared.length > 0) {
      const section = el("div", { class: "section" }, [el("div", { class: "legend" }, ["Tweaks"])]);
      for (const tweak of shared) section.append(tweakField(null, tweak));
      for (const tweak of own) section.append(tweakField(showing()?.id ?? null, tweak));
      body.append(section);
    }

    if (comments.length > 0) {
      const copy = el("button", { type: "button", class: "primary" }, [
        `Copy ${String(comments.length)} comment${comments.length === 1 ? "" : "s"}`,
      ]);
      on(copy, "click", copyBatch);
      // Verbosity sits HERE, beside Copy, rather than in the kebab menu. It
      // changes what the next paste says, so it belongs next to the button
      // that does the pasting and it has to show which one is selected — as a
      // menu row reading "verbosity: brief" it was both hidden and ambiguous
      // about whether it named the current state or the action.
      const detail = segmented(
        [
          { value: "brief", label: "Brief", title: "Location and words" },
          { value: "full", label: "Full", title: "Adds selector, viewport and theme" },
        ],
        verbosity,
        (value) => {
          verbosity = value === "full" ? "full" : "brief";
          render();
        },
      );
      body.append(
        el("div", { class: "section" }, [
          el("div", { class: "legend" }, ["Comments"]),
          commentList(),
          el("div", { class: "row" }, [copy, detail]),
        ]),
      );
    } else if (mode === "comment") {
      body.append(
        el("div", { class: "section" }, [
          el("div", { class: "hint" }, [
            "Click anything to comment on it. Shift+click to gather several into one comment. Esc to stop.",
          ]),
        ]),
      );
    }

    if (status !== "") body.append(el("div", { class: "hint" }, [status]));

    const panel = el("div", { class: "panel" }, [
      el("div", { class: "head" }, [el("div", { class: "title" }, ["Comments"]), kebab, shrink]),
      body,
    ]);
    root.append(panel);
    if (menuOpen) root.append(cornerMenu());
  }

  // ---- wiring --------------------------------------------------------------

  /**
   * Pins are positioned from live rects, so anything that moves an element
   * moves its pin. Capture-phase `scroll` because a scrolling container does
   * not bubble one, and a frame gate because both of these fire in floods.
   */
  let frame = 0;
  const repaint = (): void => {
    if (frame !== 0) return;
    frame = globalThis.requestAnimationFrame(() => {
      frame = 0;
      renderPins();
    });
  };

  for (const type of SWALLOWED) document.addEventListener(type, swallow, true);
  document.addEventListener("mousemove", onMove, true);
  document.addEventListener("keydown", onKey, true);
  document.addEventListener("scroll", repaint, true);
  globalThis.addEventListener("resize", repaint);
  const unsubscribe = settings.state.subscribe?.(() => {
    render();
  });

  render();

  /** Open the composer on one element from outside — the text-selection path. */
  const compose = (element: Element, quote: string): void => {
    const chosen = place(element, quote);
    placed.set(chosen.target.id, element);
    composing = [chosen];
    pending = [];
    minimised = false;
    render();
  };

  return {
    sync: render,
    compose,
    update: (next) => {
      settings = { ...settings, ...next };
      if (next.minimised !== undefined) minimised = next.minimised;
      render();
    },
    comments: () => comments,
    drain: () => {
      const held = comments;
      archiveComments();
      comments = readComments();
      placed.clear();
      render();
      return held;
    },
    mode: () => mode,
    setMode,
    destroy: () => {
      for (const type of SWALLOWED) document.removeEventListener(type, swallow, true);
      document.removeEventListener("mousemove", onMove, true);
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("scroll", repaint, true);
      globalThis.removeEventListener("resize", repaint);
      globalThis.clearTimeout(statusTimer);
      globalThis.clearTimeout(toastTimer);
      unsubscribe?.();
      document.documentElement.removeAttribute(MODE_ATTRIBUTE);
      host.remove();
      outline.remove();
      edge.remove();
      pageStyle.remove();
    },
  };
};

export type { Annotator, AnnotatorOptions, StateAdapter, Tweak, Variant } from "./types.ts";
