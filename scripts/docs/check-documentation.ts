import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { execFileSync } from 'node:child_process';

const root = resolve(import.meta.dir, "../..");
const errors: string[] = [];
const read = (path: string) => readFileSync(path, "utf8");
const hash = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");
const ledger = JSON.parse(read(resolve(root, "docs/status/business-acceptance-evidence.json"))) as {
  records: Array<{ evidence: Array<{ path: string; sha256: string }> }>;
};
const protectedPaths = new Set(ledger.records.flatMap(row => row.evidence.map(item => item.path)));
const sourceSnapshots=JSON.parse(read(resolve(root,'docs/status/historical-source-snapshots.json'))) as {entries:Array<{document:string;commit:string}>};
function historicalSource(document:string,file:string) {
 const snapshot=sourceSnapshots.entries.find(entry=>entry.document===document);
 if(!snapshot||!file.startsWith('src/')||!/^[a-f0-9]{40}$/.test(snapshot.commit))return false;
 try {execFileSync('git',['cat-file','-e',`${snapshot.commit}:${file}`],{cwd:root,stdio:'ignore'});return true;}catch{return false;}
}
for (const row of ledger.records) for (const evidence of row.evidence) {
  const path = resolve(root, evidence.path);
  if (!existsSync(path) || hash(path) !== evidence.sha256) errors.push(`Missing/changed evidence: ${evidence.path}`);
}

const documents: string[] = [];
function walk(directory: string) {
  if (!existsSync(directory)) return;
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) walk(path);
    else if (/\.mdx?$/.test(path)) documents.push(path);
  }
}
for (const directory of ["docs", "ops", "Designed", "fixtures", ".agents", ".github", ".claude"]) walk(resolve(root, directory));
for (const name of ["README.md", "AGENTS.md", "CLAUDE.md", "CONTEXT.md", "CONTRACT_POLICY.md", "CUTOVER.md"])
  if (existsSync(resolve(root, name))) documents.push(resolve(root, name));
const withoutCode = (text: string) => text.replace(/```[\s\S]*?```|~~~[\s\S]*?~~~/g, "");
const anchorCache = new Map<string, Set<string>>();
function anchors(path: string) {
  if (anchorCache.has(path)) return anchorCache.get(path)!;
  const text = withoutCode(read(path)), ids = new Set<string>(), counts = new Map<string, number>();
  for (const heading of text.matchAll(/^#{1,6}\s+(.+)$/gm)) {
    const label = heading[1].replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/<[^>]*>/g, "");
    const slug = label.toLowerCase().replace(/[^\p{L}\p{N}\p{M}_\s-]/gu, "").replace(/\s/g, "-");
    const count = counts.get(slug) ?? 0;
    counts.set(slug, count + 1);
    ids.add(count ? `${slug}-${count}` : slug);
  }
  for (const attribute of text.matchAll(/(?:id|name)=["']([^"']+)["']/g)) ids.add(attribute[1]);
  anchorCache.set(path, ids);
  return ids;
}

let checked = 0;
for (const path of documents) {
  const name = relative(root, path);
  // Immutable historical evidence retains its original link topology. It is
  // checksum-checked above, not rewritten to today's routes or files.
  if (protectedPaths.has(name) || name.startsWith("docs/testing/evidence/") || name.startsWith("docs/archive/")) continue;
  const text = withoutCode(read(path));
  const targets = [
    ...text.matchAll(/!?\[[^\]]*\]\((<[^>]+>|[^\s)]+)(?:\s+["'][^"']*["'])?\)/g),
    ...text.matchAll(/^\s*\[[^\]]+\]:\s*(<[^>]+>|\S+)/gm),
  ];
  for (const target of targets) {
    const raw = target[1].replace(/^<|>$/g, "");
    if (/^[\w+.-]+:|^\/\//.test(raw)) continue;
    const [destination, fragment] = raw.split("#");
    const resolved = destination ? resolve(dirname(path), decodeURIComponent(destination)) : path;
    checked++;
    if (!existsSync(resolved)&&!historicalSource(name,relative(root,resolved))) errors.push(`${name}: missing local link ${raw}`);
    else if (fragment && /\.mdx?$/.test(resolved) && !anchors(resolved).has(decodeURIComponent(fragment)))
      errors.push(`${name}: missing anchor ${raw}`);
  }
}

// The machine-readable registers are executable inputs, not disposable plans.
const register = JSON.parse(read(resolve(root, "tests/catalogue/engineering-scenarios.json"))) as {
  capabilities: Array<{ id: string; cases: Array<{ file: string }> }>;
};
for (const capability of register.capabilities) for (const item of capability.cases)
  if (!existsSync(resolve(root, item.file))) errors.push(`${capability.id}: missing executed-case source ${item.file}`);
for (const name of ["engineering-gate-scenarios.json", "legal-uat-engineering-cases.json", "engineering-attachment-owner-cases.json"])
  if (!existsSync(resolve(root, "tests/catalogue", name))) errors.push(`Missing runner register ${name}`);
if (existsSync(resolve(root, "docs/testing/uat-route-inventory.json"))) errors.push("UAT route register belongs in tests/catalogue, not docs/testing.");

const generated = Bun.spawnSync([process.execPath, "scripts/docs/generate-capability-matrix.ts", "--check"], { cwd: root, stdout: "pipe", stderr: "pipe" });
if (generated.exitCode !== 0) errors.push(generated.stderr.toString().trim() || "Generated capability artifacts are stale.");
const routes = Bun.spawnSync([process.execPath, "scripts/docs/generate-uat-route-inventory.ts", "--check"], { cwd: root, stdout: "pipe", stderr: "pipe" });
if (routes.exitCode !== 0) errors.push(routes.stderr.toString().trim() || "Generated UAT route inventory is stale.");
if (errors.length) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else console.log(`Documentation verified: ${documents.length} Markdown files inventoried; ${checked} local links/anchors, ${protectedPaths.size} ledger files, scenario sources and generated artifacts checked.`);
