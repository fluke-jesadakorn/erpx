import {z} from "zod";
import type {Row} from "@erpx/server-commerce/public/commerce";
import {consumptionLotSchema} from "@erpx/server-shared/index";
const reason=z.string().trim().min(3).max(2000),version=z.number().int().positive(),date=z.string().date();
export const createWorkSchema=z.object({assetId:z.string().min(1),assetVersion:version,title:z.string().trim().min(3).max(160),priority:z.enum(['normal','high','critical']),ownerId:z.string().min(1),scheduledAt:z.number().int().positive().nullable().optional(),reason});
export const workActionSchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('schedule'),expectedVersion:version,ownerId:z.string().min(1),scheduledAt:z.number().int().positive(),reason}),
 z.object({action:z.enum(['start','resume','cancel']),expectedVersion:version,reason}),
 z.object({action:z.literal('block'),expectedVersion:version,reason}),
 z.object({action:z.literal('issue-part'),expectedVersion:version,warehouseId:z.string().min(1),productId:z.string().min(1),quantityMilli:z.number().int().positive().safe(),entryDate:date,allocations:z.array(consumptionLotSchema).max(200).default([]),reason}),
 z.object({action:z.literal('complete'),expectedVersion:version,resultingCondition:z.enum(['good','degraded','unsafe']),evidence:z.string().trim().min(3).max(4000),reason}),
]);
export type Work=Row&{id:string;version:number;status:string;owner_id:string;created_by:string;updated_at:number;owner_eligible:number;can_review:number;scheduled_at:number|null;blocked_reason:string|null;asset_code:string;asset_name:string;title:string;priority:string};
export type MaintenanceDowntimeEvidenceRow={id:string;version:string;status:string;title:string;asset_code:string;asset_name:string;downtime_started_at:string;downtime_ended_at:string};

export type MaintenanceCreateInput=z.infer<typeof createWorkSchema>;
export type MaintenanceAction=z.infer<typeof workActionSchema>;
export type MaintenanceWork=Work&{asset_id:string;asset_version:number;downtime_started_at:number|null};
export type MaintenanceState={status:string;owner:string;scheduled:number|null;downtimeStart:number|null;downtimeEnd:number|null};

export const maintenanceStates:Record<MaintenanceAction["action"],string[]>={schedule:["open","scheduled"],start:["open","scheduled"],block:["in_progress"],resume:["blocked"],complete:["in_progress"],"issue-part":["in_progress"],cancel:["open","scheduled","in_progress","blocked"]};
export function maintenanceState(row:MaintenanceWork,input:MaintenanceAction,now:number):MaintenanceState {return {status:({schedule:"scheduled",start:"in_progress",block:"blocked",resume:"in_progress",complete:"completed","issue-part":"in_progress",cancel:"cancelled"})[input.action],owner:input.action==="schedule"?input.ownerId:row.owner_id,scheduled:input.action==="schedule"?input.scheduledAt:row.scheduled_at,downtimeStart:input.action==="start"?now:row.downtime_started_at,downtimeEnd:["complete","cancel"].includes(input.action)&&row.downtime_started_at?now:null};}
