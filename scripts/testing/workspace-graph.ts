import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve, posix } from "node:path";
import { execFileSync } from "node:child_process";
import { parseWorkspaceSource, type WorkspaceSource } from "./workspace-imports";
import { workspaceSourceCache } from "./workspace-source-cache";
import { appDefinitions, appDefinition } from "@erpx/contracts/apps";
import { verificationSources, resolveImport, type Suite, repositoryRoot } from "./catalogue";

export type WorkspaceGraph = { sourceCache?: {hits:number;misses:number;persisted:boolean;loaded:boolean}; runtimeImports: Map<string, Set<string>>; files: Map<string, string>; imports: Map<string, Set<string>>; reverse: Map<string, Set<string>>; apps: Map<string, Set<string>> };
const resourcePrefix = 'resource-input:';
export function workspaceInputFile(file: string) { return file.startsWith(resourcePrefix) ? file.slice(resourcePrefix.length) : file; }
const sourceExtension = /\.(?:[cm]?[jt]sx?|json|css|mjs|md|mdx|yml|yaml|py|sh|plist|toml|html|sql)$/;
const ignored = new Set(["node_modules", ".next", ".build", ".cache", ".turbo"]);
export function workspaceSourcePaths(root = repositoryRoot): string[] {
  function visit(directory: string): string[] {
    if (!existsSync(resolve(root, directory))) return [];
    return readdirSync(resolve(root, directory), { withFileTypes: true }).flatMap(entry => {
      const file = `${directory}/${entry.name}`;
      return entry.isDirectory() && !ignored.has(entry.name) ? visit(file)
        : entry.isFile() && (sourceExtension.test(entry.name) || /^(?:public|docs|fixtures)(?:\/|$)/.test(directory)) && !["next-env.d.ts", "next.config.compiled.js"].includes(entry.name) ? [file] : [];
    });
  }
  return [...["apps", "packages", "src", "scripts", "tests", "fixtures", "public", "docs", ".github", "ops", "migrations"].flatMap(visit),
    ...["next.config.ts", "tsconfig.base.json", "tsconfig.json", "postcss.config.mjs", "package.json", "bun.lock", "eslint.config.mjs", "README.md", "AGENTS.md", "CONTEXT.md", "CONTRACT_POLICY.md", ".bun-version", ".node-version", "bunfig.toml", "turbo.json", ".gitignore"].filter(file => existsSync(resolve(root, file)))].sort();
}
export function createWorkspaceGraph(files: Map<string, string>, parseSource: (file:string,source:string)=>WorkspaceSource = parseWorkspaceSource): WorkspaceGraph {
  files = new Map(files);
  const names = new Set(files.keys()), imports = new Map<string, Set<string>>(), reverse = new Map<string, Set<string>>();
  const parsed=new Map<string,WorkspaceSource>();
  const resources=new Map<string,Set<string>>();
  const manifests=new Map<string,{exports?:Record<string,string>}>();
  for(const [file,text] of files){const owner=/^packages\/([^/]+)\/package\.json$/.exec(file)?.[1];if(owner)manifests.set(owner,JSON.parse(text));}
  const directories = new Map<string, string[]>();
  for (const file of names) {
    const parts = file.split("/");
    for (let length = 1; length < parts.length; length++) {
      const directory = parts.slice(0, length).join("/"), entries = directories.get(directory) ?? [];
      entries.push(file); directories.set(directory, entries);
    }
  }
  for (const [file, text] of [...files]) {
    const dependencies = new Set<string>();
    const reads=new Set<string>();resources.set(file,reads);
    const syntax=/\.[cm]?[jt]sx?$/.test(file)?parseSource(file,text):undefined;
    if(syntax)parsed.set(file,syntax);
    const styleImports = file.endsWith('.css') ? [...text.matchAll(/@(?:import|reference)\s+["']([^"']+)["']/g)].map(match => match[1]) : [];
    for (const specifier of syntax ? syntax.modules.map(item=>item.specifier) : styleImports) {
      let target = resolveImport(file, specifier, names);
      if (specifier.startsWith("@erpx/")) {
        const [, owner, ...subpath] = specifier.split("/");
        const manifest = manifests.get(owner);
        if (manifest) {
          const exported = manifest.exports?.[subpath.length ? `./${subpath.join("/")}` : "."];
          if (typeof exported !== "string" || !exported.startsWith("./src/") || exported.split("/").includes(".."))
            throw new Error(`WORKSPACE_EXPORT_UNDECLARED:${file}:${specifier}`);
          target = `packages/${owner}/${exported.slice(2)}`;
          if (!names.has(target)) throw new Error(`WORKSPACE_EXPORT_UNRESOLVED:${file}:${specifier}`);
        }
      }
      if (target) {
        const sourceApp = /^apps\/([^/]+)\//.exec(file)?.[1];
        const targetApp = /^apps\/([^/]+)\//.exec(target)?.[1];
        if (targetApp && (sourceApp && sourceApp !== targetApp || file.startsWith("packages/")))
          throw new Error(`WORKSPACE_APP_IMPORT_FORBIDDEN:${file}:${target}`);
        dependencies.add(target);
      }
      else if (specifier.startsWith("@erpx/")) throw new Error(`WORKSPACE_EXPORT_UNRESOLVED:${file}:${specifier}`);
      else if (specifier.startsWith(".") || specifier.startsWith("@/")) throw new Error(`WORKSPACE_IMPORT_UNRESOLVED:${file}:${specifier}`);
    }
    if (file.endsWith('.css')) for (const match of text.matchAll(/@source\s+["']([^"']+)["']/g)) {
      const path = posix.normalize(posix.join(posix.dirname(file), match[1])).replace(/\/$/, "");
      const sources = names.has(path) ? [path] : directories.get(path) ?? [];
      if (!sources.length) throw new Error(`WORKSPACE_STYLE_SOURCE_UNRESOLVED:${file}:${match[1]}`);
      for (const source of sources) {
        // Tailwind scans text. It does not execute that file's JavaScript
        // imports, so use a leaf input instead of expanding the server graph.
        if (!/\.[cm]?[jt]sx?$|\.html?$/.test(source)) continue;
        const sourceApp = /^apps\/([^/]+)\//.exec(file)?.[1], targetApp = /^apps\/([^/]+)\//.exec(source)?.[1];
        if (targetApp && (sourceApp && sourceApp !== targetApp || file.startsWith("packages/"))) throw new Error(`WORKSPACE_APP_IMPORT_FORBIDDEN:${file}:${source}`);
        const resource = resourcePrefix + source;
        files.set(resource, createHash('sha256').update(files.get(source)!).digest('hex'));
        imports.set(resource, new Set());
        dependencies.add(resource);
        reads.add(resource);
        const readers = reverse.get(source) ?? new Set<string>(); readers.add(resource); reverse.set(source, readers);
      }
    }
    // Resolve resource syntax against today's file inventory, never cached targets.
    if(syntax){
      for(const literal of syntax.strings)if(names.has(literal))reads.add(literal);
      for(const path of syntax.paths){if(names.has(path))reads.add(path);else for(const candidate of directories.get(path)??[])reads.add(candidate);}
      for(const url of syntax.urls){const target=resolveImport(file,url,names);if(target)reads.add(target);}
      for(const target of reads)dependencies.add(target);
    }
    imports.set(file, dependencies);
    for (const target of dependencies) { const users = reverse.get(target) ?? new Set<string>(); users.add(file); reverse.set(target, users); }
  }
  const runtimeImports = new Map<string, Set<string>>();
  for (const [file, dependencies] of imports) {
    const runtime = new Set(dependencies);
    const syntax=parsed.get(file);
    if(syntax){
      const executable=new Map<string,boolean>();
      for(const {specifier,runtime:value} of syntax.modules){
        let target=resolveImport(file,specifier,names);
        if(specifier.startsWith('@erpx/')){const [,owner,...subpath]=specifier.split('/'),exported=manifests.get(owner)?.exports?.[subpath.length?`./${subpath.join('/')}`:'.'];if(typeof exported==='string')target=`packages/${owner}/${exported.slice(2)}`;}
        if(target)executable.set(target,executable.get(target)===true||value);
      }
      for(const [target,value]of executable)if(!value&&!resources.get(file)?.has(target))runtime.delete(target);
    }
    runtimeImports.set(file, runtime);
  }
  const graph: WorkspaceGraph = { files, imports, runtimeImports, reverse, apps: new Map() };
  for (const app of appDefinitions) {
    const entries = [...names].filter(file => file.startsWith(`${app.directory}/src/`) || file === `${app.directory}/next.config.mjs` || file === `${app.directory}/package.json`);
    graph.apps.set(app.id, dependencyClosure(graph, entries));
  }
  return graph;
}
function graphWithSourceCache(files:Map<string,string>,root:string,cacheRoot=root):WorkspaceGraph{
  const cache=workspaceSourceCache(root,cacheRoot);
  try{const graph=createWorkspaceGraph(files,cache.parse);cache.flush();graph.sourceCache={...cache.stats};return graph;}catch(error){cache.flush();throw error;}
}
export function loadWorkspaceGraph(root = repositoryRoot, options:{cacheRoot?:string}={}): WorkspaceGraph {
  return graphWithSourceCache(new Map(workspaceSourcePaths(root).map(file => [file, /^(?:public|docs|\.github|ops|migrations)\//.test(file) || /^(?:README|AGENTS|CONTEXT|CONTRACT_POLICY)\.md$/.test(file) ? "resource-sha256:" + createHash("sha256").update(readFileSync(resolve(root, file))).digest("hex") : readFileSync(resolve(root, file), "utf8")])),root,options.cacheRoot);
}

export function dependencyClosure(graph: Pick<WorkspaceGraph, "imports">, entries: Iterable<string>): Set<string> {
  const seen = new Set<string>(), pending = [...entries];
  for (const file of pending) { if (seen.has(file)) continue; seen.add(file); for (const dependency of graph.imports.get(file) ?? []) if (!seen.has(dependency)) pending.push(dependency); }
  return seen;
}
export function sourceUsers(graph: WorkspaceGraph, changed: Iterable<string>): Set<string> {
  const users = new Set<string>(), pending = [...changed];
  for (const file of pending) { if (users.has(file)) continue; users.add(file); for (const user of graph.reverse.get(file) ?? []) if (!users.has(user)) pending.push(user); }
  return users;
}
export function changedWorkspaceFiles(root = repositoryRoot, base = "HEAD", head?: string): string[] {
  const git = (args: string[]) => execFileSync("git", args, { cwd: root, encoding: "utf8", maxBuffer:64*1024*1024 }).split("\0").filter(Boolean);
  execFileSync("git", ["rev-parse", "--verify", `${base}^{commit}`], { cwd: root, stdio: "ignore" });
  const paths = git(["diff", "--name-only", "--no-renames", "-z", base, ...(head ? [head] : [])]);
  if (!head) paths.push(...git(["ls-files", "--others", "--exclude-standard", "-z"]));
  return [...new Set(paths)].sort();
}
export function selectWorkspaceSuites(graph: WorkspaceGraph, suites: readonly Suite[], options: { apps?: string[]; changed?: string[] }) {
  for (const id of options.apps ?? []) appDefinition(id);
  const users = sourceUsers(graph, options.changed ?? []);
  const selected = suites.filter(suite => {
    if (suite.lane === "provider" || suite.lane === "uat") return false;
    if (verificationSources([suite]).some(file => users.has(file))) return true;
    if (options.apps?.length) {
      if (!suite.apps || !suite.packages) throw new Error(`WORKSPACE_SUITE_OWNERSHIP_REQUIRED:${suite.id}`);
      return suite.apps.some(id => options.apps!.includes(id));
    }
    return false;
  });
  const affectedApps = options.apps?.length ? options.apps : appDefinitions.filter(app => [...graph.apps.get(app.id)!].some(file => users.has(file))).map(app => app.id);
  return { scope: "development" as const, acceptance: false as const, apps: affectedApps, suites: selected };
}
export function workspaceFingerprint(graph: WorkspaceGraph, entries: Iterable<string>, additional: Iterable<string> = []): string {
  const hash = createHash("sha256").update("erpx-workspace-input/v1:node26.11.1:bun1.4.3");
  const files = new Set([...dependencyClosure(graph, entries), ...additional, "bun.lock", "tsconfig.base.json"]);
  for (const file of [...files].sort()) { const content = graph.files.get(file); if (content !== undefined) hash.update(file).update("\0").update(content).update("\0"); }
  return hash.digest("hex");
}
export function appRuntimeFingerprint(graph: WorkspaceGraph, id: string) {
  const app = appDefinition(id);
  const runtime = { ...graph, imports: graph.runtimeImports, files:new Map(graph.files) };
  // This file is derived from the runtime identity during artifact assembly;
  // including its previous value would make that identity self-referential.
  if(runtime.files.has('packages/catalogue/src/release-artifact.json'))runtime.files.set('packages/catalogue/src/release-artifact.json','generated-runtime-release-identity');
  for (const [file, text] of runtime.files) if (/^(?:apps|packages)\/[^/]+\/package\.json$/.test(file)) {
    const manifest = JSON.parse(text);
    if (manifest.scripts) for (const name of Object.keys(manifest.scripts)) if (/^(?:test(?::|$)|lint(?::|$)|typecheck(?::|$))/.test(name)) delete manifest.scripts[name];
    runtime.files.set(file, JSON.stringify(manifest));
  }
  const entries = [...graph.files.keys()].filter(file => file.startsWith(`${app.directory}/src/`) || file.startsWith(`${app.directory}/`) && /(?:config|package)\./.test(file));
  const source = workspaceFingerprint(runtime, entries, ["scripts/local/app-next-config.ts", "next.config.ts", "postcss.config.mjs", ...[...graph.files.keys()].filter(file => /^(?:public|migrations)\//.test(file))]);
  return createHash("sha256").update(source).update(JSON.stringify(Object.entries(process.env).filter(([key]) => key.startsWith("NEXT_PUBLIC_")).sort(([a],[b]) => a.localeCompare(b)))).digest("hex");
}
if (import.meta.main) {
  const graph = loadWorkspaceGraph();
  console.log(JSON.stringify({ files: graph.files.size, apps: appDefinitions.map(app => ({ id: app.id, dependencies: graph.apps.get(app.id)!.size, fingerprint: appRuntimeFingerprint(graph, app.id) })) }, null, 2));
}

/** Read both sides of a candidate without switching the shared checkout. */
export function loadRevisionWorkspaceGraph(revision: string, root = repositoryRoot): WorkspaceGraph {
  const sha = execFileSync('git', ['rev-parse', '--verify', `${revision}^{commit}`], { cwd: root, encoding: 'utf8' }).trim();
  const paths = execFileSync('git', ['ls-tree', '-rz', '--name-only', sha], { cwd: root, encoding: 'utf8', maxBuffer:64*1024*1024 }).split('\0')
    .filter(file => (sourceExtension.test(file) || /^(?:public|docs|fixtures)\//.test(file)) && (/^(?:apps|packages|src|scripts|tests|fixtures|public|docs|\.github|ops|migrations)\//.test(file) || ['next.config.ts','tsconfig.base.json','tsconfig.json','postcss.config.mjs','package.json','bun.lock','eslint.config.mjs','README.md','AGENTS.md','CONTEXT.md','CONTRACT_POLICY.md','.bun-version','.node-version','bunfig.toml','turbo.json','.gitignore'].includes(file)));
  if (paths.some(file => /[\r\n]/.test(file))) throw new Error('UNSUPPORTED_GIT_SOURCE_NAME');
  const contents = execFileSync('git', ['cat-file', '--batch'], { cwd: root, input: paths.map(file => `${sha}:${file}\n`).join(''), maxBuffer: 512 * 1024 * 1024 });
  const files = new Map<string,string>(); let offset = 0;
  for (const file of paths) {
    const end = contents.indexOf(10, offset), metadata = contents.subarray(offset, end).toString().split(' '), size = Number(metadata.at(-1));
    if (metadata[1] !== 'blob' || !Number.isSafeInteger(size)) throw new Error(`GIT_SOURCE_UNREADABLE:${file}`);
    offset = end + 1; files.set(file, /^(?:public|docs|\.github|ops|migrations)\//.test(file) || /^(?:README|AGENTS|CONTEXT|CONTRACT_POLICY)\.md$/.test(file) ? "resource-sha256:" + createHash("sha256").update(contents.subarray(offset,offset + size)).digest("hex") : contents.subarray(offset, offset + size).toString()); offset += size + 1;
  }
  return graphWithSourceCache(files,root);
}
export function selectCandidateSuites(head: WorkspaceGraph, base: WorkspaceGraph, suites: readonly Suite[], changed: string[], apps: string[] = []) {
  const known = changed.filter(file => head.files.has(file) || base.files.has(file));
  const missing = changed.filter(file => !head.files.has(file) && !base.files.has(file) && !/^(?:docs|public|migrations|ops|\.github)\//.test(file) && !/^(?:[A-Z][A-Z_.-]*\.md|\.gitignore|\.bun-version|\.node-version|bunfig\.toml|turbo\.json|tsconfig(?:\.[\w-]+)?\.json)$/.test(file));
  if (missing.length) throw new Error(`WORKSPACE_CHANGE_UNMAPPED:${missing.join(',')}`);
  const a = selectWorkspaceSuites(head, suites, { apps, changed: known });
  const b = selectWorkspaceSuites(base, suites, { changed: known });
  const global = changed.some(file => ['bun.lock','package.json','tsconfig.base.json','next.config.ts','postcss.config.mjs'].includes(file));
  const migrations=changed.filter(file=>file.startsWith('migrations/postgres/'));
  const affectedApps = global || migrations.length ? appDefinitions.map(app => app.id) : [...new Set([...a.apps, ...b.apps])];
  const selected = new Set([...a.suites, ...b.suites].map(suite => suite.id));
  // App entry points and package/public assets have no test import edge. Their
  // registered owners supply the integration and UI boundary coverage.
  if (global) for (const suite of suites) if (suite.lane !== 'provider' && suite.lane !== 'uat') selected.add(suite.id);
  const tracks=new Set(migrations.map(file=>file.split('/')[2]));
  const track=(slot:string)=>slot==='CONTROL_DB'?'control':slot==='CONTROL_READ_DB'?'control-read':slot.includes('_READ_')?'holding-read':'holding-write';
  for (const suite of suites) if (suite.postgres?.slots.some(slot=>tracks.has(track(slot)))) selected.add(suite.id);
  const boundaryApps = changed.some(file => /^(?:apps|public)\//.test(file)) ? affectedApps : [];
  if (changed.some(file => file.endsWith('.css'))) for (const suite of suites)
    if (['e2e','journeys'].includes(suite.lane) && suite.apps?.some(id => affectedApps.includes(id))) selected.add(suite.id);
  for (const suite of suites) if (suite.apps?.some(id => boundaryApps.includes(id)) && suite.lane !== 'provider' && suite.lane !== 'uat') selected.add(suite.id);
  const covered = new Set<string>();
  for (const graph of [head,base]) for (const suite of suites) for (const file of dependencyClosure(graph,verificationSources([suite]))) covered.add(file);
  const unmapped = known.filter(file => /^(?:packages|src|apps|tests\/(?:unit|integration))\//.test(file) && !covered.has(file) && !affectedApps.length && head.files.has(file));
  if (unmapped.length) throw new Error(`WORKSPACE_TEST_MAPPING_REQUIRED:${unmapped.join(',')}`);
  return { scope: 'development' as const, acceptance: false as const, apps: affectedApps, changed, suites: suites.filter(suite => selected.has(suite.id)), removed: changed.filter(file => base.files.has(file) && !head.files.has(file)) };
}
