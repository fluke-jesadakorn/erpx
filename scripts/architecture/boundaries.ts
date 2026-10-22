import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import {builtinModules} from "node:module";
import relocationInventory from "./workspace-relocations.json";
import { ownersForSource } from "@erpx/contracts/ownership";

export type BoundaryIssue = {
  id: string;
  source: string;
  target: string;
  rule: string;
  line: number;
  grandfatherable: boolean;
};

const relocatedSources = new Map(relocationInventory.moves.map(move=>[move.from,move.to]));
const extensions = ["", ".ts", ".tsx", ".mts", ".json", "/index.ts", "/index.tsx"];
const forbiddenCorePackages = /^(?:next(?:\/|$)|react(?:\/|$)|server-only$|node:|pg$|drizzle-orm(?:\/|$)|@aws-sdk\/)/;
const nodePackages=new Set(builtinModules.map(name=>name.replace(/^node:/,"")));
const forbiddenCoreNames = new Set(["SqlDatabase", "TransactionalSqlDatabase", "SqlStatement", "PostgresDatabase", "FileStore", "RuntimeEnv", "holdingResources", "runtimeEnv", "controlResources", "iamDb"]);
// UPDATE requires SET; imperative workflow guidance without SET is not SQL.
// Template heads are checked against the complete template below so interpolated targets remain guarded.
const sql = /^\s*(?:SELECT\s+(?:[\s\S]*\bFROM\b|(?:DISTINCT\s+)?(?:\d+|true|false|null|\?)(?:\s*[,;()]|\s*$)|[\w]+\s*\()|INSERT\s+INTO\s|UPDATE\s+(?!FROM(?:\s|$))[\s\S]*\bSET\b|DELETE\s+FROM\s|WITH\s+\w+\s+AS\s*\(|CREATE\s+TABLE\s|ALTER\s+TABLE\s|DROP\s+TABLE\s)/i;
const publicEntry = /^(?:src\/server\/modules\/[^/]+\/|packages\/server-[^/]+\/src\/)(?:(?:index|contracts|expense)\.ts|public\/.*\.ts)$/;
// Presentation calls semantic owner APIs; inbound HTTP/worker adapters can compose typed resources.
const runtimeGateway = /^(?:src\/server\/(?:runtime(?:\/env)?|modules\/runtime\/(?:public\/runtime(?:\/env)?|adapters\/runtime(?:\/env)?))|packages\/server-runtime\/src\/(?:public|adapters)\/runtime(?:\/env)?)\.ts$/;
const resourceAcquisition = new Set(["holdingResources", "holdingMigrationDatabase", "runtimeEnv", "getRuntime", "controlResources", "controlDatabase", "iamDb", "iamContext", "bookingDatabase", "registeredResources", "registeredMigrationDatabase", "staffPage", "portalContext", "customerPortalContext", "PostgresDatabase"]);
const resourceBinding = /^(?:CONTROL|H\d{3})(?:_READ)?_(?:DB|FILES)$/;
const presentationSqlExports: Record<string, ReadonlySet<string>> = {
  "packages/server-pos/src/public/pos-core.ts": new Set(["one", "all", "run"]),
  "packages/server-work_projection/src/public/calendar/access.ts": new Set(["rows", "one", "run"]),
  "packages/server-work_projection/src/public/list-page.ts": new Set(["readListPage"]),
  "packages/server-workflow/src/public/commands.ts": new Set(["assertion"]),
};

/** Imperative UI sentences can contain FROM without being a SQL statement. */
function isUiInstruction(node: ts.Node): boolean {
  return ts.isStringLiteral(node) && /^(?:Select (?:(?:a|an|the) [\p{L}\p{N} ,–-]+|functional-currency [\p{L}\p{N} ,–-]+ accounts from this [\p{L}\p{N} ,–-]+|original posted document accounts from two active Group Companies in the same transaction currency)\.(?: Secondary books use the same business statement\.)?|Update [\p{L}\p{N} ,–-]+ before [\p{L}\p{N} ,–-]+\.)$/u.test(node.text);
}
function isStructuredErrorInstruction(node: ts.Node): boolean {
  if (!isUiInstruction(node)) return false;
  const isStatus = (value: ts.Expression | undefined) => !!value && ts.isNumericLiteral(value) && Number(value.text) >= 400 && Number(value.text) <= 599;
  const isCode = (value: ts.Expression | undefined) => !!value && ts.isStringLiteral(value) && /^[A-Z][A-Z0-9_]+$/.test(value.text);
  if ((ts.isCallExpression(node.parent) || (ts.isNewExpression(node.parent) && /^(?:BusinessRuleError|ApiError)$/.test(node.parent.expression.getText()))) && node.parent.arguments?.[2] === node) return isStatus(node.parent.arguments[0]) && isCode(node.parent.arguments[1]);
  if (!ts.isPropertyAssignment(node.parent) || node.parent.name.getText() !== "message" || !ts.isObjectLiteralExpression(node.parent.parent)) return false;
  const properties = node.parent.parent.properties;
  const property = (name: string) => properties.find((entry): entry is ts.PropertyAssignment => ts.isPropertyAssignment(entry) && entry.name.getText() === name)?.initializer;
  return ts.isCallExpression(node.parent.parent.parent) && isStatus(property("status")) && isCode(property("code"));
}

export function resolveSource(root: string, source: string, specifier: string): string | null {
  const workspace = /^@erpx\/([^/]+)\/(.+)$/.exec(specifier);
  const candidate = workspace ? path.join(root,"packages",workspace[1],"src",workspace[2]) : specifier.startsWith("@/")
    ? path.join(root, "src", specifier.slice(2))
    : specifier.startsWith(".")
      ? path.resolve(root, path.dirname(source), specifier)
      : null;
  if (!candidate) return null;
  for (const extension of extensions) {
    const original = path.relative(root,`${candidate}${extension}`).replaceAll("\\","/");
    const moved = relocatedSources.get(original);
    const file = moved ? path.join(root,moved) : `${candidate}${extension}`;
    if (fs.existsSync(file) && fs.statSync(file).isFile()) return path.relative(root, file).replaceAll("\\", "/");
  }
  // Missing local imports are still checked; deleting a private file cannot bypass a boundary.
  const unresolved = path.relative(root, candidate).replaceAll("\\", "/").replace(/\.(tsx?|mts)$/, "");
  return unresolved + (/^src\/server\/modules\/[^/]+$/.test(unresolved) ? "/index.ts" : ".ts");
}

function moduleLayer(source: string): { owner: string; layer: string } | null {
  const match = /^(?:src\/server\/modules\/|packages\/server-)([^/]+)\/(?:src\/)?(domain|application|adapters)\//.exec(source);
  return match ? { owner: match[1], layer: match[2] } : null;
}

function importNames(node: ts.ImportDeclaration | ts.ExportDeclaration): string {
  if (ts.isExportDeclaration(node)) return node.exportClause?.getText().replaceAll(/\s/g, "") ?? "*";
  return node.importClause?.getText().replaceAll(/\s/g, "") ?? "side-effect";
}

/** Match global effect syntax, including spacing and literal bracket access, without matching injected ports. */
function globalEffectName(expression: ts.Expression): string | null {
  const memberPath = (node: ts.Expression): string | null => {
    if (ts.isParenthesizedExpression(node) || ts.isNonNullExpression(node)) return memberPath(node.expression);
    if (ts.isIdentifier(node)) return node.text;
    if (ts.isPropertyAccessExpression(node)) {
      const parent = memberPath(node.expression);
      return parent ? `${parent}.${node.name.text}` : null;
    }
    if (ts.isElementAccessExpression(node) && ts.isStringLiteral(node.argumentExpression)) {
      const parent = memberPath(node.expression);
      return parent ? `${parent}.${node.argumentExpression.text}` : null;
    }
    return null;
  };
  const name = memberPath(expression)?.replace(/^globalThis\./, "");
  return name && /^(?:Date(?:\.now)?|Math\.random|crypto\.randomUUID|fetch)$/.test(name) ? name : null;
}

/** Cook only literal SQL pieces; real template expressions are evaluated by the caller. */
function cookedSqlLiteral(expression: ts.Expression): string | null {
  if (ts.isStringLiteral(expression) || ts.isNoSubstitutionTemplateLiteral(expression)) return expression.text;
  if (ts.isTemplateExpression(expression)) return expression.head.text + expression.templateSpans.map(span => "${value}" + span.literal.text).join("");
  if (ts.isParenthesizedExpression(expression)) return cookedSqlLiteral(expression.expression);
  if (ts.isBinaryExpression(expression) && expression.operatorToken.kind === ts.SyntaxKind.PlusToken) {
    const left = cookedSqlLiteral(expression.left), right = cookedSqlLiteral(expression.right);
    return left !== null || right !== null ? `${left ?? "${value}"}${right ?? "${value}"}` : null;
  }
  return null;
}

/** Resolve immutable local constants without confusing nested parameters or shadowed names. */
function constantResolver(parsed: ts.SourceFile) {
  type Binding = { value: ts.Expression | null; declaration: ts.Node };
  const scopes = new WeakMap<ts.Node, Map<string, Binding>>();
  const isScope = (node: ts.Node) => ts.isSourceFile(node) || ts.isBlock(node) || ts.isFunctionLike(node) || ts.isCatchClause(node) || ts.isForStatement(node) || ts.isForOfStatement(node) || ts.isForInStatement(node);
  const bind = (scope: Map<string, Binding>, name: ts.BindingName, value: ts.Expression | null, declaration: ts.Node) => {
    if (ts.isIdentifier(name)) scope.set(name.text, { value, declaration });
    else for (const element of name.elements) if (ts.isBindingElement(element)) bind(scope, element.name, null, element);
  };
  const visit = (node: ts.Node, scope: Map<string, Binding>) => {
    if ((ts.isFunctionDeclaration(node) || ts.isClassDeclaration(node)) && node.name) bind(scope, node.name, null, node);
    if (isScope(node)) { scope = new Map(); scopes.set(node, scope); }
    if (ts.isFunctionExpression(node) && node.name) bind(scope, node.name, null, node);
    if (ts.isParameter(node)) bind(scope, node.name, null, node);
    if (ts.isVariableDeclaration(node)) {
      let declarationScope = scope;
      if (ts.isVariableDeclarationList(node.parent) && !(node.parent.flags & ts.NodeFlags.BlockScoped)) for (let parent: ts.Node | undefined = node.parent; parent; parent = parent.parent) {
        if (ts.isFunctionLike(parent) || ts.isSourceFile(parent)) { declarationScope = scopes.get(parent) ?? scope; break; }
      }
      bind(declarationScope, node.name, ts.isVariableDeclarationList(node.parent) && !!(node.parent.flags & ts.NodeFlags.Const) ? node.initializer ?? null : null, node);
    }
    if (ts.isImportDeclaration(node) && node.importClause) {
      if (node.importClause.name) scope.set(node.importClause.name.text, { value: null, declaration: node.importClause });
      const bindings = node.importClause.namedBindings;
      if (bindings && ts.isNamespaceImport(bindings)) scope.set(bindings.name.text, { value: null, declaration: bindings });
      else if (bindings) for (const binding of bindings.elements) scope.set(binding.name.text, { value: null, declaration: binding });
    }
    if (ts.isImportEqualsDeclaration(node)) scope.set(node.name.text, { value: null, declaration: node });
    ts.forEachChild(node, child => visit(child, scope));
  };
  visit(parsed, new Map());
  const binding = (identifier: ts.Identifier): Binding | undefined => {
    for (let node: ts.Node | undefined = identifier.parent; node; node = node.parent) {
      const scope = scopes.get(node);
      if (scope?.has(identifier.text)) return scope.get(identifier.text);
    }
    return undefined;
  };
  return Object.assign((identifier: ts.Identifier): ts.Expression | null => binding(identifier)?.value ?? null, { binding });
}

function transparentExpression(expression: ts.Expression): ts.Expression {
  return ts.isParenthesizedExpression(expression) || ts.isAsExpression(expression) || ts.isTypeAssertionExpression(expression) || ts.isNonNullExpression(expression) || ts.isSatisfiesExpression(expression)
    ? transparentExpression(expression.expression) : expression;
}

function resolvedSqlLiteral(expression: ts.Expression, resolve: ReturnType<typeof constantResolver>, seen = new Set<ts.Expression>()): string | null {
  expression = transparentExpression(expression);
  if (seen.has(expression)) return null;
  seen.add(expression);
  if (ts.isIdentifier(expression)) { const value = resolve(expression); return value ? resolvedSqlLiteral(value, resolve, seen) : null; }
  if (ts.isTemplateExpression(expression)) return expression.head.text + expression.templateSpans.map(span => (resolvedSqlLiteral(span.expression, resolve, new Set(seen)) ?? "${value}") + span.literal.text).join("");
  if (ts.isBinaryExpression(expression) && expression.operatorToken.kind === ts.SyntaxKind.PlusToken) {
    const left = resolvedSqlLiteral(expression.left, resolve, new Set(seen)), right = resolvedSqlLiteral(expression.right, resolve, new Set(seen));
    return left !== null || right !== null ? `${left ?? "${value}"}${right ?? "${value}"}` : null;
  }
  return cookedSqlLiteral(expression);
}

/** SQL belongs to owner adapters even when a callable hides its SQL argument position. */
function sqlArgument(expression: ts.Expression, resolve: ReturnType<typeof constantResolver>, seen = new Set<ts.Expression>()): boolean {
  expression = transparentExpression(expression);
  if (seen.has(expression)) return false;
  seen.add(expression);
  if (ts.isIdentifier(expression)) {
    const value = resolve(expression);
    return !!value && sqlArgument(value, resolve, seen);
  }
  if (isUiInstruction(expression)) return false;
  const value = resolvedSqlLiteral(expression, resolve);
  if (value !== null) return sql.test(value);
  if (ts.isConditionalExpression(expression)) return sqlArgument(expression.whenTrue, resolve, new Set(seen)) || sqlArgument(expression.whenFalse, resolve, new Set(seen));
  if (ts.isObjectLiteralExpression(expression)) return expression.properties.some(property => ts.isPropertyAssignment(property) ? sqlArgument(property.initializer, resolve, new Set(seen)) : ts.isShorthandPropertyAssignment(property) ? sqlArgument(property.name, resolve, new Set(seen)) : ts.isSpreadAssignment(property) && sqlArgument(property.expression, resolve, new Set(seen)));
  if (ts.isArrayLiteralExpression(expression)) return expression.elements.some(element => sqlArgument(ts.isSpreadElement(element) ? element.expression : element, resolve, new Set(seen)));
  return false;
}

export function inspectSource(root: string, source: string, text: string, coreContext?: {owner:string;layer:string}): BoundaryIssue[] {
  const parsed = ts.createSourceFile(source, text, ts.ScriptTarget.Latest, true, source.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const issues: BoundaryIssue[] = [];
  const layer = coreContext??moduleLayer(source);
  const core = layer?.layer === "domain" || layer?.layer === "application";
  const component = source.startsWith("src/components/") || /^packages\/[^/]+\/src\/components\//.test(source);
  const presentation = component || (/^(?:src\/app\/|apps\/[^/]+\/src\/app\/|packages\/[^/]+\/src\/routes\/).*\.tsx?$/.test(source) && !/\/route\.tsx?$/.test(source));
  const entrypoint = component || /^(?:src\/(?:app|workers)\/|apps\/[^/]+\/src\/app\/|packages\/[^/]+\/src\/routes\/)/.test(source);
  const resolveConstant = constantResolver(parsed);
  const presentationRule = component ? "component-runtime-composition" : "presentation-runtime-composition";
  const namespaceTargets = new Map<string, string>();
  const owners = coreContext?ownersForSource(`packages/server-${coreContext.owner}/src/domain/__context.ts`):ownersForSource(source);
  const add = (node: ts.Node, rule: string, target: string, grandfatherable = false) => {
    const line = parsed.getLineAndCharacterOfPosition(node.getStart(parsed)).line + 1;
    const symbol = ts.isImportDeclaration(node) || ts.isExportDeclaration(node) ? importNames(node) : node.getText(parsed).replaceAll(/\s/g, "");
    const id = `${source} :: ${rule} :: ${target} :: ${symbol}`;
    if (!issues.some((issue) => issue.id === id)) issues.push({ id, source, rule, target, line, grandfatherable });
  };
  const resourceIssue = (node: ts.Node, target: string) => {
    if (!issues.some(issue => issue.rule === presentationRule)) add(node, presentationRule, target);
  };
  const constantString = (expression: ts.Expression, seen = new Set<ts.Expression>()): string | null => {
    expression = transparentExpression(expression);
    if (seen.has(expression)) return null;
    seen.add(expression);
    if (ts.isStringLiteralLike(expression)) return expression.text;
    const value = ts.isIdentifier(expression) ? resolveConstant(expression) : null;
    return value ? constantString(value, seen) : null;
  };
  const memberName = (expression: ts.Expression): string | null => {
    if (ts.isPropertyAccessExpression(expression)) return expression.name.text;
    if (!ts.isElementAccessExpression(expression) || !expression.argumentExpression) return null;
    return constantString(expression.argumentExpression);
  };
  const namespaceTarget = (expression: ts.Expression, seen = new Set<ts.Expression>()): string | null => {
    expression = transparentExpression(expression);
    if (seen.has(expression)) return null;
    seen.add(expression);
    if (ts.isAwaitExpression(expression)) return namespaceTarget(expression.expression, seen);
    if (ts.isIdentifier(expression)) {
      const declaration = resolveConstant.binding(expression)?.declaration;
      const target = declaration && (ts.isNamespaceImport(declaration) || ts.isImportEqualsDeclaration(declaration)) ? namespaceTargets.get(expression.text) : null;
      const value = resolveConstant(expression);
      return target ?? (value ? namespaceTarget(value, seen) : null);
    }
    if (ts.isCallExpression(expression) && expression.arguments[0] && (expression.expression.kind === ts.SyntaxKind.ImportKeyword || (ts.isIdentifier(expression.expression) && expression.expression.text === "require"))) { const specifier = constantString(expression.arguments[0]); return specifier ? resolveSource(root, source, specifier) : null; }
    return null;
  };
  const isResourceCallable = (expression: ts.Expression, seen = new Set<ts.Expression>()): boolean => {
    expression = transparentExpression(expression);
    if (seen.has(expression)) return false;
    seen.add(expression);
    if (ts.isIdentifier(expression)) {
      if (resourceAcquisition.has(expression.text)) return true;
      const value = resolveConstant(expression);
      return !!value && isResourceCallable(value, seen);
    }
    const member = memberName(expression);
    const namespace = ts.isPropertyAccessExpression(expression) || ts.isElementAccessExpression(expression) ? namespaceTarget(expression.expression) : null;
    return !!member && (resourceAcquisition.has(member) || !!(namespace && presentationSqlExports[namespace]?.has(member)));
  };
  const checkDependency = (node: ts.Node, specifier: string, typeOnly = false) => {
    if (path.isAbsolute(specifier) || /^(?:[A-Za-z]:[\\/]|\\\\|file:)/.test(specifier)) {
      add(node, "nonportable-source-import", specifier);
      return;
    }
    const target = resolveSource(root, source, specifier);
    if (core && (forbiddenCorePackages.test(specifier)||nodePackages.has(specifier))) add(node, "core-infrastructure", specifier);
    if (presentation && !typeOnly && /^(?:pg|drizzle-orm)(?:\/|$)/.test(specifier)) resourceIssue(node, specifier);
    if (target) {
      const bindings = ts.isImportDeclaration(node) ? node.importClause?.namedBindings : undefined;
      const importsResource = bindings && ts.isNamedImports(bindings) && bindings.elements.some(binding => !binding.isTypeOnly && (resourceAcquisition.has((binding.propertyName ?? binding.name).text) || presentationSqlExports[target]?.has((binding.propertyName ?? binding.name).text)));
      const namespace = bindings && ts.isNamespaceImport(bindings);
      if (namespace) namespaceTargets.set(bindings.name.text, target);
      if (presentation && !typeOnly && (importsResource || (runtimeGateway.test(target) && (!ts.isImportDeclaration(node) || namespace || !node.importClause || !!node.importClause.name)))) resourceIssue(node, target);
      const targetLayer = moduleLayer(target);
      if(publicEntry.test(target)&&!fs.existsSync(path.join(root,target)))add(node,"public-entry-missing",target);
      if (core && target.startsWith("src/server/") && !targetLayer && !publicEntry.test(target)) add(node, "core-legacy-service", target);
      if (core && targetLayer && (targetLayer.layer === "adapters" || (layer?.layer === "domain" && targetLayer.layer === "application"))) add(node, "core-dependency-direction", target);
      if (core && publicEntry.test(target) && !typeOnly && !target.includes("/shared/") && !target.startsWith("packages/server-shared/") && !target.includes("/public/rules/")) add(node, "core-runtime-facade", target);
      const targetOwners = ownersForSource(target);
      const sameOwner = owners.some((owner) => targetOwners.some((other) => owner.id === other.id));
      const sharedCore = targetLayer?.owner === "shared" && targetLayer.layer !== "adapters";
      if (targetOwners.length && !sameOwner && !sharedCore && !publicEntry.test(target)) add(node, "owner-private-import", target, !core && !moduleLayer(source));
      if (targetLayer && !sameOwner && !sharedCore) add(node, "module-private-import", target, !core && !moduleLayer(source));
    }
  };
  const visit = (node: ts.Node) => {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
      const bindings=ts.isImportDeclaration(node)?node.importClause?.namedBindings:node.exportClause;
      const namedTypes=bindings&&(ts.isNamedImports(bindings)||ts.isNamedExports(bindings))&&bindings.elements.length>0&&bindings.elements.every(entry=>entry.isTypeOnly);
      const typeOnly = ts.isImportDeclaration(node) ? !!node.importClause?.isTypeOnly||!!namedTypes&&!node.importClause?.name : node.isTypeOnly||!!namedTypes;
      checkDependency(node, node.moduleSpecifier.text, typeOnly);
      if (/^packages\/server-|^src\/server\/modules\//.test(source) && ts.isExportDeclaration(node) && !node.exportClause) add(node, "public-wildcard-export", node.moduleSpecifier.text);
    }
    if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference) && node.moduleReference.expression && ts.isStringLiteral(node.moduleReference.expression)) {
      const specifier = node.moduleReference.expression.text, target = resolveSource(root, source, specifier);
      if (target) namespaceTargets.set(node.name.text, target);
      checkDependency(node, specifier, node.isTypeOnly);
    }
    if (ts.isCallExpression(node) && node.arguments[0] && (node.expression.kind === ts.SyntaxKind.ImportKeyword || (ts.isIdentifier(node.expression) && node.expression.text === "require"))) { const specifier = constantString(node.arguments[0]); if (specifier) checkDependency(node, specifier); }
    if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument) && ts.isStringLiteral(node.argument.literal)) checkDependency(node, node.argument.literal.text, true);
    if (presentation) {
      if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
        if (isResourceCallable(node.expression)) resourceIssue(node, node.expression.getText(parsed));
      }
      if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) { const member = memberName(node); if (member && resourceBinding.test(member)) resourceIssue(node, node.getText(parsed)); }
      if (ts.isBindingElement(node)) {
        const name = node.propertyName ? ts.isStringLiteralLike(node.propertyName) ? node.propertyName.text : node.propertyName.getText(parsed) : ts.isIdentifier(node.name) ? node.name.text : null;
        const declaration = node.parent.parent;
        const target = ts.isVariableDeclaration(declaration) && declaration.initializer ? namespaceTarget(declaration.initializer) : null;
        if (name && (resourceBinding.test(name) || resourceAcquisition.has(name) || (target && presentationSqlExports[target]?.has(name)))) resourceIssue(node, name);
      }
    }
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && /^(?:prepare|query)$/.test(node.expression.name.text) && node.arguments[0]) {
      const query = cookedSqlLiteral(node.arguments[0]);
      if (query !== null && sql.test(query)) {
        const placeholder = /\$\{\s*[A-Za-z_$][\w$]*ProjectionSources\s*\.\s*[A-Za-z_$][\w$]*\s*\}/.exec(query)?.[0];
        if (placeholder) add(node, "sql-unexpanded-projection", placeholder);
      }
    }
    if (entrypoint && (ts.isCallExpression(node) || ts.isNewExpression(node)) && node.arguments?.some(argument => sqlArgument(argument, resolveConstant))) add(node, "entrypoint-sql", "SQL");
    if (entrypoint && ts.isTaggedTemplateExpression(node) && sqlArgument(node.template, resolveConstant)) add(node, "entrypoint-sql", "SQL");
    if (core) {
      if (ts.isIdentifier(node) && forbiddenCoreNames.has(node.text)) add(node, "core-persistence-handle", node.text);
      if ((ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isTemplateHead(node)) && sql.test(ts.isTemplateHead(node) && ts.isTemplateExpression(node.parent) ? node.parent.getText(parsed).slice(1,-1) : node.text) && !isStructuredErrorInstruction(node)) add(node, "core-sql", "SQL");
      if (ts.isPropertyAccessExpression(node) && node.getText(parsed) === "process.env") add(node, "core-global-environment", "process.env");
      if (ts.isCallExpression(node)) {
        const effect = globalEffectName(node.expression);
        if (effect && effect !== "Date") add(node, "core-global-effect", effect);
      }
      if (ts.isNewExpression(node) && !node.arguments?.length && globalEffectName(node.expression) === "Date") add(node, "core-global-effect", "new Date()");
    }
    ts.forEachChild(node, visit);
  };
  visit(parsed);
  return issues;
}

export function architectureSources(root: string): string[] {
  const results: string[] = [];
  const walk = (directory: string) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory() && !["node_modules",".next",".build",".cache",".turbo"].includes(entry.name)) walk(file);
      else if (/\.(?:ts|tsx|mts)$/.test(entry.name)) results.push(path.relative(root, file).replaceAll("\\", "/"));
    }
  };
  for (const folder of ["src/server", "src/app", "src/workers", "src/components", "apps", "packages"]) if (fs.existsSync(path.join(root, folder))) walk(path.join(root, folder));
  return results.sort();
}

export function inspectArchitecture(root: string): BoundaryIssue[] {
  const sources=architectureSources(root),issues=sources.flatMap(source=>inspectSource(root,source,fs.readFileSync(path.join(root,source),"utf8")));
  // A portable library cannot conceal infrastructure from a domain/application.
  // Type-only imports are erased and never pull their implementation into a core.
  for(const source of sources){const context=moduleLayer(source);if(!context||!["domain","application"].includes(context.layer))continue;
    const seen=new Set<string>();
    const visit=(current:string,traversalContext=context)=>{
      const ruleOwner=/^(?:src\/server\/modules\/|packages\/server-)([^/]+)\/(?:src\/)?public\/rules\//.exec(current);
      if(ruleOwner)traversalContext={owner:ruleOwner[1],layer:"domain"};
      if(seen.has(current)||!fs.existsSync(path.join(root,current)))return;seen.add(current);
      const text=fs.readFileSync(path.join(root,current),"utf8"),parsed=ts.createSourceFile(current,text,ts.ScriptTarget.Latest,true);
      if(current!==source&&(!moduleLayer(current)||current.includes("/public/rules/")))for(const issue of inspectSource(root,current,text,traversalContext))issues.push({...issue,id:`${source} :: core-transitive-dependency :: ${issue.id}`,source,rule:"core-transitive-dependency",target:`${current}:${issue.line} ${issue.rule}: ${issue.target}`,grandfatherable:false});
      const dependency=(specifier:string)=>{const target=resolveSource(root,current,specifier);if(target&&/\.(ts|tsx|mts)$/.test(target)&&((!moduleLayer(target)&&!publicEntry.test(target))||target.includes("/public/rules/")))visit(target,traversalContext);};
      const walk=(node:ts.Node)=>{
        if(ts.isImportDeclaration(node)&&node.moduleSpecifier&&ts.isStringLiteral(node.moduleSpecifier)&&!node.importClause?.isTypeOnly){const bindings=node.importClause?.namedBindings;if(!(bindings&&ts.isNamedImports(bindings)&&bindings.elements.every(entry=>entry.isTypeOnly)))dependency(node.moduleSpecifier.text);}
        if(ts.isExportDeclaration(node)&&node.moduleSpecifier&&ts.isStringLiteral(node.moduleSpecifier)&&!node.isTypeOnly){const clause=node.exportClause;if(!(clause&&ts.isNamedExports(clause)&&clause.elements.every(entry=>entry.isTypeOnly)))dependency(node.moduleSpecifier.text);}
        if(ts.isCallExpression(node)&&node.arguments[0]&&ts.isStringLiteral(node.arguments[0])&&(node.expression.kind===ts.SyntaxKind.ImportKeyword||(ts.isIdentifier(node.expression)&&node.expression.text==="require")))dependency(node.arguments[0].text);
        ts.forEachChild(node,walk);
      };walk(parsed);
    };visit(source);
  }
  return issues;
}

export function evaluateBaseline(issues: BoundaryIssue[], legacyIds: string[]): { violations: BoundaryIssue[]; stale: string[] } {
  const allowlist = new Set(legacyIds);
  const current = new Set(issues.map((issue) => issue.id));
  return {
    violations: issues.filter((issue) => !issue.grandfatherable || !allowlist.has(issue.id)),
    stale: legacyIds.filter((id) => !current.has(id)),
  };
}
