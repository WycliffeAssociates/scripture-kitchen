/**
 * The batch of comments, where it is kept, and what it looks like on the
 * clipboard.
 *
 * ## Storage
 *
 * `sessionStorage`, so a hot reload does not eat ten minutes of writing. Per
 * tab, which is right: two tabs are two things being reviewed.
 *
 * ## Copy clears, but softly
 *
 * Copying empties the active list — stale comments getting pasted a second
 * time is the worse failure, and the one that wastes somebody else's time. But
 * the batch is kept as `last`, and the panel offers to bring it back, because
 * losing a morning's notes to a misclicked button is a close second.
 *
 * ## Markdown, not JSON
 *
 * The destination is a chat box. Two verbosity levels, because three is a menu
 * nobody reads: `brief` is the URL, the build, and then a source location and
 * the words per comment — most comments are "this padding is wrong" and the
 * location IS the payload. `full` adds viewport, theme, selector and the
 * nearby text for the ones where the words are not enough.
 */

import type { Comment, Target, Verbosity } from "./types.ts";

const ACTIVE = "plans.annotate.comments";
const LAST = "plans.annotate.lastBatch";

/**
 * Storage is not trusted input, but it is not hostile either — it is this
 * tool's own writes from a moment ago, possibly from an older shape of the
 * type. Each entry is checked for the two fields everything downstream reads,
 * and anything else is dropped rather than rendered as `undefined`.
 */
const isComment = (value: unknown): value is Comment => {
  if (typeof value !== "object" || value === null) return false;
  return (
    "id" in value &&
    typeof value.id === "string" &&
    "text" in value &&
    typeof value.text === "string"
  );
};

/**
 * Bring a stored comment up to the current shape.
 *
 * The older stored shape carries one place, flat: `source`, `selector`,
 * `nearby`, `data`. The current one carries a list of `targets`, because one
 * sentence is often about two elements. A reload mid-review would otherwise throw away the
 * morning's notes — or worse, render them with `undefined` where the location
 * should be — so the old shape is lifted into a single target.
 */
const lift = (comment: Comment): Comment => {
  if (Array.isArray(comment.targets)) return comment;
  // SAFETY: reached only when `targets` is absent, which means this is the
  // older flat shape written by this same module. Every field is read through
  // `Partial`, so a comment that is neither shape yields empty strings rather
  // than `undefined` in the paste.
  const flat = comment as unknown as Partial<Target>;
  return {
    ...comment,
    targets: [
      {
        id: `${comment.id}t0`,
        source: flat.source ?? null,
        selector: flat.selector ?? "",
        nearby: flat.nearby ?? "",
        data: flat.data ?? "",
        anchors: "",
        section: "",
        quote: "",
      },
    ],
  };
};

const readList = (key: string): readonly Comment[] => {
  try {
    const held: unknown = JSON.parse(sessionStorage.getItem(key) ?? "[]");
    return Array.isArray(held) ? held.filter(isComment).map(lift) : [];
  } catch {
    // A tool that refuses to open because storage is disabled would be a worse
    // tool than one that forgets.
    return [];
  }
};

const writeList = (key: string, comments: readonly Comment[]): void => {
  try {
    sessionStorage.setItem(key, JSON.stringify(comments));
  } catch {
    // See above: forgetting beats failing.
  }
};

export const readComments = (): readonly Comment[] => readList(ACTIVE);
export const readLastBatch = (): readonly Comment[] => readList(LAST);

export const addComment = (comment: Comment): readonly Comment[] => {
  const next = [...readComments(), comment];
  writeList(ACTIVE, next);
  return next;
};

export const removeComment = (id: string): readonly Comment[] => {
  const next = readComments().filter((comment) => comment.id !== id);
  writeList(ACTIVE, next);
  return next;
};

/** Empties the active list, keeping it recoverable. */
export const archiveComments = (): void => {
  const current = readComments();
  if (current.length > 0) writeList(LAST, current);
  writeList(ACTIVE, []);
};

export const restoreLastBatch = (): readonly Comment[] => {
  const last = readLastBatch();
  if (last.length === 0) return readComments();
  const next = [...readComments(), ...last];
  writeList(ACTIVE, next);
  writeList(LAST, []);
  return next;
};

export const renderMarkdown = (
  comments: readonly Comment[],
  verbosity: Verbosity,
  context: Readonly<Record<string, string>>,
): string => {
  if (comments.length === 0) return "";
  const header = Object.entries(context)
    .map(([key, value]) => `${key} ${value}`)
    .filter((part) => part !== "");
  const lines: string[] = header.length > 0 ? [header.join(" · "), ""] : [];

  /**
   * The URL is printed whenever it CHANGES, not once at the top.
   *
   * A batch is often a walk: three remarks on the project list, then two on
   * key terms, then one in the editor. Putting the first comment's URL in the
   * header and stopping there quietly asserted that a batch happens on one
   * screen, so everything after the first navigation was reported against the
   * wrong page — which is worse than not reporting the page at all.
   *
   * Printed as a heading between the groups, so the batch reads as the route
   * it actually was.
   */
  let showing: string | null = null;

  // One counter across the whole batch rather than per comment, because the
  // number is also what is painted on the pin over the element. `[3]` in the
  // paste and the `3` on the screen have to be the same thing, or the numbering
  // is decoration.
  let n = 0;

  for (const comment of comments) {
    if (comment.url !== showing) {
      showing = comment.url;
      lines.push(`## ${comment.url}`, "");
    }
    for (const target of comment.targets) {
      n += 1;
      const place = target.source ?? target.selector;
      // A highlighted quote says it better than the element's own text.
      const quoted = target.quote !== undefined && target.quote !== "";
      const label = target.nearby === "" || quoted ? "" : `  "${target.nearby}"`;
      const data = target.data === "" ? "" : `  [${target.data}]`;
      const selector =
        verbosity === "full" && target.source !== null ? `  · ${target.selector}` : "";
      lines.push(`[${String(n)}] ${place}${label}${data}${selector}`);
      // Always printed: a reused component is only findable by where it sits.
      if (target.anchors !== undefined && target.anchors !== "") lines.push(`    at ${target.anchors}`);
      if (target.section !== undefined && target.section !== "") lines.push(`    under "${target.section}"`);
      if (target.quote !== undefined && target.quote !== "") {
        for (const line of target.quote.split("\n")) lines.push(`    > ${line}`);
      }
    }
    // Indented under its targets, every line of it: Shift+Enter makes
    // multi-line comments, and a continuation flush against the margin reads as
    // a new item rather than as more of the same one.
    for (const line of comment.text.split("\n")) lines.push(`    ${line}`);
    if (verbosity === "full") {
      lines.push(`    (viewport ${comment.viewport} · theme ${comment.theme})`);
    }
    lines.push("");
  }

  return lines.join("\n").trimEnd();
};

/**
 * `navigator.clipboard` needs a secure context and a user gesture, and this
 * runs from a click so the gesture is there — but a plain `http://` host on
 * the network is not secure, and a deployed prototype is exactly that. The
 * textarea fallback is not nostalgia; it is the path that works there.
 */
export const copyText = async (text: string): Promise<boolean> => {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const area = document.createElement("textarea");
      area.value = text;
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.append(area);
      area.select();
      const copied = document.execCommand("copy");
      area.remove();
      return copied;
    } catch {
      return false;
    }
  }
};
