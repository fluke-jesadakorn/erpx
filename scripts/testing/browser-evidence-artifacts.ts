import { readFile, realpath } from "node:fs/promises";
import { isDeepStrictEqual } from "node:util";
import { basename, dirname, join, resolve, sep } from "node:path";
import type { BrowserCiReceipt } from "./browser-sharding";
import type { BrowserRunReceipt } from "./browser-runner";

const evidencePathPrefix = ".scratch/dev/test-suites/";
const evidencePathPattern = /^\.scratch\/dev\/test-suites\/browser\/\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/;

/** Find the test-suites root even when CI download artifacts add nested directories. */
export function browserEvidenceArtifactRootFromReceiptPath(receiptPath: string): string {
  const absoluteReceipt = resolve(receiptPath);
  let directory = dirname(absoluteReceipt);
  while (directory !== dirname(directory)) {
    if (basename(directory) === "test-suites") return directory;
    directory = dirname(directory);
  }
  if (/^browser-\d+-\d+\.json$/.test(basename(absoluteReceipt)) && basename(dirname(absoluteReceipt)) === "ci") {
    return dirname(dirname(absoluteReceipt));
  }
  throw new Error("CI_BROWSER_EVIDENCE_ROOT_REQUIRED");
}

/** Bind every CI cohort path to its persisted receipt inside that candidate artifact. */
export async function verifyBrowserEvidenceArtifacts(receipt: BrowserCiReceipt, artifactRoot: string): Promise<void> {
  if (!Array.isArray(receipt.cohorts)) throw new Error("CI_BROWSER_EVIDENCE_PATH_REQUIRED");
  const entries = receipt.cohorts.flatMap(cohort => [
    { path: cohort.forwardEvidenceDirectory, embedded: cohort.forward },
    { path: cohort.reverseEvidenceDirectory, embedded: cohort.reverse },
    ...(cohort.probe ? [{ path: cohort.probeEvidenceDirectory, embedded: cohort.probe }] : []),
  ]);
  if (!entries.length || entries.some(({ path, embedded }) => typeof path !== "string" || !embedded ||
    !evidencePathPattern.test(path) || embedded.evidencePath !== path) ||
    new Set(entries.map(entry => entry.path)).size !== entries.length) throw new Error("CI_BROWSER_EVIDENCE_PATH_REQUIRED");

  const root = await realpath(artifactRoot);
  for (const { path, embedded } of entries) {
    if (typeof path !== "string" || !embedded || !path.startsWith(evidencePathPrefix)) throw new Error("CI_BROWSER_EVIDENCE_PATH_REQUIRED");
    const actualDirectory = resolve(root, path.slice(evidencePathPrefix.length));
    if (!actualDirectory.startsWith(`${root}${sep}`)) throw new Error("CI_BROWSER_EVIDENCE_PATH_REQUIRED");
    const realDirectory = await realpath(actualDirectory);
    if (!realDirectory.startsWith(`${root}${sep}`)) throw new Error("CI_BROWSER_EVIDENCE_PATH_REQUIRED");
    const persisted = JSON.parse(await readFile(join(realDirectory, "receipt.json"), "utf8")) as BrowserRunReceipt;
    if (!isDeepStrictEqual(persisted, embedded)) throw new Error("CI_BROWSER_EVIDENCE_RECEIPT_MISMATCH");
  }
}
