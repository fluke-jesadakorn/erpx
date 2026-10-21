import { createHash } from "node:crypto";
import { readFile, realpath } from "node:fs/promises";
import { basename, isAbsolute, join, relative, resolve, sep } from "node:path";
import { verifyEngineeringCandidateInventory } from "./engineering/engineering-candidate-inventory";

export type FullAcceptanceRelease = { directory: string; id: string; fileManifestSha256: string };
const releaseIdPattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,100}$/;

/** Resolve either the just-built latest release or a prebuilt canonical release for plan binding. */
export async function verifyFullAcceptanceRelease(root: string, runtimeFingerprint: string, requestedDirectory?: string): Promise<FullAcceptanceRelease> {
  const canonicalRoot = await realpath(root);
  const releaseRoot = await realpath(join(canonicalRoot, ".runtime/releases"));
  const requested = requestedDirectory === undefined
    ? join(canonicalRoot, ".runtime/releases/latest")
    : isAbsolute(requestedDirectory) ? requestedDirectory : resolve(canonicalRoot, requestedDirectory);
  const directory = await realpath(requested);
  if (requestedDirectory !== undefined && requested !== directory)
    throw new Error("FULL_REGRESSION_RELEASE_PATH_NOT_CANONICAL");
  const relativeRelease = relative(releaseRoot, directory);
  if (!relativeRelease || relativeRelease === ".." || relativeRelease.startsWith(`..${sep}`) || isAbsolute(relativeRelease) || relativeRelease.includes(sep))
    throw new Error("FULL_REGRESSION_RELEASE_DIRECTORY_REQUIRED");

  const manifest = JSON.parse(await readFile(join(directory, "release.json"), "utf8")) as {
    id?: unknown; sourceFingerprint?: unknown; node?: unknown; fileManifestSha256?: unknown;
  };
  if (typeof manifest.id !== "string" || !releaseIdPattern.test(manifest.id))
    throw new Error("FULL_REGRESSION_RELEASE_ID_INVALID");
  if (manifest.id !== basename(directory) || manifest.node !== "26.11.1" ||
    manifest.sourceFingerprint !== runtimeFingerprint || typeof manifest.fileManifestSha256 !== "string" || !/^[a-f0-9]{64}$/.test(manifest.fileManifestSha256))
    throw new Error("FULL_REGRESSION_CANDIDATE_MISMATCH");
  const fileManifest = await readFile(join(directory, "release-files.json"));
  const fileManifestSha256 = createHash("sha256").update(fileManifest).digest("hex");
  if (manifest.fileManifestSha256 !== fileManifestSha256) throw new Error("FULL_REGRESSION_RELEASE_MANIFEST_MISMATCH");
  await verifyEngineeringCandidateInventory(directory, fileManifestSha256);
  return { directory, id: manifest.id, fileManifestSha256 };
}

/** Full acceptance can build a fresh release or verify one prepared for host-plan binding. */
export async function prepareFullAcceptanceRelease(
  root: string,
  runtimeFingerprint: string,
  requestedDirectory: string | undefined,
  buildRelease: () => Promise<void>,
): Promise<FullAcceptanceRelease> {
  if (requestedDirectory === undefined) await buildRelease();
  return verifyFullAcceptanceRelease(root, runtimeFingerprint, requestedDirectory);
}
