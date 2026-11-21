#!/usr/bin/env bun
// Root-complete TypeScript reachability audit for packages/server-*/src modules.
//
// Builds a static import graph from every runtime/test entry root and reports
// whether the target modules are reachable:
//   roots = packages/*/src/routes/** route files + apps/*/src/app/** route files
//           + src/** (workers, server runtime)
//           + scripts/** + tests/** + fixtures/** (all TS/JS entry modules)
//
// Usage: bun scripts/testing/ts-reachability.mjs <file...>   (repo-relative paths)
//        bun scripts/testing/ts-reachability.mjs --list      (dump reachable set)
// Exit code 0 = every target unreachable; 1 = at least one target reachable.
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();

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
  // exports-map value: string, or a condition object (possibly nested).
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

// [^'"] (not [^'"\n"]) lets the lazy scan span multi-line import statements
// (e.g. "import {\n  a,\n} from './x'"); the quote barrier still keeps each
// match inside its own statement.
//
// Keyword boundaries: the whitespace after `import`/`export` is optional
// (`\s*` + negative lookahead) because minified/compact code writes
// "import{x}from'y'" with no spaces at all; a mandatory `\s+` silently
// dropped those statements. The lookahead (?![A-Za-z0-9_$]) keeps the match
// on the real keyword and not on the prefix of a longer identifier
// ("importData", "exportX"), and (?<![\w$]) keeps it off method/property
// names ending in the keyword ("obj.import", "this.require").
//
// Whitespace around `from` is likewise optional: compact re-exports such as
// "export {x}from './y'" omit the space before `from`.
const importRe = /(?<![\w$])(?:import(?![A-Za-z0-9_$])\s*(?:type\s+)?(?:[^'"]*?\s*from\s*)?|export(?![A-Za-z0-9_$])\s*(?:type\s+)?[^'"]*?\s*from\s*|import\s*\(\s*|require\s*\(\s*)['"]([^'"]+)['"]/g;

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

// Next.js route files in app packages.
for (const d of fs.readdirSync(path.join(root, 'apps'))) walk(path.join(root, 'apps', d, 'src', 'app'), routeFile, entries);
// Next.js route files in feature packages (workplace etc. mount routes).
for (const d of fs.readdirSync(path.join(root, 'packages'))) walk(path.join(root, 'packages', d, 'src', 'routes'), routeFile, entries);
// Non-route entries: workers, repo-level src, scripts, tests, fixtures (test oracles are consumers).
walk(path.join(root, 'src'), null, entries);
walk(path.join(root, 'scripts'), null, entries);
walk(path.join(root, 'tests'), null, entries);
walk(path.join(root, 'fixtures'), null, entries);

const seen = new Set();
const parent = new Map();
const queue = [...new Set(entries)];
for (const e of queue) { seen.add(e); parent.set(e, null); }
while (queue.length) {
  const f = queue.shift();
  let text;
  try { text = fs.readFileSync(f, 'utf8'); } catch { continue; }
  for (const m of text.matchAll(importRe)) {
    const r = resolveSpec(m[1], f);
    if (r && !seen.has(r)) { seen.add(r); parent.set(r, f); queue.push(r); }
  }
}

const args = process.argv.slice(2);
if (args.includes('--list')) {
  for (const f of [...seen].sort()) console.log(path.relative(root, f));
  process.exit(0);
}
let any = false;
for (const t of args) {
  const abs = path.resolve(root, t);
  if (seen.has(abs)) {
    any = true;
    const chain = [];
    let cur = parent.get(abs);
    while (cur) { chain.push(path.relative(root, cur)); cur = parent.get(cur); }
    console.log('REACHABLE', t, '\n  via ' + chain.slice(0, 8).join(' <- '));
  } else console.log('UNREACHABLE', t);
}
console.log('roots:', entries.length, 'modules:', seen.size);
process.exit(any ? 1 : 0);
