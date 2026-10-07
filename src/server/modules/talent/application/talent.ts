import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { hasPermission } from "@/lib/permissions";
import { talentCandidateSchema, talentApplicationSchema, talentProposalSchema, talentOfferSchema, talentActionSchema, talentTransition, talentNeedsCurrentCandidate, talentWorkItems, type TalentRow, type TalentKind, type TalentCandidateInput, type TalentApplicationInput, type TalentProposalInput, type TalentOfferInput, type TalentActionInput } from "../domain/talent";

export type TalentMutation = { readonly native: unknown };
export type TalentCommand = { actor: WorkContext; key: string; operation: string; id: string; input: unknown; version: number; result: Record<string, unknown>; now: number };
export interface TalentCommandPorts {
  authorize(actor: WorkContext, write?: boolean): void;
  error(violation: { status: number; code: string; message: string }): Error;
  now(): number;
  today(): string;
  replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string, unknown> | null>;
  commit(command: TalentCommand, mutation: TalentMutation): Promise<Record<string, unknown>>;
}
export function createTalentWorkspace(ports: { authorize: TalentCommandPorts["authorize"]; requisitions(actor: WorkContext): Promise<TalentRow[]>; candidates(actor: WorkContext): Promise<TalentRow[]>; applications(actor: WorkContext): Promise<TalentRow[]>; people(actor: WorkContext): Promise<TalentRow[]> }) {
  return async (actor: WorkContext) => { ports.authorize(actor); const [requisitions, candidates, applications, people] = await Promise.all([ports.requisitions(actor), ports.candidates(actor), ports.applications(actor), ports.people(actor)]); return { requisitions, candidates, applications, people, actorId: actor.userId, canReview: true }; };
}
export function createTalentCandidate(ports: TalentCommandPorts & { prepare(command: TalentCommand, input: TalentCandidateInput): Promise<TalentMutation> }) {
  return async (actor: WorkContext, raw: unknown, key: string) => {
    ports.authorize(actor, true); const input = talentCandidateSchema.parse(raw), id = `talent-candidate-${key}`, operation = "talent.candidate.create", prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    if (input.retentionUntil <= ports.today()) throw ports.error({ status: 422, code: "INVALID_RETENTION", message: "Retention date must be in the future." });
    const command: TalentCommand = { actor, id, key, operation, input, now: ports.now(), version: 1, result: { id, status: "active", version: 1 } }; return ports.commit(command, await ports.prepare(command, input));
  };
}
export function createTalentApplication(ports: TalentCommandPorts & { currentRequisition(actor: WorkContext, input: TalentApplicationInput): Promise<TalentRow | null>; prepare(command: TalentCommand, input: TalentApplicationInput): Promise<TalentMutation> }) {
  return async (actor: WorkContext, raw: unknown, key: string) => {
    ports.authorize(actor, true); const input = talentApplicationSchema.parse(raw), id = `talent-application-${key}`, operation = "talent.application.create", prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    if (!await ports.currentRequisition(actor, input)) throw ports.error({ status: 409, code: "TALENT_SOURCE_CHANGED", message: "Choose a current open requisition." });
    const command: TalentCommand = { actor, id, key, operation, input, now: ports.now(), version: 1, result: { id, status: "applied", version: 1 } }; return ports.commit(command, await ports.prepare(command, input));
  };
}
export function createTalentProposal(ports: TalentCommandPorts & { prepare(command: TalentCommand, input: TalentProposalInput): Promise<TalentMutation> }) {
  return async (actor: WorkContext, raw: unknown, key: string) => {
    ports.authorize(actor, true); const input = talentProposalSchema.parse(raw), id = `talent-proposal-${key}`, operation = "talent.proposal.create", prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    const command: TalentCommand = { actor, id, key, operation, input, now: ports.now(), version: 1, result: { id, status: "submitted", version: 1 } }; return ports.commit(command, await ports.prepare(command, input));
  };
}
export function createTalentOffer(ports: TalentCommandPorts & { moneyUnits(value: string): number; snapshot(actor: WorkContext, id: string, input: TalentOfferInput, amount: number): Promise<unknown>; prepare(command: TalentCommand, input: TalentOfferInput, amount: number, snapshot: unknown): Promise<TalentMutation> }) {
  return async (actor: WorkContext, raw: unknown, key: string) => {
    ports.authorize(actor, true); const input = talentOfferSchema.parse(raw), id = `talent-offer-${key}`, operation = "talent.offer.create", prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    if (input.expiresOn < input.startDate) throw ports.error({ status: 422, code: "INVALID_OFFER_DATES", message: "Offer expiry must not precede start date." });
    const now = ports.now(), amount = ports.moneyUnits(input.baseAmount), snapshot = await ports.snapshot(actor, id, input, amount);
    const command: TalentCommand = { actor, id, key, operation, input, now, version: 1, result: { id, status: "draft", version: 1 } }; return ports.commit(command, await ports.prepare(command, input, amount, snapshot));
  };
}
export type TalentCandidateFacts = { id: string; version: number; status: string; retention_until: string };
export type TalentChangeFacts = { row: TalentRow; rule: [string, string]; candidate: TalentCandidateFacts | null; candidateDate: string | null };
export function createChangeTalentRecord(ports: TalentCommandPorts & { record(actor: WorkContext, kind: TalentKind, id: string): Promise<TalentRow | null>; candidate(actor: WorkContext, applicationId: string): Promise<TalentCandidateFacts | null>; prepare(command: TalentCommand, kind: TalentKind, input: TalentActionInput, facts: TalentChangeFacts): Promise<TalentMutation> }) {
  return async (actor: WorkContext, kind: TalentKind, id: string, raw: unknown, key: string) => {
    ports.authorize(actor, true); const input = talentActionSchema.parse(raw), operation = `talent.${kind}.${input.action}`, prior = await ports.replay(actor, key, operation, id, input); if (prior) return prior;
    const row = await ports.record(actor, kind, id); if (!row) throw ports.error({ status: 404, code: "TALENT_RECORD_NOT_FOUND", message: "Talent record not found." });
    const rule = talentTransition(kind, input.action, String(row.status));
    if (!rule || row.status !== rule[0] || Number(row.version) !== input.expectedVersion) throw ports.error({ status: 409, code: "CONFLICT", message: "Record changed or transition is invalid." });
    if (["approve", "reject", "accept-proposal", "reject-proposal", "approve-offer"].includes(input.action) && (row.created_by === actor.userId || row.submitted_by === actor.userId)) throw ports.error({ status: 409, code: "CONFLICT", message: "Independent review is required." });
    if (input.action === "reassign" && !input.ownerId) throw ports.error({ status: 422, code: "OWNER_REQUIRED", message: "Choose an eligible owner." });
    if (input.action === "accept-offer" && !input.acceptanceEvidence) throw ports.error({ status: 422, code: "ACCEPTANCE_EVIDENCE_REQUIRED", message: "Record candidate acceptance evidence." });
    const now = ports.now(); let candidate: TalentCandidateFacts | null = null, candidateDate: string | null = null;
    if (talentNeedsCurrentCandidate(kind, input.action)) {
      candidate = await ports.candidate(actor, kind === "application" ? id : String(row.application_id));
      if (!candidate || candidate.status !== "active" || candidate.retention_until < ports.today()) throw ports.error({ status: 409, code: "TALENT_CANDIDATE_PROCESSING_BLOCKED", message: "Current recruitment consent and retention evidence are required before progressing this application." });
      candidateDate = ports.today();
    }
    const command: TalentCommand = { actor, id, key, operation, input, now, version: input.expectedVersion + 1, result: { id, status: rule[1], version: input.expectedVersion + 1 } };
    return ports.commit(command, await ports.prepare(command, kind, input, { row, rule, candidate, candidateDate }));
  };
}
export function createTalentDetail(ports: { authorize: TalentCommandPorts["authorize"]; error: TalentCommandPorts["error"]; application(actor: WorkContext, id: string): Promise<TalentRow | null>; history(id: string): Promise<TalentRow[]>; proposal(id: string): Promise<TalentRow | null>; offer(id: string): Promise<TalentRow | null>; handoff(offerId: string | number | null): Promise<TalentRow | null> }) {
  return async (actor: WorkContext, id: string) => {
    ports.authorize(actor); const application = await ports.application(actor, id); if (!application) throw ports.error({ status: 404, code: "TALENT_APPLICATION_NOT_FOUND", message: "Application not found." });
    const history = await ports.history(id), proposal = await ports.proposal(id), offer = await ports.offer(id), handoff = offer ? await ports.handoff(offer.id) : null;
    return { application, history, proposal, offer, handoff, actorId: actor.userId, canReview: true, canManageDocuments: hasPermission(actor, "talent.documents.manage") };
  };
}
export type TalentExecutiveEvidenceMetric = "open-requisitions" | "application-stage-counts";
export function createTalentExecutiveEvidence(ports: { authorize: TalentCommandPorts["authorize"]; error: TalentCommandPorts["error"]; requisition(actor: WorkContext, id: string): Promise<TalentRow | null>; applications(actor: WorkContext, id: string, range: { from: number; toExclusive: number }): Promise<TalentRow | null> }) {
  return async (actor: WorkContext, metric: TalentExecutiveEvidenceMetric, recordId: string, range?: { from: number; toExclusive: number }) => {
    ports.authorize(actor); const id = z.string().min(1).max(200).parse(recordId); if (metric === "open-requisitions") return ports.requisition(actor, id);
    if (!range) throw ports.error({ status: 422, code: "EVIDENCE_FILTER_REQUIRED", message: "An application update date range is required." }); return ports.applications(actor, id, range);
  };
}
export function createTalentWork(ports: { authorize: TalentCommandPorts["authorize"]; rows(actor: WorkContext): Promise<TalentRow[]> }) { return async (actor: WorkContext) => { try { ports.authorize(actor); } catch { return []; } return talentWorkItems(actor, await ports.rows(actor)); }; }
