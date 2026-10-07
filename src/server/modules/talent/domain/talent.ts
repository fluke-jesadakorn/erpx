import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { WorkItem } from "@/lib/work-item";
export type TalentRow = Record<string, string | number | null>;
export type TalentKind = "requisition" | "application" | "proposal" | "offer";
const reason = z.string().trim().min(3).max(3000), version = z.number().int().positive(), date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
export const talentCandidateSchema = z.object({ legalName: z.string().trim().min(2).max(200), email: z.string().email(), phone: z.string().trim().max(60).optional(), consentEvidence: reason, retentionUntil: date });
export const talentApplicationSchema = z.object({ requisitionId: z.string().min(1), requisitionVersion: version, candidateId: z.string().min(1), ownerId: z.string().min(1) });
export const talentProposalSchema = z.object({ applicationId: z.string().min(1), applicationVersion: version, recommendation: z.enum(["hire", "hold", "reject"]), scoreBps: z.number().int().min(0).max(10000), evidence: reason });
export const talentOfferSchema = z.object({ applicationId: z.string().min(1), applicationVersion: version, currency: z.string().length(3).transform(value => value.toUpperCase()), baseAmount: z.string().regex(/^\d+(\.\d{1,2})?$/), startDate: date, expiresOn: date, terms: reason });
export const talentActionSchema = z.object({ action: z.enum(["approve", "reject", "open", "close", "cancel", "interview", "select", "withdraw", "accept-proposal", "reject-proposal", "approve-offer", "send", "accept-offer", "decline", "reassign"]), expectedVersion: version, reason, ownerId: z.string().min(1).optional(), acceptanceEvidence: reason.optional() });
export type TalentCandidateInput = z.infer<typeof talentCandidateSchema>;
export type TalentApplicationInput = z.infer<typeof talentApplicationSchema>;
export type TalentProposalInput = z.infer<typeof talentProposalSchema>;
export type TalentOfferInput = z.infer<typeof talentOfferSchema>;
export type TalentActionInput = z.infer<typeof talentActionSchema>;
export function talentTransition(kind: TalentKind, action: TalentActionInput["action"], status: string): [string, string] | undefined {
  const rules: Record<string, [string, string]> = kind === "requisition" ? { approve: ["draft", "approved"], reject: ["draft", "rejected"], open: ["approved", "open"], close: ["open", "closed"], cancel: ["draft", "cancelled"] } : kind === "application" ? { interview: ["applied", "interview"], select: ["interview", "selected"], reject: ["applied", "rejected"], withdraw: [["applied", "interview", "selected"].includes(status) ? status : "applied", "withdrawn"], reassign: [status, status] } : kind === "proposal" ? { "accept-proposal": ["submitted", "accepted"], "reject-proposal": ["submitted", "rejected"] } : { "approve-offer": ["draft", "approved"], send: ["approved", "sent"], "accept-offer": ["sent", "accepted"], decline: ["sent", "declined"], withdraw: [["draft", "approved", "sent"].includes(status) ? status : "approved", "withdrawn"] };
  return rules[action];
}
export function talentNeedsCurrentCandidate(kind: TalentKind, action: TalentActionInput["action"]) { return kind !== "requisition" && !["withdraw", "decline", "reject", "reject-proposal"].includes(action); }
export function talentWorkItems(a: WorkContext, rows: TalentRow[]): WorkItem[] {
  return rows.map(x => ({ id: String(x.id), type: "talent_application", title: `Talent · ${x.candidate_name}`, detail: `${x.job_title} · ${x.status}`, href: `/talent/${x.id}`, status: String(x.status), updatedAt: Number(x.updated_at), queue: x.owner_id === a.userId ? "todo" : "waiting", canAct: x.owner_id === a.userId && Boolean(x.owner_eligible), blocker: Boolean(x.owner_eligible) ? null : "Owner is no longer eligible; a Talent reviewer must reassign this application.", nextAction: String(x.status) === "applied" ? "Schedule interview" : String(x.status) === "interview" ? "Record proposal" : "Complete offer", relationships: [x.owner_id === a.userId ? "assigned" : "team"] }));
}
