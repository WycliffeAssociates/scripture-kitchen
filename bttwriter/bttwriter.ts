/**
 * bttwriter: BTT Writer projects' save histories, replayed into one git pack.
 * TypeScript over one wasm module; the walk, the template, the clock fix and
 * the pack writer are the Rust's (`bttwriter/src`), once.
 *
 *     const bttwriter = await import("@wycliffeassociates/scripture-kitchen/bttwriter");
 *     await bttwriter.init();                                  // or initSync({ module: bytes })
 *
 *     const source = bttwriter.isomorphicGitSource({ git, fs, dir });
 *     const rom = await bttwriter.readHistory(source, { source: "owner/repo" });
 *     rom.project;  rom.title;  rom.saves;  rom.progress(now);  rom.usfm();
 *
 *     const done = bttwriter.consolidate([rom, tit], { now }, project, closing);
 *     // write done.pack / done.idx as objects/pack/<done.name>.{pack,idx}, point a branch at done.head
 *
 * Imported lazily, on the migration screen: nobody else pays for the module.
 */
import wasmInit, * as raw from "./pkg-web/bttwriter.js";
import type { NewBibleProject } from "../bento/bento.ts";

/** Start the module: from its URL (the default), a `Response`, or its bytes. */
export const init = (input?: Parameters<typeof wasmInit>[0]) => wasmInit(input);
/** Start the module from bytes already in hand (`{ module: bytes }`). */
export const initSync = raw.initSync;

// --- The port ----------------------------------------------------------------

export interface GitObject {
  type: "commit" | "tree" | "blob";
  /** The object's content, without git's `"<type> <length>\0"` header. */
  bytes: Uint8Array;
}

/** What the walk needs from a git: HEAD, the shallow list, and objects by id. */
export interface ObjectSource {
  head(): Promise<string | undefined>;
  /** `.git/shallow`: commits to treat as roots. */
  shallow(): Promise<string[]>;
  /** Batched; an id it can't find is left out. */
  read(ids: string[]): Promise<Map<string, GitObject>>;
}

/**
 * The port over isomorphic-git, structurally: pass its functions in, so this
 * package doesn't depend on it.
 */
export const isomorphicGitSource = (options: {
  git: {
    resolveRef(args: { fs: unknown; dir: string; ref: string }): Promise<string>;
    readObject(args: { fs: unknown; dir: string; oid: string; format: "content"; cache?: object }): Promise<{
      type: string;
      object: unknown;
    }>;
  };
  /** The fs isomorphic-git was given: `readFile` (or `promises.readFile`) is used for `.git/shallow`. */
  fs: unknown;
  dir: string;
}): ObjectSource => {
  const { git, fs, dir } = options;
  const cache = {};
  type ReadFile = (path: string, encoding: string) => Promise<string | Uint8Array>;
  const files = fs as { promises?: { readFile: ReadFile }; readFile?: ReadFile };
  const readFile = files.promises?.readFile ?? files.readFile;
  return {
    head: () => git.resolveRef({ fs, dir, ref: "HEAD" }).catch(() => undefined),
    shallow: async () => {
      if (readFile === undefined) return [];
      try {
        const text = String(await readFile.call(files.promises ?? files, `${dir}/.git/shallow`, "utf8"));
        return text.split("\n").map((l) => l.trim()).filter((l) => l !== "");
      } catch {
        return [];
      }
    },
    read: async (ids) => {
      const out = new Map<string, GitObject>();
      await Promise.all(
        ids.map(async (oid) => {
          try {
            const { type, object } = await git.readObject({ fs, dir, oid, format: "content", cache });
            if ((type === "commit" || type === "tree" || type === "blob") && object instanceof Uint8Array)
              out.set(oid, { type, bytes: object });
          } catch {
            // Missing: readHistory says which.
          }
        }),
      );
      return out;
    },
  };
};

// --- Reading one Writer repo -------------------------------------------------

export interface Signature {
  name: string;
  email: string;
  /** Unix seconds. */
  at: number;
  /** The zone's offset from UTC, in minutes. */
  offset: number;
}

export interface WriterProject {
  /** `ROM` */
  book: string;
  language: { id: string; name?: string; direction: "ltr" | "rtl" };
  /** `reg`, `ulb` */
  resource: string;
  sources: Array<{ relation: "source"; language: string; resource: string; version?: string }>;
  translators: string[];
  /** The manifest's `project.name`, when it has one. */
  name?: string;
}

/** One Writer book project's saves, held in the module until replayed. */
export class WriterHistory {
  /** @internal */
  raw: raw.WriterHistory | undefined;
  readonly project: WriterProject;
  readonly source: string;
  readonly saves: number;

  /** @internal */
  constructor(handle: raw.WriterHistory) {
    this.raw = handle;
    this.project = JSON.parse(handle.project());
    this.source = handle.source;
    this.saves = handle.saves;
  }

  private held(): raw.WriterHistory {
    if (this.raw === undefined) throw new Error(`${this.project.book}'s history was used up by replay or consolidate`);
    return this.raw;
  }

  /** The book's title at its last save (its `\h`), else its English name. */
  get title(): string {
    return this.held().title();
  }

  /** The first save's author, with its date estimated if the device's clock can't be right. */
  first(now: number): Signature | undefined {
    const s = this.held().first(now);
    return s === undefined ? undefined : JSON.parse(s);
  }

  last(now: number): Signature | undefined {
    const s = this.held().last(now);
    return s === undefined ? undefined : JSON.parse(s);
  }

  /** `[at, verses]` after each save: raw verse-marker counts, a bridge `1-3` as three. */
  progress(now: number): Array<[at: number, verses: number]> {
    const flat = this.held().progress(now);
    const out: Array<[number, number]> = [];
    for (let i = 0; i + 1 < flat.length; i += 2) {
      const at = flat[i];
      const verses = flat[i + 1];
      if (at !== undefined && verses !== undefined) out.push([at, verses]);
    }
    return out;
  }

  /** The book at its last save, as the template renders it. */
  usfm(): string {
    return this.held().usfm();
  }
}

const KIND = { commit: 0, tree: 1, blob: 2 } as const;

/**
 * Read one Writer repo's history: drives the walk, a round of reads at a
 * time (one per commit, one per tree depth, one for every changed file).
 * Book, language and resource come from the repo's own `manifest.json`.
 */
export const readHistory = async (source: ObjectSource, options: { source: string }): Promise<WriterHistory> => {
  const head = await source.head();
  if (head === undefined) throw new Error(`${options.source}: no HEAD`);
  const walk = new raw.HistoryWalk(head, await source.shallow());
  for (let wants = walk.wants(); wants.length > 0; wants = walk.wants()) {
    const got = await source.read(wants);
    const missing = wants.filter((id) => !got.has(id));
    if (missing.length > 0) throw new Error(`${options.source}: missing objects ${missing.slice(0, 3).join(", ")}`);
    const ids: string[] = [];
    const kinds = new Uint8Array(got.size);
    const lengths = new Uint32Array(got.size);
    const bytes = new Uint8Array([...got.values()].reduce((n, o) => n + o.bytes.length, 0));
    let at = 0;
    for (const [id, object] of got) {
      kinds[ids.length] = KIND[object.type];
      lengths[ids.length] = object.bytes.length;
      ids.push(id);
      bytes.set(object.bytes, at);
      at += object.bytes.length;
    }
    walk.give(ids, kinds, lengths, bytes);
  }
  return new WriterHistory(walk.finish(options.source));
};

// --- Replay ------------------------------------------------------------------

export interface ReplayOptions {
  /** Unix seconds: a device clock later than a day past this counts as wrong. */
  now: number;
  /** Store each book's versions as deltas (default true). */
  deltas?: boolean;
}

export interface Replayed {
  /** The last commit: for `replay`, the last save; for `consolidate`, the closing commit. */
  head: string;
  /** → `.git/objects/pack/<name>.pack` */
  pack: Uint8Array;
  /** → `.git/objects/pack/<name>.idx` (v2) */
  idx: Uint8Array;
  /** `pack-<sha of the pack>` */
  name: string;
  books: Array<{ book: string; file: string; title: string }>;
  commits: number;
  /** Saves that changed no rendered text: no commit for them. */
  unchanged: number;
  /** Commits whose date was estimated (they carry a `Writer-Clock` trailer). */
  estimated: number;
  stats: { objects: number; deltas: number; packBytes: number };
}

const handles = (histories: readonly WriterHistory[]): raw.WriterHistory[] =>
  histories.map((h) => {
    const handle = h.raw;
    if (handle === undefined) throw new Error(`${h.project.book}'s history was used up by replay or consolidate`);
    h.raw = undefined;
    return handle;
  });

/**
 * Every history's saves as one line of commits in one pack, as a new root;
 * `undefined` when no save changed any book. Nothing is committed on top.
 * Uses the histories up. Throws on two histories of one book.
 */
export const replay = (histories: readonly WriterHistory[], options: ReplayOptions): Replayed | undefined =>
  (raw.replay(handles(histories), options.now, options.deltas ?? true) as Replayed | undefined) ?? undefined;

/** Who makes the closing commit, and what it says (end it with `\n`). */
export interface Closing {
  author: Signature;
  /** Default: the author. */
  committer?: Signature;
  message: string;
}

/**
 * {@link replay}, then a closing commit holding the project's manifests
 * (bento's `createBible` over each book at its last save). Uses the
 * histories up.
 */
export const consolidate = (
  histories: readonly WriterHistory[],
  options: ReplayOptions,
  project: Omit<NewBibleProject, "books">,
  closing: Closing,
  formats: ReadonlyArray<"burrito" | "rc"> = ["burrito", "rc"],
): Replayed & { manifests: Map<string, Uint8Array> } =>
  raw.consolidate(
    handles(histories),
    options.now,
    options.deltas ?? true,
    JSON.stringify({ ...project, books: [] }),
    JSON.stringify(closing),
    [...formats],
  ) as Replayed & { manifests: Map<string, Uint8Array> };
