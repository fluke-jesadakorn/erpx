import "server-only";
import {randomUUID} from "node:crypto";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {workContextGuards,financialConstraintError,replayCommand} from "@erpx/server-workflow/public/commands";
import {commercePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure,type PackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {crmFinanceCustomerSources,crmFinanceSourceReadLocks} from "@erpx/server-finance/public/crm-purchase-sources";
import {crmCommerceOrderSources,crmCommerceSourceReadLocks} from "@erpx/server-commerce/public/crm-purchase-sources";
import {crmHotelGuestSources,crmHotelReservationSources,crmHotelSourceReadLocks} from "@erpx/server-pms/public/crm-purchase-sources";
import {crmMarketingCandidateSources,crmMarketingCampaignSources,crmMarketingSourceReadLocks,marketingCandidateLeadInput,marketingActionSchema} from "@erpx/server-marketing/public/crm-purchase-sources";
import type {Row} from "@erpx/server-commerce/public/commerce";
import {crmLeadSchema,type CRMLeadInput} from "@erpx/server-crm/domain/leads";
import {salesRecipientEligibility} from "@erpx/server-sales/public/sales-recipient";

export type CRMPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export type CRMPurchaseTarget={kind:"lead"|"activity"|"account"|"contact"|"guest-link";id:string};
type Source={id:string;company_id:string;branch_id:string|null;department_id:string|null;witness:string};
type Check=()=>Promise<void>;
type Handoff={id:string;key:string;raw:unknown;input:CRMLeadInput};
type Composition={companyId:string;checks:Check[];handoff?:Handoff};
const compositions=new WeakMap<SqlDatabase,Composition>();

const accountIdentity="jsonb_build_array(account.id,account.company_id,account.kind,account.created_by)",contactIdentity="jsonb_build_array(contact.id,contact.company_id,contact.account_id,contact.created_by)";
export const crmPurchaseAccountRows=`SELECT account.id,account.company_id,NULL::text branch_id,NULL::text department_id,account.created_by,${accountIdentity} witness FROM crm_accounts account LEFT JOIN (${crmFinanceCustomerSources}) customer ON customer.id=account.finance_party_id AND customer.company_id=account.company_id AND customer.kind='customer' WHERE account.finance_party_id IS NULL OR customer.id IS NOT NULL`;
export const crmPurchaseContactRows=`SELECT contact.id,contact.company_id,NULL::text branch_id,NULL::text department_id,account.created_by,${contactIdentity} witness FROM crm_contacts contact JOIN (${crmPurchaseAccountRows}) account ON account.id=contact.account_id AND account.company_id=contact.company_id`;
export const crmPurchaseLeadRows=`SELECT lead.id,lead.company_id,lead.branch_id,lead.department_id,
 jsonb_build_object('lead',jsonb_build_array(lead.id,lead.company_id,lead.branch_id,lead.department_id,lead.account_id,lead.contact_id,lead.created_by),'account',${accountIdentity},'contact',${contactIdentity},'customer',jsonb_build_array(customer.id,customer.company_id,customer.kind)) witness
 FROM crm_leads lead JOIN crm_accounts account ON account.id=lead.account_id AND account.company_id=lead.company_id
 JOIN crm_contacts contact ON contact.id=lead.contact_id AND contact.account_id=account.id AND contact.company_id=lead.company_id
 LEFT JOIN (${crmFinanceCustomerSources}) customer ON customer.id=account.finance_party_id AND customer.company_id=lead.company_id AND customer.kind='customer'
 WHERE account.finance_party_id IS NULL OR customer.id IS NOT NULL`;
export const crmPurchaseActivityRows=`SELECT activity.id,activity.company_id,lead.branch_id,lead.department_id,jsonb_build_object('activity',jsonb_build_array(activity.id,activity.company_id,activity.lead_id,activity.created_by),'lead',lead.witness) witness FROM crm_lead_activities activity JOIN (${crmPurchaseLeadRows}) lead ON lead.id=activity.lead_id AND lead.company_id=activity.company_id`;
export const crmPurchaseOpportunityRows=`SELECT lead.id,lead.company_id,lead.branch_id,lead.department_id,jsonb_build_object('lead',lead.witness,'opportunity',jsonb_build_array(opportunity.id,opportunity.lead_id,opportunity.customer_id,opportunity.currency,opportunity.quote_id,opportunity.sales_id,opportunity.reservation_id),'quote',to_jsonb(quote),'sale',to_jsonb(sale),'reservation',to_jsonb(reservation)) witness
 FROM (${crmPurchaseLeadRows}) lead JOIN crm_leads native ON native.id=lead.id JOIN crm_accounts account ON account.id=native.account_id
 JOIN crm_opportunities opportunity ON opportunity.lead_id=lead.id JOIN (${crmFinanceCustomerSources}) customer ON customer.id=opportunity.customer_id AND customer.company_id=lead.company_id AND customer.kind='customer' AND account.finance_party_id=customer.id AND account.reviewed_by IS NOT NULL
 LEFT JOIN (${crmCommerceOrderSources}) quote ON quote.id=opportunity.quote_id AND quote.company_id=lead.company_id AND quote.branch_id=lead.branch_id AND quote.department_id IS NOT DISTINCT FROM lead.department_id AND quote.party_id=customer.id AND quote.currency=opportunity.currency AND quote.kind='quote'
 LEFT JOIN (${crmCommerceOrderSources}) sale ON sale.id=opportunity.sales_id AND sale.company_id=lead.company_id AND sale.branch_id=lead.branch_id AND sale.department_id IS NOT DISTINCT FROM lead.department_id AND sale.party_id=customer.id AND sale.currency=opportunity.currency AND sale.kind='sales' AND sale.source_id=quote.id
 LEFT JOIN (${crmHotelReservationSources}) reservation ON reservation.id=opportunity.reservation_id AND reservation.company_id=lead.company_id AND reservation.branch_id=lead.branch_id AND EXISTS(SELECT 1 FROM crm_guest_links guest_link WHERE guest_link.contact_id=native.contact_id AND guest_link.company_id=lead.company_id AND guest_link.guest_id=reservation.guest_id AND guest_link.property_id=reservation.property_id)
 WHERE (opportunity.quote_id IS NULL OR quote.id IS NOT NULL) AND (opportunity.sales_id IS NULL OR sale.id IS NOT NULL) AND (opportunity.reservation_id IS NULL OR reservation.id IS NOT NULL)`;
export const crmPurchaseGuestLinkRows=`SELECT link.id,link.company_id,guest.branch_id,NULL::text department_id,account.created_by,
 jsonb_build_object('link',jsonb_build_array(link.id,link.company_id,link.contact_id,link.guest_id,link.property_id),'contact',${contactIdentity},'guest',to_jsonb(guest)) witness FROM crm_guest_links link
 JOIN crm_contacts contact ON contact.id=link.contact_id AND contact.company_id=link.company_id
 JOIN crm_accounts account ON account.id=contact.account_id AND account.company_id=link.company_id
 JOIN (${crmHotelGuestSources}) guest ON guest.id=link.guest_id AND guest.property_id=link.property_id AND guest.company_id=link.company_id`;
const sourceRows={lead:crmPurchaseLeadRows,activity:crmPurchaseActivityRows,account:crmPurchaseAccountRows,contact:crmPurchaseContactRows,"guest-link":crmPurchaseGuestLinkRows};
export async function crmNativeReadFilter(kind:CRMPurchaseTarget['kind'],alias:'l'|'x'|'a'|'c'|'g'){
 if(await commercePackageEnforcementMode()==='disabled')return 'true';
 return `EXISTS(SELECT 1 FROM (${sourceRows[kind]}) crm_native WHERE crm_native.id=${alias}.id AND crm_native.company_id=${alias}.company_id)`+(kind==='lead'?` AND (NOT EXISTS(SELECT 1 FROM crm_opportunities native_opportunity WHERE native_opportunity.lead_id=${alias}.id) OR EXISTS(SELECT 1 FROM (${crmPurchaseOpportunityRows}) native_opportunity WHERE native_opportunity.id=${alias}.id AND native_opportunity.company_id=${alias}.company_id))`:'');
}
/** Writer source leases avoid SHARE→write upgrades across distinct Companies.
 * Shared customer, quote and Hotel sources remain owned by their own modules. */
export function crmPurchaseSourceReadLocks(database:SqlDatabase){return [
 ...crmFinanceSourceReadLocks(database),...crmCommerceSourceReadLocks(database),...crmHotelSourceReadLocks(database),...crmMarketingSourceReadLocks(database),
 database.prepare("LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE")];}

async function source(database:SqlDatabase,actor:WorkContext,target:CRMPurchaseTarget):Promise<Source>{
 const identity=target.kind==="account"?"l.account_id":target.kind==="contact"?"l.contact_id":"l.contact_id";
 const scoped=target.kind==="lead"||target.kind==="activity"
  ?"(?::text IS NULL OR actual.branch_id=?) AND (?::text IS NULL OR actual.department_id=?)"
  :`(?::text IS NULL OR actual.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE ${identity}=${target.kind==="guest-link"?"(SELECT contact_id FROM crm_guest_links WHERE id=actual.id)":"actual.id"} AND l.company_id=actual.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))`;
 const args=target.kind==="lead"||target.kind==="activity"?[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]:[actor.branchId,actor.userId,actor.branchId,actor.departmentId,actor.departmentId];
 const row=await database.prepare(`SELECT id,company_id,branch_id,department_id,witness::text witness FROM (${sourceRows[target.kind]}) actual WHERE id=? AND company_id=? AND ${scoped}`).bind(target.id,actor.companyId,...args).first<Source>();
 if(!row||target.kind==="guest-link"&&actor.branchId&&row.branch_id!==actor.branchId)throw new ApiError(404,"CRM_SOURCE_NOT_FOUND","The original CRM source is unavailable in this workspace.");
 if(target.kind==='lead'&&await database.prepare('SELECT 1 FROM crm_opportunities WHERE lead_id=?').bind(target.id).first()&&!await database.prepare(`SELECT 1 FROM (${crmPurchaseOpportunityRows}) opportunity WHERE id=? AND company_id=?`).bind(target.id,actor.companyId).first())throw new ApiError(404,'CRM_SOURCE_NOT_FOUND','The actual original opportunity source is unavailable.');
 return row;
}
export async function requireCRMCurrentAuthority(database:SqlDatabase,actor:WorkContext,permissions:readonly string[],key:string,record?:Source){
 const scope={company_id:record?.company_id??actor.companyId,branch_id:record?.branch_id??actor.branchId,department_id:record?.department_id??actor.departmentId};
 for(const permission of new Set(permissions)){
  if(!hasPermission(actor,permission))throw new ApiError(403,"CRM_ACCESS_CHANGED","Choose a current authorized sales role.");
  const row=await database.prepare(`SELECT ${assignmentReadSql("crm_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) crm_scope`).bind(actor.assignmentId,actor.userId,actor.role,scope.company_id,scope.branch_id,scope.department_id).first<{allowed:boolean}>();
  if(!row?.allowed)throw new ApiError(403,"CRM_ACCESS_CHANGED","The original sales assignment is no longer current.");
 }
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
export async function crmSourceAccess(database:SqlDatabase,actor:WorkContext,target?:CRMPurchaseTarget):Promise<CRMPurchaseControls|undefined>{
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;
 const actual=target?await source(database,actor,target):undefined;await requireCRMCurrentAuthority(database,actor,["commerce.read"],`crm-read:${randomUUID()}`,actual);
 const access=(await readPurchasedAccesses(database,mode,actor.companyId,["sales.crm"]))["sales.crm"];
 return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
export async function requireCRMPurchasedRead(database:SqlDatabase,actor:WorkContext,target?:CRMPurchaseTarget){const access=await crmSourceAccess(database,actor,target);if(access&&!access.history)throw new ApiError(403,"PACKAGE_REQUIRED","Sales CRM or retained history is required.");return access;}
export async function crmReferenceHistory(database:SqlDatabase,actor:WorkContext,capability:string){const mode=await commercePackageEnforcementMode();if(mode==='disabled')return true;
 const permission=capability==='pms.reservations'?'pms.read':capability==='sales.marketing'?'records.read':'commerce.read';
 if(!hasPermission(actor,permission)||capability==='sales.marketing'&&!actor.modules.includes('marketing'))return false;
 const current=await database.prepare(`SELECT ${assignmentReadSql('crm_reference',permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) crm_reference`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();
 return !!current?.allowed&&!!(await readPurchasedAccesses(database,mode,actor.companyId,[capability]))[capability].history;
}
/** Minimal references for controls; no grants, assignments or source snapshots
 * are serialized to the client. Actual commands reauthorize their native source. */
export async function crmReferencePurchaseControls(database:SqlDatabase,actor:WorkContext){const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;const access=await readPurchasedAccesses(database,mode,actor.companyId,['sales.marketing','pms.reservations']),result:{marketing:CRMPurchaseControls;hotel:CRMPurchaseControls}={marketing:{newWork:false,settleExisting:false,history:false,revision:''},hotel:{newWork:false,settleExisting:false,history:false,revision:''}};
 for(const [kind,capability] of [['marketing','sales.marketing'],['hotel','pms.reservations']] as const){const allowed=await crmReferenceHistory(database,actor,capability),value=access[capability];result[kind]={newWork:allowed&&value.newWork,settleExisting:allowed&&value.settleExisting,history:allowed&&value.history,revision:value.revision};}return result;
}

type CRMCommand={key:string;operation:string;target:CRMPurchaseTarget;purpose:CapabilityPurpose;permissions?:readonly string[];existing?:boolean;sources?:readonly CRMPurchaseTarget[];capabilities?:readonly string[];createInput?:CRMLeadInput;eligibleOwner?:boolean;opportunity?:boolean};
async function owned<Result>(owner:SqlDatabase,actor:WorkContext,mode:PackageEnforcementMode,run:(database:SqlDatabase,composition:Composition)=>Promise<Result>){
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"CRM_TRANSACTION_REQUIRED","CRM requires its original owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  if(mode==="disabled")return run(database,{companyId:actor.companyId,checks:[]});
  const locks=companyPurchaseReadLocks(database,actor.companyId);await database.batch([locks[0],...crmPurchaseSourceReadLocks(database),...locks.slice(1)]);
  const composition:Composition={companyId:actor.companyId,checks:[]};compositions.set(database,composition);
  try{const result=await run(database,composition);await flushFinanceDeferredEffects(database);for(const check of composition.checks)await check();return result;}finally{compositions.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export async function withCRMPurchasedCommand<Result>(actor:WorkContext,command:CRMCommand,run:(database:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase){
 const mode=await commercePackageEnforcementMode(),owner=(await holdingResources(actor.holdingId)).write;if(mode==="disabled")return run(provided??owner);
 const execute=async(database:SqlDatabase)=>{
  const composition=compositions.get(database);if(!composition||composition.companyId!==actor.companyId)throw new ApiError(503,"CRM_COMPOSITION_REQUIRED","The handoff requires its complete owned CRM transaction.");
  const key=`crm-purchase:${randomUUID()}`,receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,command.target.id,actor.userId,actor.companyId).first();
  const handoff=composition.handoff,retainedHandoff=!!(handoff&&command.operation==="crm.lead.create"&&command.key===`${handoff.key}-crm`&&command.target.id===`lead-${handoff.key}-crm`&&command.createInput&&JSON.stringify(command.createInput)===JSON.stringify(handoff.input));
  if(provided&&command.operation==="crm.lead.create"&&!retainedHandoff)throw new ApiError(503,"CRM_HANDOFF_SOURCE_REQUIRED","An exact reviewed Marketing candidate is required.");
  const purpose=receipt?"history" as const:retainedHandoff?"settle-existing" as const:command.purpose,before=command.existing||receipt?await source(database,actor,command.target):undefined,sources=await Promise.all((command.sources??[]).map(async target=>({target,source:await source(database,actor,target)})));
  const permissions=["commerce.read","commerce.sales",...(command.permissions??[])],capabilities=[...new Set(["sales.crm",...(command.capabilities??[])])];
  await requireCRMCurrentAuthority(database,actor,permissions,key,before);
  const scopes=capabilities.map(capability=>({companyId:actor.companyId,capability,purpose})),boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await database.batch(boundaries.flatMap(item=>item.boundary.before));const result=await run(database),after=await source(database,actor,command.target);
  if(before&&after.witness!==before.witness)throw new ApiError(409,"CRM_SOURCE_CHANGED","The original CRM source changed.");
  const opportunity=command.opportunity?await database.prepare(`SELECT witness::text witness FROM (${crmPurchaseOpportunityRows}) opportunity WHERE id=? AND company_id=?`).bind(command.target.id,actor.companyId).first<{witness:string}>():null;if(command.opportunity&&!opportunity)throw new ApiError(409,'CRM_SOURCE_CHANGED','The actual linked opportunity source is unavailable.');
  const check=async()=>{const current=await source(database,actor,command.target);if(current.witness!==after.witness)throw new ApiError(409,"CRM_SOURCE_CHANGED","The original CRM source changed before commit.");for(const item of sources)if((await source(database,actor,item.target)).witness!==item.source.witness)throw new ApiError(409,"CRM_SOURCE_CHANGED","The captured CRM identity changed before commit.");
   if(opportunity&&(await database.prepare(`SELECT witness::text witness FROM (${crmPurchaseOpportunityRows}) opportunity WHERE id=? AND company_id=?`).bind(command.target.id,actor.companyId).first<{witness:string}>())?.witness!==opportunity.witness)throw new ApiError(409,'CRM_SOURCE_CHANGED','The actual linked opportunity source changed before commit.');
   await requireCRMCurrentAuthority(database,actor,permissions,key,current);
   if(command.eligibleOwner&&!receipt){const valid=command.target.kind==='activity'?await database.prepare(`SELECT 1 FROM crm_lead_activities c JOIN crm_leads l ON l.id=c.lead_id AND l.company_id=c.company_id WHERE c.id=? AND c.company_id=? AND ${salesRecipientEligibility('l','c.owner_id')}`).bind(command.target.id,actor.companyId).first():await database.prepare(`SELECT 1 FROM crm_leads l WHERE l.id=? AND l.company_id=? AND ${salesRecipientEligibility('l','l.owner_id')}`).bind(command.target.id,actor.companyId).first();if(!valid)throw new ApiError(409,'CRM_OWNER_CHANGED','The assigned original salesperson is no longer eligible.');}
   const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));boundaries[0]?.boundary.observe?.(observed);};
  await check();composition.checks.push(check);return result;
 };
 if(provided){const ownedRuntime=owner as SqlDatabase&{assertActiveTransaction?:(database:SqlDatabase)=>void};if(!ownedRuntime.assertActiveTransaction)throw new ApiError(503,"CRM_TRANSACTION_OWNERSHIP_REQUIRED","CRM requires an active owned Holding transaction.");ownedRuntime.assertActiveTransaction(provided);return execute(provided);}
 return owned(owner,actor,mode,database=>execute(database));
}

/** Marketing owns this exact parent/child protocol. The private transaction
 * identity, captured native candidate and final immutable receipts cannot be
 * replaced by a supplied connection or a form source label. */
export async function crmMarketingCandidateTransaction<Result>(actor:WorkContext,id:string,raw:unknown,key:string,run:(database:SqlDatabase,now?:number)=>Promise<Result>){
 const mode=await commercePackageEnforcementMode(),owner=(await holdingResources(actor.holdingId)).write;
 if(mode==="disabled")return owned(owner,actor,mode,database=>run(database));
 const input=marketingActionSchema.parse(raw);if(input.action!=="handoff")throw new ApiError(422,"CRM_HANDOFF_SOURCE_REQUIRED","Only the existing candidate handoff uses this composition.");
 return owned(owner,actor,mode,async(database,composition)=>{
  const current=async(key:string)=>{await requireCRMCurrentAuthority(database,actor,["records.read","records.write","commerce.read","commerce.sales"],key);if(!actor.modules.includes("marketing"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Marketing.");const selected=await database.prepare("SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?").bind(actor.assignmentId).first<{allowed:boolean}>();if(!selected?.allowed)throw new ApiError(403,"CRM_ACCESS_CHANGED","The original Marketing assignment is no longer selected.");};
  await current(`marketing-handoff:${randomUUID()}`);
  const candidate=await database.prepare(`SELECT * FROM (${crmMarketingCandidateSources}) candidate WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)`).bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Row>();
  if(!candidate)throw new ApiError(404,"MARKETING_CANDIDATE_NOT_FOUND","The captured consented candidate is unavailable.");
  const prior=await replayCommand<Record<string,unknown>>(database,actor,key,"marketing.candidate.handoff",id,input),purpose=prior?"history" as const:"settle-existing" as const;
  if(!prior&&(candidate.status!=="pending"||candidate.created_by===actor.userId||Number(candidate.version)!==input.expectedVersion))throw new ApiError(409,"CONFLICT","Independent current candidate review is required.");
  const now=Date.now(),captured=prior?await database.prepare("SELECT snapshot_json::text snapshot_json FROM crm_case_events WHERE id=? AND lead_id=? AND action='create' AND actor_id=?").bind(`crm-event-${key}-crm-lead`,`lead-${key}-crm`,actor.userId).first<{snapshot_json:string}>():null;
  const capturedInput=captured?crmLeadSchema.parse(JSON.parse(captured.snapshot_json)):null,dueDate=input.dueDate??capturedInput?.dueDate??new Date(now+7*86400000).toISOString().slice(0,10),leadInput=crmLeadSchema.parse(marketingCandidateLeadInput(candidate,dueDate));composition.handoff={id,key,raw:input,input:leadInput};
  const scopes=["sales.crm","sales.marketing"].map(capability=>({companyId:actor.companyId,capability,purpose})),boundaryKey=`marketing-handoff-purchase:${randomUUID()}`,boundaries=scopes.map((scope,index)=>({key:`${boundaryKey}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${boundaryKey}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const result=await run(database,now);
  composition.checks.push(async()=>{
   await current(boundaryKey);
   const final=await database.prepare(`SELECT * FROM (${crmMarketingCandidateSources}) candidate WHERE id=? AND company_id=?`).bind(id,actor.companyId).first<Row>(),receipt=await replayCommand<Record<string,unknown>>(database,actor,key,"marketing.candidate.handoff",id,input),childId=`lead-${key}-crm`;
   if(!final||final.status!=="handed_off"||final.crm_lead_id!==childId||final.reviewed_by!==actor.userId||Number(final.version)!==input.expectedVersion+1||!receipt||receipt.crmLeadId!==childId||["form_id","form_version","contact_name","contact_detail","consent_evidence","attribution_json","brief","branch_id","department_id","created_by"].some(field=>final[field]!==candidate[field]))throw new ApiError(409,"CRM_HANDOFF_SOURCE_CHANGED","The original candidate handoff changed before commit.");
   const child=await replayCommand<Record<string,unknown>>(database,actor,`${key}-crm`,"crm.lead.create",childId,leadInput);if(!child||child.id!==childId)throw new ApiError(409,"CRM_HANDOFF_SOURCE_CHANGED","The exact child lead receipt is required.");
   const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));boundaries[0]?.boundary.observe?.(observed);
  });return result;
 });
}

/** Cross-owner references are checked by their actual native source and original
 * authority; sourceKind labels never select a broader grant. */
export async function crmLeadCreateReferences(database:SqlDatabase,actor:WorkContext,input:CRMLeadInput){
 if(await commercePackageEnforcementMode()==='disabled')return;
 if(input.campaignId&&!await database.prepare(`SELECT 1 FROM (${crmMarketingCampaignSources}) campaign WHERE id=? AND company_id=?`).bind(input.campaignId,actor.companyId).first())throw new ApiError(404,"CRM_CAMPAIGN_NOT_FOUND","Campaign not found in this Company.");
 if(input.sourceKind==="form"&&!await database.prepare(`SELECT 1 FROM (${crmMarketingCandidateSources}) candidate WHERE id=? AND company_id=? AND branch_id=? AND (?::text IS NULL OR department_id=?)`).bind(input.sourceId??"",actor.companyId,input.branchId,actor.departmentId,actor.departmentId).first())throw new ApiError(404,"MARKETING_CANDIDATE_NOT_FOUND","The actual form candidate is required.");
 if(input.sourceKind==="pms"){await requireCRMCurrentAuthority(database,actor,["pms.read"],`crm-hotel-reference:${randomUUID()}`);if(!await database.prepare(`SELECT 1 FROM (${crmHotelReservationSources}) reservation WHERE id=? AND company_id=? AND branch_id=?`).bind(input.sourceId??"",actor.companyId,input.branchId).first())throw new ApiError(404,"CRM_RESERVATION_NOT_FOUND","The actual Hotel reservation is required.");}
}

export async function crmOpportunityReference(database:SqlDatabase,actor:WorkContext,kind:"quote"|"sales"|"reservation",id:string){
 if(await commercePackageEnforcementMode()==='disabled')return;
 const rows=kind==="reservation"?crmHotelReservationSources:crmCommerceOrderSources;
 const row=await database.prepare(`SELECT id FROM (${rows}) actual WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) ${kind==="reservation"?"":"AND kind=? AND (?::text IS NULL OR department_id=?)"}`).bind(id,actor.companyId,actor.branchId,actor.branchId,...(kind==="reservation"?[]:[kind,actor.departmentId,actor.departmentId])).first();
 if(!row)throw new ApiError(404,"CRM_SOURCE_NOT_FOUND","The actual linked customer document is unavailable in this workspace.");
}
