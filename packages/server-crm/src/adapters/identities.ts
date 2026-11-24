import { nativeCrmExecute } from './native-crm-command';
import { activeCustomerPartiesSql } from './active-customer-parties';
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as pmsProjectionSources } from "@erpx/server-pms/public/commercial-owner-projection-sources";
import 'server-only';

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";

import {BusinessRuleError} from "@erpx/server-shared/index";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {createCRMIdentityApplication,type CRMIdentityPorts,type CRMIdentityMutation} from "@erpx/server-crm/application/identities";
import {crmIdentityCommand} from "@erpx/server-crm/domain/identities";
import {requireCRMPurchasedRead,crmReferenceHistory,crmReferencePurchaseControls,withCRMPurchasedCommand,crmNativeReadFilter,type CRMPurchaseTarget} from "@erpx/server-crm/adapters/purchases";
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);
const ports:CRMIdentityPorts={canGuestReview:hasPermission(a,'commerce.approve')&&hasPermission(a,'pms.read'),canReview:hasPermission(a,'commerce.approve'),canCreate:hasPermission(a,'commerce.sales'),clock:{now:()=>Date.now()},authorize:permission=>{requirePermission(a,permission);},
replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),
async identities(){const write=await database(); const [accounts,contacts,customers]=await Promise.all([
  write.prepare(`SELECT a.id,a.kind,a.name,a.finance_party_id,a.review_evidence,a.version,p.name finance_customer_name FROM crm_accounts a LEFT JOIN (${financeProjectionSources.finance_parties}) p ON p.id=a.finance_party_id AND p.company_id=a.company_id WHERE a.company_id=? AND ${await crmNativeReadFilter('account','a')} AND (?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))) ORDER BY a.updated_at DESC LIMIT 200`).bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare(`SELECT c.id,c.account_id,c.name,c.detail,c.version FROM crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.company_id=? AND ${await crmNativeReadFilter('contact','c')} AND (?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.company_id=c.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))) ORDER BY c.updated_at DESC LIMIT 300`).bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare(activeCustomerPartiesSql).bind(a.companyId).all<Row>()
 ]);
 const guestLinks=hasPermission(a,'pms.read')&&await crmReferenceHistory(write,a,'pms.reservations')?(await write.prepare(`SELECT g.id,g.contact_id,g.guest_id,g.property_id,g.sharing_state,g.sharing_evidence,g.version,p.name property_name
  FROM crm_guest_links g JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=g.property_id AND p.company_id=g.company_id
  JOIN crm_contacts c ON c.id=g.contact_id AND c.company_id=g.company_id JOIN crm_accounts acc ON acc.id=c.account_id
  WHERE g.company_id=? AND ${await crmNativeReadFilter('guest-link','g')} AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR acc.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))) ORDER BY g.created_at DESC LIMIT 200`)
  .bind(a.companyId,a.branchId,a.branchId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results:[];
 return {accounts:accounts.results,contacts:contacts.results,customers:customers.results,guestLinks};},
async contact(contactId){const write=await database();return await write.prepare("SELECT c.id,c.name,c.account_id FROM crm_contacts c JOIN crm_accounts acc ON acc.id=c.account_id WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR acc.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))").bind(contactId,a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).first<Row>();},
async stays(contactId){const write=await database();return (await write.prepare(`SELECT g.guest_id,r.id reservation_id,r.data_json::jsonb->>'state' reservation_state,
  s.id stay_id,s.data_json::jsonb->>'state' stay_state,s.data_json::jsonb->>'arrival' arrival,
  s.data_json::jsonb->>'departure' departure,p.id property_id
  FROM crm_guest_links g JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=g.property_id AND p.company_id=g.company_id
  JOIN (${pmsProjectionSources.pms_records}) r ON r.property_id=p.id AND r.kind='reservation' AND r.data_json::jsonb->>'guestId'=g.guest_id
  LEFT JOIN (${pmsProjectionSources.pms_records}) s ON s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'reservationId'=r.id
  WHERE g.sharing_state='granted' AND g.contact_id=? AND g.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY r.updated_at DESC,s.id LIMIT 201`).bind(contactId,a.companyId,a.branchId,a.branchId).all<Row>()).results;},commit:async(mutation,result)=>commit(await database(),a,mutation,result),
};return createCRMIdentityApplication(ports);}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:CRMIdentityMutation,result:Result):Promise<Result>{
 const {input:i,recordId,key,now,version}=m;
 if(i.action==='create-account') await nativeCrmExecute(a,'crm_stmt_9',[recordId,a.companyId,i.kind,i.name,a.userId,now,now],key);
 if(i.action==='create-contact') await nativeCrmExecute(a,'crm_stmt_10',[recordId,a.companyId,i.accountId,i.name,i.detail,a.userId,now,now],key);
 if(i.action==='link-finance') await nativeCrmExecute(a,'crm_stmt_11',[i.customerId,i.evidence,a.userId,now,i.accountId],key);
 if(i.action==='link-guest'){
  await nativeCrmExecute(a,'crm_stmt_12',[`guest-link-${key}`,a.companyId,i.contactId,i.guestId,i.propertyId,i.evidence,a.userId,now,i.sharingEvidence?'granted':'pending',i.sharingEvidence??'',i.sharingEvidence?a.userId:null,i.sharingEvidence?now:null],key);
  if(i.sharingEvidence) await nativeCrmExecute(a,'crm_stmt_13',[`sharing-${key}`,`guest-link-${key}`,a.companyId,'granted',i.sharingEvidence,a.userId,1,now],key);
 }
 if(i.action==='guest-sharing'){
  await nativeCrmExecute(a,'crm_stmt_14',[i.state,i.evidence,a.userId,now,i.linkId,i.expectedVersion],key);
  await nativeCrmExecute(a,'crm_stmt_15',[`sharing-${key}`,i.linkId,a.companyId,i.state,i.evidence,a.userId,version,now],key);
 }
 return result;
}

async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function crmIdentities(a:WorkContext){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireCRMPurchasedRead(db,a),view=await application(a,db).workspace(),hotel=await crmReferenceHistory(db,a,'pms.reservations');return {...view,canGuestReview:view.canGuestReview&&hotel,...(purchase?{purchase,referencePurchases:await crmReferencePurchaseControls(db,a)}: {})};});}
export function changeCRMIdentity(a:WorkContext,raw:unknown,key:string){return translated(()=>{const input=crmIdentityCommand.parse(raw),target:CRMPurchaseTarget=input.action==='create-account'?{kind:'account',id:`account-${key}`}:input.action==='create-contact'?{kind:'contact',id:`contact-${key}`}:input.action==='link-finance'?{kind:'account',id:input.accountId}:input.action==='guest-sharing'?{kind:'guest-link',id:input.linkId}:{kind:'contact',id:input.contactId};return withCRMPurchasedCommand(a,{key,operation:`crm.identity.${input.action}`,target,existing:input.action!=='create-account'&&input.action!=='create-contact',purpose:input.action==='create-account'||input.action==='create-contact'||input.action==='link-guest'||input.action==='guest-sharing'&&input.state==='granted'?'new-work':'settle-existing',sources:input.action==='create-contact'?[{kind:'account',id:input.accountId}]:[],permissions:input.action==='link-finance'?['commerce.approve']:input.action==='link-guest'||input.action==='guest-sharing'?['commerce.approve','pms.read']:[],capabilities:input.action==='link-guest'||input.action==='guest-sharing'?['pms.reservations']:[]},db=>application(a,db).change(input,key));});}
export function crmContactHistory(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write;await requireCRMPurchasedRead(db,a,{kind:'contact',id});if(!await crmReferenceHistory(db,a,'pms.reservations'))throw new ApiError(403,'PACKAGE_REQUIRED','Retained Hotel reservation history is required.');return application(a,db).history(id);});}
