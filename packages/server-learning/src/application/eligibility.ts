import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { LearningCertificate, LearningBenefitInput, LearningBenefitElection, LearningRevocationInput } from "@erpx/server-learning/domain/eligibility";
import type { LearningCommandPorts, LearningMutation, LearningCommand } from "@erpx/server-learning/application/learning";

export function createLearningBenefitConsumption(ports: { error: LearningCommandPorts["error"]; today(): string; certificates(actor: WorkContext, input: LearningBenefitInput, today: string): Promise<LearningCertificate[]>; digest(value: string): Promise<string>; prepare(actor: WorkContext, input: LearningBenefitInput, certificate: LearningCertificate, digest: string): Promise<unknown> }) {
  return async (actor: WorkContext, input: LearningBenefitInput) => {
    const rows = await ports.certificates(actor, input, ports.today());
    if (!rows.length) throw ports.error({ status: 409, code: "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED", message: "Complete the required learning or reviewed waiver with a current certificate covering the benefit start date." });
    const certificate = rows[0], digest = await ports.digest(JSON.stringify(certificate));
    return { certificateId: certificate.id, native: await ports.prepare(actor, input, certificate, digest) };
  };
}
export function createValidateBenefitLearning(ports: { error: LearningCommandPorts["error"]; today(): string; requirement(actor: WorkContext, planId: string): Promise<{ requiredEligibilityCode: string | null } | null>; receipt(actor: WorkContext, election: LearningBenefitElection): Promise<{ certificate_id: string } | null>; guards(actor: WorkContext, key: string, certificateId: string, personId: string, code: string, effectiveDate: string): unknown; current(actor: WorkContext, certificateId: string, today: string): Promise<boolean> }) {
  return async (actor: WorkContext, row: LearningBenefitElection, key: string) => {
    const plan = await ports.requirement(actor, row.plan_id); if (!plan?.requiredEligibilityCode) return null;
    const receipt = await ports.receipt(actor, row);
    if (!row.learning_certificate_id || receipt?.certificate_id !== row.learning_certificate_id) throw ports.error({ status: 409, code: "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED", message: "The election must retain its original learning consumption receipt." });
    const guards = ports.guards(actor, key, row.learning_certificate_id, row.person_id, plan.requiredEligibilityCode, row.effective_from);
    if (!await ports.current(actor, row.learning_certificate_id, ports.today())) throw ports.error({ status: 409, code: "BENEFIT_LEARNING_CERTIFICATE_EXPIRED", message: "The retained learning certificate expired, was revoked, or its source changed. Renew eligibility before continuing the election." });
    return guards;
  };
}
export function createRevokeLearningCertificate(ports: Pick<LearningCommandPorts, "replay" | "error" | "now" | "commit"> & { authorize(actor: WorkContext): WorkContext; certificate(actor: WorkContext, id: string, input: LearningRevocationInput): Promise<LearningCertificate | null>; prepare(command: LearningCommand, input: LearningRevocationInput, certificate: LearningCertificate): Promise<LearningMutation> }) {
  return async (actor: WorkContext, id: string, input: LearningRevocationInput, key: string) => {
    actor = ports.authorize(actor); const operation = "learning.certificate.revoke", prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    const certificate = await ports.certificate(actor, id, input);
    if (!certificate) throw ports.error({ status: 404, code: "LEARNING_CERTIFICATE_NOT_FOUND", message: "Current in-scope learning certificate not found." });
    const command: LearningCommand = { actor, key, id, input, operation, now: ports.now(), version: input.expectedAssignmentVersion, result: { id, status: "revoked", version: input.expectedAssignmentVersion } };
    return ports.commit(command, await ports.prepare(command, input, certificate));
  };
}
