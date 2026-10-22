import {z} from "zod";
import {BusinessRuleError} from "@erpx/server-shared/index";
export const analyticsSchema=z.object({from:z.iso.date(),to:z.iso.date(),branchId:z.string().optional(),propertyId:z.string().optional(),campaignId:z.string().optional(),ownerId:z.string().optional()});
export function thaiDate(timestamp:number){return new Date(timestamp+7*3600000).toISOString().slice(0,10);}
export function analyticsRange(now:number){return {from:thaiDate(now-30*86400000),to:thaiDate(now)};}
export type AnalyticsFilters=z.infer<typeof analyticsSchema>;
export function parseAnalyticsFilters(a:{branchId:string|null},raw:unknown){
 const f=analyticsSchema.parse(raw);
 if(f.from>f.to)throw new BusinessRuleError(422,'REPORT_DATE_RANGE','The start date must not follow the end date.');
 if(a.branchId&&f.branchId&&a.branchId!==f.branchId)throw new BusinessRuleError(403,'REPORT_SCOPE','Branch is outside your workspace.');
 return f;
}
export function bounds(f:AnalyticsFilters){return {start:Date.parse(`${f.from}T00:00:00+07:00`),end:Date.parse(`${f.to}T23:59:59.999+07:00`)};}
export function metric(id:string,label:string,value:number|null,unit:string,formula:string,source:string,f:AnalyticsFilters,drilldown:string[]=[]){return {id,label,value,unit,formula,source,from:f.from,to:f.to,timezone:'Asia/Bangkok',drilldown};}
export function definition(id:string,unit:string,formula:string,source:string,f:AnalyticsFilters){return {id,unit,formula,source,from:f.from,to:f.to,timezone:'Asia/Bangkok'};}
export function n(v:unknown){return Number(v??0);}
export function s(v:unknown){return String(v??'');}
