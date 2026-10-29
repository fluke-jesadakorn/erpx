import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";

export type HistoricalSourceSnapshot = { document: string; commit: string; repositoryRoot?: string };
export type PrivateArtifactReference = {
  id: string;
  document: string;
  path: string;
  sha256: string;
  source: { commit: string; documentSha256: string };
  retention: { status: "recorded-private"; repositoryRoot: string; observedAt: string };
  publication: "unpublished";
};
export type DocumentationReference =
  | { kind: "current"; path: string }
  | { kind: "historical-source"; path: string; commit: string; markdown?: string }
  | { kind: "private-artifact"; path: string; id: string; availability: "checksum-verified" | "unverified" }
  | { kind: "missing"; path: string };

const sha256 = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
const repoPath = (path: string) => path.split(sep).join("/");
const inside = (root: string, path: string) => {
  const value = relative(root, path);
  return value !== ".." && !value.startsWith(`..${sep}`) && !isAbsolute(value);
};
function safeRelative(path: string) {
  return !!path && !isAbsolute(path) && !path.includes("\\") && !path.includes("\0") && !path.split("/").some(part => part === ".." || part === ".");
}
function gitSource(root: string, commit: string, path: string): { kind: "blob"; bytes: Buffer } | { kind: "tree" } | null {
  if (!/^[a-f0-9]{40}$/.test(commit) || !safeRelative(path)) return null;
  try {
    if (execFileSync("git", ["cat-file", "-t", commit], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim() !== "commit") return null;
    const object = `${commit}:${path}`;
    const kind = execFileSync("git", ["cat-file", "-t", object], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
    if (kind === "tree") return { kind };
    if (kind !== "blob") return null;
    return { kind, bytes: execFileSync("git", ["cat-file", "blob", object], { cwd: root, stdio: ["ignore", "pipe", "ignore"], maxBuffer: 8 * 1024 * 1024 }) };
  } catch { return null; }
}
function historical(root: string, snapshot: HistoricalSourceSnapshot, path: string): DocumentationReference {
  const source = gitSource(root, snapshot.commit, path);
  return source ? { kind: "historical-source", path, commit: snapshot.commit, ...(source.kind === "blob" && /\.mdx?$/.test(path) ? { markdown: source.bytes.toString("utf8") } : {}) } : { kind: "missing", path };
}

function privateArtifact(root: string, entry: PrivateArtifactReference): DocumentationReference {
  if (!safeRelative(entry.document) || !safeRelative(entry.path) || !entry.path.startsWith(".scratch/") || !/^[a-f0-9]{64}$/.test(entry.sha256) ||
      !/^[a-f0-9]{64}$/.test(entry.source.documentSha256) || entry.retention.status !== "recorded-private" || entry.publication !== "unpublished" ||
      !isAbsolute(entry.retention.repositoryRoot) || entry.retention.repositoryRoot.split(/[\\/]/).includes("..") || !/^\d{4}-\d{2}-\d{2}$/.test(entry.retention.observedAt))
    throw new Error(`INVALID_PRIVATE_ARTIFACT_METADATA:${entry.id}`);
  const document = resolve(root, entry.document), source = gitSource(root, entry.source.commit, entry.document);
  if (!existsSync(document) || source?.kind !== "blob" || sha256(source.bytes) !== entry.source.documentSha256 || sha256(readFileSync(document)) !== entry.source.documentSha256 || !source.bytes.toString("utf8").includes(entry.sha256))
    throw new Error(`PRIVATE_ARTIFACT_SOURCE_PROVENANCE_MISMATCH:${entry.id}`);
  let found = false;
  for (const retainedRoot of new Set([root, resolve(entry.retention.repositoryRoot)])) {
    const candidate = resolve(retainedRoot, entry.path);
    if (!existsSync(candidate)) continue;
    const privateRoot = resolve(retainedRoot, ".scratch");
    if (!statSync(candidate).isFile() || !inside(realpathSync(privateRoot), realpathSync(candidate)))
      throw new Error(`PRIVATE_ARTIFACT_PATH_ESCAPE:${entry.id}`);
    if (sha256(readFileSync(candidate)) !== entry.sha256) throw new Error(`PRIVATE_ARTIFACT_CHECKSUM_MISMATCH:${entry.id}`);
    found = true;
  }
  return { kind: "private-artifact", path: entry.path, id: entry.id, availability: found ? "checksum-verified" : "unverified" };
}

/** Resolve only current repo paths, explicitly recorded historical roots or source-bound private evidence. */
export function resolveDocumentationReference(options: {
  root: string;
  document: string;
  destination: string;
  snapshots: readonly HistoricalSourceSnapshot[];
  privateArtifacts: readonly PrivateArtifactReference[];
}): DocumentationReference {
  const root = resolve(options.root);
  if (!safeRelative(options.document)) throw new Error("INVALID_DOCUMENT_PATH");
  const destination = decodeURIComponent(options.destination);
  if (destination.includes("\\") || destination.includes("\0")) throw new Error("INVALID_DOCUMENT_REFERENCE_PATH");
  const snapshot = options.snapshots.find(entry => entry.document === options.document);
  if (isAbsolute(destination)) {
    if (!snapshot?.repositoryRoot || !isAbsolute(snapshot.repositoryRoot) || destination.split("/").includes("..") || !inside(resolve(snapshot.repositoryRoot), resolve(destination)))
      throw new Error("UNREGISTERED_ABSOLUTE_DOCUMENT_REFERENCE");
    return historical(root, snapshot, repoPath(relative(resolve(snapshot.repositoryRoot), resolve(destination))));
  }
  const path = destination ? resolve(dirname(resolve(root, options.document)), destination) : resolve(root, options.document);
  if (!inside(root, path)) throw new Error("DOCUMENT_REFERENCE_PATH_ESCAPE");
  const name = repoPath(relative(root, path));
  const matches = options.privateArtifacts.filter(entry => entry.document === options.document && entry.path === name);
  if (matches.length > 1) throw new Error("DUPLICATE_PRIVATE_ARTIFACT_REFERENCE");
  if (matches.length) return privateArtifact(root, matches[0]);
  if (name.startsWith(".scratch/")) throw new Error("UNREGISTERED_PRIVATE_ARTIFACT_REFERENCE");
  if (existsSync(path)) {
    if (!inside(realpathSync(root), realpathSync(path))) throw new Error("DOCUMENT_REFERENCE_SYMLINK_ESCAPE");
    return { kind: "current", path };
  }
  return snapshot && name.startsWith("src/") ? historical(root, snapshot, name) : { kind: "missing", path: name };
}
