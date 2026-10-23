import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync, symlinkSync } from "node:fs";
import { extname, resolve } from "node:path";
import ts from "typescript";
import { refreshWorkspaceStyles } from "../assets/workspace-styles";
import { appDefinitions } from "@erpx/contracts/apps";

const root = resolve(import.meta.dirname, "../.."), rootPackage = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
function files(directory: string): string[] {
  if (!existsSync(resolve(root, directory))) return [];
  return readdirSync(resolve(root, directory), { withFileTypes: true }).flatMap(entry => entry.isDirectory() && !["node_modules", ".next", ".build", ".cache"].includes(entry.name)
    ? files(`${directory}/${entry.name}`) : entry.isFile() ? [`${directory}/${entry.name}`] : []);
}
const allNames = new Set(readdirSync(resolve(root, "packages"), { withFileTypes: true }).filter(entry => entry.isDirectory()).map(entry => `@erpx/${entry.name}`));
const version = (name: string) => rootPackage.dependencies[name] ?? rootPackage.devDependencies[name];
function dependencies(sourceFiles: string[]) {
  const result: Record<string, string> = {};
  for (const file of sourceFiles.filter(file => /\.[cm]?[jt]sx?$/.test(file))) {
    const tree = ts.createSourceFile(file, readFileSync(resolve(root, file), "utf8"), ts.ScriptTarget.Latest, true);
    function visit(node: ts.Node) {
      let specifier: string | undefined;
      if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) specifier = node.moduleSpecifier.text;
      if (ts.isCallExpression(node) && (node.expression.kind === ts.SyntaxKind.ImportKeyword || ts.isIdentifier(node.expression) && node.expression.text === "require") && node.arguments.length && ts.isStringLiteral(node.arguments[0])) specifier = node.arguments[0].text;
      if (specifier && !specifier.startsWith(".") && !specifier.startsWith("node:") && !specifier.startsWith("@/")) {
        const name = specifier.startsWith("@") ? specifier.split("/").slice(0, 2).join("/") : specifier.split("/")[0];
        if (allNames.has(name)) result[name] = "workspace:*";
        else if (version(name)) result[name] = version(name);
      }
      ts.forEachChild(node, visit);
    }
    visit(tree);
  }
  return Object.fromEntries(Object.entries(result).sort(([a], [b]) => a.localeCompare(b)));
}
for (const name of [...allNames].sort()) {
  const directory = `packages/${name.slice("@erpx/".length)}`, sourceFiles = files(`${directory}/src`), exports: Record<string, string> = {};
  for (const file of sourceFiles) {
    const path = file.slice(`${directory}/src/`.length), extension = extname(path);
    const key = /\.[cm]?[jt]sx?$/.test(extension) ? path.slice(0, -extension.length) : path;
    exports[`./${key}`] = `./src/${path}`;
  }
  const deps = dependencies(sourceFiles); delete deps[name];
  writeFileSync(resolve(root, directory, "package.json"), JSON.stringify({ name, version: "0.0.0", private: true, type: "module", exports,
    scripts: { typecheck: "tsc --noEmit --project tsconfig.json", lint: "eslint src" }, dependencies: deps }, null, 2) + "\n");
  writeFileSync(resolve(root, directory, "tsconfig.json"), JSON.stringify({ extends: "../../tsconfig.base.json", compilerOptions: { incremental: true, tsBuildInfoFile: ".cache/typecheck.tsbuildinfo" }, include: ["src/**/*.ts", "src/**/*.tsx", "src/**/*.mts"], exclude: ["node_modules"] }, null, 2) + "\n");
  // Workspaces are linked by Bun; root orchestration must not depend on every owner.
}
for (const app of [...appDefinitions, { id: "compat", directory: "apps/compat", package: "@erpx/app-compat" }]) {
  const directory = app.directory, deps = dependencies(files(`${directory}/src`));
  Object.assign(deps, { next: version("next"), react: version("react"), "react-dom": version("react-dom"), "@erpx/contracts": "workspace:*" });
  writeFileSync(resolve(root, directory, "package.json"), JSON.stringify({ name: app.package, version: "0.0.0", private: true, type: "module", scripts: {
    dev: app.id === "compat" ? "bun run --cwd ../.. dev" : `bun run --cwd ../.. dev -- --app=${app.id}`,
    build: `bun ../../scripts/local/app-task.ts build --app=${app.id}`,
    typecheck: app.id === "compat" ? "tsc --noEmit --project tsconfig.json" : `bun ../../scripts/testing/app-typecheck.ts --app=${app.id}`, lint: app.id === "compat" ? "eslint src" : `bun ../../scripts/testing/scoped-lint.ts --app=${app.id}`,
  }, dependencies: deps }, null, 2) + "\n");
  writeFileSync(resolve(root, directory, "next.config.mjs"), 'import { appNextConfig } from "../../scripts/local/app-next-config.ts";\nexport default appNextConfig(' + JSON.stringify(app.id) + ');\n');
  writeFileSync(resolve(root, directory, "postcss.config.mjs"), 'export { default } from "../../postcss.config.mjs";\n');
  writeFileSync(resolve(root, directory, "next-env.d.ts"), '/// <reference types="next" />\n/// <reference types="next/image-types/global" />\n');
  writeFileSync(resolve(root, directory, "tsconfig.json"), JSON.stringify({ extends: "../../tsconfig.base.json", compilerOptions: { incremental: true, plugins: [{ name: "next" }], tsBuildInfoFile: ".cache/typecheck.tsbuildinfo" }, include: ["next-env.d.ts", "src/**/*.ts", "src/**/*.tsx", ".next/types/**/*.ts", ".next/dev/types/**/*.ts"], exclude: ["node_modules"] }, null, 2) + "\n");
  if (!existsSync(resolve(root, directory, "public"))) symlinkSync("../../public", resolve(root, directory, "public"), "dir");
  mkdirSync(resolve(root, directory, "src"), { recursive: true });
  writeFileSync(resolve(root, directory, "src/proxy.ts"), `import { appProxy } from "../../../scripts/local/app-proxy";\nexport const proxy = appProxy(${JSON.stringify(app.id)});\nexport const config = { matcher: ["/:path*"] };\n`);

}
refreshWorkspaceStyles();
writeFileSync(resolve(root, "package.json"), JSON.stringify(rootPackage, null, 2) + "\n");
console.log(`WORKSPACE_MANIFESTS:${allNames.size} packages, ${appDefinitions.length} product/shared apps plus compatibility`);
