import type {LearningPurchaseControls} from "@erpx/server-learning/domain/purchase-controls";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { courseActionSchema, learningAssignSchema, learningActionSchema, learningActionStatus, learningActionAllowed, learningWorkItems, type LearningRow, type LearningAssignment, type LearningCourseAction, type LearningAssignInput, type LearningActionInput } from "@erpx/server-learning/domain/learning";

export type LearningViolation = { status: number; code: string; message: string };
export type LearningMutation = { readonly native: unknown };
export type LearningCommand = { actor: WorkContext; key: string; operation: string; id: string; input: unknown; version: number; result: Record<string, unknown>; now: number };
export interface LearningCommandPorts {
  authorize(actor: WorkContext, write?: boolean): void;
  reviewer(actor: WorkContext): void;
  error(violation: LearningViolation): Error;
  now(): number;
  today(): string;
  replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string, unknown> | null>;
  commit(command: LearningCommand, mutation: LearningMutation): Promise<Record<string, unknown>>;
}
export type LearningWorkspace = { courses: LearningRow[]; assignments: LearningAssignment[]; people: LearningRow[]; canWrite: boolean; canReview: boolean; actorId: string; purchases?:LearningPurchaseControls };
export function createLearningWorkspace(ports: { authorize: LearningCommandPorts["authorize"]; courses(actor: WorkContext): Promise<LearningRow[]>; assignments(actor: WorkContext, personal: boolean): Promise<LearningAssignment[]>; people(actor: WorkContext): Promise<LearningRow[]> }) {
  return async (actor: WorkContext, personal = false): Promise<LearningWorkspace> => {
    ports.authorize(actor);
    const courses = await ports.courses(actor), assignments = await ports.assignments(actor, personal), people = await ports.people(actor);
    return { courses, assignments, people, canWrite: hasPermission(actor, "records.write"), canReview: hasPermission(actor, "records.review"), actorId: actor.userId };
  };
}
export function createChangeLearningCourse(ports: LearningCommandPorts & { course(actor: WorkContext, id: string): Promise<LearningRow | null>; prepare(command: LearningCommand, input: LearningCourseAction, expected: string, status: string): Promise<LearningMutation> }) {
  return async (actor: WorkContext, id: string, raw: unknown, key: string) => {
    ports.authorize(actor, true); ports.reviewer(actor);
    const input = courseActionSchema.parse(raw), row = await ports.course(actor, id);
    if (!row) throw ports.error({ status: 404, code: "LEARNING_COURSE_NOT_FOUND", message: "Course not found." });
    const expected = input.action === "retire" ? "published" : "draft", status = input.action === "publish" ? "published" : input.action === "reject" ? "rejected" : "retired", operation = `learning.course.${input.action}`;
    const prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    const command: LearningCommand = { actor, key, operation, id, input, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1, status }, now: ports.now() };
    return ports.commit(command, await ports.prepare(command, input, expected, status));
  };
}
export function createLearningAssignment(ports: LearningCommandPorts & { person(actor: WorkContext, learnerId: string): Promise<LearningRow | null>; prepare(command: LearningCommand, input: LearningAssignInput, person: LearningRow): Promise<LearningMutation> }) {
  return async (actor: WorkContext, raw: unknown, key: string) => {
    ports.authorize(actor, true); ports.reviewer(actor);
    const input = learningAssignSchema.parse(raw), person = await ports.person(actor, input.learnerId);
    if (!person) throw ports.error({ status: 422, code: "LEARNING_LEARNER_INELIGIBLE", message: "Choose an active worker." });
    const id = `learning-assignment-${key}`, operation = "learning.assignment.create", prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    const command: LearningCommand = { actor, key, operation, id, input, version: 1, result: { id, version: 1, status: "assigned" }, now: ports.now() };
    return ports.commit(command, await ports.prepare(command, input, person));
  };
}
export type LearningActionFacts = { row: LearningAssignment & { required_for: string | null }; status: string; attendance: string | number | null; score: string | number | null; evidence: string | number | null; completion: string | null; certificateFrom: string; certificateEnd: string | null; issueCertificate: boolean; attachment: unknown };
export function createChangeLearningAssignment(ports: LearningCommandPorts & { assignment(actor: WorkContext, id: string): Promise<(LearningAssignment & { required_for: string | null }) | null>; attachment(actor: WorkContext, id: string, key: string, input: Extract<LearningActionInput, { action: "complete" }>): Promise<unknown>; prepare(command: LearningCommand, input: LearningActionInput, facts: LearningActionFacts): Promise<LearningMutation> }) {
  return async (actor: WorkContext, id: string, raw: unknown, key: string) => {
    ports.authorize(actor, true);
    const input = learningActionSchema.parse(raw), operation = `learning.assignment.${input.action}`, prior = await ports.replay(actor, key, operation, id, input);
    if (["waive", "cancel"].includes(input.action)) ports.reviewer(actor);
    if (prior) return prior;
    const row = await ports.assignment(actor, id);
    if (!row) throw ports.error({ status: 404, code: "LEARNING_ASSIGNMENT_NOT_FOUND", message: "Learning assignment not found." });
    const own = row.learner_id === actor.userId && Number(row.learner_eligible) > 0, review = hasPermission(actor, "records.review");
    if (["start", "complete"].includes(input.action) && !own) throw ports.error({ status: 403, code: "LEARNING_LEARNER_REQUIRED", message: "The assigned learner must record this action." });
    if (["waive", "cancel"].includes(input.action) && !review) throw ports.error({ status: 403, code: "LEARNING_REVIEW_REQUIRED", message: "A reviewer must waive or cancel learning." });
    if (!learningActionAllowed(input.action, row.status)) throw ports.error({ status: 409, code: "LEARNING_STATE_CHANGED", message: "Refresh the assignment." });
    const status = learningActionStatus(input.action), attachment = input.action === "complete" && input.attachmentSetIds.length ? await ports.attachment(actor, id, key, input) : null;
    const now = ports.now(), certificateFrom = input.action === "complete" ? input.certificateValidFrom ?? ports.today() : ports.today(), certificateEnd = input.action === "complete" ? input.certificateExpiresOn ?? null : null;
    if (certificateEnd && certificateEnd < certificateFrom) throw ports.error({ status: 422, code: "LEARNING_CERTIFICATE_DATES_INVALID", message: "Certificate expiry must follow its valid-from date." });
    const facts: LearningActionFacts = { row, status, attachment, attendance: input.action === "complete" ? input.attendance : row.attendance, score: input.action === "complete" ? input.scoreBps : row.score_bps, evidence: input.action === "complete" ? input.evidence : row.result_evidence, completion: ["complete", "waive"].includes(input.action) ? input.reason : null, certificateFrom, certificateEnd, issueCertificate: ["complete", "waive"].includes(input.action) && Boolean(row.required_for) };
    const command: LearningCommand = { actor, id, key, operation, input, now, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1, status } };
    return ports.commit(command, await ports.prepare(command, input, facts));
  };
}
export function createLearningDetail(ports: { workspace(actor: WorkContext): Promise<LearningWorkspace>; error: LearningCommandPorts["error"]; history(id: string,includeResults?:boolean): Promise<LearningRow[]>; handoff(id: string): Promise<LearningRow | null>; certificate(id: string): Promise<LearningRow | null> }) {
  return async (actor: WorkContext, id: string) => {
    const data = await ports.workspace(actor), assignment = data.assignments.find(row => row.id === id);
    if (!assignment) throw ports.error({ status: 404, code: "LEARNING_ASSIGNMENT_NOT_FOUND", message: "Learning assignment not found." });
    return { ...data, assignment, history: await ports.history(id,!data.purchases||data.purchases["learn.results"]?.history===true), handoff: !data.purchases||data.purchases["learn.certificates"]?.history===true?await ports.handoff(id):null, certificate: !data.purchases||data.purchases["learn.certificates"]?.history===true?await ports.certificate(id):null };
  };
}
export type LearningExecutiveEvidenceMetric = "learning-completions" | "overdue-required-learning";
export function createLearningExecutiveEvidence(ports: { authorize: LearningCommandPorts["authorize"]; error: LearningCommandPorts["error"]; completions(actor: WorkContext, id: string, range: { from: number; toExclusive: number }): Promise<LearningRow | null>; overdue(actor: WorkContext, id: string): Promise<LearningRow | null> }) {
  return async (actor: WorkContext, metric: LearningExecutiveEvidenceMetric, recordId: string, range?: { from: number; toExclusive: number }) => {
    ports.authorize(actor); const id = z.string().min(1).max(200).parse(recordId);
    if (metric === "learning-completions") { if (!range) throw ports.error({ status: 422, code: "EVIDENCE_FILTER_REQUIRED", message: "A completion date range is required." }); return ports.completions(actor, id, range); }
    return ports.overdue(actor, id);
  };
}
export function createLearningWork(ports: { workspace(actor: WorkContext): Promise<LearningWorkspace> }) { return async (actor: WorkContext) => !hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "learning") ? [] : learningWorkItems(actor, (await ports.workspace(actor)).assignments); }
