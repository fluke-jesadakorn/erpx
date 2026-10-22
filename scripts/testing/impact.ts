import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { importSpecifiers, loadCatalogue, repositoryRoot, resolveImport, type Catalogue, type Suite } from "./catalogue";

export type SourceTree = ReadonlyMap<string, string>;
export type SourceOwnership = { owners: { id: string; modules: string[]; source: string[] }[] };
export type ChangedFile = { status: string; path: string; previousPath?: string };
export type ImpactResult = { mode: "full" | "affected" | "none"; reasons: string[]; modules: string[]; suites: Suite[]; changed: ChangedFile[] };
export type ImpactProfile = "acceptance" | "pull-request";

export function patternMatches(path: string, pattern: string): boolean {
  const expression = pattern.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*\*/g, "\u0000").replace(/\*/g, "[^/]*").replaceAll("\u0000", ".*");
  return new RegExp(`^${expression}$`).test(path);
}

const globalPatterns = [
  "migrations/**", "src/server/runtime*", "src/server/runtime/**", "src/server/auth*", "src/server/authorization*", "src/server/iam*", "src/server/transaction*", "src/server/modules/workflow/public/commands.ts", "src/server/modules/finance/public/finance-transactions.ts", "src/server/modules/identity_access/**", "src/server/modules/finance/**/finance-transactions.ts", "src/server/modules/shared/**", "src/server/modules/composition-root.ts", "src/server/modules/ownership*", "src/server/modules/shared.*/**", "src/data/module-registry*", "src/lib/permissions*", "src/app/layout.tsx",
  "tests/support/**", "tests/catalogue/**", "tests/config/**", "tests/runtime-*", "docs/testing/**", "fixtures/**", "scripts/**", "ops/**", "public/**", ".github/**", "*.json", "*.mts", "*.mjs", "*.config.ts", "bun.lock", "AGENTS.md",
];
const documentationOnly = (path: string) => /^(?:docs\/|adr\/|\.agents\/|Designed\/)/.test(path) || /\.(?:md|txt)$/.test(path);

function reverseGraph(tree: SourceTree): Map<string, Set<string>> {
  const files = new Set(tree.keys()), reverse = new Map<string, Set<string>>();
  for (const [file, source] of tree) for (const specifier of importSpecifiers(source, file)) {
    const target = resolveImport(file, specifier, files);
    if (!target) continue;
    const importers = reverse.get(target) ?? new Set<string>();
    importers.add(file); reverse.set(target, importers);
  }
  return reverse;
}

/** Keep both graphs: a deleted dependency and the old name of a moved file still have consumers. */
export function selectImpact(input: {
  changed: ChangedFile[]; base: SourceTree; head: SourceTree;
  baseCatalogue: Catalogue; headCatalogue: Catalogue;
  baseOwnership: SourceOwnership; headOwnership: SourceOwnership;
}, profile: ImpactProfile = "acceptance"): ImpactResult {
  const { changed, headCatalogue } = input;
  const required = headCatalogue.suites.filter(suite => suite.lane !== "provider");
  const result: ImpactResult = { mode: "none", reasons: [], modules: [], suites: [], changed };
  const full = (reason: string): ImpactResult => ({ ...result, mode: "full", reasons: [reason], modules: headCatalogue.modules, suites: required });
  const relevant = changed.flatMap(change => [change.path, ...(change.previousPath ? [change.previousPath] : [])]).filter(path =>
    !(profile === "pull-request" && /^docs\/testing\/.*\.(?:md|txt)$/i.test(path)) &&
    (globalPatterns.some(pattern => patternMatches(path, pattern)) || !documentationOnly(path)));
  if (!relevant.length) return result;
  const global = relevant.find(path => globalPatterns.some(pattern => patternMatches(path, pattern)));
  if (global) return full(`GLOBAL_DEPENDENCY:${global}`);
  const reverse = [reverseGraph(input.base), reverseGraph(input.head)];
  const catalogues = [input.baseCatalogue, headCatalogue], ownership = [input.baseOwnership, input.headOwnership];
  const modules = new Set<string>(), suiteIds = new Set<string>(), seen = new Set<string>();
  const own = (path: string) => ownership.flatMap(manifest => manifest.owners.filter(owner => owner.source.some(pattern => patternMatches(path, pattern))));
  const directSuites = (path: string) => catalogues.flatMap(catalogue => catalogue.suites.filter(suite => suite.files.includes(path)));
  for (const path of relevant) {
    const owners = own(path), suites = directSuites(path);
    if (!owners.length && !suites.length) return full(`UNMAPPED_SOURCE:${path}`);
    if (owners.some(owner => owner.id.startsWith("shared.") || ["shared", "workflow", "runtime", "files", "work_projection", "records", "commerce", "billing"].includes(owner.id))) return full(`SHARED_DEPENDENCY:${path}`);
    for (const owner of owners) for (const moduleId of owner.modules) modules.add(moduleId);
    for (const suite of suites) {
      suiteIds.add(suite.id);
      for (const moduleId of suite.modules) modules.add(moduleId);
    }
    const pending = [path];
    while (pending.length) {
      const file = pending.pop()!;
      if (seen.has(file)) continue;
      seen.add(file);
      for (const owner of own(file)) for (const moduleId of owner.modules) modules.add(moduleId);
      for (const suite of directSuites(file)) {
        suiteIds.add(suite.id);
        for (const moduleId of suite.modules) modules.add(moduleId);
      }
      for (const graph of reverse) pending.push(...graph.get(file) ?? []);
    }
  }
  result.modules = [...modules].sort();
  result.suites = required.filter(suite => suiteIds.has(suite.id) || suite.modules.some(module => modules.has(module)));
  if (!result.suites.length) return full("EMPTY_IMPACT_MAPPING");
  result.mode = "affected";
  result.reasons = ["OWNERS_AND_REVERSE_DEPENDENCIES_FROM_BASE_AND_HEAD"];
  return result;
}

const codeFile = (path: string) => /\.(?:[cm]?[jt]sx?|json)$/.test(path) && /^(?:src|tests|fixtures|scripts)\//.test(path);
function git(root: string, args: string[], input?: string): Buffer {
  return execFileSync("git", args, { cwd: root, input, maxBuffer: 256 * 1024 * 1024, stdio: ["pipe", "pipe", "pipe"] });
}
function checkedRef(root: string, reference: string): string {
  if (!reference || reference.startsWith("-")) throw new Error("INVALID_IMPACT_GIT_REFERENCE");
  return git(root, ["rev-parse", "--verify", `${reference}^{commit}`]).toString().trim();
}
export function snapshot(root: string, reference?: string): Map<string, string> {
  if (!reference) {
    const paths = git(root, ["ls-files", "--cached", "--others", "--exclude-standard", "-z"]).toString().split("\0").filter(codeFile);
    return new Map(paths.filter(path => existsSync(resolve(root, path))).map(path => [path, readFileSync(resolve(root, path), "utf8")]));
  }
  const entries = git(root, ["ls-tree", "-r", "-z", reference]).toString().split("\0").filter(Boolean).map(entry => {
    const [metadata, path] = entry.split("\t"); return { path, sha: metadata.split(" ")[2] };
  }).filter(entry => codeFile(entry.path));
  if (!entries.length) return new Map();
  const data = git(root, ["cat-file", "--batch"], entries.map(entry => entry.sha).join("\n") + "\n");
  let offset = 0; const tree = new Map<string, string>();
  for (const entry of entries) {
    const newline = data.indexOf(10, offset);
    const header = data.subarray(offset, newline).toString();
    const size = Number(header.split(" ")[2]);
    if (!Number.isSafeInteger(size)) throw new Error(`IMPACT_SNAPSHOT_READ_FAILED:${entry.path}`);
    offset = newline + 1;
    tree.set(entry.path, data.subarray(offset, offset + size).toString("utf8"));
    offset += size + 1;
  }
  return tree;
}
export function parseChanges(output: string): ChangedFile[] {
  const fields = output.split("\0").filter(Boolean), changed: ChangedFile[] = [];
  for (let index = 0; index < fields.length;) {
    const status = fields[index++], first = fields[index++];
    if (!first) throw new Error("INVALID_GIT_CHANGE_OUTPUT");
    if (/^[RC]/.test(status)) { const next = fields[index++]; if (!next) throw new Error("INVALID_GIT_RENAME_OUTPUT"); changed.push({ status, previousPath: first, path: next }); }
    else changed.push({ status, path: first });
  }
  return changed;
}
export function impactFromGit(baseReference: string, root = repositoryRoot, headReference?: string, profile: ImpactProfile = "acceptance"): ImpactResult {
  const baseRef = checkedRef(root, baseReference), headRef = headReference ? checkedRef(root, headReference) : "HEAD";
  const base = snapshot(root, baseRef), head = snapshot(root, headReference ? headRef : undefined);
  const changed = parseChanges(git(root, ["diff", "--name-status", "--find-renames", "-z", baseRef, headRef]).toString());
  if (!headReference) {
    changed.push(...parseChanges(git(root, ["diff", "--name-status", "--find-renames", "-z", "HEAD"]).toString()));
    changed.push(...git(root, ["ls-files", "--others", "--exclude-standard", "-z"]).toString().split("\0").filter(Boolean).map(path => ({ status: "A", path })));
  }
  const read = <T>(tree: SourceTree, path: string): T | undefined => tree.has(path) ? JSON.parse(tree.get(path)!) as T : undefined;
  const headCatalogue = read<Catalogue>(head, "tests/catalogue/suite-manifest.json") ?? loadCatalogue(root);
  const baseCatalogue = read<Catalogue>(base, "tests/catalogue/suite-manifest.json"), baseOwnership = read<SourceOwnership>(base, "src/server/modules/ownership.json"), headOwnership = read<SourceOwnership>(head, "src/server/modules/ownership.json");
  // The first adoption PR cannot reconstruct a missing historical mapping safely.
  if (!baseCatalogue || !baseOwnership || !headOwnership) return { mode: "full", reasons: ["BASE_OR_HEAD_MAPPING_MISSING"], modules: headCatalogue.modules, suites: headCatalogue.suites.filter(suite => suite.lane !== "provider"), changed };
  return selectImpact({ changed, base, head, baseCatalogue, headCatalogue, baseOwnership, headOwnership }, profile);
}
