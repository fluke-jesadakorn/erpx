import { nativeCrmExecute } from './native-crm-command';
import { activeCustomerPartiesSql } from './active-customer-parties';
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as pmsProjectionSources } from "@erpx/pms-server/public/commercial-owner-projection-sources";
import 'server-only';

import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {Row} from '@erpx/commerce-server/public/commerce';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";

import {BusinessRuleError} from "@erpx/platform-shared-server/index";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {createCRMIdentityApplication,type CRMIdentityPorts,type CRMIdentityMutation} from "@erpx/crm-server/application/identities";
import {crmIdentityCommand} from "@erpx/crm-server/domain/identities";
import {requireCRMPurchasedRead,crmReferenceHistory,crmReferencePurchaseControls,withCRMPurchasedCommand,crmNativeReadFilter,type CRMPurchaseTarget} from "@erpx/crm-server/adapters/purchases";
import {crmCreatorLeadScopeSql} from "./read-tail-r24-sql";
import {crmContactAccountReadSql} from "./read-tail-r35-sql";
import {crmScopedDescListSql} from "./read-tail-r40-sql";
/** G4 r24: the creator (created_by) lead-scope guards render through the one
 * owner helper; every rendered string stays byte-identical to the prior text. */
const contactScopeSql=crmCreatorLeadScopeSql('acc','l','l.contact_id=c.id','');
const accountOrContactLeadScopeSql=(field:string,ref:string,company:string)=>crmCreatorLeadScopeSql('a','l',`l.${field}=${ref}`,`l.company_id=${company}`);
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);
const ports:CRMIdentityPorts={canGuestReview:hasPermission(a,'commerce.approve')&&hasPermission(a,'pms.read'),canReview:hasPermission(a,'commerce.approve'),canCreate:hasPermission(a,'commerce.sales'),clock:{now:()=>Date.now()},authorize:permission=>{requirePermission(a,permission);},
replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),
async identities(){const write=await database(); const [accounts,contacts,customers]=await Promise.all([
  write.prepare(crmScopedDescListSql(`a.id,a.kind,a.name,a.finance_party_id,a.review_evidence,a.version,p.name finance_customer_name FROM crm_accounts a LEFT JOIN (${financeProjectionSources.finance_parties}) p ON p.id=a.finance_party_id AND p.company_id=a.company_id WHERE a.company_id=? AND ${await crmNativeReadFilter('account','a')} AND ${accountOrContactLeadScopeSql('account_id','a.id','a.company_id')}`,'a.updated_at DESC LIMIT 200')).bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare(crmContactAccountReadSql('c.id,c.account_id,c.name,c.detail,c.version','a',`WHERE c.company_id=? AND ${await crmNativeReadFilter('contact','c')} AND ${accountOrContactLeadScopeSql('contact_id','c.id','c.company_id')} ORDER BY c.updated_at DESC LIMIT 300`)).bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare(activeCustomerPartiesSql).bind(a.companyId).all<Row>()
 ]);
 const guestLinks=hasPermission(a,'pms.read')&&await crmReferenceHistory(write,a,'pms.reservations')?(await write.prepare(crmScopedDescListSql(`g.id,g.contact_id,g.guest_id,g.property_id,g.sharing_state,g.sharing_evidence,g.version,p.name property_name
  FROM crm_guest_links g JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=g.property_id AND p.company_id=g.company_id
  JOIN crm_contacts c ON c.id=g.contact_id AND c.company_id=g.company_id JOIN crm_accounts acc ON acc.id=c.account_id
  WHERE g.company_id=? AND ${await crmNativeReadFilter('guest-link','g')} AND (?::text IS NULL OR p.branch_id=?) AND ${contactScopeSql}`,'g.created_at DESC LIMIT 200'))
  .bind(a.companyId,a.branchId,a.branchId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results:[];
 return {accounts:accounts.results,contacts:contacts.results,customers:customers.results,guestLinks};},
commit:async(mutation,result)=>commit(await database(),a,mutation,result),
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
