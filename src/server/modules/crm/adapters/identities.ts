import { commercialOwnerProjectionSources as financeProjectionSources } from "../../finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as pmsProjectionSources } from "../../pms/public/commercial-owner-projection-sources";
import 'server-only';

import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {Row} from "@/server/modules/commerce/public/commerce";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {hasPermission} from "@/lib/identity_access/permissions";

import {BusinessRuleError} from "../../shared";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {createCRMIdentityApplication,type CRMIdentityPorts,type CRMIdentityMutation} from "../application/identities";
import {crmIdentityCommand} from "../domain/identities";
import {requireCRMPurchasedRead,crmReferenceHistory,crmReferencePurchaseControls,withCRMPurchasedCommand,crmNativeReadFilter,type CRMPurchaseTarget} from "./purchases";
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);
const ports:CRMIdentityPorts={canGuestReview:hasPermission(a,'commerce.approve')&&hasPermission(a,'pms.read'),canReview:hasPermission(a,'commerce.approve'),canCreate:hasPermission(a,'commerce.sales'),clock:{now:()=>Date.now()},authorize:permission=>{requirePermission(a,permission);},
replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),
async identities(){const write=await database(); const [accounts,contacts,customers]=await Promise.all([
  write.prepare(`SELECT a.id,a.kind,a.name,a.finance_party_id,a.review_evidence,a.version,p.name finance_customer_name FROM crm_accounts a LEFT JOIN (${financeProjectionSources.finance_parties}) p ON p.id=a.finance_party_id AND p.company_id=a.company_id WHERE a.company_id=? AND ${await crmNativeReadFilter('account','a')} AND (?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))) ORDER BY a.updated_at DESC LIMIT 200`).bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare(`SELECT c.id,c.account_id,c.name,c.detail,c.version FROM crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.company_id=? AND ${await crmNativeReadFilter('contact','c')} AND (?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.company_id=c.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))) ORDER BY c.updated_at DESC LIMIT 300`).bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare(`SELECT id,name FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name`).bind(a.companyId).all<Row>()
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
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:CRMIdentityMutation,result:Result):Promise<Result>{const {input:i,recordId,operation,key,now,version}=m,steps=[];
 if(i.action==='create-account')steps.push(write.prepare('INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?)').bind(recordId,a.companyId,i.kind,i.name,a.userId,now,now));
 if(i.action==='create-contact')steps.push(
  assertion(write,key,'account',"EXISTS(SELECT 1 FROM crm_accounts x WHERE x.id=? AND x.company_id=? AND (?::text IS NULL OR x.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=x.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))))",[i.accountId,a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId]),
  write.prepare('INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)').bind(recordId,a.companyId,i.accountId,i.name,i.detail,a.userId,now,now));
 if(i.action==='link-finance'){

  steps.push(assertion(write,key,'account',"EXISTS(SELECT 1 FROM crm_accounts x WHERE x.id=? AND x.company_id=? AND x.version=? AND x.finance_party_id IS NULL AND x.created_by<>? AND (?::text IS NULL OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=x.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))))",[i.accountId,a.companyId,i.expectedVersion,a.userId,a.branchId,a.branchId,a.departmentId,a.departmentId]),
   assertion(write,key,'customer',`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active')`,[i.customerId,a.companyId]),
   write.prepare('UPDATE crm_accounts SET finance_party_id=?,review_evidence=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?').bind(i.customerId,i.evidence,a.userId,now,i.accountId));
 }
 if(i.action==='link-guest'){

  steps.push(assertion(write,key,'guest',`EXISTS(SELECT 1 FROM crm_contacts c JOIN (${pmsProjectionSources.pms_records}) g ON g.id=? AND g.kind='guest' JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=g.property_id WHERE c.id=? AND c.company_id=? AND c.created_by<>? AND p.id=? AND p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))))`,[i.guestId,i.contactId,a.companyId,a.userId,i.propertyId,a.companyId,a.branchId,a.branchId,a.branchId,a.branchId,a.departmentId,a.departmentId]),
   write.prepare('INSERT INTO crm_guest_links(id,company_id,contact_id,guest_id,property_id,evidence,reviewed_by,created_at,sharing_state,sharing_evidence,sharing_by,sharing_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(`guest-link-${key}`,a.companyId,i.contactId,i.guestId,i.propertyId,i.evidence,a.userId,now,i.sharingEvidence?'granted':'pending',i.sharingEvidence??'',i.sharingEvidence?a.userId:null,i.sharingEvidence?now:null));
 }

 if(i.action==='link-guest'&&i.sharingEvidence)steps.push(write.prepare('INSERT INTO crm_guest_sharing_events(id,link_id,company_id,state,evidence,actor_id,version,created_at) VALUES(?,?,?,?,?,?,1,?)').bind(`sharing-${key}`,`guest-link-${key}`,a.companyId,'granted',i.sharingEvidence,a.userId,now));
 if(i.action==='guest-sharing'){
  steps.push(assertion(write,key,'guest-sharing-scope',`EXISTS(SELECT 1 FROM crm_guest_links g JOIN crm_contacts c ON c.id=g.contact_id AND c.company_id=g.company_id JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=g.property_id AND p.company_id=g.company_id WHERE g.id=? AND g.company_id=? AND g.version=? AND g.sharing_state<>? AND (?::text IS NULL OR p.branch_id=?) AND (?='revoked' OR c.created_by<>?) AND (?::text IS NULL OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))))`,[i.linkId,a.companyId,i.expectedVersion,i.state,a.branchId,a.branchId,i.state,a.userId,a.branchId,a.branchId,a.departmentId,a.departmentId]),
   write.prepare('UPDATE crm_guest_links SET sharing_state=?,sharing_evidence=?,sharing_by=?,sharing_at=?,version=version+1 WHERE id=? AND version=?').bind(i.state,i.evidence,a.userId,now,i.linkId,i.expectedVersion),
   write.prepare('INSERT INTO crm_guest_sharing_events(id,link_id,company_id,state,evidence,actor_id,version,created_at) VALUES(?,?,?,?,?,?,?,?)').bind(`sharing-${key}`,i.linkId,a.companyId,i.state,i.evidence,a.userId,version,now));
 }
 return executeCommand(write,a,key,operation,i,recordId,version,result,steps);
}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function crmIdentities(a:WorkContext){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireCRMPurchasedRead(db,a),view=await application(a,db).workspace(),hotel=await crmReferenceHistory(db,a,'pms.reservations');return {...view,canGuestReview:view.canGuestReview&&hotel,...(purchase?{purchase,referencePurchases:await crmReferencePurchaseControls(db,a)}: {})};});}
export function changeCRMIdentity(a:WorkContext,raw:unknown,key:string){return translated(()=>{const input=crmIdentityCommand.parse(raw),target:CRMPurchaseTarget=input.action==='create-account'?{kind:'account',id:`account-${key}`}:input.action==='create-contact'?{kind:'contact',id:`contact-${key}`}:input.action==='link-finance'?{kind:'account',id:input.accountId}:input.action==='guest-sharing'?{kind:'guest-link',id:input.linkId}:{kind:'contact',id:input.contactId};return withCRMPurchasedCommand(a,{key,operation:`crm.identity.${input.action}`,target,existing:input.action!=='create-account'&&input.action!=='create-contact',purpose:input.action==='create-account'||input.action==='create-contact'||input.action==='link-guest'||input.action==='guest-sharing'&&input.state==='granted'?'new-work':'settle-existing',sources:input.action==='create-contact'?[{kind:'account',id:input.accountId}]:[],permissions:input.action==='link-finance'?['commerce.approve']:input.action==='link-guest'||input.action==='guest-sharing'?['commerce.approve','pms.read']:[],capabilities:input.action==='link-guest'||input.action==='guest-sharing'?['pms.reservations']:[]},db=>application(a,db).change(input,key));});}
export function crmContactHistory(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write;await requireCRMPurchasedRead(db,a,{kind:'contact',id});if(!await crmReferenceHistory(db,a,'pms.reservations'))throw new ApiError(403,'PACKAGE_REQUIRED','Retained Hotel reservation history is required.');return application(a,db).history(id);});}
