import {filterCRMPurchasedWork} from "./record-purchases";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/commercial-owner-projection-sources";
import 'server-only';

import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase} from "@/server/modules/runtime/public/runtime/types";
import type {Row} from "@/server/modules/commerce/public/commerce";

import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasPermission} from "@/lib/identity_access/permissions";
import {samePerson} from "@/lib/identity_access/iam";
import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {salesRecipientEligibility} from "@/server/modules/sales/public/sales-recipient";

import {BusinessRuleError} from "../../shared";
import {prepareCrmActivityNotification as notify} from "../../collaboration/public/crm-notifications";
import {createCRMActivityApplication,type CRMActivityPorts,type CRMActivityMutation} from "../application/activities";
import type {CRMActivity} from "../domain/activities";
import {crmActivityCreateSchema,crmActivityChangeSchema} from "../domain/activities";
import {requireCRMPurchasedRead,crmSourceAccess,withCRMPurchasedCommand,crmNativeReadFilter} from "./purchases";
export {crmActivityCreateSchema,crmActivityChangeSchema} from "../domain/activities";
const scope="l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=salesRecipientEligibility('l','l.owner_id');


async function lead(db:SqlDatabase,a:WorkContext,leadId:string){
 const row=await db.prepare(`SELECT l.id,l.company_id,l.branch_id,l.department_id,l.title,l.status,l.version,v.stage FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.id=? AND ${scope} AND ${await crmNativeReadFilter('lead','l')}`).bind(leadId,...args(a)).first<Row>();

 return row;
}
async function rows(db:SqlDatabase,a:WorkContext,leadId:string){
 return (await db.prepare(`SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE x.company_id=? AND x.lead_id=? AND ${scope} AND ${await crmNativeReadFilter('activity','x')} ORDER BY x.due_date,x.created_at,x.id`).bind(a.companyId,leadId,...args(a)).all<CRMActivity>()).results;
}
async function recipients(db:SqlDatabase,a:WorkContext,leadId:string){
 return (await db.prepare(`SELECT DISTINCT a.user_id id,p.legal_name name FROM (${identity_accessProjectionSources.assignments}) a JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN crm_leads l ON l.id=? WHERE a.company_id=? AND ${salesRecipientEligibility('l','a.user_id')} ORDER BY p.legal_name`).bind(leadId,a.companyId).all<{id:string;name:string}>()).results;
}
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);
const ports:CRMActivityPorts={actorId:a.userId,hasRead:hasPermission(a,'commerce.read'),sales:hasPermission(a,'commerce.sales'),manager:hasPermission(a,'commerce.approve'),clock:{now:()=>Date.now()},isSamePerson:id=>samePerson(a,id),authorize:permission=>{requirePermission(a,permission);},
replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),
lead:async(id)=>lead(await database(),a,id),activities:async(id)=>rows(await database(),a,id),recipients:async(id)=>recipients(await database(),a,id),
async history(leadId){const write=await database();return (await write.prepare('SELECT e.* FROM crm_lead_activity_events e JOIN crm_lead_activities x ON x.id=e.activity_id WHERE x.company_id=? AND x.lead_id=? ORDER BY e.created_at DESC,e.version DESC').bind(a.companyId,leadId).all<Row>()).results;},
async activity(id){const write=await database(); return await write.prepare(`SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE x.id=? AND x.company_id=? AND ${scope} AND ${await crmNativeReadFilter('activity','x')}`).bind(id,a.companyId,...args(a)).first<CRMActivity>();
},
async workRows(){const write=await database();return (await write.prepare(`SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE ${scope} AND ${await crmNativeReadFilter('activity','x')} ORDER BY x.due_date,x.updated_at DESC`).bind(...args(a)).all<CRMActivity>()).results;},commit:async(mutation,result)=>commit(await database(),a,mutation,result),
};return createCRMActivityApplication(ports);}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:CRMActivityMutation,result:Result):Promise<Result>{const {id,key,operation,now}=m;switch(m.action){
case "create":{const input=m.input,l=m.lead; return executeCommand(write,a,key,operation,input,id,1,result,[
  assertion(write,key,'lead-source',`EXISTS(SELECT 1 FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.id=? AND l.version=? AND l.status IN ('open','qualified') AND COALESCE(v.stage,'discovery') NOT IN ('won','lost') AND ${scope})`,[input.leadId,input.expectedVersion,...args(a)]),
  write.prepare('INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,input.leadId,input.activityType,input.subject,input.details,input.ownerId,input.dueDate,a.userId,now,now),
  assertion(write,key,'recipient',`EXISTS(SELECT 1 FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.id=? AND ${eligible})`,[id]),
  write.prepare("INSERT INTO crm_lead_activity_events VALUES(?,?,1,'create',?,?,?,?,?,?)").bind(`crm-activity-event-${key}`,id,a.userId,input.ownerId,input.dueDate,input.details,JSON.stringify(input),now),
  notify(write,id,key)
 ],{companyId:a.companyId,branchId:String(l.branch_id),departmentId:l.department_id?String(l.department_id):null});}
case "change":{const input=m.input,r=m.record,owner=m.owner,due=m.due,status=m.status;const steps=[assertion(write,key,'version',`EXISTS(SELECT 1 FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.id=? AND x.company_id=? AND x.version=? AND x.status='open' AND ${scope})`,[id,a.companyId,input.expectedVersion,...args(a)]),write.prepare('UPDATE crm_lead_activities SET owner_id=?,due_date=?,status=?,outcome=?,version=version+1,updated_at=? WHERE id=?').bind(owner,due,status,m.outcome,now,id)];
 if(input.action==='reassign')steps.push(assertion(write,key,'recipient',`EXISTS(SELECT 1 FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.id=? AND ${eligible})`,[id]));
 steps.push(write.prepare('INSERT INTO crm_lead_activity_events VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`crm-activity-event-${key}`,id,input.expectedVersion+1,input.action,a.userId,owner,due,input.reason,JSON.stringify(input),now),notify(write,id,key));
 return executeCommand(write,a,key,operation,input,id,input.expectedVersion+1,result,steps,{companyId:a.companyId,branchId:String(r.branch_id),departmentId:r.department_id?String(r.department_id):null});}
}}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function crmActivityWorkspace(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireCRMPurchasedRead(db,a,{kind:'lead',id}),view=await application(a,db).workspace(id);return {...view,...(purchase?{purchase}: {})};});}
export function createCRMActivity(a:WorkContext,raw:unknown,key:string){return translated(()=>{const input=crmActivityCreateSchema.parse(raw);return withCRMPurchasedCommand(a,{key,operation:'crm.activity.create',target:{kind:'activity',id:`crm-activity-${key}`},purpose:'new-work',eligibleOwner:true,sources:[{kind:'lead',id:input.leadId}]},db=>application(a,db).create(input,key));});}
export function changeCRMActivity(a:WorkContext,id:string,raw:unknown,key:string){return translated(()=>{const input=crmActivityChangeSchema.parse(raw);return withCRMPurchasedCommand(a,{key,operation:`crm.activity.${input.action}`,target:{kind:'activity',id},existing:true,eligibleOwner:input.action==='reassign',purpose:'settle-existing',permissions:input.action==='reassign'?['commerce.approve']:[]},db=>application(a,db).change(id,input,key));});}
export function crmActivityWork(a:WorkContext){return translated(async()=>{if(!hasPermission(a,'commerce.read'))return [];const db=(await holdingResources(a.holdingId)).write,purchase=await crmSourceAccess(db,a);if(purchase&&!purchase.history)return [];const rows=await application(a,db).work();return filterCRMPurchasedWork(db,a,purchase&&!purchase.settleExisting?rows.map(row=>({...row,canAct:false,queue:row.queue==='todo'?'waiting' as const:row.queue})):rows);});}
