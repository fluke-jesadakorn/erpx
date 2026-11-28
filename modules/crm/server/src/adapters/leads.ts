import { nativeCrmExecute } from './native-crm-command';
import { activeCustomerPartiesSql } from './active-customer-parties';
import { leadOwnerNameSql } from './lead-owner-name';
import { crmGuestLinkReservationSourceSql } from './read-tail-r26-sql';
import { crmCampaignOptionsSql } from './read-tail-r27-sql';
import { crmOpportunityProbeSql } from './read-tail-r28-sql';
import { nativeCrmCaseEventHistory } from './native-crm-history';
import { crmLeadOwnerGuardSql } from './read-tail-r29-sql';
import { crmCommerceOrderReadSql } from './read-tail-r32-sql';
import { crmSingleKeySourceSql } from './read-tail-r37-sql';
import { crmScopedDescListSql, crmDistinctOwnerSql } from './read-tail-r40-sql';
import { crmSelectBodySql } from './read-tail-r46-sql';
import { nativeCrmActivityWork } from './native-activity-workspace';
import {filterCRMPurchasedWork} from "@erpx/crm-server/adapters/record-purchases";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/commerce-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/identity-access-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as marketingProjectionSources } from "@erpx/marketing-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/people-payroll-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as pmsProjectionSources } from "@erpx/pms-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as salesProjectionSources } from "@erpx/sales-server/public/commercial-owner-projection-sources";
import { supportCustomerSourceCases } from "@erpx/support-server/public/sales-projection";
import 'server-only';

import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";

import type {Row} from '@erpx/commerce-server/public/commerce';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";

import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {salesRecipientEligibility} from "@erpx/sales-server/public/sales-recipient";
import {BusinessRuleError} from "@erpx/platform-shared-server/index";
import {nativeCrmLeadNotification as notify} from "./native-crm-notification";
import {createCRMApplication,type CRMRepository,type CRMMutation} from "@erpx/crm-server/application/leads";
import {crmLeadSchema,crmLeadChangeSchema,crmOpportunitySchema} from "@erpx/crm-server/domain/leads";
import {requireCRMPurchasedRead,crmSourceAccess,crmReferenceHistory,crmReferencePurchaseControls,withCRMPurchasedCommand,crmLeadCreateReferences,crmOpportunityReference,crmNativeReadFilter,crmLeadVersionProjection,crmBranchOptionsSql} from "@erpx/crm-server/adapters/purchases";
export {crmLeadSchema,crmLeadChangeSchema,crmOpportunitySchema} from "@erpx/crm-server/domain/leads";
const scope="l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)",args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId],eligible=salesRecipientEligibility('l','l.owner_id');
async function records(db:SqlDatabase,a:WorkContext,id?:string){return (await db.prepare(crmSelectBodySql(`l.*,v.id opportunity_id,v.stage,v.expected_minor,v.currency,v.close_date,v.quote_id,v.quote_version,v.sales_id,v.reservation_id,v.win_source,v.won_at,v.version opportunity_version,v.customer_id,p.name customer_name,ca.finance_party_id account_customer_id,ca.version account_version,q.state quote_state,q.version current_quote_version,${leadOwnerNameSql} owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_leads l LEFT JOIN crm_accounts ca ON ca.id=l.account_id AND ca.company_id=l.company_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id LEFT JOIN (${financeProjectionSources.finance_parties}) p ON p.id=v.customer_id AND p.company_id=l.company_id LEFT JOIN (${commerceProjectionSources.commerce_orders}) q ON q.id=v.quote_id WHERE ${scope} AND ${await crmNativeReadFilter('lead','l')} AND (?::text IS NULL OR l.id=?) ORDER BY l.updated_at DESC,l.id`)).bind(...args(a),id??null,id??null).all<Row>()).results;}
/** Narrow source projection for Executive evidence; never load CRM contact or brief fields. */
function ownerGuard(db:SqlDatabase,a:WorkContext,id:string,key:string,v:number){return assertion(db,key,'crm-owner',crmLeadOwnerGuardSql(scope,eligible),[id,v,a.userId,...args(a)]);}

function application(a:WorkContext,provided?:SqlDatabase,includeSupport=true){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);
return createCRMApplication({actorId:a.userId,canRead:hasPermission(a,'commerce.read'),canSales:hasPermission(a,'commerce.sales'),canApprove:hasPermission(a,'commerce.approve'),canPms:hasPermission(a,'pms.read'),canSupport:includeSupport&&hasPermission(a,'support.read'),clock:{now:()=>Date.now()},authorize:permission=>{requirePermission(a,permission);},activityWork:async()=>nativeCrmActivityWork(a),async repository(){const write=await database();const repository:CRMRepository={
records:(id)=>records(write,a,id),
async branches(){return (await write.prepare(crmBranchOptionsSql('',true)).bind(a.companyId,a.branchId,a.branchId).all<Row>()).results;},
async campaigns(){if(!await crmReferenceHistory(write,a,'sales.marketing'))return [];return (await write.prepare(crmCampaignOptionsSql(marketingProjectionSources.marketing_campaigns,'marketing_campaigns')).bind(a.companyId).all<Row>()).results;},
async evidence(id){return await write.prepare(`${crmLeadVersionProjection}v.stage,v.expected_minor,v.currency,v.close_date,v.version opportunity_version
 FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id WHERE ${scope} AND v.id=? LIMIT 1`).bind(...args(a),id).first<Row>();},
async currency(){return write.prepare(crmSingleKeySourceSql('currency',commerceProjectionSources.commerce_settings,'commerce_settings')).bind(a.companyId).first<Row>();},
async recipients(id){return (await write.prepare(crmDistinctOwnerSql(`c.owner_id id,p.legal_name name FROM (${crmSingleKeySourceSql('DISTINCT user_id owner_id',identity_accessProjectionSources.assignments,'assignments')}) c JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=c.owner_id AND p.company_id=? JOIN crm_leads l ON l.id=? WHERE ${salesRecipientEligibility('l','c.owner_id')} ORDER BY p.legal_name`)).bind(a.companyId,a.companyId,id).all<{id:string;name:string}>()).results;},
async customers(){return (await write.prepare(activeCustomerPartiesSql).bind(a.companyId).all<Row>()).results;},
async quotes(row){return (await write.prepare(crmCommerceOrderReadSql('o.id,o.number,o.version,o.state','o',`WHERE o.company_id=? AND o.party_id=? AND o.branch_id=? AND o.department_id IS NOT DISTINCT FROM ?::text AND o.kind='quote' AND o.state='confirmed' ORDER BY o.updated_at DESC`)).bind(a.companyId,row.customer_id,row.branch_id,row.department_id).all<Row>()).results;},
async sales(row){return (await write.prepare(crmCommerceOrderReadSql('id,number,version,state','commerce_orders',`WHERE company_id=? AND source_id=? AND kind='sales' AND state IN ('confirmed','partially_fulfilled','fulfilled') ORDER BY updated_at DESC`)).bind(a.companyId,row.quote_id).all<Row>()).results;},
async guestCandidates(){if(!await crmReferenceHistory(write,a,'pms.reservations'))return [];return (await write.prepare(crmScopedDescListSql(`g.id,g.data_json::jsonb->>'name' name,p.id property_id,p.name property_name FROM (${pmsProjectionSources.pms_records}) g JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=g.property_id AND p.company_id=? WHERE g.kind='guest' AND (?::text IS NULL OR p.branch_id=?)`,'g.updated_at DESC LIMIT 200')).bind(a.companyId,a.branchId,a.branchId).all<Row>()).results;},
async reservations(row){if(!await crmReferenceHistory(write,a,'pms.reservations'))return [];return (await write.prepare(crmGuestLinkReservationSourceSql(`r.id,r.data_json::jsonb->>'number' number,r.data_json::jsonb->>'state' state,p.id property_id`,pmsProjectionSources.pms_properties,pmsProjectionSources.pms_records,'',' ORDER BY r.updated_at DESC',' ')).bind(row.contact_id,a.companyId,a.branchId,a.branchId).all<Row>()).results;},
async sourceOrders(row){return (await write.prepare(crmCommerceOrderReadSql('id,number,kind,state,order_date','commerce_orders',`WHERE company_id=? AND party_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC LIMIT 50`)).bind(a.companyId,row.account_customer_id,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results;},
sourceSupport: row => supportCustomerSourceCases(write, a, String(row.account_customer_id)),

async sourceFollowups(row){return (await write.prepare(crmScopedDescListSql(`f.id,f.status,f.due_date,f.order_id FROM (${salesProjectionSources.customer_followups}) f JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id WHERE f.company_id=? AND o.party_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)`,'f.updated_at DESC LIMIT 50')).bind(a.companyId,row.account_customer_id,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results;},
async history(id){return nativeCrmCaseEventHistory(a,id);},

replay:(key,operation,id,input)=>replayCommand(write,a,key,operation,id,input),commit:(mutation,result)=>commit(write,a,mutation,result),
};return repository;}});}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:CRMMutation,result:Result):Promise<Result>{
 const {id,key,now}=m;
 switch(m.action){
  case "create":{
   const i=m.input,accountId=m.accountId,contactId=m.contactId;
   if(!i.accountId) await nativeCrmExecute(a,'crm_stmt_2',[accountId,a.companyId,'individual',i.contactName,a.userId,now,now],key);
   if(!i.contactId) await nativeCrmExecute(a,'crm_stmt_3',[contactId,a.companyId,accountId,i.contactName,i.contactDetail,a.userId,now,now],key);
   await nativeCrmExecute(a,'crm_stmt_4',[id,a.companyId,i.branchId,a.departmentId,i.title,i.contactName,i.contactDetail,i.source,i.brief,a.userId,i.dueDate,a.userId,now,now,accountId,contactId],key);
   await nativeCrmExecute(a,'crm_stmt_1',[`crm-event-${key}-lead`,id,null,1,'create',a.userId,i.brief,JSON.stringify(i),now],key);
   return result;
  }
  case "lead":{
   const i=m.input;
   if(i.action==='reassign') await nativeCrmExecute(a,'crm_stmt_5',[i.ownerId,i.dueDate,now,id],key);
   else await nativeCrmExecute(a,'crm_stmt_6',[i.action==='reopen'?'open':'disqualified',now,id],key);
   await nativeCrmExecute(a,'crm_stmt_1',[`crm-event-${key}-lead`,id,null,i.expectedVersion+1,i.action,a.userId,i.reason,JSON.stringify(i),now],key);
   return result;
  }
  case "opportunity":{
   const i=m.input,opportunityId=m.opportunityId,stage=m.stage;
   if(i.action==='win') await nativeCrmExecute(a,'crm_stmt_17',[i.salesId,now,i.salesId,now,opportunityId],key);
   else await nativeCrmExecute(a,'crm_stmt_7',[stage,i.action==='reopen',i.action==='reopen',now,opportunityId],key);
   await nativeCrmExecute(a,'crm_stmt_8',[now,id],key);
   await nativeCrmExecute(a,'crm_stmt_1',[`crm-event-${key}-lead`,id,null,i.leadVersion+1,`opportunity-${i.action}`,a.userId,i.reason,JSON.stringify(i),now],key);
   await nativeCrmExecute(a,'crm_stmt_1',[`crm-event-${key}-deal`,id,opportunityId,i.expectedVersion+1,i.action,a.userId,i.reason,JSON.stringify({...i,stage}),now],key);
   return result;
  }
 }
}

async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function getCRMLead(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write;await requireCRMPurchasedRead(db,a,{kind:'lead',id});return application(a,db).getLead(id);});}
export function crmOpportunityEvidenceRow(a:WorkContext,id:string,provided?:SqlDatabase){return translated(async()=>{const db=provided??(await holdingResources(a.holdingId)).write;await requireCRMPurchasedRead(db,a);const source=await db.prepare(crmOpportunityProbeSql('lead_id','id=?')).bind(id).first<{lead_id:string}>();if(source)await requireCRMPurchasedRead(db,a,{kind:'lead',id:source.lead_id});return application(a,db).evidence(id);});}
export function crmWorkspace(a:WorkContext){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireCRMPurchasedRead(db,a),view=await application(a,db).workspace();return {...view,...(purchase?{purchase,referencePurchases:await crmReferencePurchaseControls(db,a)}: {})};});}
export function crmDetail(a:WorkContext,id:string,options:{includeSupport?:boolean}={}){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireCRMPurchasedRead(db,a,{kind:'lead',id}),view=await application(a,db,options.includeSupport).detail(id);return {...view,...(purchase?{purchase,referencePurchases:await crmReferencePurchaseControls(db,a)}: {})};});}
function create(a:WorkContext,raw:unknown,key:string,provided?:SqlDatabase){const input=crmLeadSchema.parse(raw);return withCRMPurchasedCommand(a,{key,operation:'crm.lead.create',target:{kind:'lead',id:`lead-${key}`},purpose:'new-work',eligibleOwner:true,createInput:input,capabilities:[...(input.campaignId||input.sourceKind==='form'?['sales.marketing']:[]),...(input.sourceKind==='pms'?['pms.reservations']:[])]},async db=>{await crmLeadCreateReferences(db,a,input);return application(a,db).create(input,key);},provided);}
export function createCRMLead(a:WorkContext,raw:unknown,key:string){return translated(()=>create(a,raw,key));}
export function changeCRMLead(a:WorkContext,id:string,raw:unknown,key:string){return translated(()=>{const input=crmLeadChangeSchema.parse(raw);return withCRMPurchasedCommand(a,{key,operation:`crm.lead.${input.action}`,target:{kind:'lead',id},existing:true,eligibleOwner:input.action!=='reopen',purpose:input.action==='reopen'?'new-work':'settle-existing',permissions:input.action==='reassign'||input.action==='reopen'?['commerce.approve']:[]},db=>application(a,db).changeLead(id,input,key));});}
export function changeCRMOpportunity(a:WorkContext,id:string,raw:unknown,key:string){return translated(()=>{const input=crmOpportunitySchema.parse(raw);return withCRMPurchasedCommand(a,{key,operation:`crm.opportunity.${input.action}`,target:{kind:'lead',id},existing:true,opportunity:true,eligibleOwner:input.action!=='reopen',purpose:input.action==='reopen'?'new-work':'settle-existing',permissions:input.action==='reopen'?['commerce.approve']:[],capabilities:input.action==='win'?['sales.orders']:[]},async db=>{if(input.action==='win')await crmOpportunityReference(db,a,'sales',input.salesId);return application(a,db).changeOpportunity(id,input,key);});});}
export function crmWork(a:WorkContext){return translated(async()=>{if(!hasPermission(a,'commerce.read'))return [];const db=(await holdingResources(a.holdingId)).write,purchase=await crmSourceAccess(db,a);if(purchase&&!purchase.history)return [];const rows=await application(a,db).work();return filterCRMPurchasedWork(db,a,purchase&&!purchase.settleExisting?rows.map(row=>({...row,canAct:false,queue:row.queue==='todo'?'waiting' as const:row.queue})):rows);});}

/** Use the caller's transaction for an atomic source handoff. */
export function createCRMLeadInTransaction(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return translated(()=>create(a,raw,key,db));}
