import {filterCRMPurchasedWork} from "@erpx/server-crm/adapters/record-purchases";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import 'server-only';

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {Row} from '@erpx/server-commerce/public/commerce';

import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {ApiError} from "@erpx/server-runtime/public/http";
import {salesRecipientEligibility} from "@erpx/server-sales/public/sales-recipient";

import {BusinessRuleError} from "@erpx/server-shared/index";
import {createCRMActivityApplication,type CRMActivityPorts} from "@erpx/server-crm/application/activities";
import type {CRMActivity} from "@erpx/server-crm/domain/activities";
import {requireCRMPurchasedRead,crmSourceAccess,crmNativeReadFilter} from "@erpx/server-crm/adapters/purchases";
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
lead:async(id)=>lead(await database(),a,id),activities:async(id)=>rows(await database(),a,id),recipients:async(id)=>recipients(await database(),a,id),
async history(leadId){const write=await database();return (await write.prepare('SELECT e.* FROM crm_lead_activity_events e JOIN crm_lead_activities x ON x.id=e.activity_id WHERE x.company_id=? AND x.lead_id=? ORDER BY e.created_at DESC,e.version DESC').bind(a.companyId,leadId).all<Row>()).results;},
async workRows(){const write=await database();return (await write.prepare(`SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE ${scope} AND ${await crmNativeReadFilter('activity','x')} ORDER BY x.due_date,x.updated_at DESC`).bind(...args(a)).all<CRMActivity>()).results;},
};return createCRMActivityApplication(ports);}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function crmActivityWorkspace(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireCRMPurchasedRead(db,a,{kind:'lead',id}),view=await application(a,db).workspace(id);return {...view,...(purchase?{purchase}: {})};});}
export function crmActivityWork(a:WorkContext){return translated(async()=>{if(!hasPermission(a,'commerce.read'))return [];const db=(await holdingResources(a.holdingId)).write,purchase=await crmSourceAccess(db,a);if(purchase&&!purchase.history)return [];const rows=await application(a,db).work();return filterCRMPurchasedWork(db,a,purchase&&!purchase.settleExisting?rows.map(row=>({...row,canAct:false,queue:row.queue==='todo'?'waiting' as const:row.queue})):rows);});}
