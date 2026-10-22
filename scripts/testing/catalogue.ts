import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { internalRequiredCases, loadRequiredCases } from "./required-case-inventory";
import { loadProviderRehearsals } from "./provider-rehearsals";

export const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const testLanes = ["unit", "integration", "e2e", "journeys", "uat", "provider"] as const;
export type TestLane = typeof testLanes[number];
export type FixtureProfile = "none" | "postgres" | "local-seed" | "general-company" | "expense" | "staff-work" | "work-cards" | "messaging" | "services" | "executive-work" | "hospitality" | "booking" | "legal" | "engineering" | "uat" | "provider";
const fixtureProfiles = new Set<FixtureProfile>(["none", "postgres", "local-seed", "general-company", "expense", "staff-work", "work-cards", "messaging", "services", "executive-work", "hospitality", "booking", "legal", "engineering", "uat", "provider"]);
export type Suite = {
  id: string;
  lane: TestLane;
  owner: string;
  modules: string[];
  files: string[];
  verificationInputs?: string[];
  fixture: FixtureProfile;
  requires: { jobs: boolean; scanner: boolean };
  ci?: { critical: boolean; reason: string };
  apps?: string[];
  packages?: string[];
  postgres?: { mode: "template" | "raw"; slots: string[]; migratedSlots: string[] };
  serial?: { reason: string };
  fictionalProvider?: "pms-stripe";
  provider?: { environment: Record<string, string>; required: string[] };
  uatAuth?: { mode: "case-annotations" | "self-authenticated" };
};
export type Catalogue = { version: 1; modules: string[]; suites: Suite[] };
export type Selection = { lane?: TestLane; modules?: string[]; suiteIds?: string[]; files?: string[] };

export function loadCatalogue(root = repositoryRoot): Catalogue {
  return JSON.parse(readFileSync(resolve(root, "tests/catalogue/suite-manifest.json"), "utf8")) as Catalogue;
}

export function selectSuites(catalogue: Catalogue, selection: Selection = {}): Suite[] {
  for (const id of selection.modules ?? []) if (!catalogue.modules.includes(id)) throw new Error(`UNKNOWN_TEST_MODULE:${id}`);
  for (const id of selection.suiteIds ?? []) {
    const suite = catalogue.suites.find(suite => suite.id === id);
    if (!suite) throw new Error(`UNKNOWN_TEST_SUITE:${id}`);
    if (selection.lane && suite.lane !== selection.lane) throw new Error(`TEST_SUITE_LANE_MISMATCH:${id}:${suite.lane}:${selection.lane}`);
  }
  for (const file of selection.files ?? []) {
    const suite = catalogue.suites.find(suite => suite.files.includes(file));
    if (!suite) throw new Error(`UNKNOWN_TEST_FILE:${file}`);
    if (selection.lane && suite.lane !== selection.lane) throw new Error(`TEST_FILE_LANE_MISMATCH:${file}:${suite.lane}:${selection.lane}`);
  }
  const selected = catalogue.suites.filter(suite =>
    (!selection.lane || suite.lane === selection.lane) &&
    (!selection.modules?.length || selection.modules.some(id => suite.modules.includes(id))) &&
    (!selection.suiteIds?.length || selection.suiteIds.includes(suite.id)) &&
    (!selection.files?.length || selection.files.some(file => suite.files.includes(file))));
  if (!selected.length) throw new Error("EMPTY_TEST_SELECTION");
  assertExplicitSelectionIncluded(selection, selected);
  return selected;
}

/** Explicit requests must never disappear through intersections or runner lane filtering. */
export function assertExplicitSelectionIncluded(selection: Selection, suites: Suite[]): void {
  for (const file of selection.files ?? []) if (!suites.some(suite => suite.files.includes(file))) throw new Error(`EXCLUDED_EXPLICIT_TEST_FILE:${file}`);
  for (const id of selection.suiteIds ?? []) if (!suites.some(suite => suite.id === id)) throw new Error(`EXCLUDED_EXPLICIT_TEST_SUITE:${id}`);
}

export function discoverTests(root: string): string[] {
  const visit = (directory: string): string[] => readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? visit(path) : /\.(?:test\.[cm]?[jt]sx?|spec\.[cm]?[jt]sx?)$/.test(entry.name) ? [relative(root, path).replaceAll("\\", "/")] : [];
  });
  return visit(resolve(root, "tests")).sort();
}

/** Imports, re-exports, literal import() and require() participate in impact and isolation checks. */
export function importSpecifiers(source: string, filename = "source.ts"): string[] {
  const found = new Set<string>();
  const tree = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
  const visit = (node: ts.Node) => {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) found.add(node.moduleSpecifier.text);
    if (ts.isCallExpression(node) && (node.expression.kind === ts.SyntaxKind.ImportKeyword || ts.isIdentifier(node.expression) && node.expression.text === "require") && node.arguments.length && ts.isStringLiteralLike(node.arguments[0])) found.add(node.arguments[0].text);
    ts.forEachChild(node, visit);
  };
  visit(tree);
  return [...found];
}

export function resolveImport(from: string, specifier: string, files: ReadonlySet<string>): string | undefined {
  const workspace = /^@erpx\/([^/]+)\/(.+)$/.exec(specifier);
  if (!workspace && !specifier.startsWith(".") && !specifier.startsWith("@/")) return undefined;
  const path = workspace ? `packages/${workspace[1]}/src/${workspace[2]}` : specifier.startsWith("@/") ? `src/${specifier.slice(2)}` : relative("/", resolve("/", dirname(from), specifier)).replaceAll("\\", "/");
  const stem = /\.[cm]?js$/.test(path) ? path.replace(/\.[cm]?js$/, "") : path;
  return [path, ...[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".json"].map(extension => stem + extension), ...[".ts", ".tsx", ".js", ".mjs"].map(extension => `${path}/index${extension}`)].find(candidate => files.has(candidate));
}

export function reachesDatabaseFixture(file: string, root: string, visited = new Set<string>()): boolean {
  if (file === "tests/support/runtime-env.ts" || file === "tests/support/runtime-setup.ts") return true;
  if (visited.has(file) || !existsSync(resolve(root, file))) return false;
  visited.add(file);
  const source = readFileSync(resolve(root, file), "utf8");
  const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  let importsPostgres = false;
  const inspect = (node: ts.Node) => {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier) && node.moduleSpecifier.text === "pg") {
      const clause = node.importClause;
      const typeOnly = clause?.isTypeOnly || (!clause?.name && clause?.namedBindings && ts.isNamedImports(clause.namedBindings) && clause.namedBindings.elements.length > 0 && clause.namedBindings.elements.every(binding => binding.isTypeOnly));
      if (!typeOnly) importsPostgres = true;
    }
    if (ts.isCallExpression(node) && (node.expression.kind === ts.SyntaxKind.ImportKeyword || ts.isIdentifier(node.expression) && node.expression.text === "require") && node.arguments.length && ts.isStringLiteralLike(node.arguments[0]) && node.arguments[0].text === "pg") importsPostgres = true;
    ts.forEachChild(node, inspect);
  };
  inspect(tree);
  if (importsPostgres) return true;
  for (const specifier of importSpecifiers(source, file)) {
    if (!specifier.startsWith(".")) continue;
    const base = relative(root, resolve(dirname(resolve(root, file)), specifier)).replaceAll("\\", "/");
    // Only the fixture tree is relevant: mocked server imports may otherwise lead through arbitrary implementation.
    if (!base.startsWith("tests/") && !base.startsWith("fixtures/")) continue;
    const path = [base, base + ".ts", base + ".tsx", base + "/index.ts"].find(candidate => existsSync(resolve(root, candidate)));
    if (path && reachesDatabaseFixture(path, root, visited)) return true;
  }
  return false;
}

/** This helper reads the Engineering Business snapshot, which ordinary demo seeds do not create. */
export function requiresEngineeringBusinessFixture(source: string, file: string): boolean {
  const filename = resolve("/", file);
  const tree = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
  const options: ts.CompilerOptions = { noLib: true, noResolve: true, types: [] };
  const host: ts.CompilerHost = {
    ...ts.createCompilerHost(options),
    getCurrentDirectory: () => "/",
    getSourceFile: path => path === filename ? tree : undefined,
    fileExists: path => path === filename,
    readFile: path => path === filename ? source : undefined,
  };
  const checker = ts.createProgram([filename], options, host).getTypeChecker();
  const helper = new Set(["tests/support/browser/engineering-business.ts"]);
  function fromHelper(binding: ts.ImportSpecifier | ts.NamespaceImport): boolean {
    let parent: ts.Node | undefined = binding.parent;
    while (parent && !ts.isImportDeclaration(parent)) parent = parent.parent;
    if (!parent || !ts.isStringLiteral(parent.moduleSpecifier) || parent.importClause?.isTypeOnly || ts.isImportSpecifier(binding) && binding.isTypeOnly) return false;
    return resolveImport(file, parent.moduleSpecifier.text, helper) === "tests/support/browser/engineering-business.ts";
  }
  function callsFixture(expression: ts.Expression): boolean {
    while (ts.isParenthesizedExpression(expression) || ts.isAsExpression(expression) || ts.isNonNullExpression(expression)) expression = expression.expression;
    if (ts.isIdentifier(expression)) return checker.getSymbolAtLocation(expression)?.declarations?.some(binding =>
      ts.isImportSpecifier(binding) && (binding.propertyName ?? binding.name).text === "businessFixture" && fromHelper(binding)) ?? false;
    const namespace = ts.isPropertyAccessExpression(expression) && expression.name.text === "businessFixture" ? expression.expression
      : ts.isElementAccessExpression(expression) && ts.isStringLiteralLike(expression.argumentExpression) && expression.argumentExpression.text === "businessFixture" ? expression.expression : undefined;
    return !!namespace && ts.isIdentifier(namespace) && (checker.getSymbolAtLocation(namespace)?.declarations?.some(binding => ts.isNamespaceImport(binding) && fromHelper(binding)) ?? false);
  }
  let required = false;
  function visit(node: ts.Node) {
    if (ts.isCallExpression(node) && callsFixture(node.expression)) required = true;
    ts.forEachChild(node, visit);
  }
  visit(tree);
  return required;
}

export function validateBrowserFixtureContract(suite: Suite, root = repositoryRoot): void {
  for (const file of suite.files) if (requiresEngineeringBusinessFixture(readFileSync(resolve(root, file), "utf8"), file) &&
    suite.fixture !== "engineering" && !(suite.fixture === "uat" && suite.lane === "uat")) {
    throw new Error(`ENGINEERING_BROWSER_FIXTURE_REQUIRED:${suite.id}:${file}`);
  }
}

export function verificationSources(suites: readonly Suite[]) {
  return [...new Set(suites.flatMap(suite => [...suite.files, ...(suite.verificationInputs ?? [])]))];
}

export function validateCatalogue(catalogue: Catalogue, root = repositoryRoot): void {
  if (catalogue.version !== 1 || !Array.isArray(catalogue.modules) || !Array.isArray(catalogue.suites)) throw new Error("INVALID_TEST_MANIFEST");
  const registered = (JSON.parse(readFileSync(resolve(root, "packages/catalogue/src/module-registry.json"), "utf8")) as { id: string }[]).map(module => module.id).sort();
  if (JSON.stringify([...catalogue.modules].sort()) !== JSON.stringify(registered)) throw new Error("TEST_MODULE_REGISTRY_MISMATCH");
  const ids = new Set<string>(), files = new Set<string>();
  for (const suite of catalogue.suites) {
    if (!suite.id || ids.has(suite.id)) throw new Error(`DUPLICATE_TEST_SUITE:${suite.id}`);
    ids.add(suite.id);
    if (!testLanes.includes(suite.lane) || !suite.owner || !Array.isArray(suite.modules) || !suite.modules.length || !Array.isArray(suite.files) || !suite.files.length || !fixtureProfiles.has(suite.fixture) || typeof suite.requires?.jobs !== "boolean" || typeof suite.requires?.scanner !== "boolean") throw new Error(`INVALID_TEST_SUITE:${suite.id}`);
    if (suite.serial !== undefined && (typeof suite.serial?.reason !== "string" || !suite.serial.reason.trim())) throw new Error(`SERIAL_SUITE_REASON_REQUIRED:${suite.id}`);
    if (suite.verificationInputs !== undefined && (!Array.isArray(suite.verificationInputs) || new Set(suite.verificationInputs).size !== suite.verificationInputs.length || suite.verificationInputs.some(file =>
      typeof file !== "string" || isAbsolute(file) || file.includes("\\") || file.split("/").some(part => !part || part === "." || part === ".." || ["node_modules", ".next", ".build", ".cache", ".turbo", ".scratch", ".runtime", ".git"].includes(part)) || !existsSync(resolve(root,file))))) throw new Error(`INVALID_SUITE_VERIFICATION_INPUT:${suite.id}`);
    if (!suite.modules.every(module => catalogue.modules.includes(module))) throw new Error(`UNKNOWN_SUITE_MODULE:${suite.id}`);
    if (suite.lane === "unit" || suite.lane === "integration") {
      if (suite.ci && (typeof suite.ci.critical !== "boolean" || typeof suite.ci.reason !== "string" || !suite.ci.reason.trim())) throw new Error(`NATIVE_CI_CLASSIFICATION_REQUIRED:${suite.id}`);
    } else if (suite.ci !== undefined) throw new Error(`NON_NATIVE_CI_CLASSIFICATION_FORBIDDEN:${suite.id}`);
    if (suite.fictionalProvider !== undefined && (suite.fictionalProvider !== 'pms-stripe' || suite.fixture !== 'booking' || suite.lane !== 'e2e' || !suite.requires.jobs || !suite.requires.scanner)) throw new Error(`INVALID_FICTIONAL_PROVIDER_SUITE:${suite.id}`);
    for (const file of suite.files) {
      if (isAbsolute(file) || file.split("/").includes("..") || files.has(file) || !existsSync(resolve(root, file))) throw new Error(`INVALID_OR_DUPLICATE_TEST_FILE:${file}`);
      files.add(file);
      if (["e2e", "journeys", "uat"].includes(suite.lane) !== /\.spec\.[cm]?[jt]sx?$/.test(file)) throw new Error(`TEST_FRAMEWORK_LANE_MISMATCH:${file}`);
      if (suite.lane === "unit" && reachesDatabaseFixture(file, root)) throw new Error(`UNIT_IMPORTS_POSTGRES_FIXTURE:${file}`);
    }
    if (suite.lane === "provider" && !suite.provider) throw new Error(`PROVIDER_PREREQUISITES_REQUIRED:${suite.id}`);
    if (suite.lane === "uat" && !["case-annotations", "self-authenticated"].includes(suite.uatAuth?.mode ?? "")) throw new Error(`UAT_AUTH_MAPPING_REQUIRED:${suite.id}`);
    if (["e2e", "journeys", "uat"].includes(suite.lane)) validateBrowserFixtureContract(suite, root);
  }
  const discovered = discoverTests(root);
  if (discovered.some(file => !files.has(file)) || [...files].some(file => !discovered.includes(file))) throw new Error(`TEST_INVENTORY_MISMATCH:${discovered.filter(file => !files.has(file)).join(",")}`);
  for (const moduleId of registered) if (!catalogue.suites.some(suite => suite.modules.includes(moduleId))) throw new Error(`MODULE_WITHOUT_TESTS:${moduleId}`);
  for (const test of loadRequiredCases(root)) {
    const suite = catalogue.suites.find(suite => suite.files.includes(test.file));
    if (!suite || (["e2e", "journeys", "uat"].includes(suite.lane) !== (test.runner === "playwright"))) throw new Error(`REGISTERED_REQUIRED_CASE_WITHOUT_SUITE:${test.file}`);
  }
  internalRequiredCases(loadRequiredCases(root), catalogue.suites);
  loadProviderRehearsals(root, catalogue.modules);
}

export function listSelection(suites: Suite[]): string {
  return suites.map(suite => `${suite.id}\t${suite.lane}\t${suite.modules.join(",")}\t${suite.files.join(",")}`).join("\n");
}
