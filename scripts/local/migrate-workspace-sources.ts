/** One-time, checkpointed source relocation. Never operates on installed services. */
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, writeFileSync, statSync } from "node:fs";
import { dirname, extname, relative, resolve } from "node:path";
import { createHash } from "node:crypto";
import ts from "typescript";
import { appDefinitions, appDirectory, type AppId } from "@erpx/contracts/apps";
import { productPageAllowed } from "@erpx/feature-runtime/lib/product-route-policy";

const root = resolve(import.meta.dirname, "../..");
const slash = (path: string) => path.replaceAll("\\", "/");
function files(directory: string): string[] {
  if (!existsSync(resolve(root, directory))) return [];
  return readdirSync(resolve(root, directory), { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? files(`${directory}/${entry.name}`) : [`${directory}/${entry.name}`]);
}
const appFiles = files("src/app"), allSource = files("src");
if (!appFiles.length) throw new Error("SOURCE_MIGRATION_ALREADY_APPLIED");
const methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const words = new Set<string>(["overview", "draft", "approve", "setup", "accounts", "periods", "settings", "item"]);
for (const file of ["packages/feature-runtime/src/lib/product-route-policy.ts", "packages/feature-runtime/src/lib/product-api-policy.ts"]) {
  const source = ts.createSourceFile(file, readFileSync(resolve(root, file), "utf8"), ts.ScriptTarget.Latest, true);
  function visit(node: ts.Node) {
    if (ts.isStringLiteral(node) && /^[a-z][a-z0-9_/-]{0,90}$/.test(node.text)) words.add(node.text);
    ts.forEachChild(node, visit);
  }
  visit(source);
}
function routePath(file: string): string {
  return "/" + dirname(file.slice("src/app/".length)).split("/")
    .filter(part => part !== "." && !part.startsWith("@") && !/^\([^)]*\)$/.test(part))
    .map(part => part.replace(/^\(\.\.?\.?\)/, "")).join("/");
}
function samples(file: string): string[] {
  const path = routePath(file).replace(/\[holdingSlug\]/g, "simulation").replace(/\[companySlug\]/g, "simulation")
    .replace(/\[\[\.\.\.[^\]]+\]\]/g, "__tail__").replace(/\[\.\.\.[^\]]+\]/g, "__tail__")
    .replace(/\[[^\]]+\]/g, "item");
  const paths = path.includes("__tail__")
    ? [path.replace(/\/?__tail__/g, ""), ...[...words].map(word => path.replace("__tail__", word))]
    : [path];
  return [...new Set(paths.flatMap(value => [value, value.replace("/holdings/simulation/", "/holdings/erpx/")]))];
}
const entries = appFiles.filter(file => /\/(?:page|route)\.[cm]?[jt]sx?$/.test(file) && !file.split("/").some(part => part.startsWith("_")));
const memberships = new Map<string, AppId[]>();
for (const file of entries) {
  const path = routePath(file), candidates = samples(file);
  const hosts = appDefinitions.filter(app => {
    if (path === "/product-entry" || path.startsWith("/sso/") || path === "/api/health" || path === "/api/ready") return true;
    if (path.startsWith("/front/") || path.startsWith("/front-access")) return app.id === "pos" || app.id === "pms";
    if (path.startsWith("/api/v1/front/")) return app.id === "pos" || app.id === "pms";
    // Front's public parser accepts arbitrary daily paths; it does not own Office pages.
    if ((app.id === "pos" || app.id === "pms") && !path.startsWith("/api/") && !path.startsWith("/holdings/") && path !== "/login" && path !== "/") return false;
    return candidates.some(candidate => methods.some(method => productPageAllowed(app.id, candidate, method)));
  }).map(app => app.id);
  memberships.set(file, hosts);
}
type Move = { from: string; to: string; package: string; key: string; sha256: string };
const moves = new Map<string, Move>();
function add(file: string, pkg: string, tail: string) {
  const to = `packages/${pkg}/src/${tail}`, extension = extname(tail);
  const key = /\.[cm]?[jt]sx?$/.test(extension) ? tail.slice(0, -extension.length) : tail;
  moves.set(file, { from: file, to, package: `@erpx/${pkg}`, key,
    sha256: createHash("sha256").update(readFileSync(resolve(root, file))).digest("hex") });
}
for (const file of allSource) {
  let match: RegExpExecArray | null;
  if ((match = /^src\/components\/([^/]+)\/(.+)$/.exec(file))) add(file, match[1] === "ui" || match[1] === "workplace" || match[1] === "front" ? match[1] : `feature-${match[1]}`, `components/${match[2]}`);
  else if ((match = /^src\/lib\/([^/]+)\/(.+)$/.exec(file))) add(file, `feature-${match[1]}`, `lib/${match[2]}`);
  else if ((match = /^src\/server\/modules\/([^/]+)\/(.+)$/.exec(file))) add(file, `server-${match[1]}`, match[2]);
  else if (/^src\/server\/modules\/ownership\.(?:ts|json)$/.test(file)) add(file, "contracts", file.split("/").at(-1)!);
  else if (file.startsWith("src/data/")) add(file, "catalogue", file.slice("src/data/".length));
  else if (file === "src/proxy.ts") add(file, "workplace", "proxy.ts");
  else if (file.startsWith("src/app/")) {
    const hosts = memberships.get(file), own = hosts?.length === 1 ? `feature-${hosts[0]}` : "workplace";
    add(file, own, `routes/${file.slice("src/app/".length)}`);
  }
}
const resolveFile = (from: string, specifier: string): string | undefined => {
  if (!specifier.startsWith(".") && !specifier.startsWith("@/")) return;
  const path = specifier.startsWith("@/") ? `src/${specifier.slice(2)}` : slash(relative(root, resolve(root, dirname(from), specifier)));
  const stem = path.replace(/\.[cm]?js$/, "");
  return [path, ...[".ts", ".tsx", ".mts", ".js", ".json", ".css"].map(extension => stem + extension), `${path}/index.ts`, `${path}/index.tsx`].find(candidate => existsSync(resolve(root, candidate)) && !statSync(resolve(root, candidate)).isDirectory());
};
const packageSpec = (move: Move) => `${move.package}/${move.key}`;
function rewrite(file: string, text: string): string {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const edits: { start: number; end: number; value: string }[] = [];
  function visit(node: ts.Node) {
    if (ts.isStringLiteralLike(node) && (
      (ts.isImportDeclaration(node.parent) || ts.isExportDeclaration(node.parent)) && node.parent.moduleSpecifier === node
      || ts.isCallExpression(node.parent) && (node.parent.expression.kind === ts.SyntaxKind.ImportKeyword || ts.isIdentifier(node.parent.expression) && node.parent.expression.text === "require")
    )) {
      const target = resolveFile(file, node.text), move = target && moves.get(target);
      if (move) edits.push({ start: node.getStart(source) + 1, end: node.getEnd() - 1, value: packageSpec(move) });
      else if (target && moves.has(file)) {
        let path = slash(relative(dirname(moves.get(file)!.to), target));
        if (!path.startsWith(".")) path = "./" + path;
        edits.push({ start: node.getStart(source) + 1, end: node.getEnd() - 1, value: path });
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  for (const edit of edits.sort((a, b) => b.start - a.start)) text = text.slice(0, edit.start) + edit.value + text.slice(edit.end);
  // Stable repository-relative data/source references, not executable imports.
  text = text.replace(/src\/(?:components|lib|server\/modules|data)\/[A-Za-z0-9_./*-]+/g, name => {
    const exact = moves.get(name);
    if (exact) return exact.to;
    for (const [before, pkg, part] of [["src/components/", "feature-", "components"], ["src/lib/", "feature-", "lib"], ["src/server/modules/", "server-", ""]]) {
      if (!name.startsWith(before)) continue;
      const rest = name.slice(before.length), split = rest.indexOf("/");
      if (split < 0) return name;
      const owner = rest.slice(0, split), prefix = before === "src/components/" && ["ui", "workplace", "front"].includes(owner) ? owner : pkg + owner;
      return `packages/${prefix}/src/${part ? part + "/" : ""}${rest.slice(split + 1)}`;
    }
    return name;
  });
  if (moves.has(file) && /\.(?:ts|tsx)$/.test(file)) {
    const helpers = ["PageProps", "LayoutProps", "RouteContext"].filter(name => new RegExp(`\\b${name}\\s*<`).test(text));
    if (helpers.length) {
      const directive = /^(?:['"]use (?:client|server)['"];?\s*)/.exec(text)?.[0] ?? "";
      text = directive + `import type { ${helpers.join(", ")} } from "@erpx/contracts/next-routes";\n` + text.slice(directive.length);
    }
  }
  return text;
}
function entrySource(file: string): string {
  const move = moves.get(file)!, text = readFileSync(resolve(root, file), "utf8"), tree = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const names = new Set<string>(), config: string[] = [];
  const configNames = new Set(["dynamic", "dynamicParams", "revalidate", "fetchCache", "runtime", "preferredRegion", "maxDuration", "experimental_ppr"]);
  for (const statement of tree.statements) {
    if (ts.isExportAssignment(statement)) names.add("default");
    if (ts.isExportDeclaration(statement) && statement.exportClause && ts.isNamedExports(statement.exportClause)) for (const entry of statement.exportClause.elements) names.add(entry.name.text);
    if (ts.canHaveModifiers(statement)) {
      const modifiers = ts.getModifiers(statement) ?? [];
      if (modifiers.some(modifier => modifier.kind === ts.SyntaxKind.DefaultKeyword)) names.add("default");
      else if (modifiers.some(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword)) {
        if (ts.isFunctionDeclaration(statement) && statement.name) names.add(statement.name.text);
        if (ts.isVariableStatement(statement)) for (const declaration of statement.declarationList.declarations) {
          if (!ts.isIdentifier(declaration.name)) continue;
          if (configNames.has(declaration.name.text)) config.push(`export const ${declaration.getText(tree)};`);
          else names.add(declaration.name.text);
        }
      }
    }
  }
  let source = `// App-owned entry; its payload has no dependency on another Next app.\n`;
  if (file === "packages/workplace/src/routes/layout.tsx") source += 'import "./globals.css";\n';
  if (names.size) source += `export { ${[...names].join(", ")} } from ${JSON.stringify(packageSpec(move))};\n`;
  return source + config.join("\n") + "\n";
}
const entryText = new Map(appFiles.filter(file => /\/(?:page|layout|route|loading|error|global-error|not-found|forbidden|unauthorized|default|template|icon|apple-icon|manifest|robots|sitemap|opengraph-image|twitter-image)\.[cm]?[jt]sx?$/.test(file)).map(file => [file, entrySource(file)]));
const plan = { version: 1, moves: [...moves.values()], apps: appDefinitions.map(app => ({ ...app, entries: [...memberships].filter(([, ids]) => ids.includes(app.id)).map(([file]) => file) })) };
mkdirSync(resolve(root, ".scratch/dev/monorepo-migration"), { recursive: true });
writeFileSync(resolve(root, ".scratch/dev/monorepo-migration/source-plan.json"), JSON.stringify(plan, null, 2) + "\n");
if (!process.argv.includes("--apply")) {
  console.log(JSON.stringify({ moves: moves.size, apps: plan.apps.map(app => ({ id: app.id, entries: app.entries.length })) }));
  process.exit(0);
}
const active = [...new Set([...allSource, ...files("scripts"), ...files("tests"), ...files("fixtures")])];
for (const file of active) {
  if (!/\.(?:[cm]?[jt]sx?|json)$/.test(file)) continue;
  const text = readFileSync(resolve(root, file), "utf8"), changed = rewrite(file, text), to = moves.get(file)?.to ?? file;
  mkdirSync(dirname(resolve(root, to)), { recursive: true });
  writeFileSync(resolve(root, to), changed);
}
// Retain non-code workspace assets at the same package-relative location.
for (const move of moves.values()) {
  if (!existsSync(resolve(root, move.to))) { mkdirSync(dirname(resolve(root, move.to)), { recursive: true }); renameSync(resolve(root, move.from), resolve(root, move.to)); }
}
for (const [id, selected] of [["compat", entries], ...plan.apps.map(app => [app.id, app.entries] as const)] as const) {
  const entrySet = new Set(selected);
  for (const file of entryText.keys()) {
    const parent = dirname(file), ancestor = [...entrySet].some(entry => entry.startsWith(parent + "/"));
    if (!entrySet.has(file) && !(/\/(?:layout|loading|error|global-error|not-found|forbidden|unauthorized|default|template)\./.test(file) && ancestor)
      && !(parent === "src/app" && !/\/(?:page|route)\./.test(file))) continue;
    const target = resolve(root, `${appDirectory(id)}/src/app/${file.slice("src/app/".length)}`);
    mkdirSync(dirname(target), { recursive: true }); writeFileSync(target, entryText.get(file)!);
  }
  for (const file of appFiles.filter(file => /\/(?:favicon\.ico|icon\.(?:svg|png)|apple-icon\.png)$/.test(file))) {
    const target = resolve(root, `${appDirectory(id)}/src/app/${file.slice("src/app/".length)}`);
    mkdirSync(dirname(target), { recursive: true }); writeFileSync(target, readFileSync(resolve(root, file)));
  }
}
for (const move of moves.values()) {
  const path = resolve(root, move.from);
  if (existsSync(path)) { const { unlinkSync } = await import("node:fs"); unlinkSync(path); }
}
// Root styles are shared; each app adds only its reachable component sources.
const rootLayout = resolve(root, moves.get("packages/workplace/src/routes/layout.tsx")!.to);
writeFileSync(rootLayout, readFileSync(rootLayout, "utf8").replace(/^import\s+["'](?:@erpx\/[^"']+|\.\/globals\.css)["'];?\s*$/m, ""));
const globalStyles = resolve(root, moves.get("src/app/globals.css")!.to);
writeFileSync(globalStyles, readFileSync(globalStyles, "utf8").replace(/@import\s+["']\.\.\/components\/ui\/([^"']+)["'];/g, '@import "@erpx/ui/components/$1";').replace(/^@source.*;\s*$/gm, ""));
mkdirSync(resolve(root, "scripts/architecture"), { recursive: true });
writeFileSync(resolve(root, "scripts/architecture/workspace-relocations.json"), JSON.stringify({ version: 1, moves: [...moves.values()] }, null, 2) + "\n");
writeFileSync(resolve(root, "scripts/architecture/workspace-app-routes.json"), JSON.stringify({ version: 1, apps: plan.apps }, null, 2) + "\n");
console.log(`WORKSPACE_SOURCES_RELOCATED:${moves.size}`);
