import { expect, it } from "vitest";
import { productionScope, legalProductionScope, criteriaForScope } from "../../src/data/production-scope";
import { createReleaseAcceptanceApplication, type ReleaseRepository } from "../../src/server/modules/platform_administration/application/release-acceptance";
import type { ReleaseRow } from "../../src/server/modules/platform_administration/domain/release-acceptance";
import { createSystemEvidenceApplication, type SystemEvidenceTransaction } from "../../src/server/modules/platform_administration/application/system-evidence";
import { createEngineeringVerificationApplication } from "../../src/server/modules/platform_administration/application/engineering-verification";
import { isEngineeringVerificationOrigin, type EngineeringAttestationFacts } from "../../src/server/modules/platform_administration/domain/engineering-verification";
import { createPlatformExecutiveApplication, type PlatformExecutiveRepository } from "../../src/server/modules/platform_administration/application/platform-executive";

function repository<T extends object>(operations: Partial<T>): T { return new Proxy(operations as T, { get(target, name) { if (name === "then") return undefined; if (name in target) return Reflect.get(target, name); return () => { throw new Error(`unexpected operation: ${String(name)}`); }; } }); }
const error = (_status: number, code: string) => new Error(code);
const release = (source: ReleaseRepository, overrides: Partial<Parameters<typeof createReleaseAcceptanceApplication>[0]> = {}) => createReleaseAcceptanceApplication({ companyId: "company", companyScope: true, samePerson: () => false, authority: async () => true, softwareVersion: "build", clock: { now: () => 100 }, identifiers: { next: () => "identifier" }, hash: () => "digest", repository: source, error, ...overrides });
const row: ReleaseRow = { id: "release", company_id: "company", scope_version: productionScope.version, software_version: "build", configuration_revision: 3, setup_version: 2, evidence_sha256: "digest", evidence_json: "[]", requested_by: "author", version: 4, revoked_at: null, reason: "Reviewed evidence", created_at: 90, updated_at: 90 };
const facts = { revision: { revision: 3 }, setup: { version: 2, status: "accepted" }, branches: { count: 1 }, currency: { compatible: true } };
const submit = { scopeVersion: productionScope.version, softwareVersion: "build", expectedConfigurationRevision: 3, expectedSetupVersion: 2, reason: "Candidate evidence", evidence: criteriaForScope(productionScope.version).map(criterion => ({ criterionId: criterion.id, summary: "Reviewed candidate evidence", files: [{ reference: "candidate/evidence", sha256: "a".repeat(64) }] })) };
const review = { expectedVersion: 4, criterionId: criteriaForScope(productionScope.version)[0].id, state: "passed", reason: "Independent review" };

it("a release replay returns its retained receipt before reading current inputs or allocating another release", async () => {
  const source = repository<ReleaseRepository>({ replay: async () => ({ id: "retained", version: 1 }) });
  expect(await release(source, { clock: { now: () => { throw new Error("unexpected clock"); } }, identifiers: { next: () => { throw new Error("unexpected identifier"); } } }).submitReleaseAcceptance(submit, "key")).toEqual({ id: "retained", version: 1 });
});
it("current Company authority is required even when a release receipt already exists", async () => {
  await expect(release(repository<ReleaseRepository>({}), { authority: async () => false }).submitReleaseAcceptance(submit, "key")).rejects.toThrow("RELEASE_REVIEW_AUTHORITY_REQUIRED");
});
it("release submit rejects duplicate or missing catalogue evidence before replay", async () => {
  await expect(release(repository<ReleaseRepository>({})).submitReleaseAcceptance({ ...submit, evidence: [submit.evidence[0], submit.evidence[0]] }, "key")).rejects.toThrow();
});
it("a changed setup/configuration cannot allocate a new release or consult the command clock", async () => {
  const source = repository<ReleaseRepository>({ replay: async () => null, inputFacts: async () => ({ ...facts, revision: { revision: 4 } }) });
  await expect(release(source, { clock: { now: () => { throw new Error("unexpected clock"); } } }).submitReleaseAcceptance(submit, "key")).rejects.toThrow("RELEASE_INPUTS_CHANGED");
});
it("a requester cannot approve their own retained release even with current review authority", async () => {
  const source = repository<ReleaseRepository>({ replay: async () => null, row: async () => row, latest: async () => ({ id: row.id }) });
  await expect(release(source, { samePerson: id => id === "author" }).reviewReleaseAcceptance(row.id, review, "key")).rejects.toThrow("INDEPENDENT_REVIEW_REQUIRED");
});
it("a superseded release is refused before independent review or changing any gate", async () => {
  const source = repository<ReleaseRepository>({ replay: async () => null, row: async () => row, latest: async () => ({ id: "newer" }) });
  await expect(release(source).reviewReleaseAcceptance(row.id, review, "key")).rejects.toThrow("RELEASE_SUPERSEDED");
});
it("revoking a reviewer's authority makes its passed gate pending while retaining review evidence", async () => {
  const source = repository<ReleaseRepository>({ row: async () => row, inputFacts: async () => facts, reviews: async () => [{ criterion_id: review.criterionId, state: "passed", reviewer_id: "revoked-reviewer", reason: "Prior independent review", reviewed_at: 80, authority_current: false }] });
  const status = await release(source).companyReleaseStatus();
  expect(status.state).toBe("pending");
  expect(status.gates.find(gate => gate.id === review.criterionId)).toMatchObject({ state: "pending", reviewerId: "revoked-reviewer", reason: "Prior independent review", reviewedAt: 80 });
});
it("Legal release scope keeps its existing independent inputs without imposing hotel establishment/currency rules", async () => {
  const source = repository<ReleaseRepository>({ inputFacts: async () => ({ ...facts, branches: { count: 3 }, currency: { compatible: false } }) });
  expect(await release(source).currentReleaseInputs(legalProductionScope.version)).toMatchObject({ establishmentCount: 3, currencyCompatible: true, setupAccepted: true });
});
it("release evidence is prepared before the commit timestamp and is handed to a single transaction", async () => {
  const order: string[] = [];
  const source = repository<ReleaseRepository>({ replay: async () => null, evidenceRow: async () => ({ version: 4, revoked_at: null }), prepareEvidence: async (_command, version) => { expect(version).toBe(5); order.push("prepare"); return { commit: async now => { order.push("commit"); expect(now).toBe(100); return { id: row.id, version }; } }; } });
  expect(await release(source, { clock: { now: () => { order.push("clock"); return 100; } } }).bindReleaseEvidence(row.id, { expectedVersion: 4, attachmentSetIds: ["set"], reason: "Supplemental evidence" }, "key")).toEqual({ id: row.id, version: 5 });
  expect(order).toEqual(["prepare", "clock", "commit"]);
});

const systemInput = { expectedVersion: 2, attachmentSetIds: ["set"], reason: "Sanitized evidence" };
it("a stale IAM evidence owner cannot prepare files or advance a version", async () => {
  const application = createSystemEvidenceApplication({ authorizeCompanyWithMfa: async () => {}, clock: { now: () => 100 }, error, run: async (_command, permission, action) => { expect(permission).toBe("iam.manage"); return action(repository<SystemEvidenceTransaction>({ lock: async () => ({ version: 3, status: "pending" }) })); } });
  await expect(application("iam_review", "review", systemInput, "key")).rejects.toThrow("STALE_VERSION");
});
it("a failed attachment commit prevents its IAM/provisioning owner from advancing", async () => {
  const application = createSystemEvidenceApplication({ authorizeCompanyWithMfa: async () => {}, clock: { now: () => { throw new Error("unexpected clock"); } }, error, run: async (_command, _permission, action) => action(repository<SystemEvidenceTransaction>({ lock: async () => ({ version: 2, status: "pending" }), prepare: async () => ({ manifest: [], commit: async () => { throw new Error("file commit failed"); } }) })) });
  await expect(application("people_provision_operation", "provision", systemInput, "key")).rejects.toThrow("file commit failed");
});
it("sanitized provisioning evidence leaves the business status pending and advances only after its file commit", async () => {
  const order: string[] = [];
  const application = createSystemEvidenceApplication({ authorizeCompanyWithMfa: async () => { order.push("mfa"); }, clock: { now: () => { order.push("clock"); return 100; } }, error, run: async (command, permission, action) => { expect(permission).toBe("iam.directory.apply"); return action({ lock: async () => ({ version: 2, status: "pending" }), prepare: async (_command, binding) => { expect(binding).toEqual({ version: 3, purpose: "sanitized_provisioning_evidence", bindingId: "provision:evidence:3" }); return { manifest: [], commit: async () => { order.push("files"); } }; }, advance: async (next, now) => { expect(next).toBe(command); expect(now).toBe(100); order.push("advance"); } }); } });
  expect(await application("people_provision_operation", "provision", systemInput, "key")).toEqual({ id: "provision", version: 3, status: "pending", manifest: [] });
  expect(order).toEqual(["mfa", "files", "clock", "advance"]);
});

const run = "abcdef123456", fingerprint = "a".repeat(64), digest = "b".repeat(64), directory = `/repo/.runtime/browser/${run}`;
const attestation = { scope: "isolated-engineering" as const, run, sourceFingerprint: fingerprint, softwareVersion: `release-${fingerprint}`, fileManifestSha256: digest };
const attestationFacts: EngineeringAttestationFacts = { directory, release: `${directory}/release`, attestationPath: `${directory}/engineering-attestation.json`, manifestPath: `${directory}/release/release-files.json`, attestation, releaseInfo: attestation, digest };
const environment = { ERPX_ENGINEERING_VERIFICATION_RUN: run, ERPX_ENVIRONMENT: "local", PORT: "3104", ERPX_BROWSER_SIMULATION_PORT: "3104", ERPX_DATA_DIR: directory };
it("engineering verification requires the exact isolated port before reading any attestation", () => {
  const application = createEngineeringVerificationApplication({ artifactVersion: attestation.softwareVersion, clock: { now: () => 100 }, readAttestation: () => { throw new Error("unexpected read"); } });
  expect(application.engineeringVerificationContext({ ...environment, PORT: "3101" })).toBeNull();
  expect(application.engineeringVerificationContext({ ...environment, ERPX_ENVIRONMENT: "production" })).toBeNull();
});
it("tampered manifests, escaped release paths and malformed attestations cannot enable engineering guards", () => {
  for (const facts of [{ ...attestationFacts, digest: "c".repeat(64) }, { ...attestationFacts, release: "/other/release" }, { ...attestationFacts, attestation: null }]) {
    const application = createEngineeringVerificationApplication({ artifactVersion: attestation.softwareVersion, clock: { now: () => 100 }, readAttestation: () => facts as EngineeringAttestationFacts });
    expect(application.engineeringVerificationContext(environment)).toBeNull();
  }
});
it("an attested candidate permits only its local HTTP verification origins", () => {
  const application = createEngineeringVerificationApplication({ artifactVersion: attestation.softwareVersion, clock: { now: () => 100 }, readAttestation: () => attestationFacts });
  const context = application.engineeringVerificationContext(environment);
  expect(context).toEqual(attestation);
  expect(isEngineeringVerificationOrigin(new URL("http://dev-cohort.localhost:3104/"), context)).toBe(true);
  for (const url of ["https://localhost:3104/", "http://localhost:3100/", "http://example.test:3104/"]) expect(isEngineeringVerificationOrigin(new URL(url), context)).toBe(false);
});

it("Platform insights preserve exact large integer strings and keep private sources behind current permission", async () => {
  const source = repository<PlatformExecutiveRepository>({ failed: async () => [{ label: "2026-10", value: "9007199254740992" }], abnormal: async () => [] });
  const application = createPlatformExecutiveApplication({ canRead: () => true, clock: { now: () => 100 }, hash: async () => "digest", repository: source, error });
  expect((await application.platformExecutiveSnapshot({ from: "2026-01-01", to: "2026-10-01" })).metrics[0].total).toBe("9007199254740992");
  const denied = createPlatformExecutiveApplication({ canRead: () => false, clock: { now: () => { throw new Error("unexpected clock"); } }, hash: async () => "digest", repository: repository<PlatformExecutiveRepository>({}), error });
  await expect(denied.platformExecutiveSnapshot()).rejects.toThrow("PLATFORM_EXECUTIVE_FORBIDDEN");
});
