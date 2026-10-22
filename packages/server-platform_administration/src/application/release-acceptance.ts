import type { Clock, IdentifierSource } from "@erpx/server-shared/application/ports";
import { productionScope, legalProductionScope, allReleaseCriteria, criteriaForScope, type ReleaseScopeVersion, type GateState } from "@erpx/catalogue/production-scope";
import { releaseSubmitSchema, releaseReviewSchema, releaseRevokeSchema, scopeVersionSchema, inputBlockers, evidenceSchema, type ReleaseRow, type ReviewRow, type ReleaseInputs, type ReleaseSubmit, type ReleaseReview, type ReleaseRevoke } from "@erpx/server-platform_administration/domain/release-acceptance";
import { systemEvidenceInput, type SystemEvidenceInput } from "@erpx/server-platform_administration/domain/system-evidence";
import type { z } from "zod";
export type ReleaseCommand<Input = ReleaseSubmit | ReleaseReview | ReleaseRevoke | SystemEvidenceInput> = { key: string; operation: string; id: string; input: Input };
export type ReleaseRepository = {
  inputFacts(scope: ReleaseScopeVersion): Promise<{ revision: { revision: number } | null; setup: { version: number; status: string } | null; branches: { count: number } | null; currency: { compatible: boolean } | null }>;
  row(releaseId: string | undefined, scope: ReleaseScopeVersion): Promise<ReleaseRow | null>; latest(scope: ReleaseScopeVersion): Promise<{ id: string } | null>; reviews(row: ReleaseRow): Promise<ReviewRow[]>;
  replay(command: ReleaseCommand): Promise<Record<string, unknown> | null>;
  submit(command: ReleaseCommand<ReleaseSubmit>, facts: { id: string; now: number; current: ReleaseInputs; evidence: string; digest: string }): Promise<Record<string, unknown>>;
  review(command: ReleaseCommand<ReleaseReview>, row: ReleaseRow, facts: { permission: string; inputs: ReleaseInputs; scope: ReleaseScopeVersion; version: number; now: number }): Promise<Record<string, unknown>>;
  revoke(command: ReleaseCommand<ReleaseRevoke>, facts: { version: number; now: number }): Promise<Record<string, unknown>>;
  evidenceRow(id: string): Promise<{ version: number; revoked_at: number | null } | null>;
  prepareEvidence(command: ReleaseCommand<SystemEvidenceInput>, version: number): Promise<{ commit(now: number): Promise<Record<string, unknown>> }>;
};
export function createReleaseAcceptanceApplication(ports: {
  companyId: string; companyScope: boolean; samePerson(userId: string): boolean; authority(permission: string): Promise<boolean>; softwareVersion: string; clock: Clock; identifiers: IdentifierSource; hash(value: string): string;
  repository: ReleaseRepository; error(status: number, code: string, message: string): Error;
}) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function requireReviewer(permission: string) { if (!ports.companyScope || !await ports.authority(permission)) return fail(403, "RELEASE_REVIEW_AUTHORITY_REQUIRED", "Use an active Company assignment with the required review permission."); }
  async function currentReleaseInputs(scope: ReleaseScopeVersion = productionScope.version): Promise<ReleaseInputs> {
    const { revision, setup, branches, currency } = await ports.repository.inputFacts(scope);
    return { scopeVersion: scope, softwareVersion: ports.softwareVersion, configurationRevision: Number(revision?.revision ?? 0), setupVersion: Number(setup?.version ?? 0), setupAccepted: setup?.status === "accepted", establishmentCount: Number(branches?.count ?? 0), currencyCompatible: scope === legalProductionScope.version || currency?.compatible === true };
  }
  async function companyReleaseStatus(releaseId?: string, scope: ReleaseScopeVersion = productionScope.version) {
    const row = await ports.repository.row(releaseId, scope), selectedScope = row ? scopeVersionSchema.parse(row.scope_version) : scope, inputs = await currentReleaseInputs(selectedScope), criteria = criteriaForScope(selectedScope);
    if (!row) return { state: "pending" as GateState, inputs, release: null, gates: criteria.map(criterion => ({ ...criterion, state: "pending" as GateState, reviewerId: null as string | null, reason: null as string | null, reviewedAt: null as number | null })), blockers: ["No release has been submitted for independent review.", ...(inputs.softwareVersion === "unbuilt" ? ["Use an isolated production release build for release acceptance."] : []), ...(!inputs.setupAccepted ? ["Company setup needs current independent acceptance."] : []), ...(selectedScope === productionScope.version && inputs.establishmentCount !== 1 ? ["This release requires exactly one active establishment."] : []), ...(!inputs.currencyCompatible ? ["Company settings must use THB and Thai payroll jurisdiction."] : [])] };
    const latest = releaseId ? await ports.repository.latest(selectedScope) : row, reviews = await ports.repository.reviews(row);
    const gates = criteria.map(criterion => { const review = reviews.find(entry => entry.criterion_id === criterion.id); return { ...criterion, state: review?.authority_current ? review.state : "pending" as GateState, reviewerId: review?.reviewer_id ?? null, reason: review?.reason ?? null, reviewedAt: review?.reviewed_at ?? null }; });
    const blockers = [...(latest?.id === row.id ? [] : ["A newer release superseded this acceptance."]), ...inputBlockers(row, inputs), ...gates.filter(gate => gate.state !== "passed").map(gate => `${gate.label}: independent acceptance pending.`)];
    return { state: (blockers.length ? "pending" : "passed") as GateState, inputs, release: { id: row.id, version: Number(row.version), requestedBy: row.requested_by, reason: row.reason, scopeVersion: row.scope_version, softwareVersion: row.software_version, configurationRevision: Number(row.configuration_revision), setupVersion: Number(row.setup_version), evidenceSha256: row.evidence_sha256, evidence: JSON.parse(row.evidence_json) as z.infer<typeof evidenceSchema>[], revokedAt: row.revoked_at, createdAt: row.created_at }, gates, blockers };
  }
  async function releaseAcceptance(releaseId?: string, scope: ReleaseScopeVersion = productionScope.version) {
    if (!ports.companyScope) return fail(403, "RELEASE_REVIEW_AUTHORITY_REQUIRED", "Open the Company workspace.");
    const result = await companyReleaseStatus(releaseId, scope), permissions = [...new Set(criteriaForScope(result.inputs.scopeVersion).map(row => row.permission))], allowed = await Promise.all(permissions.map(permission => ports.authority(permission)));
    if (!allowed.some(Boolean)) return fail(403, "RELEASE_REVIEW_AUTHORITY_REQUIRED", "A release review assignment is required.");
    if (releaseId && !result.release) return fail(404, "RELEASE_NOT_FOUND", "Release not found.");
    return result;
  }
  async function submitReleaseAcceptance(raw: unknown, key: string) {
    await requireReviewer("organization.manage"); const input = releaseSubmitSchema.parse(raw), command = { key, operation: "release.acceptance.submit", id: ports.companyId, input }, prior = await ports.repository.replay(command);
    if (prior) return prior;
    const current = await currentReleaseInputs(input.scopeVersion);
    if (current.softwareVersion === "unbuilt") return fail(409, "RELEASE_BUILD_REQUIRED", "Submit from an isolated production release build.");
    if (input.softwareVersion !== current.softwareVersion || input.expectedConfigurationRevision !== current.configurationRevision || input.expectedSetupVersion !== current.setupVersion || !current.setupAccepted || (input.scopeVersion === productionScope.version && current.establishmentCount !== 1) || !current.currencyCompatible) return fail(409, "RELEASE_INPUTS_CHANGED", "Review the current build, Company setup and configuration before submitting.");
    const id = `release-${ports.identifiers.next()}`, now = ports.clock.now(), evidence = JSON.stringify(input.evidence), digest = ports.hash(evidence);
    return ports.repository.submit(command, { id, now, current, evidence, digest });
  }
  async function reviewReleaseAcceptance(id: string, raw: unknown, key: string) {
    const input = releaseReviewSchema.parse(raw), criterion = allReleaseCriteria.find(row => row.id === input.criterionId)!;
    await requireReviewer(criterion.permission); const command = { key, operation: "release.acceptance.review", id, input }, prior = await ports.repository.replay(command);
    if (prior) return prior;
    const row = await ports.repository.row(id, productionScope.version); if (!row) return fail(404, "RELEASE_NOT_FOUND", "Release not found.");
    const scope = scopeVersionSchema.parse(row.scope_version);
    if (!criteriaForScope(scope).some(required => required.id === input.criterionId)) return fail(422, "RELEASE_CRITERION_SCOPE", "Choose a criterion belonging to this release scope.");
    if ((await ports.repository.latest(scope))?.id !== id) return fail(409, "RELEASE_SUPERSEDED", "Review the latest release submission.");
    if (ports.samePerson(row.requested_by)) return fail(403, "INDEPENDENT_REVIEW_REQUIRED", "A different person must review the submitted release evidence.");
    if (Number(row.version) !== input.expectedVersion) return fail(409, "STALE_VERSION", "Reload the release before reviewing it.");
    const inputs = await currentReleaseInputs(scope);
    if (inputBlockers(row, inputs).length) return fail(409, "RELEASE_INPUTS_CHANGED", "Submit a new release after build, scope or configuration changes.");
    return ports.repository.review(command, row, { permission: criterion.permission, inputs, scope, version: input.expectedVersion + 1, now: ports.clock.now() });
  }
  async function revokeReleaseAcceptance(id: string, raw: unknown, key: string) {
    await requireReviewer("organization.manage"); const input = releaseRevokeSchema.parse(raw), command = { key, operation: "release.acceptance.revoke", id, input }, prior = await ports.repository.replay(command);
    if (prior) return prior;
    return ports.repository.revoke(command, { version: input.expectedVersion + 1, now: ports.clock.now() });
  }
  async function bindReleaseEvidence(id: string, raw: unknown, key: string) {
    await requireReviewer("organization.manage"); const input = systemEvidenceInput.parse(raw), command = { key, operation: "release.acceptance.evidence.bind", id, input }, prior = await ports.repository.replay(command);
    if (prior) return prior;
    const row = await ports.repository.evidenceRow(id);
    if (!row || row.revoked_at !== null) return fail(404, "RELEASE_NOT_FOUND", "Release not found.");
    if (Number(row.version) !== input.expectedVersion) return fail(409, "STALE_VERSION", "Reload the release before binding its sanitized evidence.");
    const prepared = await ports.repository.prepareEvidence(command, input.expectedVersion + 1);
    return prepared.commit(ports.clock.now());
  }
  return { currentReleaseInputs, companyReleaseStatus, releaseAcceptance, submitReleaseAcceptance, reviewReleaseAcceptance, revokeReleaseAcceptance, bindReleaseEvidence };
}
