import {z} from "zod";
import type {Row} from "@erpx/server-commerce/public/commerce";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive();
export const marketingCandidateSchema = z.object({
  formId: z.string().min(1),
  formVersion: version,
  branchId: z.string().min(1),
  contactName: z.string().trim().min(2).max(160),
  contactDetail: reason,
  consentEvidence: reason,
  attribution: z.record(z.string(), z.string().max(300)),
  brief: reason,
});
export const marketingActionSchema = z.object({
  action: z.enum([
    "activate",
    "reject",
    "retire",
    "handoff",
    "record-engagement",
  ]),
  expectedVersion: version,
  reason,
  eventType: z.string().trim().min(2).max(80).optional(),
  channel: z.string().trim().min(2).max(80).optional(),
  dueDate: z.string().date().optional(),
});

export type MarketingCandidateInput=z.infer<typeof marketingCandidateSchema>;
export type MarketingAction=z.infer<typeof marketingActionSchema>;
export type MarketingExecutiveEvidenceMetric="marketing-lead-candidates"|"marketing-engagement-events";
export function marketingWorkItems(rows:Row[],actorId:string):WorkItem[]{return rows.map((x) => ({
    id: String(x.id),
    type: "marketing_candidate",
    title: `Marketing lead · ${x.contact_name}`,
    detail: String(x.brief),
    href: `/marketing/${x.id}`,
    status: String(x.status),
    updatedAt: Number(x.updated_at),
    queue: x.created_by === actorId ? "waiting" : "todo",
    canAct: x.created_by !== actorId,
    nextAction: "Review consent and hand off to CRM",
    relationships: [x.created_by === actorId ? "requested" : "approval"],
  }));
}
export function marketingCandidateLeadInput(row:Row,dueDate:string){return {branchId:String(row.branch_id),title:`Marketing lead · ${row.contact_name}`,contactName:String(row.contact_name),contactDetail:String(row.contact_detail),source:`Marketing form ${row.form_id}; attribution ${row.attribution_json}`,sourceEvidence:`Reviewed candidate ${row.id} from form ${row.form_id} version ${row.form_version}`,sourceKind:"form",sourceId:String(row.id),campaignId:(()=>{try{return JSON.parse(String(row.attribution_json)).campaignId||undefined;}catch{return undefined;}})(),brief:String(row.brief),dueDate};}
