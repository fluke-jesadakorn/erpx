#!/usr/bin/env bun
// Independent AST-based reachability cross-check for the regex scanner
// (scripts/testing/ts-reachability.mjs).
//
// Edge extraction is DIFFERENT by design: the TypeScript compiler API
// (ts.createSourceFile + AST walk) instead of a regex scan. This guards
// against scanner-regex blind spots (e.g. compact "export {x}from './y'"
// re-exports that a mandatory-space regex silently drops).
//
// Known extractor caveat: a dynamic import() call is a CallExpression whose
// `expression` is an ImportKeyword TOKEN, not an Identifier — ts.isIdentifier
// returns false for it, so the handler must test SyntaxKind.ImportKeyword
// explicitly (bug #4: the first version silently dropped every
// string-literal dynamic import; those files showed up as "phantom
// reachable" in the regex scanner, which does see `import( 'x'` forms).
//
// Roots and module resolution are intentionally shared with the regex
// scanner (the entry-point and exports-map semantics are audited once,
// not twice).
//
// Usage: bun scripts/testing/ts-reachability-ast.mjs
//   prints: TS-universe, TS-reachable count, TS dead set (one path per
//   line), plus any unresolvable dynamic-import specifiers (template
//   literals with substitutions) as an explicit blind-spot report.
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

const root = process.cwd();

// --- package registry (shared with ts-reachability.mjs) ---
const pkgDirs = [];
for (const d of fs.readdirSync(path.join(root, 'packages'))) {
  const pj = path.join(root, 'packages', d, 'package.json');
  if (fs.existsSync(pj)) pkgDirs.push(['packages/' + d, JSON.parse(fs.readFileSync(pj, 'utf8'))]);
}
for (const d of fs.readdirSync(path.join(root, 'apps'))) {
  const pj = path.join(root, 'apps', d, 'package.json');
  if (fs.existsSync(pj)) pkgDirs.push(['apps/' + d, JSON.parse(fs.readFileSync(pj, 'utf8'))]);
}
const byName = new Map();
for (const [dir, pj] of pkgDirs) byName.set(pj.name, { dir, exports: pj.exports ?? {} });

const exts = ['.ts', '.tsx', '.mts', '.js', '.mjs'];
function resolveFile(base) {
  for (const e of exts) if (fs.existsSync(base + e)) return base + e;
  for (const e of exts) if (fs.existsSync(path.join(base, 'index' + e))) return path.join(base, 'index' + e);
  if (fs.existsSync(base) && fs.statSync(base).isFile()) return base;
  return null;
}
function resolveExportTarget(value) {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') {
    for (const k of ['default', 'require', 'import', 'types', 'node', 'bun']) {
      if (k in value) return resolveExportTarget(value[k]);
    }
    const first = Object.values(value)[0];
    return resolveExportTarget(first);
  }
  return null;
}
function resolveSpec(spec, fromFile) {
  let out = null;
  if (spec.startsWith('.')) out = resolveFile(path.resolve(path.dirname(fromFile), spec));
  else {
    const parts = spec.split('/');
    const name = spec.startsWith('@erpx/') ? parts.slice(0, 2).join('/') : parts[0];
    const rest = spec.slice(name.length + 1);
    const pkg = byName.get(name);
    if (pkg) {
      const key = rest ? './' + rest : '.';
      const target = resolveExportTarget(pkg.exports[key] ?? (typeof pkg.exports === 'string' ? pkg.exports : null));
      if (target) out = resolveFile(path.join(root, pkg.dir, target.replace(/^\.\//, '')));
    }
  }
  if (out && (out.endsWith('.css') || out.endsWith('.rs') || out.endsWith('.json'))) return null;
  return out;
}

// --- roots (shared with ts-reachability.mjs) ---
const entries = [];
function walk(dir, match, acc) {
  if (!fs.existsSync(dir)) return;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (['node_modules', '.next', 'target', '.turbo'].includes(e.name)) continue;
      walk(p, match, acc);
    } else if (/\.(ts|tsx|mts|js|mjs)$/.test(e.name) && (!match || match.test(e.name))) acc.push(p);
  }
}
const routeFile = /^(route|page|layout|default)\.(ts|tsx)$/;
for (const d of fs.readdirSync(path.join(root, 'apps'))) walk(path.join(root, 'apps', d, 'src', 'app'), routeFile, entries);
for (const d of fs.readdirSync(path.join(root, 'packages'))) walk(path.join(root, 'packages', d, 'src', 'routes'), routeFile, entries);
walk(path.join(root, 'src'), null, entries);
walk(path.join(root, 'scripts'), null, entries);
walk(path.join(root, 'tests'), null, entries);
walk(path.join(root, 'fixtures'), null, entries);

// --- edge extraction via the TypeScript AST ---
const allFiles = new Set(entries);
const queue = [...new Set(entries)];
const seen = new Set(queue);
const unresolvable = []; // (file, specifier text) dynamic imports the AST cannot resolve
const parseErrors = [];

function collectSpecs(file, out) {
  const text = fs.readFileSync(file, 'utf8');
  const kind = file.endsWith('.tsx') || file.endsWith('.jsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.ESNext, true, kind);
  for (const diag of sf.parseDiagnostics ?? []) parseErrors.push([file, diag.messageText]);
  function visit(node) {
    if (ts.isImportDeclaration(node)) {
      if (ts.isStringLiteralLike(node.moduleSpecifier)) out.specs.push(node.moduleSpecifier.text);
      else out.unresolvable.push(node.moduleSpecifier ? node.moduleSpecifier.getText(sf) : '(none)');
    } else if (ts.isExportDeclaration(node) && node.moduleSpecifier) {
      if (ts.isStringLiteralLike(node.moduleSpecifier)) out.specs.push(node.moduleSpecifier.text);
      else out.unresolvable.push(node.moduleSpecifier.getText(sf));
    } else if (ts.isImportEqualsDeclaration(node) && node.moduleReference) {
      const ref = node.moduleReference;
      if (ts.isStringLiteralLike(ref.expression ?? ref)) out.specs.push((ref.expression ?? ref).text);
    } else if (ts.isCallExpression(node)) {
      const expr = node.expression;
      const isImport =
        (ts.isIdentifier(expr) && expr.text === 'import') ||
        expr.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isPropertyAccessExpression(expr) && expr.name.text === 'import');
      const isRequire = ts.isIdentifier(expr) && expr.text === 'require';
      if ((isImport || isRequire) && node.arguments.length === 1) {
        const a = node.arguments[0];
        if (ts.isStringLiteral(a) || ts.isNoSubstitutionTemplateLiteral(a)) out.specs.push(a.text);
        else out.unresolvable.push(a.getText(sf));
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(sf);
}

while (queue.length) {
  const f = queue.shift();
  const out = { specs: [], unresolvable: [] };
  collectSpecs(f, out);
  for (const u of out.unresolvable) unresolvable.push([path.relative(root, f), u]);
  for (const s of out.specs) {
    const r = resolveSpec(s, f);
    if (r && !seen.has(r)) {
      seen.add(r);
      queue.push(r);
      allFiles.add(r);
    }
  }
}

// Note: BFS only follows resolved edges, so the reachable `seen` set equals
// the scanner's semantics; the difference under test is edge extraction.
const universe = [];
function walkUniverse(dir) {
  if (!fs.existsSync(dir)) return;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (['node_modules', '.next', 'target', '.turbo'].includes(e.name)) continue;
      walkUniverse(p);
    } else if (/\.(ts|tsx)$/.test(e.name) && p.includes('/src/')) universe.push(p);
  }
}
for (const d of fs.readdirSync(path.join(root, 'packages'))) {
  if (d.startsWith('server-')) walkUniverse(path.join(root, 'packages', d));
}
const dead = universe.filter((f) => !seen.has(f)).sort();
console.error(`TS cross-check: universe=${universe.length} reachable=${seen.size} dead=${dead.length} unresolvable-dynamics=${unresolvable.length} parse-errors=${parseErrors.length}`);
for (const [file, spec] of unresolvable.slice(0, 40)) console.error(`  UNRESOLVED ${file}: ${spec.slice(0, 80)}`);
for (const f of dead) console.log(path.relative(root, f));
