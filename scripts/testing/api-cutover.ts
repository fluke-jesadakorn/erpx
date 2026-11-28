import { ownerWorkspaceDirectories } from "../local/workspace-paths";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const root = resolve(import.meta.dirname, "../..");
const output = resolve(root, ".scratch/api-cutover");
const manifestFile = join(output, "route-manifest.json");

type OwnerImport = { owner: string };
type Route = {
  package: string;
  file: string;
  path: string;
  methods: string[];
  owners: string[];
  nativeRefs: number;
  sqlTokens: number;
  bytes: number;
  consumers: string[];
  consumerCount: number;
  standalone: boolean;
};

function walk(directory: string, files: string[] = []): string[] {
  if (!existsSync(directory)) return files;
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) walk(path, files);
    else if (entry.name === "route.ts") files.push(path);
  }
  return files;
}

function pattern(path: string): string {
  return path
    .replace(/^packages\/[^/]+\/src\/routes/, "")
    .replace(/^apps\/[^/]+\/src\/app/, "")
    .replace(/\/route\.ts$/, "")
    .replace(/\/\[\[\.\.\.([^\]]+)\]\]/g, "/{...$1}")
    .replace(/\/\[\.\.\.([^\]]+)\]/g, "/{...$1}")
    .replace(/\/\[([^\]]+)\]/g, "/{$1}")
    .replace(/\/+$/, "") || "/";
}

function methods(source: string): string[] {
  const found = new Set<string>();
  for (const match of source.matchAll(/export\s+(?:async\s+)?function\s+(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\b/g)) found.add(match[1]);
  for (const match of source.matchAll(/export\s*\{([^}]+)\}\s*(?:from\s*["'][^"']+["'])?/g)) {
    for (const name of match[1].split(",")) {
      const method = name.trim().split(/\s+as\s+/).pop()?.trim();
      if (method && /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)$/.test(method)) found.add(method);
    }
  }
  return [...found].sort();
}

function owners(source: string): string[] {
  const found = new Set<string>();
  for (const match of source.matchAll(/@erpx\/server-([a-z0-9_]+)/g)) found.add(match[1]);
  return [...found].sort();
}

function consumerTarget(source: string): string | undefined {
  const match = /from\s*["'](@erpx\/(?:workplace|feature-[^/]+)\/[^"']+)["']/.exec(source);
  if (!match) return undefined;
  const value = match[1];
  return `packages/${value.slice("@erpx/".length).replace(/^([^/]+)\//, "$1/src/")}.ts`;
}

function routeFor(file: string, source: string, standalone: boolean): Route {
  const relativeFile = relative(root, file);
  return {
    package: standalone ? `app:${relativeFile.split("/")[1]}` : relativeFile.split("/")[1]!,
    file: relativeFile,
    path: pattern(relativeFile),
    methods: methods(source),
    owners: owners(source),
    nativeRefs: [...source.matchAll(/native[A-Z_]|native-|native_/g)].length,
    sqlTokens: [...source.matchAll(/\b(SELECT|INSERT|UPDATE|DELETE)\b/g)].length,
    bytes: Buffer.byteLength(source),
    consumers: [],
    consumerCount: 0,
    standalone,
  };
}

function inventory() {
  const packageFiles = ownerWorkspaceDirectories.flatMap(directory => walk(join(root, directory), [])).filter((file) => file.includes("/src/routes/"));
  const appFiles = walk(join(root, "apps"), []).filter((file) => file.includes("/src/app/api/"));
  const consumers = new Map<string, string[]>();
  for (const file of appFiles) {
    const target = consumerTarget(readFileSync(file, "utf8"));
    if (!target) continue;
    const list = consumers.get(target) ?? [];
    list.push(relative(root, file));
    consumers.set(target, list);
  }
  const packageRoutes: Route[] = packageFiles.filter((file) => file.includes("/src/routes/api/")).map((file) => {
    const relativeFile = relative(root, file);
    const route = routeFor(file, readFileSync(file, "utf8"), false);
    route.consumers = consumers.get(relativeFile) ?? [];
    route.consumerCount = route.consumers.length;
    return route;
  });
  const standaloneRoutes = appFiles.filter((file) => !consumerTarget(readFileSync(file, "utf8"))).map((file) => routeFor(file, readFileSync(file, "utf8"), true));
  const routes = [...packageRoutes, ...standaloneRoutes];
  const byPackage = Object.fromEntries([...new Set(routes.map((route) => route.package))].sort().map((id) => [id, routes.filter((route) => route.package === id).length]));
  const manifest = {
    version: 1,
    sourceSha: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
    generatedAt: new Date().toISOString(),
    summary: {
      appRouteFiles: appFiles.length,
      packageRouteFiles: packageFiles.length,
      businessRouteFiles: packageRoutes.length,
      standaloneAppRoutes: standaloneRoutes.length,
      totalHandlers: routes.length,
      routePaths: new Set(routes.map((route) => route.path)).size,
      methods: routes.reduce((total, route) => total + route.methods.length, 0),
      withSql: routes.filter((route) => route.sqlTokens > 0).length,
      withNative: routes.filter((route) => route.nativeRefs > 0).length,
      withOwnerTs: routes.filter((route) => route.owners.length > 0).length,
      withoutNative: routes.filter((route) => route.nativeRefs === 0).length,
      unmappedAppRoutes: standaloneRoutes.length,
      byPackage,
    },
    routes,
  };
  mkdirSync(output, { recursive: true });
  writeFileSync(manifestFile, JSON.stringify(manifest, null, 2) + "\n");
  console.log(JSON.stringify({ manifest: relative(root, manifestFile), ...manifest.summary }, null, 2));
}

function counts() {
  const app = walk(join(root, "apps"), []).filter((file) => file.includes("/src/app/api/")).length;
  const handlers = ownerWorkspaceDirectories.flatMap(directory => walk(join(root, directory), [])).filter((file) => file.includes("/src/routes/api/")).length;
  return { sourceSha: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(), app, handlers, total: app + handlers };
}

function check() {
  const manifest = JSON.parse(readFileSync(manifestFile, "utf8")) as { sourceSha: string; routes: Route[]; summary: Record<string, unknown> };
  const current = counts();
  if (manifest.sourceSha !== current.sourceSha) throw new Error(`API_CUTOVER_MANIFEST_STALE:${manifest.sourceSha}:${current.sourceSha}`);
  const expected = (manifest.summary.appRouteFiles as number) + (manifest.summary.businessRouteFiles as number);
  if (current.total !== expected) throw new Error(`API_CUTOVER_ROUTE_COUNT_CHANGED:${expected}:${current.total}`);
  console.log(JSON.stringify(current, null, 2));
}

if (process.argv[2] === "check") check();
else if (process.argv[2] === "count") console.log(JSON.stringify(counts(), null, 2));
else inventory();
