import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, expect, it } from "vitest";
import { resolveDocumentationReference, type PrivateArtifactReference } from "../../../scripts/docs/documentation-references";

const roots: string[] = [];
const digest = (value: Buffer) => createHash("sha256").update(value).digest("hex");
function fixture(expectedHash?: string) {
  const root = mkdtempSync(join(tmpdir(), "erpx-documentation-reference-")); roots.push(root);
  const document = "docs/evidence.md", sourcePath = "src/retired.ts", artifactPath = ".scratch/dev/evidence.json";
  const artifact = Buffer.from('{"scope":"fictional unit evidence"}\n');
  const sha256 = expectedHash ?? digest(artifact);
  for (const path of [document, sourcePath]) mkdirSync(dirname(join(root, path)), { recursive: true });
  writeFileSync(join(root, document), `# Dated evidence\n\n[Private evidence](../${artifactPath}), SHA-256 \`${sha256}\`.\n`);
  writeFileSync(join(root, sourcePath), "export const original = 'recorded source';\n");
  const git = (...args: string[]) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  git("init", "--quiet", "--initial-branch=development");
  git("add", "--", document, sourcePath);
  git("-c", "user.name=Documentation Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Record fictional source evidence");
  const commit = git("rev-parse", "HEAD");
  const entry: PrivateArtifactReference = {
    id: "fictional-private-evidence", document, path: artifactPath, sha256,
    source: { commit, documentSha256: digest(readFileSync(join(root, document))) },
    retention: { status: "recorded-private", repositoryRoot: root, observedAt: "2026-10-27" }, publication: "unpublished",
  };
  return { root, document, sourcePath, artifactPath, artifact, entry, commit };
}
type Fixture = ReturnType<typeof fixture>;
function reference(f: Fixture, destination: string, overrides: Partial<Parameters<typeof resolveDocumentationReference>[0]> = {}) {
  return resolveDocumentationReference({ root: f.root, document: f.document, destination, snapshots: [], privateArtifacts: [], ...overrides });
}
function retain(f: Fixture) {
  const path = join(f.root, f.artifactPath); mkdirSync(dirname(path), { recursive: true, mode: 0o700 }); writeFileSync(path, f.artifact, { mode: 0o600 });
}
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

it("resolves existing current links while keeping unknown active links missing", () => {
  const f = fixture(); writeFileSync(join(f.root, "docs/current.md"), "# Current documentation\n");
  expect(reference(f, "./current.md")).toEqual({ kind: "current", path: join(f.root, "docs/current.md") });
  expect(reference(f, "./unknown.md")).toEqual({ kind: "missing", path: "docs/unknown.md" });
});

it("validates the real Git blob for an explicitly recorded historical absolute root", () => {
  const f = fixture(); rmSync(join(f.root, f.sourcePath));
  const snapshots = [{ document: f.document, commit: f.commit, repositoryRoot: "/historical/erpx" }];
  expect(reference(f, "/historical/erpx/src/retired.ts", { snapshots })).toMatchObject({ kind: "historical-source", path: f.sourcePath, commit: f.commit });
  expect(reference(f, "/historical/erpx/src", { snapshots })).toMatchObject({ kind: "historical-source", path: "src", commit: f.commit });
  expect(reference(f, "/historical/erpx/src/missing.ts", { snapshots })).toEqual({ kind: "missing", path: "src/missing.ts" });
  expect(reference(f, "/historical/erpx/src/retired.ts", { snapshots: [{ ...snapshots[0], commit: "f".repeat(40) }] }).kind).toBe("missing");
});

it("rejects arbitrary absolute roots, lookalike prefixes and repository escapes", () => {
  const f = fixture(), snapshots = [{ document: f.document, commit: f.commit, repositoryRoot: "/historical/erpx" }];
  for (const path of ["/unregistered/src/retired.ts", "/historical/erpx-other/src/retired.ts", "/historical/erpx/src/../../outside.ts"])
    expect(() => reference(f, path, { snapshots })).toThrow("UNREGISTERED_ABSOLUTE_DOCUMENT_REFERENCE");
  expect(() => reference(f, "../../outside.md")).toThrow("DOCUMENT_REFERENCE_PATH_ESCAPE");
  expect(() => reference(f, "%2e%2e/%2e%2e/outside.md")).toThrow("DOCUMENT_REFERENCE_PATH_ESCAPE");
});

it("reports source-bound unpublished private evidence as availability unverified when no copy exists", () => {
  const f = fixture();
  expect(reference(f, "../.scratch/dev/evidence.json", { privateArtifacts: [f.entry] })).toEqual({ kind: "private-artifact", path: f.artifactPath, id: f.entry.id, availability: "unverified" });
});

it("checks a retained private copy and fails when its bytes do not match the source-bound checksum", () => {
  const f = fixture(); retain(f);
  expect(reference(f, "../.scratch/dev/evidence.json", { privateArtifacts: [f.entry] })).toMatchObject({ kind: "private-artifact", availability: "checksum-verified" });
  writeFileSync(join(f.root, f.artifactPath), "changed fictional evidence");
  expect(() => reference(f, "../.scratch/dev/evidence.json", { privateArtifacts: [f.entry] })).toThrow("PRIVATE_ARTIFACT_CHECKSUM_MISMATCH");
});

it("rejects a forged expected private hash even when the referring document records that same claim", () => {
  const f = fixture("0".repeat(64)); retain(f);
  expect(() => reference(f, "../.scratch/dev/evidence.json", { privateArtifacts: [f.entry] })).toThrow("PRIVATE_ARTIFACT_CHECKSUM_MISMATCH");
});

it("requires the exact referring source and does not exempt other ignored links", () => {
  const f = fixture(); retain(f);
  expect(() => reference(f, "../.scratch/dev/evidence.json")).toThrow("UNREGISTERED_PRIVATE_ARTIFACT_REFERENCE");
  writeFileSync(join(f.root, f.document), "A different document without the original checksum.");
  expect(() => reference(f, "../.scratch/dev/evidence.json", { privateArtifacts: [f.entry] })).toThrow("PRIVATE_ARTIFACT_SOURCE_PROVENANCE_MISMATCH");
});

it("rejects unsafe private metadata and symlinks outside its retained private directory", () => {
  const f = fixture();
  expect(() => reference(f, "../.scratch/dev/evidence.json", { privateArtifacts: [{ ...f.entry, sha256: "not-a-digest" }] })).toThrow("INVALID_PRIVATE_ARTIFACT_METADATA");
  const path = join(f.root, f.artifactPath); mkdirSync(dirname(path), { recursive: true });
  writeFileSync(join(f.root, "outside.json"), f.artifact); symlinkSync(join(f.root, "outside.json"), path);
  expect(() => reference(f, "../.scratch/dev/evidence.json", { privateArtifacts: [f.entry] })).toThrow("PRIVATE_ARTIFACT_PATH_ESCAPE");
});
