import {z} from "zod";
import type {Row} from "../../commerce/public/commerce";
import type {WorkItem} from "@/lib/work-item";
const reason=z.string().trim().min(3).max(2000),version=z.number().int().positive(),id=z.string().min(1);
export const crmLeadSchema=z.object({branchId:id,title:z.string().trim().min(3).max(160),contactName:z.string().trim().min(2).max(160),contactDetail:reason,source:reason,sourceEvidence:reason.optional(),sourceKind:z.enum(['staff','form','pms']).optional(),sourceId:id.optional(),campaignId:id.optional(),accountId:id.optional(),contactId:id.optional(),brief:reason,dueDate:z.iso.date()});
export const crmLeadChangeSchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('reassign'),expectedVersion:version,ownerId:id,dueDate:z.iso.date(),reason}),
 z.object({action:z.enum(['disqualify','reopen']),expectedVersion:version,reason}),
 z.object({action:z.literal('qualify'),expectedVersion:version,customerId:id,expectedValue:z.string().regex(/^\d{1,12}(\.\d{1,2})?$/),closeDate:z.iso.date(),reason})
]);
export const crmOpportunitySchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('link-quote'),expectedVersion:version,leadVersion:version,quoteId:id,quoteVersion:version,reason}),
 z.object({action:z.enum(['negotiate','reopen']),expectedVersion:version,leadVersion:version,reason}),
 z.object({action:z.literal('lose'),expectedVersion:version,leadVersion:version,lossReason:z.enum(['price','availability','competitor','timing','no_response','other']).default('other'),reason}),
 z.object({action:z.literal('win'),expectedVersion:version,leadVersion:version,salesId:id,salesVersion:version,reason}),
 z.object({action:z.literal('win-pms'),expectedVersion:version,leadVersion:version,reservationId:id,propertyId:id,reason})
]);

export type CRMLeadInput=z.infer<typeof crmLeadSchema>;
export type CRMLeadChange=z.infer<typeof crmLeadChangeSchema>;
export type CRMOpportunityChange=z.infer<typeof crmOpportunitySchema>;
export function crmLeadWorkItems(all:Row[],actorId:string,canSales:boolean):WorkItem[]{return all.map(r=>{const closed=r.status==='disqualified'||['won','lost'].includes(String(r.stage)),canAct=!closed&&r.owner_id===actorId&&canSales&&Number(r.owner_eligible)>0;return {id:String(r.id),type:'crm_lead' as const,title:`${r.opportunity_id?'Opportunity':'Lead'} · ${r.title}`,detail:String(r.brief),href:`/crm/${r.id}`,status:String(r.stage??r.status),updatedAt:Number(r.updated_at),queue:closed?'completed' as const:canAct?'todo' as const:'waiting' as const,canAct,responsibleQueue:String(r.owner_name??'Assigned salesperson'),nextAction:r.opportunity_id?'Review quotation and next sales action':'Qualify customer needs or record disqualification',dueAt:Date.parse(String(r.close_date??r.due_date)+'T00:00:00Z'),blocker:!closed&&!Number(r.owner_eligible)?'Assigned salesperson is no longer eligible; a sales manager must reassign with a reason':null,relationships:[r.owner_id===actorId?'assigned' as const:r.created_by===actorId?'requested' as const:'team' as const]};});}
