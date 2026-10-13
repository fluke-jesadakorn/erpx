import { describe, expect, it, vi } from "vitest";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { createChangeLearningCourse, createLearningAssignment, createChangeLearningAssignment, createLearningDetail, createLearningExecutiveEvidence, createLearningWorkspace, createLearningWork, type LearningCommandPorts, type LearningWorkspace } from "../../../src/server/modules/learning/application/learning";
import { createLearningBenefitConsumption, createValidateBenefitLearning, createRevokeLearningCertificate } from "../../../src/server/modules/learning/application/eligibility";
import { learningActionAllowed, learningWorkItems, type LearningAssignment } from "../../../src/server/modules/learning/domain/learning";
import type { LearningCertificate } from "../../../src/server/modules/learning/domain/eligibility";
import { createTalentCandidate, createTalentApplication, createTalentProposal, createTalentOffer, createChangeTalentRecord, createTalentDetail, createTalentExecutiveEvidence, createTalentWorkspace, createTalentWork, type TalentCommandPorts } from "../../../src/server/modules/talent/application/talent";
import { talentTransition, talentWorkItems } from "../../../src/server/modules/talent/domain/talent";

const actor = { userId: "learner", companyId: "company", holdingId: "holding", branchId: "branch", departmentId: "department", modules: ["learning", "talent"], role: "holding_owner" } as WorkContext;
const mutation = { native: "owned-transaction-work" };
function common(): LearningCommandPorts & TalentCommandPorts {
  return { authorize: vi.fn(), reviewer: vi.fn(), now: () => 100, today: () => "2026-10-06", error: violation => Object.assign(new Error(violation.message), violation), replay: vi.fn(async () => null), commit: vi.fn(async command => command.result) };
}
const assignment: LearningAssignment & { required_for: string | null } = { id: "assignment", version: 4, status: "in_progress", learner_id: "learner", created_by: "reviewer", updated_at: 1, learner_eligible: 1, attendance: null, score_bps: null, result_evidence: null, required_for: "safety", course_title: "Safety", learner_name: "Learner", due_date: "2026-10-10" };
const workspace: LearningWorkspace = { courses: [], assignments: [assignment], people: [], actorId: "learner", canWrite: true, canReview: true };
const complete = { action: "complete", expectedVersion: 4, reason: "Completed safely", attendance: "attended", scoreBps: 9000, evidence: "Certificate reference", attachmentSetIds: ["evidence-set"] };
const certificate: LearningCertificate = { id: "certificate", handoff_id: "handoff", company_id: "company", person_id: "person", assignment_id: "assignment", source_assignment_version: 5, eligibility_code: "safety", valid_from: "2026-10-01", expires_on: null, evidence_ref: "evidence" };
const election = { id: "election", person_id: "person", learning_certificate_id: "certificate", effective_from: "2026-10-06", plan_id: "plan" };

describe("Learning lifecycle with injected owner ports", () => {
  it("keeps course and assignment source checks before their existing receipt lookup", async () => {
    const ports = { ...common(), course: vi.fn(async () => null), prepare: vi.fn(async () => mutation) };
    await expect(createChangeLearningCourse(ports)(actor, "course", { action: "publish", expectedVersion: 1, reason: "Approved course" }, "key")).rejects.toMatchObject({ code: "LEARNING_COURSE_NOT_FOUND" });
    expect(ports.replay).not.toHaveBeenCalled();
    const assignPorts = { ...common(), person: vi.fn(async () => null), prepare: vi.fn(async () => mutation) };
    await expect(createLearningAssignment(assignPorts)(actor, { courseId: "course", courseVersion: 1, learnerId: "learner", dueDate: "2026-10-10", reason: "Required safety" }, "key")).rejects.toMatchObject({ code: "LEARNING_LEARNER_INELIGIBLE" });
    expect(assignPorts.replay).not.toHaveBeenCalled();
  });
  it("requires reviewer authority for a waiver even when its receipt already exists", async () => {
    const ports = { ...common(), assignment: vi.fn(async () => assignment), attachment: vi.fn(async () => mutation), prepare: vi.fn(async () => mutation) };
    vi.mocked(ports.replay).mockResolvedValue({ id: "original" });
    ports.reviewer = () => { throw new Error("review revoked"); };
    await expect(createChangeLearningAssignment(ports)(actor, "assignment", { action: "waive", expectedVersion: 4, reason: "Reviewed waiver" }, "key")).rejects.toThrow("review revoked");
    expect(ports.assignment).not.toHaveBeenCalled();
  });
  it("refuses completion by someone other than the active assigned learner", async () => {
    const ports = { ...common(), assignment: vi.fn(async () => ({ ...assignment, learner_eligible: 0 })), attachment: vi.fn(async () => mutation), prepare: vi.fn(async () => mutation) };
    await expect(createChangeLearningAssignment(ports)(actor, "assignment", complete, "key")).rejects.toMatchObject({ code: "LEARNING_LEARNER_REQUIRED" });
    expect(ports.attachment).not.toHaveBeenCalled(); expect(ports.commit).not.toHaveBeenCalled();
  });
  it("captures attachment binding and certificate issue as one native mutation", async () => {
    const ports = { ...common(), assignment: vi.fn(async () => assignment), attachment: vi.fn(async () => mutation), prepare: vi.fn(async () => mutation) };
    expect(await createChangeLearningAssignment(ports)(actor, "assignment", complete, "key")).toEqual({ id: "assignment", version: 5, status: "completed" });
    expect(ports.prepare).toHaveBeenCalledWith(expect.objectContaining({ operation: "learning.assignment.complete", now: 100, version: 5 }), expect.objectContaining({ action: "complete" }), expect.objectContaining({ attachment: mutation, issueCertificate: true, certificateFrom: "2026-10-06", certificateEnd: null, score: 9000 }));
    expect(ports.commit).toHaveBeenCalledTimes(1);
  });
  it("rejects invalid certificate dates after binding preparation but before commit", async () => {
    const ports = { ...common(), assignment: vi.fn(async () => assignment), attachment: vi.fn(async () => mutation), prepare: vi.fn(async () => mutation) };
    await expect(createChangeLearningAssignment(ports)(actor, "assignment", { ...complete, certificateValidFrom: "2026-10-10", certificateExpiresOn: "2026-10-09" }, "key")).rejects.toMatchObject({ code: "LEARNING_CERTIFICATE_DATES_INVALID" });
    expect(ports.attachment).toHaveBeenCalledTimes(1); expect(ports.prepare).not.toHaveBeenCalled();
  });
  it("preserves completion state rules and terminal work projection", () => {
    expect(learningActionAllowed("complete", "assigned")).toBe(false);
    expect(learningActionAllowed("waive", "in_progress")).toBe(true);
    expect(learningWorkItems(actor, [{ ...assignment, status: "completed" }])[0]).toMatchObject({ queue: "completed", canAct: false });
    expect(learningWorkItems(actor, [{ ...assignment, learner_eligible: 0 }])[0].blocker).toContain("no longer eligible");
  });
  it("does not expose detail or evidence outside the authorized workspace", async () => {
    const history = vi.fn(async () => []), nil = vi.fn(async () => null);
    await expect(createLearningDetail({ workspace: async () => ({ ...workspace, assignments: [] }), error: common().error, history, handoff: nil, certificate: nil })(actor, "assignment")).rejects.toMatchObject({ code: "LEARNING_ASSIGNMENT_NOT_FOUND" });
    expect(history).not.toHaveBeenCalled();
    const completions = vi.fn(async () => null);
    await expect(createLearningExecutiveEvidence({ authorize: vi.fn(), error: common().error, completions, overdue: nil })(actor, "learning-completions", "event")).rejects.toMatchObject({ code: "EVIDENCE_FILTER_REQUIRED" });
    expect(completions).not.toHaveBeenCalled();
  });
  it("preserves sequential workspace reads and hides work without module access", async () => {
    const calls: string[] = [], workspaceRead = vi.fn(async () => workspace);
    expect(await createLearningWorkspace({ authorize: () => { calls.push("access"); }, courses: async () => { calls.push("courses"); return []; }, assignments: async () => { calls.push("assignments"); return [assignment]; }, people: async () => { calls.push("people"); return []; } })(actor)).toEqual(workspace);
    expect(calls).toEqual(["access", "courses", "assignments", "people"]);
    expect(await createLearningWork({ workspace: workspaceRead })({ ...actor, modules: [] })).toEqual([]); expect(workspaceRead).not.toHaveBeenCalled();
  });
});

describe("Learning certificate consumption and revocation", () => {
  it("selects the first current sorted certificate and retains its snapshot digest", async () => {
    const digest = vi.fn(async () => "digest"), prepare = vi.fn(async () => mutation), input = { personId: "person", requiredCode: "safety", effectiveDate: "2026-10-06", electionId: "election", key: "key" };
    expect(await createLearningBenefitConsumption({ error: common().error, today: common().today, certificates: async () => [certificate, { ...certificate, id: "older" }], digest, prepare })(actor, input)).toEqual({ certificateId: "certificate", native: mutation });
    expect(digest).toHaveBeenCalledWith(JSON.stringify(certificate)); expect(prepare).toHaveBeenCalledWith(actor, input, certificate, "digest");
  });
  it("rejects missing certificates before receipt preparation", async () => {
    const prepare = vi.fn(async () => mutation);
    await expect(createLearningBenefitConsumption({ error: common().error, today: common().today, certificates: async () => [], digest: async () => "digest", prepare })(actor, { personId: "person", requiredCode: "safety", effectiveDate: "2026-10-06", electionId: "election", key: "key" })).rejects.toMatchObject({ code: "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED" });
    expect(prepare).not.toHaveBeenCalled();
  });
  it("never substitutes a new certificate for an existing consumption receipt", async () => {
    const ports = { error: common().error, today: common().today, requirement: async () => ({ requiredEligibilityCode: "safety" }), receipt: async () => ({ certificate_id: "different" }), guards: vi.fn(() => mutation), current: vi.fn(async () => true) };
    await expect(createValidateBenefitLearning(ports)(actor, election, "key")).rejects.toMatchObject({ code: "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED" });
    expect(ports.current).not.toHaveBeenCalled(); expect(ports.guards).not.toHaveBeenCalled();
    ports.receipt = async () => ({ certificate_id: "certificate" }); ports.current.mockResolvedValue(false);
    await expect(createValidateBenefitLearning(ports)(actor, election, "key")).rejects.toMatchObject({ code: "BENEFIT_LEARNING_CERTIFICATE_EXPIRED" });
  });
  it("replays revocation after current authority without reading another source", async () => {
    const ports = { ...common(), authorize: vi.fn((a: WorkContext) => a), certificate: vi.fn(async () => certificate), prepare: vi.fn(async () => mutation) };
    vi.mocked(ports.replay).mockResolvedValue({ id: "certificate", status: "revoked", version: 5 });
    expect(await createRevokeLearningCertificate(ports)(actor, "certificate", { expectedAssignmentVersion: 5, reason: "Evidence revoked" }, "key")).toMatchObject({ status: "revoked" });
    expect(ports.authorize).toHaveBeenCalledTimes(1); expect(ports.certificate).not.toHaveBeenCalled(); expect(ports.commit).not.toHaveBeenCalled();
  });
});

describe("Talent recruitment lifecycle with injected semantic ports", () => {
  const candidate = { legalName: "Candidate", email: "USER@example.com", consentEvidence: "Consent recorded", retentionUntil: "2026-10-07" };
  const offer = { applicationId: "application", applicationVersion: 4, currency: "thb", baseAmount: "20000.25", startDate: "2026-11-01", expiresOn: "2026-11-15", terms: "Approved monthly terms" };
  function changes() { return { ...common(), record: vi.fn(async () => ({ id: "record", status: "interview", version: 4, created_by: "another", application_id: "application" })), candidate: vi.fn(async () => ({ id: "candidate", status: "active", version: 3, retention_until: "2026-10-06" })), prepare: vi.fn(async () => mutation) }; }
  it("validates consent retention after replay and before writes", async () => {
    const ports = { ...common(), prepare: vi.fn(async () => mutation) };
    await expect(createTalentCandidate(ports)(actor, { ...candidate, retentionUntil: "2026-10-06" }, "key")).rejects.toMatchObject({ code: "INVALID_RETENTION" }); expect(ports.commit).not.toHaveBeenCalled();
    vi.mocked(ports.replay).mockResolvedValue({ id: "original-candidate" });
    expect(await createTalentCandidate(ports)(actor, { ...candidate, retentionUntil: "2026-10-06" }, "key")).toEqual({ id: "original-candidate" });
  });
  it("rejects an outdated open requisition without creating another application", async () => {
    const ports = { ...common(), currentRequisition: vi.fn(async () => null), prepare: vi.fn(async () => mutation) };
    await expect(createTalentApplication(ports)(actor, { requisitionId: "req", requisitionVersion: 2, candidateId: "candidate", ownerId: "owner" }, "key")).rejects.toMatchObject({ code: "TALENT_SOURCE_CHANGED" }); expect(ports.prepare).not.toHaveBeenCalled();
  });
  it("keeps proposal versions and results in the native guarded command", async () => {
    const ports = { ...common(), prepare: vi.fn(async () => mutation) };
    expect(await createTalentProposal(ports)(actor, { applicationId: "app", applicationVersion: 4, recommendation: "hire", scoreBps: 9000, evidence: "Interview evidence" }, "key")).toEqual({ id: "talent-proposal-key", status: "submitted", version: 1 });
    expect(ports.commit).toHaveBeenCalledWith(expect.objectContaining({ operation: "talent.proposal.create", input: expect.objectContaining({ applicationVersion: 4 }) }), mutation);
  });
  it("rejects invalid offer dates before taking a Finance money snapshot", async () => {
    const ports = { ...common(), moneyUnits: (value: string) => Number(value) * 100, snapshot: vi.fn(async () => mutation), prepare: vi.fn(async () => mutation) };
    await expect(createTalentOffer(ports)(actor, { ...offer, expiresOn: "2026-10-31" }, "key")).rejects.toMatchObject({ code: "INVALID_OFFER_DATES" }); expect(ports.snapshot).not.toHaveBeenCalled();
    expect(await createTalentOffer(ports)(actor, offer, "key")).toMatchObject({ status: "draft" });
    expect(ports.snapshot).toHaveBeenCalledWith(actor, "talent-offer-key", expect.objectContaining({ currency: "THB" }), 2000025);
    expect(ports.prepare).toHaveBeenCalledWith(expect.objectContaining({ now: 100 }), expect.anything(), 2000025, mutation);
  });
  it("denies own review and stale record versions before candidate processing", async () => {
    const ports = changes(); ports.record.mockResolvedValue({ id: "record", status: "draft", version: 4, created_by: "learner", application_id: "application" });
    await expect(createChangeTalentRecord(ports)(actor, "offer", "record", { action: "approve-offer", expectedVersion: 4, reason: "Approved independently" }, "key")).rejects.toMatchObject({ code: "CONFLICT", message: "Independent review is required." });
    expect(ports.candidate).not.toHaveBeenCalled();
    await expect(createChangeTalentRecord(ports)(actor, "offer", "record", { action: "approve-offer", expectedVersion: 3, reason: "Approved independently" }, "key")).rejects.toMatchObject({ message: "Record changed or transition is invalid." });
  });
  it("allows withdrawal with expired consent and blocks continued recruitment", async () => {
    const ports = changes(); ports.candidate.mockResolvedValue({ id: "candidate", version: 3, status: "active", retention_until: "2026-10-05" });
    await expect(createChangeTalentRecord(ports)(actor, "application", "record", { action: "select", expectedVersion: 4, reason: "Selected candidate" }, "key")).rejects.toMatchObject({ code: "TALENT_CANDIDATE_PROCESSING_BLOCKED" }); expect(ports.commit).not.toHaveBeenCalled();
    ports.candidate.mockClear();
    expect(await createChangeTalentRecord(ports)(actor, "application", "record", { action: "withdraw", expectedVersion: 4, reason: "Candidate withdrew" }, "other")).toMatchObject({ status: "withdrawn", version: 5 }); expect(ports.candidate).not.toHaveBeenCalled();
  });
  it("requires owner and acceptance evidence before native handoff work", async () => {
    const ports = changes();
    await expect(createChangeTalentRecord(ports)(actor, "application", "record", { action: "reassign", expectedVersion: 4, reason: "Reassign owner" }, "key")).rejects.toMatchObject({ code: "OWNER_REQUIRED" });
    ports.record.mockResolvedValue({ id: "record", status: "sent", version: 4, created_by: "another", application_id: "application" });
    await expect(createChangeTalentRecord(ports)(actor, "offer", "record", { action: "accept-offer", expectedVersion: 4, reason: "Accepted offer" }, "key")).rejects.toMatchObject({ code: "ACCEPTANCE_EVIDENCE_REQUIRED" }); expect(ports.prepare).not.toHaveBeenCalled();
  });
  it("passes accepted offer and current consent into one guarded hire handoff transaction", async () => {
    const ports = changes(); ports.record.mockResolvedValue({ id: "record", status: "sent", version: 4, created_by: "another", application_id: "application" });
    expect(await createChangeTalentRecord(ports)(actor, "offer", "record", { action: "accept-offer", expectedVersion: 4, reason: "Accepted offer", acceptanceEvidence: "Signed acceptance" }, "key")).toEqual({ id: "record", status: "accepted", version: 5 });
    expect(ports.prepare).toHaveBeenCalledWith(expect.objectContaining({ operation: "talent.offer.accept-offer", version: 5 }), "offer", expect.anything(), expect.objectContaining({ rule: ["sent", "accepted"], candidate: { id: "candidate", status: "active", version: 3, retention_until: "2026-10-06" }, candidateDate: "2026-10-06" }));
  });
  it("keeps terminal transitions and owner work blockers consistent", () => {
    expect(talentTransition("application", "withdraw", "selected")).toEqual(["selected", "withdrawn"]);
    expect(talentTransition("offer", "withdraw", "accepted")).toEqual(["approved", "withdrawn"]);
    expect(talentTransition("proposal", "open", "submitted")).toBeUndefined();
    expect(talentWorkItems(actor, [{ id: "app", owner_id: "learner", owner_eligible: 0, status: "interview", updated_at: 1 }])[0]).toMatchObject({ queue: "todo", canAct: false, nextAction: "Record proposal", blocker: expect.stringContaining("no longer eligible") });
  });
  it("blocks detail reads until its scoped application is present", async () => {
    const nil = vi.fn(async () => null), history = vi.fn(async () => []);
    await expect(createTalentDetail({ authorize: vi.fn(), error: common().error, application: nil, history, proposal: nil, offer: nil, handoff: nil })(actor, "foreign")).rejects.toMatchObject({ code: "TALENT_APPLICATION_NOT_FOUND" }); expect(history).not.toHaveBeenCalled();
    const applications = vi.fn(async () => null);
    await expect(createTalentExecutiveEvidence({ authorize: vi.fn(), error: common().error, requisition: nil, applications })(actor, "application-stage-counts", "app")).rejects.toMatchObject({ code: "EVIDENCE_FILTER_REQUIRED" }); expect(applications).not.toHaveBeenCalled();
  });
  it("does not expose Talent work after access denial and preserves workspace projections", async () => {
    const rows = vi.fn(async () => []);
    expect(await createTalentWork({ authorize: () => { throw new Error("privacy denied"); }, rows })(actor)).toEqual([]); expect(rows).not.toHaveBeenCalled();
    expect(await createTalentWorkspace({ authorize: vi.fn(), requisitions: rows, candidates: rows, applications: rows, people: rows })(actor)).toEqual({ requisitions: [], candidates: [], applications: [], people: [], actorId: "learner", canReview: true });
  });
});
