import {z} from 'zod';
import type {Row} from '@erpx/server-commerce/public/commerce';
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
const reason=z.string().trim().min(3).max(2000),version=z.number().int().positive();
export const followupCreateSchema=z.object({orderId:z.string().min(1),expectedVersion:version,ownerId:z.string().min(1),dueDate:z.iso.date(),nextAction:reason});
export const followupChangeSchema=z.discriminatedUnion('action',[z.object({action:z.literal('reassign'),expectedVersion:version,ownerId:z.string().min(1),dueDate:z.iso.date(),reason}),z.object({action:z.enum(['complete','cancel']),expectedVersion:version,reason})]);

export type FollowupCreate=z.infer<typeof followupCreateSchema>;
export type FollowupChange=z.infer<typeof followupChangeSchema>;
export function customerFollowupWorkItems(rows:Row[],actorId:string,canSell:boolean):WorkItem[]{return rows.map(r=>{const open=r.status==='open',eligible=Number(r.owner_eligible)>0,canAct=open&&eligible&&r.owner_id===actorId&&canSell;return {id:String(r.id),type:'customer_followup',title:`Customer follow-up · ${r.customer_name}`,detail:String(r.next_action),href:`/customer-followups/${r.id}`,status:String(r.status),updatedAt:Number(r.updated_at),queue:!open?'completed':canAct?'todo':'waiting',canAct,responsibleQueue:String(r.owner_name??'Assigned sales colleague'),nextAction:open?String(r.next_action):'Contact outcome recorded',dueAt:Date.parse(String(r.due_date)+'T00:00:00Z'),blocker:open&&!eligible?'Assigned owner is no longer eligible; a sales manager must reassign with a reason':null,relationships:[r.owner_id===actorId?'assigned':r.created_by===actorId?'requested':'team']};});}
