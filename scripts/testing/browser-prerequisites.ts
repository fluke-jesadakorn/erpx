import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import type { BrowserCase } from "./browser-results";
import type { Suite } from "./catalogue";
import { pinnedBunBinary } from "./runtime-tooling";

export const browserPrerequisiteIds = ["legal-worker", "legal-ai", "encrypted-recovery"] as const;
export type BrowserPrerequisite = typeof browserPrerequisiteIds[number];
const annotationType = "erpx-browser-prerequisites";
const legalRequired: Record<string, BrowserPrerequisite> = {
  "UAT-11/25: compiled workers notify the eligible owner and escalation reviewer once and recover without changing certification": "legal-worker",
  "UAT-22: unavailable Legal AI leaves untrusted evidence inert and a persisted manual document workflow available": "legal-ai",
  "UAT-26: encrypted restore and compatible rollback retain Legal originals, histories, ACLs, holds and queued job state": "encrypted-recovery",
};
function validated(value: unknown): BrowserPrerequisite[] {
  if (!Array.isArray(value) || new Set(value).size !== value.length || value.some(id => !browserPrerequisiteIds.includes(id))) throw new Error("INVALID_BROWSER_PREREQUISITES");
  return value as BrowserPrerequisite[];
}
export function browserPrerequisites(...requirements: BrowserPrerequisite[]) {
  return { annotation: { type: annotationType, description: JSON.stringify(validated(requirements)) } };
}
export function browserCasePrerequisites(suite: Pick<Suite, "fixture" | "lane">, wanted: BrowserCase): BrowserPrerequisite[] {
  const annotations = wanted.annotations?.filter(annotation => annotation.type === annotationType) ?? [];
  const legal = suite.fixture === "legal" && suite.lane === "uat";
  if (!annotations.length && !legal) return [];
  if (annotations.length !== 1 || annotations[0].description === undefined) throw new Error("BROWSER_CASE_PREREQUISITE_MAPPING_REQUIRED");
  let value: unknown;
  try { value = JSON.parse(annotations[0].description); } catch { throw new Error("INVALID_BROWSER_PREREQUISITES"); }
  const selected = validated(value);
  const expected = legalRequired[wanted.title];
  if (!legal && selected.length || legal && (expected ? selected.length !== 1 || selected[0] !== expected : selected.length !== 0)) throw new Error("BROWSER_CASE_PREREQUISITE_MISMATCH");
  return selected;
}

type Release = { id: string; node: string; sourceFingerprint: string; softwareVersion: string; fileManifestSha256: string };
type CompiledState = { run: string; prefix: string; origin: string; port: number; ready: boolean; sourceFingerprint: string; softwareVersion: string; release?: { id: string; fileManifestSha256: string }; fixture?: { legal?: unknown } };
export function assertCompiledLegalCase(state: CompiledState, release: Release, run: string) {
  if (!/^[a-f0-9]{12}$/.test(run) || state.run !== run || state.prefix !== `erpx_browser_${run}` || state.origin !== "http://127.0.0.1:3104" || state.port !== 3104 || !state.ready || !state.fixture?.legal || release.node !== "24.20.0" || !/^[a-f0-9]{64}$/.test(release.sourceFingerprint) || !/^[a-f0-9]{64}$/.test(release.fileManifestSha256) || state.sourceFingerprint !== release.sourceFingerprint || state.softwareVersion !== `release-${release.sourceFingerprint}` || release.softwareVersion !== state.softwareVersion || state.release?.id !== release.id || state.release.fileManifestSha256 !== release.fileManifestSha256) throw new Error("EXACT_COMPILED_LEGAL_CASE_REQUIRED");
}
export function assertRecoveryEvidence(evidence: Record<string, unknown>, release: Release) {
  const legal = evidence.legal as Record<string, unknown> | undefined;
  const facts = ["originalBytesRetained", "historyRetained", "aclRetained", "unassignedReaderRefused", "overlappingHoldsRetained", "queuedJobRetained", "commandReplayWithoutDuplicateEffects"];
  if (evidence.sourceFingerprint !== release.sourceFingerprint || evidence.softwareVersion !== release.softwareVersion || evidence.fileManifestSha256 !== release.fileManifestSha256 || evidence.compiledCandidate !== true || evidence.databaseCount !== 8 || Number(evidence.tableCount) <= 100 || ["restored", "compatibleRollback", "incompatibleRollbackRejected", "ciphertextTamperingRejected"].some(fact => evidence[fact] !== true) || !legal || facts.some(fact => legal[fact] !== true) || !/^[a-f0-9]{64}$/.test(String(legal.originalSha256)) || !/^[a-f0-9]{64}$/.test(String(evidence.ciphertextSha256))) throw new Error("EXACT_CANDIDATE_RECOVERY_EVIDENCE_REQUIRED");
}

/** A producer failure must still resume the exact owned worker before normal cleanup. */
export async function withPausedBrowserJobs<T>(control: (mode: "pause-jobs" | "resume-jobs") => Promise<void>, produce: () => Promise<T>): Promise<T> {
  await control("pause-jobs");
  let failure: unknown;
  try { return await produce(); }
  catch (error) { failure = error; throw error; }
  finally {
    try { await control("resume-jobs"); }
    catch (error) { if (failure) throw new AggregateError([failure, error], "BROWSER_PREREQUISITE_AND_RESUME_FAILED"); throw error; }
  }
}

export async function prepareBrowserCasePrerequisites(input: {
  suite: Suite; wanted: BrowserCase; run: string; directory: string; releaseDirectory?: string; evidenceDirectory: string;
  environment: Record<string, string | undefined>;
  execute: (args: string[], environment: Record<string, string | undefined>, log: string) => Promise<void>;
}): Promise<Record<string, string>> {
  const required = browserCasePrerequisites(input.suite, input.wanted);
  if (input.suite.fixture !== "legal" || input.suite.lane !== "uat") return {};
  const { repositoryRoot } = await import("./catalogue");
  const { run, execute } = input, root = repositoryRoot, directory = resolve(input.directory), output = join(input.evidenceDirectory, "prerequisites");
  if (directory !== join(root, ".runtime/browser", run) || !resolve(input.evidenceDirectory).startsWith(join(root, ".scratch/dev/test-suites/browser") + "/")) throw new Error("OWNED_BROWSER_PREREQUISITE_DIRECTORIES_REQUIRED");
  const state = JSON.parse(await readFile(join(directory, "state.json"), "utf8")) as CompiledState;
  let release: Release;
  try { release = JSON.parse(await readFile(join(directory, "release/release.json"), "utf8")) as Release; }
  catch { throw new Error("LEGAL_UAT_COMPILED_RELEASE_REQUIRED"); }
  assertCompiledLegalCase(state, release, run);
  if (!required.length) return {};
  await mkdir(output, { recursive: true, mode: 0o700 });
  const environment = { ...input.environment, ERPX_BROWSER_SIMULATION_RUN: run, ERPX_DEMO_RUN: run, ERPX_BROWSER_SIMULATION_PORT: "3104", ERPX_BROWSER_SIMULATION_PROFILE: input.suite.fixture, ERPX_BROWSER_SIMULATION_ATTACHMENTS: "local", ERPX_BROWSER_SIMULATION_JOBS: "1" };
  const bun = pinnedBunBinary(root), artifacts: { path: string; sha256: string }[] = [];
  let passed = false;
  try {
    for (const prerequisite of required) {
      if (prerequisite === "encrypted-recovery") {
        if (!input.releaseDirectory) throw new Error("ATTESTED_RECOVERY_CANDIDATE_DIRECTORY_REQUIRED");
        await execute([bun, "scripts/local/test.ts", "integration", "tests/engineering-recovery.test.ts"], {
          ...environment, ERPX_ENGINEERING_RELEASE_DIR: input.releaseDirectory, ERPX_ENGINEERING_SOURCE_FINGERPRINT: state.sourceFingerprint, ERPX_ENGINEERING_EVIDENCE_DIR: output,
        }, join(output, "encrypted-recovery.log"));
        const bytes = await readFile(join(output, "encrypted-recovery.json"));
        assertRecoveryEvidence(JSON.parse(bytes.toString()), release);
        artifacts.push({ path: join(output, "encrypted-recovery.json"), sha256: createHash("sha256").update(bytes).digest("hex") });
      } else {
        await withPausedBrowserJobs(async mode => {
          await execute([bun, "scripts/local/run-server-script.ts", "scripts/local/browser-simulation.ts", mode, run], environment, join(output, `${prerequisite}-${mode}.log`));
        }, async () => {
          await execute([bun, "scripts/local/run-server-script.ts", "scripts/fixtures/legal-production-worker-uat.ts", run, join(directory, "release"), prerequisite === "legal-ai" ? "ai" : "worker"], environment, join(output, `${prerequisite}.log`));
          const name = prerequisite === "legal-ai" ? "legal-ai-engineering-evidence.json" : "legal-production-worker-evidence.json", bytes = await readFile(join(directory, name));
          const evidence = JSON.parse(bytes.toString());
          if (evidence.run !== run || evidence.scope !== "isolated-engineering" || evidence.sourceFingerprint !== state.sourceFingerprint || evidence.softwareVersion !== release.softwareVersion || evidence.releaseManifestSha256 !== release.fileManifestSha256) throw new Error("BROWSER_PREREQUISITE_EVIDENCE_IDENTITY_CHANGED");
          const destination = join(output, name); await copyFile(join(directory, name), destination);
          artifacts.push({ path: destination, sha256: createHash("sha256").update(bytes).digest("hex") });
          if (prerequisite === "legal-worker") {
            const control = "legal-production-worker-control-fixture.json", body = await readFile(join(directory, control));
            if (evidence.controlFixture?.sha256 !== createHash("sha256").update(body).digest("hex")) throw new Error("BROWSER_WORKER_CONTROL_EVIDENCE_CHANGED");
            await copyFile(join(directory, control), join(output, control));
            artifacts.push({ path: join(output, control), sha256: createHash("sha256").update(body).digest("hex") });
          }
        });
      }
    }
    passed = true;
    return required.includes("encrypted-recovery") ? { ERPX_ENGINEERING_EVIDENCE_DIR: output } : {};
  } finally {
    await writeFile(join(output, "receipt.json"), JSON.stringify({ version: 1, passed, run, sourceFingerprint: state.sourceFingerprint, softwareVersion: release.softwareVersion, fileManifestSha256: release.fileManifestSha256, suite: input.suite.id, file: input.wanted.file, title: input.wanted.title, project: input.wanted.project, required, artifacts }, null, 2) + "\n", { mode: 0o600 });
  }
}
