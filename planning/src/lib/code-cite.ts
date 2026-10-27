/**
 * `[[code:path#symbol]]` → the lines that symbol spans in the file as it is
 * now, so a citation fails `pnpm atlas check` when the code moves rather than
 * pointing at the wrong lines.
 *
 *   onion/src/cst.rs#CloseReason       the enum, its `///` docs and `#[derive]` included
 *   onion/src/cst.rs#Builder::feed     the method inside `impl Builder`
 *   onion/src/cst.rs#impl Cst          the whole impl block
 *   galley/toc-reader.ts#Census.open   a method of a class
 *   onion/src/cst.rs#L120-L180         those lines
 *   GLOSSARY.md#lossless-invariant     that heading's section
 *   onion/src/cst.rs                   the file: a link, no lines
 */
import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";

export interface Cite {
  readonly path: string;
  readonly symbol: string | null;
  readonly shown: string;
  readonly key: string;
}

export type Lines = { readonly start: number; readonly end: number };
export type Resolved = { readonly ok: true; readonly lines: Lines | null } | { readonly ok: false; readonly error: string };

/** The part after `code:`, and the `|shown` text if any. */
export const parseCite = (target: string, shown?: string): Cite => {
  const at = target.indexOf("#");
  const path = (at < 0 ? target : target.slice(0, at)).trim();
  const symbol = at < 0 ? null : target.slice(at + 1).trim() || null;
  const fallback = symbol === null ? path : /^L\d/u.test(symbol) ? `${path.split("/").pop()}:${symbol.replace(/L/gu, "")}` : symbol;
  return { path, symbol, shown: shown?.trim() || fallback, key: citeKey(path, symbol) };
};

export const citeKey = (path: string, symbol: string | null): string => {
  let h = 0x811c9dc5;
  for (const ch of `${path}#${symbol ?? ""}`) h = Math.imul(h ^ ch.codePointAt(0)!, 0x01000193) >>> 0;
  return `c${h.toString(36)}`;
};

const cache = new Map<string, string | null>();
const read = (file: string): string | null => {
  if (!cache.has(file)) cache.set(file, statSync(file, { throwIfNoEntry: false })?.isFile() ? readFileSync(file, "utf8") : null);
  return cache.get(file)!;
};

export const resolveCite = (root: string, path: string, symbol: string | null): Resolved => {
  const text = read(join(root, path));
  if (text === null) return { ok: false, error: `${path} is not a file in the repo` };
  if (symbol === null) return { ok: true, lines: null };
  const count = text.split("\n").length;
  const range = /^L(\d+)(?:-L?(\d+))?$/u.exec(symbol);
  if (range) {
    const start = Number(range[1]);
    const end = Number(range[2] ?? range[1]);
    if (start < 1 || end < start || end > count) return { ok: false, error: `${path} has ${count} lines; L${start}-L${end} is outside it` };
    return { ok: true, lines: { start, end } };
  }
  const ext = path.split(".").pop()!.toLowerCase();
  const found =
    ext === "rs" ? rust(text, symbol)
    : ["ts", "mts", "cts", "tsx", "js", "mjs", "cjs"].includes(ext) ? script(text, symbol)
    : ["md", "mdx"].includes(ext) ? markdown(text, symbol)
    : `only line ranges can be cited in a .${ext} file`;
  return typeof found === "string" ? { ok: false, error: found.replace("$", `\`${symbol}\``).replace("@", path) } : { ok: true, lines: found };
};

const NOT_FOUND = "no $ in @";
const ambiguous = (lines: number[]): string => `$ is ambiguous in @: lines ${lines.join(", ")}`;

const IDENT = /[\w$]/u;

/** Comments and literals blanked to spaces, newlines kept, so braces and keywords in them are not code. */
const blankRust = (s: string): string => {
  const out = s.split("");
  const wipe = (from: number, to: number): void => {
    for (let k = from; k < to && k < out.length; k++) if (out[k] !== "\n") out[k] = " ";
  };
  let i = 0;
  while (i < s.length) {
    const c = s[i]!;
    const prev = s[i - 1] ?? " ";
    if (c === "/" && s[i + 1] === "/") {
      const j = s.indexOf("\n", i);
      const end = j < 0 ? s.length : j;
      wipe(i, end);
      i = end;
    } else if (c === "/" && s[i + 1] === "*") {
      let depth = 1;
      let j = i + 2;
      while (j < s.length && depth > 0) {
        if (s[j] === "/" && s[j + 1] === "*") { depth++; j += 2; }
        else if (s[j] === "*" && s[j + 1] === "/") { depth--; j += 2; }
        else j++;
      }
      wipe(i, j);
      i = j;
    } else if (/[rb]/u.test(c) && !IDENT.test(prev) && /^(b?r)(#*)"/u.test(s.slice(i, i + 260))) {
      const [, r, hashes] = /^(b?r)(#*)"/u.exec(s.slice(i, i + 260))!;
      const close = `"${hashes}`;
      const j = s.indexOf(close, i + r!.length + hashes!.length + 1);
      const end = j < 0 ? s.length : j + close.length;
      wipe(i, end);
      i = end;
    } else if (c === '"') {
      let j = i + 1;
      while (j < s.length && s[j] !== '"') j += s[j] === "\\" ? 2 : 1;
      wipe(i, j + 1);
      i = j + 1;
    } else if (c === "'") {
      if (s[i + 1] === "\\") {
        let j = i + 3;
        while (j < s.length && s[j] !== "'" && s[j] !== "\n") j++;
        wipe(i, j + 1);
        i = j + 1;
      } else {
        const width = (s.codePointAt(i + 1) ?? 0) > 0xffff ? 2 : 1;
        if (s[i + 1 + width] === "'" && s[i + 1] !== "\n") {
          wipe(i, i + 2 + width);
          i += 2 + width;
        } else i++;
      }
    } else i++;
  }
  return out.join("");
};

const REGEX_AFTER = /[(,=:[!&|?{};+\-*%<>~^]|^$|\b(?:return|typeof|case|in|of)$/u;

const blankScript = (s: string): string => {
  const out = s.split("");
  const wipe = (from: number, to: number): void => {
    for (let k = from; k < to && k < out.length; k++) if (out[k] !== "\n") out[k] = " ";
  };
  const before = (i: number): string => {
    let j = i - 1;
    while (j >= 0 && /\s/u.test(s[j]!)) j--;
    if (j < 0) return "";
    if (IDENT.test(s[j]!)) {
      let k = j;
      while (k > 0 && IDENT.test(s[k - 1]!)) k--;
      return s.slice(k, j + 1);
    }
    return s[j]!;
  };
  const template = (i: number): number => {
    let j = i + 1;
    while (j < s.length && s[j] !== "`") {
      if (s[j] === "\\") j += 2;
      else if (s[j] === "$" && s[j + 1] === "{") j = code(j + 2, true);
      else j++;
    }
    return j + 1;
  };
  const code = (i: number, nested: boolean): number => {
    let depth = 0;
    while (i < s.length) {
      const c = s[i]!;
      if (c === "/" && s[i + 1] === "/") {
        const j = s.indexOf("\n", i);
        const end = j < 0 ? s.length : j;
        wipe(i, end);
        i = end;
      } else if (c === "/" && s[i + 1] === "*") {
        const j = s.indexOf("*/", i + 2);
        const end = j < 0 ? s.length : j + 2;
        wipe(i, end);
        i = end;
      } else if (c === "'" || c === '"') {
        let j = i + 1;
        while (j < s.length && s[j] !== c && s[j] !== "\n") j += s[j] === "\\" ? 2 : 1;
        wipe(i, j + 1);
        i = j + 1;
      } else if (c === "`") {
        const end = template(i);
        wipe(i, end);
        i = end;
      } else if (c === "/" && REGEX_AFTER.test(before(i))) {
        let j = i + 1;
        let cls = false;
        while (j < s.length && s[j] !== "\n" && (cls || s[j] !== "/")) {
          if (s[j] === "\\") j++;
          else if (s[j] === "[") cls = true;
          else if (s[j] === "]") cls = false;
          j++;
        }
        wipe(i, j + 1);
        i = j + 1;
      } else {
        if (c === "{") depth++;
        else if (c === "}") {
          if (nested && depth === 0) return i + 1;
          depth--;
        }
        i++;
      }
    }
    return i;
  };
  code(0, false);
  return out.join("");
};

interface Item {
  kind: string;
  name: string;
  /** Offset of the item's first keyword in the blanked text. */
  at: number;
  /** Offsets of its body's braces, when it has one. */
  open: number | null;
  close: number;
  /** For an impl: the self type and the trait. */
  self?: string;
  trait?: string | undefined;
  parent?: Item | undefined;
}

const lineOf = (starts: number[], offset: number): number => {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid]! <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo;
};

const lineStarts = (s: string): number[] => {
  const starts = [0];
  for (let i = 0; i < s.length; i++) if (s[i] === "\n") starts.push(i + 1);
  return starts;
};

/** From `at`, the `;` that ends the item at depth 0, or the `}` that closes its first top-level `{`. */
const extent = (s: string, at: number, braces: boolean): { open: number | null; close: number } => {
  let depth = 0;
  let open: number | null = null;
  for (let i = at; i < s.length; i++) {
    const c = s[i]!;
    if (c === "(" || c === "[" || c === "{") {
      if (c === "{" && depth === 0 && braces && open === null) open = i;
      depth++;
    } else if (c === ")" || c === "]" || c === "}") {
      depth--;
      if (c === "}" && depth === 0 && open !== null) return { open, close: i };
      if (depth < 0) return { open, close: i };
    } else if (c === ";" && depth === 0) return { open, close: i };
  }
  return { open, close: s.length - 1 };
};

/** `Foo<T>`, `&'a mut path::Foo`, `dyn Trait` → the last path segment's name. */
const typeName = (t: string): string => {
  let rest = t.replace(/^[\s&]*(?:'\w+\s*)?(?:mut\s+|dyn\s+)*/u, "");
  const lt = rest.indexOf("<");
  if (lt >= 0) rest = rest.slice(0, lt);
  return rest.split("::").pop()!.trim();
};

const parseImpl = (header: string): { self: string; trait: string | undefined } => {
  let h = header.replace(/\s+/gu, " ").replace(/^\s*(?:unsafe )?impl\s*/u, "");
  if (h.startsWith("<")) {
    let depth = 0;
    let i = 0;
    for (; i < h.length; i++) {
      if (h[i] === "<") depth++;
      else if (h[i] === ">" && h[i - 1] !== "-" && --depth === 0) break;
    }
    h = h.slice(i + 1);
  }
  h = h.replace(/\swhere\s[\s\S]*$/u, "");
  let depth = 0;
  for (let i = 0; i < h.length; i++) {
    if (h[i] === "<") depth++;
    else if (h[i] === ">" && h[i - 1] !== "-") depth--;
    else if (depth === 0 && h.startsWith(" for ", i)) return { trait: typeName(h.slice(0, i).replace(/^\s*!/u, "")), self: typeName(h.slice(i + 5)) };
  }
  return { self: typeName(h), trait: undefined };
};

const RUST_ITEM =
  /^[ \t]*(?:pub[ \t]*(?:\([^)\n]*\))?[ \t]*)?(?:(?:const|async|unsafe|extern|default)[ \t]+)*(?:(fn|struct|enum|trait|const|static|type|mod|union)[ \t]+(?:mut[ \t]+)?(?:r#)?([A-Za-z_]\w*)|(macro_rules!)[ \t]*([A-Za-z_]\w*)|(impl)\b)/u;
const BODIED = new Set(["fn", "struct", "enum", "trait", "mod", "union", "impl", "macro_rules!"]);

const rustItems = (clean: string, starts: number[]): Item[] => {
  const items: Item[] = [];
  const lines = clean.split("\n");
  lines.forEach((line, n) => {
    const m = RUST_ITEM.exec(line);
    if (!m) return;
    const kind = m[1] ?? m[3] ?? "impl";
    const at = starts[n]! + m[0].lastIndexOf(kind === "impl" ? "impl" : kind);
    const { open, close } = extent(clean, at, BODIED.has(kind));
    const item: Item = { kind, name: m[2] ?? m[4] ?? "", at, open, close };
    if (kind === "impl") Object.assign(item, parseImpl(clean.slice(at, open ?? close)));
    items.push(item);
  });
  for (const item of items) {
    let best: Item | undefined;
    for (const other of items) {
      if (other.open !== null && other.open < item.at && item.at < other.close && (best === undefined || other.open > best.open!)) best = other;
    }
    item.parent = best;
  }
  return items;
};

/** The line an item's text starts on: back over `///` docs, a doc block and attributes. */
const preamble = (raw: string[], clean: string, starts: number[], line: number, attribute: RegExp): number => {
  const inAttribute = (n: number): boolean => {
    for (let k = n; k >= 0 && n - k < 40; k--) {
      const t = raw[k]!.trim();
      if (!attribute.test(t)) continue;
      const at = starts[k]! + raw[k]!.indexOf(t);
      let depth = 0;
      for (let i = at; i < clean.length; i++) {
        const c = clean[i]!;
        if ("([{".includes(c)) depth++;
        else if (")]}".includes(c) && --depth === 0) return lineOf(starts, i) >= n;
      }
      return false;
    }
    return false;
  };
  let n = line - 1;
  while (n >= 0) {
    const t = raw[n]!.trim();
    if (t.startsWith("///") || attribute.test(t)) n--;
    else if (t.endsWith("*/")) {
      let k = n;
      while (k >= 0 && !raw[k]!.trim().startsWith("/*")) k--;
      if (k < 0 || !raw[k]!.trim().startsWith("/**")) break;
      n = k - 1;
    } else if (t !== "" && inAttribute(n)) n--;
    else break;
  }
  return n + 2;
};

const span = (raw: string[], clean: string, starts: number[], item: Item, attribute: RegExp): Lines => {
  const line = lineOf(starts, item.at);
  return { start: preamble(raw, clean, starts, line, attribute), end: lineOf(starts, item.close) + 1 };
};

const pick = (found: { lines: Lines; depth: number }[]): Lines | string => {
  if (found.length === 0) return NOT_FOUND;
  const shallow = Math.min(...found.map((f) => f.depth));
  const top = found.filter((f) => f.depth === shallow);
  return top.length === 1 ? top[0]!.lines : ambiguous(top.map((f) => f.lines.start));
};

const depthOf = (item: Item): number => (item.parent === undefined ? 0 : 1 + depthOf(item.parent));

const rust = (text: string, symbol: string): Lines | string => {
  const clean = blankRust(text);
  const starts = lineStarts(clean);
  const raw = text.split("\n");
  const items = rustItems(clean, starts);
  const attribute = /^#\[/u;
  const out = (item: Item): { lines: Lines; depth: number } => ({ lines: span(raw, clean, starts, item, attribute), depth: depthOf(item) });

  const impl = /^impl\s+(?:(\S+)\s+for\s+)?(\S+)$/u.exec(symbol);
  if (impl) {
    return pick(items.filter((i) => i.kind === "impl" && i.self === impl[2] && i.trait === impl[1]).map(out));
  }
  const parts = symbol.split("::");
  const name = parts.pop()!;
  if (parts.length === 0) {
    return pick(items.filter((i) => i.kind !== "impl" && i.name === name && i.parent?.kind !== "impl" && i.parent?.kind !== "trait").map(out));
  }
  const owner = parts.pop()!;
  const owners = (i: Item | undefined): boolean =>
    i !== undefined && ((i.kind === "impl" && i.self === owner) || (["trait", "mod"].includes(i.kind) && i.name === owner));
  const members = items.filter((i) => i.kind !== "impl" && i.name === name && owners(i.parent));
  if (members.length > 0) return pick(members.map(out));
  const variants = items.filter((i) => ["enum", "struct", "union"].includes(i.kind) && i.name === owner && i.open !== null).flatMap((e) => {
    const body = clean.slice(e.open! + 1, e.close);
    const found: { lines: Lines; depth: number }[] = [];
    let depth = 0;
    for (let k = 0; k < body.length; k++) {
      const c = body[k]!;
      if ("([{".includes(c)) depth++;
      else if (")]}".includes(c)) depth--;
      else if (depth === 0 && body.startsWith(name, k) && !IDENT.test(body[k - 1] ?? " ") && (e.kind === "enum" ? /^\s*(?:[,({=]|$)/u : /^\s*:(?!:)/u).test(body.slice(k + name.length))) {
        const at = e.open! + 1 + k;
        const end = extent(clean, at, false);
        const stop = clean.slice(at, end.close).search(/,/u);
        let close = end.close - 1;
        if (stop >= 0) {
          let d = 0;
          for (let j = at; j < end.close; j++) {
            const ch = clean[j]!;
            if ("([{".includes(ch)) d++;
            else if (")]}".includes(ch)) d--;
            else if (ch === "," && d === 0) { close = j; break; }
          }
        }
        found.push(out({ kind: "variant", name, at, open: null, close, parent: e }));
      }
    }
    return found;
  });
  return pick(variants);
};

const SCRIPT_ITEM =
  /^[ \t]*(?:export[ \t]+)?(?:default[ \t]+)?(?:declare[ \t]+)?(?:abstract[ \t]+)?(?:async[ \t]+)?(function\*?|class|interface|type|const|let|var|enum|namespace)[ \t]+([A-Za-z_$][\w$]*)/u;
const SCRIPT_MEMBER =
  /^[ \t]*(?:(?:public|private|protected|static|readonly|async|override|declare|get|set|accessor)[ \t]+)*\*?#?([A-Za-z_$][\w$]*)[ \t]*[(<:=?!]/u;
const SCRIPT_BODIED = new Set(["function", "function*", "class", "interface", "enum", "namespace"]);

const script = (text: string, symbol: string): Lines | string => {
  const clean = blankScript(text);
  const starts = lineStarts(clean);
  const raw = text.split("\n");
  const lines = clean.split("\n");
  const attribute = /^@/u;
  const depthAt: number[] = [];
  let depth = 0;
  for (const line of lines) {
    depthAt.push(depth);
    for (const c of line) if (c === "{") depth++; else if (c === "}") depth--;
  }
  const top: Item[] = [];
  lines.forEach((line, n) => {
    const m = depthAt[n] === 0 ? SCRIPT_ITEM.exec(line) : null;
    if (!m) return;
    const at = starts[n]! + m[0].lastIndexOf(m[1]!);
    const { open, close } = extent(clean, at, SCRIPT_BODIED.has(m[1]!));
    top.push({ kind: m[1]!, name: m[2]!, at, open, close });
  });
  const parts = symbol.split(/\.|::/u);
  const out = (item: Item): { lines: Lines; depth: number } => ({ lines: span(raw, clean, starts, item, attribute), depth: 0 });
  if (parts.length === 1) return pick(top.filter((i) => i.name === symbol).map(out));
  const [owner, name] = parts.slice(-2) as [string, string];
  const found = top.filter((i) => i.name === owner && i.open !== null).flatMap((o) => {
    const inner = depthAt[lineOf(starts, o.open!)]! + 1;
    const first = lineOf(starts, o.open!) + 1;
    const last = lineOf(starts, o.close);
    const hits: Item[] = [];
    for (let n = first; n < last; n++) {
      const m = depthAt[n] === inner ? SCRIPT_MEMBER.exec(lines[n]!) : null;
      if (!m || m[1] !== name) continue;
      const at = starts[n]! + lines[n]!.indexOf(name);
      const body = /\(/u.test(lines[n]!.slice(lines[n]!.indexOf(name)));
      const { open, close } = extent(clean, at, body);
      hits.push({ kind: "member", name, at, open, close: Math.min(close, o.close - 1) });
    }
    return hits;
  });
  return pick(found.map(out));
};

/** GitHub's heading anchors: lower case, punctuation dropped, spaces to dashes, repeats numbered. */
export const slugify = (heading: string): string =>
  heading.trim().toLowerCase().replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, "").replace(/ /gu, "-");

const markdown = (text: string, symbol: string): Lines | string => {
  const raw = text.split("\n");
  const heads: { line: number; level: number; slug: string }[] = [];
  const seen = new Map<string, number>();
  let fenced = false;
  raw.forEach((line, n) => {
    if (/^\s*(```|~~~)/u.test(line)) fenced = !fenced;
    const m = fenced ? null : /^(#{1,6})\s+(.*?)\s*#*\s*$/u.exec(line);
    if (!m) return;
    const base = slugify(m[2]!.replace(/`/gu, ""));
    const times = seen.get(base) ?? 0;
    seen.set(base, times + 1);
    heads.push({ line: n, level: m[1]!.length, slug: times === 0 ? base : `${base}-${times}` });
  });
  const at = heads.findIndex((h) => h.slug === symbol.replace(/^#/u, ""));
  if (at < 0) return "no heading $ in @";
  const head = heads[at]!;
  const next = heads.slice(at + 1).find((h) => h.level <= head.level);
  let end = next === undefined ? raw.length - 1 : next.line - 1;
  while (end > head.line && raw[end]!.trim() === "") end--;
  return { start: head.line + 1, end: end + 1 };
};
