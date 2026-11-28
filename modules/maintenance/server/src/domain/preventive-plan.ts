import {z} from "zod";
const version=z.number().int().positive(),reason=z.string().trim().min(3).max(2000);
const timeZone=z.string().trim().min(2).max(80).refine(value=>{try{new Intl.DateTimeFormat('en',{timeZone:value});return true;}catch{return false;}},'Choose an IANA time zone.');
export const preventivePlanSchema=z.object({assetId:z.string().min(1),assetVersion:version,title:z.string().trim().min(3).max(160),priority:z.enum(['normal','high','critical']),
 ownerId:z.string().min(1),intervalDays:z.number().int().min(1).max(3650),firstDueAt:z.number().int().positive().safe(),timeZone,leadDays:z.number().int().min(0).max(60).default(7),expectedVersion:version.optional(),reason});
export const preventiveActionSchema=z.object({action:z.enum(['activate','pause','cancel','generate']),expectedVersion:version,reason});
export type Plan={id:string;company_id:string;branch_id:string;department_id:string|null;asset_id:string;title:string;priority:'normal'|'high'|'critical';owner_id:string;interval_days:number;time_zone:string;first_due_local:string;next_due_local:string;next_due_at:number;lead_days:number;status:string;version:number;created_by:string;edited_by:string;approved_by:string|null;updated_at:number;asset_code?:string;asset_name?:string};
export type PreventiveInput=z.infer<typeof preventivePlanSchema>;
export type PreventiveAction=z.infer<typeof preventiveActionSchema>;
export type PreventivePlanRow=Plan&{owner_eligible:number;owner_name:string|null};
