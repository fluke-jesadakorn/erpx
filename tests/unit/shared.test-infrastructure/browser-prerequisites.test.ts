import { describe, expect, it } from "vitest";
import { browserCasePrerequisites, browserPrerequisites, assertCompiledLegalCase, assertRecoveryEvidence, withPausedBrowserJobs } from "../../../scripts/testing/browser-prerequisites";
import { clonedFixtureMetadata, isCaseExecutionEvidence, isClonedFixtureDocument, isolatedAiCredentialEnvironment, localBrowserActorEnvironment, browserFrontOrigin, moduleBrowserHoldingDisplayName, moduleBrowserReleaseVerificationCompanyIds } from "../../../scripts/testing/browser-fixture-metadata";
import { attachmentToolEnvironment } from "../../../scripts/testing/runtime-tooling";
import { browserChildEnvironment } from "../../../scripts/testing/runner-environment";
import type { BrowserCase } from "../../../scripts/testing/browser-results";
import { prepareUatCaseEvidence, retainBrowserFixtureEvidence } from "../../../scripts/testing/browser-evidence";
import { appendFile, chmod, mkdir, mkdtemp, readFile, realpath, rm, stat, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

const seedRun = "abcdef123456", run = "123456abcdef", sourceFingerprint = "a".repeat(64), fileManifestSha256 = "b".repeat(64);
const release = { id: "candidate-release", node: "26.11.1", sourceFingerprint, softwareVersion: `release-${sourceFingerprint}`, fileManifestSha256 };
const legal = { fixture: "legal", lane: "uat" } as const;
const wanted: BrowserCase = { file: "tests/uat/legal/legal-uat.spec.ts", title: "UAT-22: unavailable Legal AI leaves untrusted evidence inert and a persisted manual document workflow available", project: "desktop", expectedStatus: "passed" };
function state() { return { run, prefix: `erpx_browser_${run}`, origin: "http://127.0.0.1:3104", port: 3104, ready: true, sourceFingerprint, softwareVersion: release.softwareVersion, release: { id: release.id, fileManifestSha256 }, fixture: { legal: {} } }; }

describe("case clone identities", () => {
  it("rebinds run metadata and private paths while preserving seeded source identities", () => {
    const oldDirectory = `/repo/.runtime/browser/${seedRun}`, directory = `/repo/.runtime/browser/${run}`;
    const original = { run: seedRun, environment: { run: seedRun }, source: { id: `demo-transit-${seedRun}`, email: `candidate-${seedRun}@simulation.test`, checksum: "immutable" }, path: `${oldDirectory}/signed.pdf`, reset: { commands: [`bun run demo:destroy -- ${seedRun}`] }, acceptance: { browserCommand: `ERPX_DEMO_RUN=${seedRun} bun run demo:verify:browser` } };
    const copy = clonedFixtureMetadata(original, seedRun, run, oldDirectory, directory);
    expect(copy).toMatchObject({ run, seedRun, environment: { run, seedRun }, source: original.source, path: `${directory}/signed.pdf`, reset: { commands: [`bun run demo:destroy -- ${run}`] }, acceptance: { browserCommand: `ERPX_DEMO_RUN=${run} bun run demo:verify:browser` } });
    expect(original.run).toBe(seedRun);
  });
  it("does not allow execution receipts to be cloned as fixture metadata", () => {
    for (const file of ["legal-production-worker-evidence.json", "legal-ai-engineering-evidence.json", "legal-production-worker-control-fixture.json", "jobs-pause.json", "engineering-attestation.json", "legal-destruction-abc-fault.json", "route-results.jsonl"]) {
      expect(isCaseExecutionEvidence(file)).toBe(true); expect(isClonedFixtureDocument(file)).toBe(false);
    }
    expect(isClonedFixtureDocument("engineering-money-people.json")).toBe(true);
    expect(isCaseExecutionEvidence("attachment-baseline.json")).toBe(false);
    expect(() => clonedFixtureMetadata({}, run, run, "/old", "/new")).toThrow("EXACT_DISTINCT_BROWSER_RUNS_REQUIRED");
  });
  it("provides a private encryption key per cohort without a provider credential", async () => {
    const environment = isolatedAiCredentialEnvironment("c".repeat(64)), key = Buffer.from(JSON.parse(environment.AI_CREDENTIAL_KEYS)[environment.AI_CREDENTIAL_KEY_ID], "base64");
    expect(key).toHaveLength(32); expect(environment).not.toHaveProperty("AI_API_KEY");
    expect(environment).toEqual(isolatedAiCredentialEnvironment("c".repeat(64)));
    expect(environment).not.toEqual(isolatedAiCredentialEnvironment("d".repeat(64)));
    const cryptoKey = await crypto.subtle.importKey("raw", key, "AES-GCM", false, ["encrypt", "decrypt"]), iv = new Uint8Array(12), message = new TextEncoder().encode("declared fixture secret");
    const encrypted = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, cryptoKey, message);
    expect(Buffer.from(await crypto.subtle.decrypt({ name: "AES-GCM", iv }, cryptoKey, encrypted)).toString()).toBe("declared fixture secret");
  });
});

describe("private fixture evidence retention", () => {
  it("retains private UAT observations after disposing auth files and isolates each case", async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), "erpx-uat-evidence-")));
    const base = join(root, ".scratch/dev/test-suites/browser/2026-10-06T00-00-00-000Z"), output = join(base, run), otherOutput = join(base, seedRun);
    const auth = join(root, ".runtime/browser", run, "uat-auth");
    try {
      await mkdir(output, { recursive: true }); await mkdir(otherOutput); await mkdir(auth, { recursive: true });
      const path = await prepareUatCaseEvidence(root, run, output), otherPath = await prepareUatCaseEvidence(root, seedRun, otherOutput);
      await appendFile(path, '{"route":"/people","status":403}\n');
      await writeFile(join(auth, "owner.json"), '{"cookies":[]}');
      await rm(join(root, ".runtime/browser", run), { recursive: true });
      expect(path).toBe(join(output, "route-results.jsonl"));
      expect(await readFile(path, "utf8")).toBe('{"route":"/people","status":403}\n');
      expect(await readFile(otherPath, "utf8")).toBe("");
      expect((await stat(path)).mode & 0o777).toBe(0o600);
      expect((await stat(output)).mode & 0o777).toBe(0o700);
      const environment = browserChildEnvironment({ ERPX_UAT_EVIDENCE_FILE: "/outside/stale.jsonl", ERPX_UAT_AUTH_DIR: "/outside/auth" }, { ERPX_UAT_EVIDENCE_FILE: path });
      expect(environment.ERPX_UAT_EVIDENCE_FILE).toBe(path);
      expect(environment).not.toHaveProperty("ERPX_UAT_AUTH_DIR");
    } finally { await rm(root, { recursive: true, force: true }); }
  });
  it("refuses inherited observations, another case, templates and symlink destinations", async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), "erpx-uat-evidence-")));
    const base = join(root, ".scratch/dev/test-suites/browser/2026-10-06T00-00-00-000Z"), output = join(base, run), template = join(base, `template-${run}`);
    try {
      await mkdir(output, { recursive: true }); await mkdir(template);
      await expect(prepareUatCaseEvidence(root, seedRun, output)).rejects.toThrow("OWNED_UAT_CASE_EVIDENCE_DIRECTORY_REQUIRED");
      await expect(prepareUatCaseEvidence(root, run, template)).rejects.toThrow("OWNED_UAT_CASE_EVIDENCE_DIRECTORY_REQUIRED");
      await writeFile(join(root, "outside.jsonl"), '"original evidence"');
      await symlink(join(root, "outside.jsonl"), join(output, "route-results.jsonl"));
      await expect(prepareUatCaseEvidence(root, run, output)).rejects.toMatchObject({ code: "EEXIST" });
      expect(await readFile(join(root, "outside.jsonl"), "utf8")).toBe('"original evidence"');
      expect(browserChildEnvironment({ ERPX_UAT_EVIDENCE_FILE: "/outside/stale.jsonl" })).not.toHaveProperty("ERPX_UAT_EVIDENCE_FILE");
    } finally { await rm(root, { recursive: true, force: true }); }
  });
  it("retains preparation snapshots even before verification succeeds and keeps every copy private", async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), "erpx-browser-evidence-")));
    const source = join(root, ".runtime/browser", run, "acme-organization"), output = join(root, ".scratch/dev/test-suites/browser/2026-10-06T00-00-00-000Z", `template-${run}`);
    try {
      await mkdir(join(source, "before-123"), { recursive: true }); await mkdir(output, { recursive: true });
      await writeFile(join(source, "before-123/acme.json"), '{"original":"retained"}');
      await writeFile(join(source, "before-123/plan.json"), '[{"fixture":"fictional"}]');
      await retainBrowserFixtureEvidence(root, run, output);
      await writeFile(join(source, "verification.json"), '{"issues":[]}');
      await chmod(join(output, "acme-organization/before-123/acme.json"), 0o644);
      await retainBrowserFixtureEvidence(root, run, output);
      await rm(join(root, ".runtime/browser", run), { recursive: true });
      const saved = join(output, "acme-organization");
      expect(await readFile(join(saved, "before-123/acme.json"), "utf8")).toBe('{"original":"retained"}');
      expect(await readFile(join(saved, "before-123/plan.json"), "utf8")).toBe('[{"fixture":"fictional"}]');
      expect(await readFile(join(saved, "verification.json"), "utf8")).toBe('{"issues":[]}');
      for (const directory of [saved, join(saved, "before-123")]) expect((await stat(directory)).mode & 0o777).toBe(0o700);
      for (const file of ["before-123/acme.json", "before-123/plan.json", "verification.json"]) expect((await stat(join(saved, file))).mode & 0o777).toBe(0o600);
    } finally { await rm(root, { recursive: true, force: true }); }
  });
  it("refuses another cohort destination and never follows an undeclared evidence symlink", async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), "erpx-browser-evidence-")));
    const source = join(root, ".runtime/browser", run, "acme-organization"), output = join(root, ".scratch/dev/test-suites/browser/2026-10-06T00-00-00-000Z", run);
    try {
      await mkdir(source, { recursive: true }); await mkdir(output, { recursive: true });
      await writeFile(join(root, "outside.json"), '"must not copy"');
      await symlink(join(root, "outside.json"), join(source, "verification.json"));
      await expect(retainBrowserFixtureEvidence(root, seedRun, output)).rejects.toThrow("OWNED_BROWSER_EVIDENCE_DIRECTORY_REQUIRED");
      await expect(retainBrowserFixtureEvidence(root, run, output)).rejects.toThrow("DECLARED_BROWSER_FIXTURE_EVIDENCE_REQUIRED");
      await expect(readFile(join(output, "acme-organization/verification.json"))).rejects.toMatchObject({ code: "ENOENT" });
    } finally { await rm(root, { recursive: true, force: true }); }
  });
});

describe("explicit prerequisite selection", () => {
  it("requires the mapped AI prerequisite and refuses omissions, unknown values and wrong cases", () => {
    expect(browserCasePrerequisites(legal, { ...wanted, annotations: [browserPrerequisites("legal-ai").annotation] })).toEqual(["legal-ai"]);
    expect(() => browserCasePrerequisites(legal, wanted)).toThrow("BROWSER_CASE_PREREQUISITE_MAPPING_REQUIRED");
    expect(() => browserCasePrerequisites(legal, { ...wanted, annotations: [browserPrerequisites().annotation] })).toThrow("BROWSER_CASE_PREREQUISITE_MISMATCH");
    expect(() => browserCasePrerequisites(legal, { ...wanted, annotations: [{ type: "erpx-browser-prerequisites", description: '["unknown"]' }] })).toThrow("INVALID_BROWSER_PREREQUISITES");
    expect(() => browserCasePrerequisites({ fixture: "expense", lane: "e2e" }, { ...wanted, annotations: [browserPrerequisites("legal-ai").annotation] })).toThrow("BROWSER_CASE_PREREQUISITE_MISMATCH");
    expect(() => browserCasePrerequisites(legal, { ...wanted, annotations: [browserPrerequisites("legal-ai").annotation, browserPrerequisites("legal-ai").annotation] })).toThrow("BROWSER_CASE_PREREQUISITE_MAPPING_REQUIRED");
  });
  it("keeps ordinary Legal and module cases independent of expensive prerequisites", () => {
    expect(browserCasePrerequisites(legal, { ...wanted, title: "UAT-01: ordinary owner review", annotations: [browserPrerequisites().annotation] })).toEqual([]);
    expect(browserCasePrerequisites({ fixture: "expense", lane: "e2e" }, wanted)).toEqual([]);
  });
  it("rejects another cohort or candidate before executing any producer", () => {
    expect(() => assertCompiledLegalCase(state(), release, run)).not.toThrow();
    for (const overrides of [{ run: seedRun }, { port: 3102 }, { ready: false }, { sourceFingerprint: "c".repeat(64) }, { release: { id: release.id, fileManifestSha256: "c".repeat(64) } }]) expect(() => assertCompiledLegalCase({ ...state(), ...overrides }, release, run)).toThrow("EXACT_COMPILED_LEGAL_CASE_REQUIRED");
  });
  it("requires real encrypted restore and all Legal facts from the exact candidate", () => {
    const evidence = { sourceFingerprint, softwareVersion: release.softwareVersion, fileManifestSha256, compiledCandidate: true, databaseCount: 8, tableCount: 600, restored: true, compatibleRollback: true, incompatibleRollbackRejected: true, ciphertextTamperingRejected: true, ciphertextSha256: "d".repeat(64), legal: { originalSha256: "e".repeat(64), originalBytesRetained: true, historyRetained: true, aclRetained: true, unassignedReaderRefused: true, overlappingHoldsRetained: true, queuedJobRetained: true, commandReplayWithoutDuplicateEffects: true } };
    expect(() => assertRecoveryEvidence(evidence, release)).not.toThrow();
    for (const overrides of [{ compiledCandidate: false }, { fileManifestSha256: "c".repeat(64) }, { restored: false }, { databaseCount: 6 }, { legal: { ...evidence.legal, queuedJobRetained: false } }]) expect(() => assertRecoveryEvidence({ ...evidence, ...overrides }, release)).toThrow("EXACT_CANDIDATE_RECOVERY_EVIDENCE_REQUIRED");
  });
  it("resumes real worker control after a producer failure and exposes both failures", async () => {
    const calls: string[] = [], failure = new Error("native producer failed");
    await expect(withPausedBrowserJobs(async mode => { calls.push(mode); }, async () => { calls.push("produce"); throw failure; })).rejects.toBe(failure);
    expect(calls).toEqual(["pause-jobs", "produce", "resume-jobs"]);
    await expect(withPausedBrowserJobs(async mode => { if (mode === "resume-jobs") throw new Error("resume failed"); }, async () => { throw failure; })).rejects.toMatchObject({ message: "BROWSER_PREREQUISITE_AND_RESUME_FAILED", errors: [failure, expect.any(Error)] });
  });
  it("uses explicitly installed scanner binaries and engine version in CI", () => {
    expect(attachmentToolEnvironment("/repo", { NODE_ENV: "test", ERPX_CLAMD_SOCKET: "/tmp/ci.sock", ERPX_CLAMAV_DATABASE_DIR: "/var/lib/clamav", ERPX_CLAMAV_VERSION: "1.4.3", ERPX_HEIF_CONVERT: "/usr/bin/heif-convert", ERPX_PDFTOPPM_BIN: "/usr/bin/pdftoppm" })).toMatchObject({ ERPX_CLAMD_SOCKET: "/tmp/ci.sock", ERPX_CLAMAV_DATABASE_DIR: "/var/lib/clamav", ERPX_CLAMAV_VERSION: "1.4.3", ERPX_HEIF_CONVERT: "/usr/bin/heif-convert", ERPX_PDFTOPPM_BIN: "/usr/bin/pdftoppm" });
  });
});

describe("browser child environment isolation", () => {
  it("removes live providers, storage, IAM and inherited cohort configuration", () => {
    const ambient = { ERPX_DEVELOPMENT_HOST_LEASE: "/outside/stale-edge.json", NODE_EXTRA_CA_CERTS: "/outside/stale-ca.pem", ERPX_MINIO_ENDPOINT: "https://storage.example.test", ERPX_MINIO_ACCESS_KEY: "live", AI_API_KEY: "live", AI_CREDENTIAL_KEYS: "live", RESEND_API_KEY: "live", STRIPE_SECRET_KEY: "live", POS_STRIPE_WEBHOOK_SECRET: "live", SITEMINDER_API_KEY: "live", ERPX_IAM_SECRET_PROFILES: "/live/iam.json", ERPX_PROCESS_ROLE: "jobs", ERPX_DATABASE_MANIFEST: "/live/databases.json", ERPX_MIGRATION_DATABASE_MANIFEST: "/live/migrations.json", ERPX_ENGINEERING_VERIFICATION_RUN: seedRun, ERPX_ENGINEERING_SOURCE_FINGERPRINT: "stale", ERPX_BROWSER_SIMULATION_RUN_ID: seedRun, ERPX_PUBLIC_ORIGIN: "https://console.example.test", ERPX_DATA_DIR: "/live/files", BETTER_AUTH_SECRET: "live", PGDATABASE: "erpx_private", ERPX_TEST_ADMIN_URL: "restricted-admin", ERPX_NODE_BIN: "/pinned/node", ERPX_CLAMD_SOCKET: "/installed/scanner.sock", PATH: "/installed/bin" };
    expect(browserChildEnvironment(ambient)).toEqual({ ERPX_TEST_ADMIN_URL: "restricted-admin", ERPX_NODE_BIN: "/pinned/node", ERPX_CLAMD_SOCKET: "/installed/scanner.sock", PATH: "/installed/bin", NODE_ENV: "development" });
    expect(ambient.ERPX_DATABASE_MANIFEST).toBe("/live/databases.json");
  });
  it("retains explicit exact-case metadata and recovery identity while filtering unsafe overrides", () => {
    const overrides = { ERPX_DEVELOPMENT_HOST_LEASE: `/repo/.runtime/browser/${run}/workspace-edge.json`, NODE_EXTRA_CA_CERTS: `/repo/.runtime/browser/${run}/workspace-tls/certificate.pem`, ERPX_DEMO_RUN: run, ERPX_BROWSER_SIMULATION_RUN_ID: run, ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3104", ERPX_DATA_DIR: `/repo/.runtime/browser/${run}`, ERPX_DATABASE_MANIFEST: `/repo/.runtime/browser/${run}/databases.json`, ERPX_ENGINEERING_SOURCE_FINGERPRINT: sourceFingerprint, ERPX_ENGINEERING_RELEASE_DIR: "/repo/.runtime/releases/candidate", ERPX_ENGINEERING_EVIDENCE_DIR: "/repo/.scratch/case/prerequisites", ERPX_PROCESS_ROLE: "jobs", BETTER_AUTH_SECRET: "isolated-cohort-secret", AI_API_KEY: "live", ERPX_MINIO_ENDPOINT: "live", STRIPE_SECRET_KEY: "live", ...isolatedAiCredentialEnvironment("c".repeat(64)) };
    const selected = browserChildEnvironment({ ERPX_DEMO_RUN: seedRun, ERPX_ENGINEERING_SOURCE_FINGERPRINT: "stale" }, overrides);
    const safe: Record<string, string | undefined> = { ...overrides, NODE_ENV: "development" }; delete safe.AI_API_KEY; delete safe.ERPX_MINIO_ENDPOINT; delete safe.STRIPE_SECRET_KEY;
    expect(selected).toEqual(safe);
    expect(selected).not.toHaveProperty("AI_API_KEY");
    expect(selected.AI_CREDENTIAL_KEYS).toBe(overrides.AI_CREDENTIAL_KEYS);
    for (const bad of [{ AI_CREDENTIAL_KEY_ID: "live", AI_CREDENTIAL_KEYS: overrides.AI_CREDENTIAL_KEYS }, { AI_CREDENTIAL_KEY_ID: "browser-v1", AI_CREDENTIAL_KEYS: '{}' }, { AI_CREDENTIAL_KEY_ID: "browser-v1", AI_CREDENTIAL_KEYS: 'null' }]) expect(() => browserChildEnvironment({}, bad)).toThrow("ISOLATED_BROWSER_AI_CREDENTIAL_RING_REQUIRED");
  });
  it("retains the deterministic booking sentinel only inside an explicit isolated sandbox", () => {
    const fixture = { ERPX_ENVIRONMENT: "local", ERPX_EMAIL_ADAPTER: "sandbox", ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3102", ERPX_DATA_DIR: `/repo/.runtime/browser/${run}`, RESEND_API_KEY: "deterministic-browser-booking", RESEND_FROM: "Fictional Hotel <hotel@example.test>" };
    expect(browserChildEnvironment({}, fixture)).toMatchObject(fixture);
    expect(browserChildEnvironment({}, { ...fixture, ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3100" })).not.toHaveProperty("RESEND_API_KEY");
    expect(browserChildEnvironment({}, { ...fixture, RESEND_API_KEY: "live" })).not.toHaveProperty("RESEND_API_KEY");
  });
  it("retains the Customer Portal invitation email sentinel only for its exact isolated sandbox", () => {
    const fixture = { ERPX_BROWSER_SIMULATION_CUSTOMER_PORTAL_INVITATIONS: "1", ERPX_ENVIRONMENT: "local", ERPX_EMAIL_ADAPTER: "sandbox", ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3102", ERPX_DATA_DIR: `/repo/.runtime/browser/${run}`, RESEND_API_KEY: "isolated-sandbox-no-provider-key", RESEND_FROM: "Fictional <no-reply@example.test>" };
    expect(browserChildEnvironment({}, fixture)).toMatchObject(fixture);
    expect(browserChildEnvironment({}, { ...fixture, ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3104" })).toMatchObject({ ...fixture, ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3104" });
    expect(browserChildEnvironment({}, { ...fixture, ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3100" })).not.toHaveProperty("RESEND_API_KEY");
    expect(browserChildEnvironment({}, { ...fixture, RESEND_API_KEY: "live" })).not.toHaveProperty("RESEND_API_KEY");
    expect(browserChildEnvironment({}, { ...fixture, ERPX_BROWSER_SIMULATION_CUSTOMER_PORTAL_INVITATIONS: undefined })).not.toHaveProperty("RESEND_API_KEY");
  });
});


describe("normal local browser actors and Front targets", () => {
  const actors = { hr: { email: "demo.bangkok.people.manager@erpx.dev", password: "Lara-ERPX-2026!" }, reader: { email: "demo.bangkok.operations.staff@erpx.dev", password: "Elena-ERPX-2026!" } };
  it("retains the normal Acme holding label only for declared staffing profiles", () => {
    for (const profile of ["local-seed", "staff-work"]) expect(moduleBrowserHoldingDisplayName(profile)).toBe("Acme Group");
    for (const profile of ["expense", "work-cards", "messaging", "services", "booking"]) expect(moduleBrowserHoldingDisplayName(profile)).toBe("Isolated Acme fixture");
  });
  it("enrolls only exact Companies created by compiled module fixtures", () => {
    for (const profile of ["local-seed", "staff-work", "expense", "messaging", "services"]) {
      expect(moduleBrowserReleaseVerificationCompanyIds(profile, { profile, releaseVerificationCompanyIds: ["company-a"] })).toEqual(["company-a"]);
      expect(() => moduleBrowserReleaseVerificationCompanyIds(profile, { profile })).toThrow("MODULE_RELEASE_COMPANY_COHORT_REQUIRED");
    }
    expect(moduleBrowserReleaseVerificationCompanyIds("work-cards", { profile: "work-cards", releaseVerificationCompanyIds: ["company-a", "company-b"] })).toEqual(["company-a", "company-b"]);
    expect(() => moduleBrowserReleaseVerificationCompanyIds("work-cards", { profile: "work-cards", releaseVerificationCompanyIds: ["company-a"] })).toThrow("MODULE_RELEASE_COMPANY_COHORT_REQUIRED");
    expect(() => moduleBrowserReleaseVerificationCompanyIds("expense", { profile: "expense", releaseVerificationCompanyIds: ["company-a", "company-a"] })).toThrow("MODULE_RELEASE_COMPANY_COHORT_REQUIRED");
    expect(moduleBrowserReleaseVerificationCompanyIds("booking", { releaseVerificationCompanyIds: ["company-a"] })).toBeNull();
    expect(moduleBrowserReleaseVerificationCompanyIds("general-company", { profile: "general-company", releaseVerificationCompanyIds: ["company-a"] })).toBeNull();
  });
  it("uses declared normal credentials only for the staffing profiles and rejects incomplete personas", () => {
    for (const profile of ["local-seed", "staff-work"]) expect(localBrowserActorEnvironment(profile, { actors })).toEqual({ ERPX_E2E_HR_EMAIL: actors.hr.email, ERPX_E2E_HR_PASSWORD: actors.hr.password, ERPX_E2E_READER_EMAIL: actors.reader.email, ERPX_E2E_READER_PASSWORD: actors.reader.password });
    expect(localBrowserActorEnvironment("expense", { actors })).toEqual({});
    expect(() => localBrowserActorEnvironment("local-seed", {})).toThrow("LOCAL_BROWSER_ACME_ACTORS_REQUIRED");
    expect(() => localBrowserActorEnvironment("staff-work", { actors: { ...actors, reader: { ...actors.reader, email: "unrelated@example.test" } } })).toThrow("LOCAL_BROWSER_ACME_ACTORS_REQUIRED");
  });
  it("keeps POS and PMS on each simulation port and refuses an inherited public Front override", () => {
    for (const port of [3102, 3104]) for (const app of ["pos", "pms"] as const) {
      const origin = `http://dev-${app}.localhost:${port}`;
      expect(browserFrontOrigin(`http://127.0.0.1:${port}`, app)).toBe(origin);
      expect(browserFrontOrigin(`http://localhost:${port}`, app, origin)).toBe(origin);
      expect(() => browserFrontOrigin(`http://localhost:${port}`, app, `https://dev-${app}.jesadakorn.com`)).toThrow("LOCAL_BROWSER_FRONT_ORIGIN_REQUIRED");
    }
    expect(() => browserFrontOrigin("http://localhost:3101", "pos")).toThrow("LOCAL_BROWSER_FRONT_ORIGIN_REQUIRED");
    expect(() => browserFrontOrigin("https://console-dev-erpx.jesadakorn.com", "pos")).toThrow("EXPLICIT_BROWSER_FRONT_ORIGIN_REQUIRED");
    expect(browserFrontOrigin("https://console-dev-erpx.jesadakorn.com", "pos", "https://dev-pos.jesadakorn.com")).toBe("https://dev-pos.jesadakorn.com");
  });
});

import {createHash} from "node:crypto";
import {relative} from "node:path";
import {captureUiBrowserEvidence,verifyUiBrowserEvidence} from "../../../scripts/testing/ui-evidence-witness";

/** File-contract fixtures only: no browser, native owner, process or database is invoked. */
async function withOriginalUiLeaves(action:(fixture:{root:string;output:string;cohort:string;reportPath:string;imagePath:string;observationPath:string;stateBytes:Buffer;capture:(localFrontApp?:"pms"|"pos")=>ReturnType<typeof captureUiBrowserEvidence>;writeObservation:(patch:Record<string,unknown>)=>Promise<void>})=>Promise<void>){
 const root=await realpath(await mkdtemp(join(tmpdir(),"erpx-original-ui-leaves-"))),output=join(root,".scratch/dev/test-suites/browser/2026-10-18T00-00-00-000Z",run),cohort=join(root,".runtime/browser",run),artifacts=join(output,"artifacts"),imagePath=join(artifacts,"native-view.png"),observationPath=join(artifacts,"observation.json"),reportPath=join(output,"result.json");
 try{
  await mkdir(artifacts,{recursive:true,mode:0o700});await mkdir(cohort,{recursive:true,mode:0o700});
  const stateBytes=Buffer.from(JSON.stringify({run,ready:true,sourceFingerprint,verificationFingerprint:sourceFingerprint,port:3104,origin:"http://127.0.0.1:3104",release:{id:"original-compiled",fileManifestSha256}}));await writeFile(join(cohort,"state.json"),stateBytes,{mode:0o600});
  const image=Buffer.from("Original PNG attachment byte contract; image decoding is a separate UI gate.");await writeFile(imagePath,image,{mode:0o600});
  const observation={schema:"erpx-ui-browser-observation/v1",result:"passed",run,sourceFingerprint,verificationFingerprint:sourceFingerprint,url:"http://127.0.0.1:3104/example",screenshot:{path:relative(root,imagePath),sha256:createHash("sha256").update(image).digest("hex"),bytes:image.length}};
  const report={suites:[{specs:[{file:"tests/uat/shared.ui/example.spec.ts",title:"original leaf capture",tests:[{projectName:"desktop",expectedStatus:"passed",status:"expected",results:[{status:"passed",attachments:[{name:"erpx-ui-observation",contentType:"application/json",path:observationPath},{name:"original-view",contentType:"image/png",path:imagePath}]}]}]}]}]};await writeFile(observationPath,JSON.stringify(observation),{mode:0o600});await writeFile(reportPath,JSON.stringify(report),{mode:0o600});
  await action({root,output,cohort,reportPath,imagePath,observationPath,stateBytes,capture:localFrontApp=>captureUiBrowserEvidence({root,run,reportPath,report,runtimeFingerprint:sourceFingerprint,verificationFingerprint:sourceFingerprint,...(localFrontApp?{localFrontApp}:{})}),writeObservation:async patch=>{await writeFile(observationPath,JSON.stringify({...observation,...patch}),{mode:0o600});}});
 }finally{await rm(root,{recursive:true,force:true});}
}

describe("original native UI evidence leaf capture",()=>{
 it("leaves ordinary non-UI cases opted out without touching the filesystem or changing receipt metadata",async()=>{
  expect(await captureUiBrowserEvidence({root:"/not-an-existing-root",run:"",reportPath:"",report:{suites:[]},runtimeFingerprint:sourceFingerprint,verificationFingerprint:sourceFingerprint})).toBeUndefined();
 });
 it("binds the exact original report, state, observation and PNG before the owned cohort disappears",async()=>{
  await withOriginalUiLeaves(async f=>{const witness=await f.capture();expect(witness).toBeDefined();expect(witness!.report.path).toBe(relative(f.root,f.reportPath));expect(witness!.observations).toHaveLength(1);expect(witness!.screenshots).toHaveLength(1);expect(await readFile(join(f.output,"state.json"))).toEqual(f.stateBytes);await verifyUiBrowserEvidence(f.root,witness!,true);await rm(f.cohort,{recursive:true});await verifyUiBrowserEvidence(f.root,witness!);expect(await readFile(join(f.output,"state.json"))).toEqual(f.stateBytes);});
 });
 it("detects post-capture report or image mutation before cleanup can certify the original case",async()=>{
  for(const leaf of ["reportPath","imagePath"] as const)await withOriginalUiLeaves(async f=>{const witness=await f.capture();await writeFile(f[leaf],"post-run replacement");await expect(verifyUiBrowserEvidence(f.root,witness!,true)).rejects.toThrow("UI_ORIGINAL_WITNESSED_LEAF_CHANGED");});
 });
 it("never overwrites a pre-existing retained state, and detects a different live jobs state",async()=>{
  await withOriginalUiLeaves(async f=>{await writeFile(join(f.output,"state.json"),"pre-existing evidence",{mode:0o600});await expect(f.capture()).rejects.toMatchObject({code:"EEXIST"});expect((await readFile(join(f.output,"state.json"))).toString()).toBe("pre-existing evidence");});
  await withOriginalUiLeaves(async f=>{const witness=await f.capture();await writeFile(join(f.cohort,"state.json"),"different live jobs state");await expect(verifyUiBrowserEvidence(f.root,witness!,true)).rejects.toThrow("UI_ORIGINAL_STATE_COPY_CHANGED");expect(await readFile(join(f.output,"state.json"))).toEqual(f.stateBytes);});
 });
 it("refuses public-development URLs instead of correlating them with a compiled3104 cohort",async()=>{
  await withOriginalUiLeaves(async f=>{await f.writeObservation({url:"https://people-dev-erpx.jesadakorn.com/example"});await expect(f.capture()).rejects.toThrow("UI_NATIVE_OBSERVATION_ORIGIN_REQUIRED");});
 });
 it("binds the exact local PMS host alias to its compiled loopback cohort and rejects unleased aliases",async()=>{
  await withOriginalUiLeaves(async f=>{await f.writeObservation({url:"http://dev-pms.localhost:3104/example"});const witness=await f.capture("pms");expect(witness?.localFrontApp).toBe("pms");await verifyUiBrowserEvidence(f.root,witness!);});
  await withOriginalUiLeaves(async f=>{await f.writeObservation({url:"http://dev-pms.localhost:3104/example"});await expect(f.capture()).rejects.toThrow("UI_NATIVE_OBSERVATION_ORIGIN_REQUIRED");});
  await withOriginalUiLeaves(async f=>{await f.writeObservation({url:"http://dev-pos.localhost:3104/example"});await expect(f.capture("pms")).rejects.toThrow("UI_NATIVE_OBSERVATION_ORIGIN_REQUIRED");});
  await withOriginalUiLeaves(async f=>{const state=JSON.parse(f.stateBytes.toString());state.productHosts=true;await writeFile(join(f.cohort,"state.json"),JSON.stringify(state),{mode:0o600});await f.writeObservation({url:"http://dev-pms.localhost:3104/example"});await expect(f.capture("pms")).rejects.toThrow("UI_NATIVE_LOCAL_FRONT_HOST_BINDING_REQUIRED");});
 });
 it("refuses a UI attachment outside its owned case artifacts or through a symlink",async()=>{
  await withOriginalUiLeaves(async f=>{const outside=join(f.root,"different-case.png"),bytes=Buffer.from("other owner bytes");await writeFile(outside,bytes);await f.writeObservation({screenshot:{path:relative(f.root,outside),sha256:createHash("sha256").update(bytes).digest("hex"),bytes:bytes.length}});await expect(f.capture()).rejects.toThrow("UI_NATIVE_LEAF_OUTSIDE_CASE");});
  await withOriginalUiLeaves(async f=>{const original=await readFile(f.imagePath);await rm(f.imagePath);const outside=join(f.root,"outside.png");await writeFile(outside,original);await symlink(outside,f.imagePath);await expect(f.capture()).rejects.toThrow("UI_NATIVE_REGULAR_BOUNDED_LEAF_REQUIRED");});
 });
});



it("does not downgrade an actual product-host state to unleased loopback UI evidence",async()=>{
 await withOriginalUiLeaves(async f=>{const state=JSON.parse(f.stateBytes.toString());state.productHosts=true;await writeFile(join(f.cohort,"state.json"),JSON.stringify(state),{mode:0o600});await expect(f.capture()).rejects.toThrow("UI_NATIVE_PRODUCT_HOST_LEASE_REQUIRED");});
});
