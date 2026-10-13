import {z} from "zod";
import type {Row} from "../../commerce/public/commerce";
import type {WorkItem} from "@/lib/work_projection/work-item";
const text=z.string().trim().min(3).max(2000),version=z.number().int().positive(),id=z.string().min(1);
export const crmActivityCreateSchema=z.object({leadId:id,expectedVersion:version,activityType:z.enum(['call','email','meeting','task']),subject:z.string().trim().min(3).max(200),details:text,ownerId:id,dueDate:z.iso.date()});
export const crmActivityChangeSchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('reassign'),expectedVersion:version,ownerId:id,dueDate:z.iso.date(),reason:text}),
 z.object({action:z.enum(['complete','cancel']),expectedVersion:version,reason:text}),
]);
export type CRMActivity={id:string;company_id:string;lead_id:string;activity_type:string;subject:string;details:string;owner_id:string;due_date:string;status:string;outcome:string|null;version:number;created_by:string;created_at:number;updated_at:number;lead_title:string;lead_status:string;lead_stage:string|null;branch_id:string;department_id:string|null;owner_name?:string;owner_eligible?:number};
export type CRMActivityCreate=z.infer<typeof crmActivityCreateSchema>;
export type CRMActivityChange=z.infer<typeof crmActivityChangeSchema>;
function crmActivityCanChange(row:CRMActivity,sales:boolean,manager:boolean,isSamePerson:(id:string)=>boolean){return sales&&(isSamePerson(row.created_by)||isSamePerson(row.owner_id)||manager);}
export function crmActivityWorkspaceView(l:Row,items:CRMActivity[],actorId:string,sales:boolean,manager:boolean,isSamePerson:(id:string)=>boolean){
 return {lead:{id:String(l.id),title:String(l.title),status:String(l.status),stage:l.stage?String(l.stage):null,version:Number(l.version)},items:items.map(x=>({...x,ownerEligible:Number(x.owner_eligible)>0,canComplete:x.status==='open'&&sales&&x.owner_id===actorId&&Number(x.owner_eligible)>0,canManage:x.status==='open'&&manager,canChange:x.status==='open'&&crmActivityCanChange(x,sales,manager,isSamePerson)})),canCreate:sales&&!['disqualified'].includes(String(l.status))&&!['won','lost'].includes(String(l.stage??'')),canManage:manager};
}
export function crmActivityWorkItems(all:CRMActivity[],actorId:string,sales:boolean):WorkItem[]{return all.map(r=>{const open=r.status==='open',eligibleOwner=Number(r.owner_eligible)>0,canAct=open&&eligibleOwner&&r.owner_id===actorId&&sales;return {id:String(r.id),type:'crm_activity',title:`CRM activity · ${r.subject}`,detail:String(r.details),href:`/crm/${r.lead_id}#crm-activities`,status:String(r.status),updatedAt:Number(r.updated_at),queue:!open?'completed':canAct?'todo':'waiting',responsibleQueue:String(r.owner_name??'Assigned salesperson'),nextAction:open?String(r.subject):'Activity outcome recorded',dueAt:Date.parse(String(r.due_date)+'T00:00:00Z'),blocker:open&&!eligibleOwner?'Assigned owner is no longer eligible; a sales manager must reassign with a reason':null,relationships:[r.owner_id===actorId?'assigned':r.created_by===actorId?'requested':'team']};});
}
