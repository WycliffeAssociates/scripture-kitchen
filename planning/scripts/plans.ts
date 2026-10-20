/**
 * The plans from the command line, for people and agents.
 *
 *   pnpm plans ls [--status s] [--kind k] [--tag t] [--all] [--sort priority|updated|created]
 *   pnpm plans open                       open questions and follow-ups, with file:line
 *   pnpm plans json                       everything, for jq
 *   pnpm plans new "Title" [--kind idea] [--status idea] [--tag t] [--priority 1]
 *   pnpm plans roadmap                    the roadmap's phases and steps, with each item's status
 *   pnpm plans check                      validate items, and the roadmap's references
 *
 *   pnpm -s plans json | jq '.[] | select(.status == "discussing") | .title'
 *   pnpm -s plans json | jq '[.[] | .lifted[] | select(.kind == "Question" and .open)] | length'
 *
 * Folders: PLANS_ITEMS (default ./items), PLANS_REPO_ROOT (default: the git checkout) for
 * paths and the commit sha.
 */

import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { existsSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { parseArgs } from "node:util";
import { parseFile } from "../src/lib/lift.ts";
import { repoRoot as root } from "../src/lib/root.ts";
import { readRoadmap, refersTo, roadmapPath } from "../src/lib/roadmap.ts";
import { CLOSED, itemSchema, KINDS, STATUSES } from "../src/lib/schema.ts";

const itemsDir = resolve(process.env["PLANS_ITEMS"] ?? "./items");

const walk = (dir: string): string[] =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return walk(path);
    return path.endsWith(".mdx") ? [path] : [];
  });

const load = () =>
  walk(itemsDir).map((file) => {
    const path = relative(root, file);
    try {
      const { frontmatter, lifted } = parseFile(file);
      return { file, path, frontmatter, lifted, error: null, result: itemSchema.safeParse(frontmatter) };
    } catch (error) {
      // An item that does not parse as MDX still shows up, with the reason.
      const reason = error instanceof Error ? error.message : String(error);
      return { file, path, frontmatter: {} as Record<string, unknown>, lifted: [], error: reason, result: null };
    }
  });

const day = (value: unknown): string =>
  value instanceof Date ? value.toISOString().slice(0, 10) : String(value ?? "");

const { positionals, values } = parseArgs({
  allowPositionals: true,
  options: {
    status: { type: "string" },
    kind: { type: "string" },
    tag: { type: "string" },
    all: { type: "boolean", default: false },
    sort: { type: "string", default: "priority" },
    priority: { type: "string" },
  },
});
const [command = "ls", ...rest] = positionals;

/** The loaded item a roadmap step names. */
const resolveRef = (all: ReturnType<typeof load>, ref: string) =>
  all.find((one) =>
    refersTo(ref, { id: String(one.frontmatter["id"]), slug: one.file.split("/").pop()!.replace(/\.mdx$/u, "") }),
  );

switch (command) {
  case "roadmap": {
    const phases = readRoadmap();
    if (phases === null) {
      console.log(`No roadmap at ${relative(root, roadmapPath())}.`);
      break;
    }
    const all = load();
    for (const phase of phases) {
      console.log(`\n${phase.title}${phase.goal ? ` — ${phase.goal}` : ""}`);
      for (const step of phase.steps) {
        const bar = step.group === null ? "  " : "  ‖ ";
        if (step.kind === "Gate") {
          console.log(`${bar}◆ ${step.text}`);
          continue;
        }
        const one = step.item ? resolveRef(all, step.item) : undefined;
        const f = one?.frontmatter ?? {};
        const title = one ? String(f["title"]) : step.item ? `MISSING ${step.item}` : step.text;
        const extras = step.extras.length > 0 ? `  + ${step.extras.join(", ")}` : "";
        console.log(`${bar}${String(step.n).padStart(2)}. ${String(f["status"] ?? "").padEnd(12)} ${title}${extras}`);
      }
    }
    break;
  }
  case "json": {
    const out = load().map(({ path, frontmatter, lifted }) => ({ ...frontmatter, path, lifted }));
    console.log(JSON.stringify(out, null, 2));
    break;
  }
  case "ls": {
    const rows = load()
      .map(({ path, frontmatter: f }) => ({ path, f }))
      .filter(({ f }) => values.all || values.status !== undefined || !CLOSED.includes(f["status"] as never))
      .filter(({ f }) => values.status === undefined || f["status"] === values.status)
      .filter(({ f }) => values.kind === undefined || f["kind"] === values.kind)
      .filter(({ f }) => values.tag === undefined || (f["tags"] as string[] | undefined)?.includes(values.tag))
      .sort((a, b) => {
        if (values.sort === "priority") {
          const rank = (f: Record<string, unknown>): number =>
            typeof f["priority"] === "number" ? f["priority"] : Number.POSITIVE_INFINITY;
          const by = rank(a.f) - rank(b.f);
          if (by !== 0) return by;
        }
        const field = values.sort === "created" ? "created" : "updated";
        return day(b.f[field]).localeCompare(day(a.f[field]));
      });
    for (const { path, f } of rows) {
      const p = typeof f["priority"] === "number" ? `P${String(f["priority"])}` : "  ";
      console.log(`${p.padEnd(3)} ${String(f["status"]).padEnd(12)} ${day(f["updated"])}  ${String(f["title"])}  (${path})`);
    }
    break;
  }
  case "open": {
    const roadmap = roadmapPath();
    const sources = [
      ...(existsSync(roadmap)
        ? [{ path: relative(root, roadmap), frontmatter: { title: "Roadmap" } as Record<string, unknown>, lifted: parseFile(roadmap).lifted }]
        : []),
      ...load(),
    ];
    for (const { path, frontmatter: f, lifted } of sources) {
      if (CLOSED.includes(f["status"] as never)) continue;
      const open = lifted.filter((l) => l.open);
      if (open.length === 0) continue;
      console.log(`\n${String(f["title"])}  (${path})`);
      for (const l of open) {
        const due = l.attrs["due"] ? ` due ${String(l.attrs["due"])}` : "";
        console.log(`  ${l.kind === "Question" ? "?" : "☐"} ${path}:${l.line}${l.id ? ` [${l.id}]` : ""}${due}  ${l.text}`);
      }
    }
    break;
  }
  case "check": {
    let bad = 0;
    const ids = new Map<string, string>();
    for (const { path, frontmatter, lifted, result, error } of load()) {
      if (error !== null || result === null) {
        bad += 1;
        console.log(`✗ ${path}: does not parse as MDX: ${error ?? ""}`);
        continue;
      }
      if (!result.success) {
        bad += 1;
        console.log(`✗ ${path}`);
        for (const issue of result.error.issues) console.log(`    ${issue.path.join(".")}: ${issue.message}`);
      }
      const id = String(frontmatter["id"]);
      if (ids.has(id)) {
        bad += 1;
        console.log(`✗ ${path}: id ${id} is also used by ${ids.get(id) ?? ""}`);
      }
      ids.set(id, path);
      const seen = new Set<string>();
      for (const l of lifted) {
        const key = `${l.kind}:${l.id ?? ""}`;
        if (l.id !== null && seen.has(key)) {
          bad += 1;
          console.log(`✗ ${path}:${l.line}: two ${l.kind}s with id "${l.id}"`);
        }
        seen.add(key);
      }
    }
    const phases = readRoadmap();
    if (phases !== null) {
      const all = load();
      const where = relative(root, roadmapPath());
      for (const step of phases.flatMap((p) => p.steps)) {
        if (step.item !== null && resolveRef(all, step.item) === undefined) {
          bad += 1;
          console.log(`✗ ${where}:${step.line}: no item "${step.item}"`);
        }
      }
    }
    console.log(bad === 0 ? "All items valid." : `${bad} problem(s).`);
    process.exitCode = bad === 0 ? 0 : 1;
    break;
  }
  case "new": {
    const title = rest.join(" ").trim();
    if (title === "") throw new Error('Usage: pnpm plans new "Title" [--kind idea]');
    const kind = values.kind ?? "idea";
    const status = values.status ?? "idea";
    if (!KINDS.includes(kind as never)) throw new Error(`kind must be one of ${KINDS.join(", ")}`);
    if (!STATUSES.includes(status as never)) throw new Error(`status must be one of ${STATUSES.join(", ")}`);
    let sha = "";
    try {
      sha = execFileSync("git", ["-C", root, "rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
    } catch {
      // Not a git checkout; the field is optional.
    }
    const today = new Date().toISOString().slice(0, 10);
    const slug = title.toLowerCase().replace(/[^a-z0-9]+/gu, "-").replace(/^-|-$/gu, "").slice(0, 50);
    const file = join(itemsDir, `${today}-${slug}.mdx`);
    if (existsSync(file)) throw new Error(`${file} exists`);
    const lines = [
      "---",
      `id: ${randomUUID()}`,
      `title: ${JSON.stringify(title)}`,
      `kind: ${kind}`,
      `status: ${status}`,
      `created: ${today}`,
      `updated: ${today}`,
      ...(sha ? [`sha: ${sha}`] : []),
      ...(values.priority !== undefined ? [`priority: ${String(Number(values.priority))}`] : []),
      `tags: [${values.tag ?? ""}]`,
      "---",
      "",
      "",
    ];
    writeFileSync(file, lines.join("\n"));
    console.log(relative(process.cwd(), file));
    break;
  }
  default:
    throw new Error(`Unknown command "${command}". Try: ls, open, json, new, check.`);
}
