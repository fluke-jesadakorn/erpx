import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { relative, resolve } from "node:path";
import { resolveDocumentationReference, type HistoricalSourceSnapshot, type PrivateArtifactReference } from "./documentation-references";

const root = resolve(import.meta.dir, "../..");
const errors: string[] = [];
const read = (path: string) => readFileSync(path, "utf8");
const hash = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");
const ledger = JSON.parse(read(resolve(root, "docs/status/business-acceptance-evidence.json"))) as {
  records: Array<{ evidence: Array<{ path: string; sha256: string }> }>;
};
const protectedPaths = new Set(ledger.records.flatMap(row => row.evidence.map(item => item.path)));
const sourceSnapshots=JSON.parse(read(resolve(root,'docs/status/historical-source-snapshots.json'))) as {entries:HistoricalSourceSnapshot[]};
const privateArtifacts=JSON.parse(read(resolve(root,'docs/status/private-artifact-references.json'))) as {version:number;entries:PrivateArtifactReference[]};
if (privateArtifacts.version !== 1 || !Array.isArray(privateArtifacts.entries)) throw new Error("INVALID_PRIVATE_ARTIFACT_REGISTER");
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
for (const name of ["README.md", "AGENTS.md", "CLAUDE.md", "CONTEXT.md", "CONTRACT_POLICY.md", "CUTOVER.md", "LICENSING.md", "CONTRIBUTING.md", "SECURITY.md", "CODE_OF_CONDUCT.md", "GOVERNANCE.md"])
  if (existsSync(resolve(root, name))) documents.push(resolve(root, name));

// Community metadata is a preparation contract, not a public-release receipt.
for (const name of ["LICENSE", "NOTICE", "LICENSING.md", "CONTRIBUTING.md", "SECURITY.md", "CODE_OF_CONDUCT.md", "GOVERNANCE.md", ".github/CODEOWNERS", ".github/PULL_REQUEST_TEMPLATE.md", ".github/ISSUE_TEMPLATE/bug_report.yml", ".github/ISSUE_TEMPLATE/proposal.yml", ".github/ISSUE_TEMPLATE/config.yml", "docs/community/open-source.md", "docs/community/provenance.json"])
  if (!existsSync(resolve(root, name))) errors.push(`Missing community foundation: ${name}`);
if (existsSync(resolve(root, "LICENSE")) && !read(resolve(root, "LICENSE")).includes("Version 2.0, January 2004"))
  errors.push("The current project license must contain the Apache-2.0 text");
if (JSON.parse(read(resolve(root, "package.json"))).license !== "Apache-2.0")
  errors.push("The root package must declare the current Apache-2.0 license");
const provenancePath = resolve(root, "docs/community/provenance.json");
if (existsSync(provenancePath)) {
  const provenance = JSON.parse(read(provenancePath)) as {
    version: number;
    inventoryComplete: boolean;
    publicReleaseReady: boolean;
    entries: Array<{ id: string; paths?: string[]; gitRefs?: string[]; noticeFiles?: string[]; reviewStatus: string; basis: string }>;
  };
  const statuses = new Set(["scope-declared", "notice-reviewed", "pending", "cleared"]);
  const ids = new Set<string>();
  if (provenance.version !== 1 || typeof provenance.inventoryComplete !== "boolean" || typeof provenance.publicReleaseReady !== "boolean" || !Array.isArray(provenance.entries) || !provenance.entries.length)
    errors.push("Invalid community provenance register");
  else for (const entry of provenance.entries) {
    if (!entry.id || ids.has(entry.id) || !statuses.has(entry.reviewStatus) || !entry.basis || !(entry.paths?.length || entry.gitRefs?.length))
      errors.push(`Invalid community provenance entry: ${entry.id}`);
    ids.add(entry.id);
    for (const name of [...(entry.paths ?? []), ...(entry.noticeFiles ?? [])]) {
      const path = resolve(root, name);
      if (relative(root, path).startsWith("..") || !existsSync(path)) errors.push(`Missing/unsafe provenance source: ${name}`);
    }
  }
  if (provenance.inventoryComplete && provenance.entries.some(entry => entry.reviewStatus !== "cleared"))
    errors.push("A complete provenance inventory requires every entry to be cleared");
  if (provenance.publicReleaseReady && !provenance.inventoryComplete)
    errors.push("Public-release readiness requires a complete provenance inventory");
}
const withoutCode = (text: string) => text.replace(/```[\s\S]*?```|~~~[\s\S]*?~~~/g, "");
const anchorCache = new Map<string, Set<string>>();
function anchors(path: string, source?: string) {
  if (anchorCache.has(path)) return anchorCache.get(path)!;
  const text = withoutCode(source ?? read(path)), ids = new Set<string>(), counts = new Map<string, number>();
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
let historicalSources = 0;
const privateReferences = new Map<string, "checksum-verified" | "unverified">();
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
    checked++;
    try {
      const reference = resolveDocumentationReference({ root, document: name, destination, snapshots: sourceSnapshots.entries, privateArtifacts: privateArtifacts.entries });
      if (reference.kind === "missing") errors.push(`${name}: missing local link ${raw}`);
      else if (reference.kind === "private-artifact") privateReferences.set(reference.id, reference.availability);
      else {
        const historical = reference.kind === "historical-source";
        if (historical) historicalSources++;
        if (fragment && /\.mdx?$/.test(reference.path) && !anchors(historical ? `${reference.commit}:${reference.path}` : reference.path, historical ? reference.markdown : undefined).has(decodeURIComponent(fragment)))
          errors.push(`${name}: missing anchor ${raw}`);
      }
    } catch (error) {
      errors.push(`${name}: invalid local link ${raw}: ${error instanceof Error ? error.message : String(error)}`);
    }
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
} else {
  const available = [...privateReferences.values()].filter(status => status === "checksum-verified").length;
  console.log(`Documentation checked: ${documents.length} Markdown files inventoried; ${checked} local references, ${historicalSources} historical source objects, ${protectedPaths.size} ledger files, scenario sources and generated artifacts checked.`);
  console.log(`Known unpublished private artifacts: ${privateReferences.size}; ${available} available and checksum-verified; ${privateReferences.size - available} with availability unverified. Private references do not establish release acceptance.`);
}
