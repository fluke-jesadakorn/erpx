import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {effectiveWorkModules} from '@erpx/server-identity_access/public/rules/work-modules';
import {currencyDefinitions,currencyScale} from '@erpx/feature-finance/lib/company-money';
import {decimalToMinor} from '@erpx/feature-finance/lib/finance-money';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {replayCommand} from '@erpx/server-workflow/public/commands';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {serviceCustomerSchema,type ServiceCustomerInput,type ServiceCustomerResult,type ServiceCustomerPreparation,type ServiceCustomerConfirmation} from '@erpx/server-finance/domain/service-customer-preparation';

const capabilities=['service.agreements','finance.service-invoice'] as const,operation='finance.party.create';
const changed=()=>new ApiError(409,'SERVICE_CUSTOMER_SOURCE_CHANGED','The original Service customer, Finance reference or receipt changed before completion.');
/** Immutable 1047 rejects a late grant image without its matching native audit.
 * Keep that exact rollback typed; every other database failure remains opaque. */
function nativeHistorySourceFailure(error:unknown){
 let current=error;
 for(let depth=0;depth<5&&current&&typeof current==='object';depth++){
  const failure=current as {code?:unknown;message?:unknown;where?:unknown;cause?:unknown};
  if(failure.code==='23514'&&failure.message==='PACKAGE_HISTORY_CURRENT_AUDIT_REQUIRED'&&typeof failure.where==='string'&&/\bcommercial_package_(?:current_audit_required|history_observation)\(\)/.test(failure.where))return changed();
  current=failure.cause;
 }
 return null;
}

const stamp=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.assignmentId,actor.role,actor.branchId,actor.departmentId,actor.authority??null,actor.iam]);
const company=(actor:WorkContext)=>!actor.beta&&actor.scope.kind==='holding'&&actor.scope.companyId===actor.companyId&&!actor.branchId&&!actor.departmentId&&hasModuleAccess(actor.modules,'field_service');
async function authority(database:SqlDatabase,actor:WorkContext,write=false){
 if(!company(actor))throw new ApiError(403,'COMPANY_SERVICE_CUSTOMER_REQUIRED','Use a current Company Service and Finance assignment.');
 let current=actor;for(const permission of ['records.read','finance.read',...(write?['finance.write']:[])]){requirePermission(actor,permission);current=await requireQueuedCommandAuthority(database,actor,permission);}
 if(!current.iam?.queuedAssignmentVersion)throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original native Service assignment version is required.');
 const assignment=await database.prepare('SELECT modules_json FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND version=?').bind(actor.assignmentId,actor.companyId,actor.userId,actor.role,current.iam.queuedAssignmentVersion).first<{modules_json:string|string[]}>();
 if(!assignment)throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Service assignment changed.');
 const assigned=typeof assignment.modules_json==='string'?JSON.parse(assignment.modules_json) as string[]:assignment.modules_json;
 if(!hasModuleAccess(effectiveWorkModules(actor.role,assigned,current.iam.permissions,!!actor.beta),'field_service'))throw new ApiError(403,'MODULE_FORBIDDEN','The original assignment does not include Service.');
 for(const feature of ['field_service','finance'])await assertProductionModuleEnabled(database,actor.companyId,feature);
 return current;
}
async function owner(actor:WorkContext){const database=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(database))throw new ApiError(503,'TRANSACTION_REQUIRED','Service customer preparation requires its original Holding transaction.');return database;}
async function leases(database:SqlDatabase,actor:WorkContext,write=false){
 await lockPackageSettlementSources(database);
 // Original disabled Finance commands can hold the Work row before inserting
 // their receipt. Wait here before taking a receipt/party writer fence.
 await database.prepare(`LOCK TABLE active_work_contexts IN ${write?'EXCLUSIVE':'SHARE'} MODE`).run();
 await database.prepare(`LOCK TABLE command_receipts,finance_parties IN ${write?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`).run();
 await database.prepare('LOCK TABLE company_money_settings,company_currency_scales IN SHARE MODE').run();
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
}
type Party={id:string;company_id:string;version:number;person_id:string|null;kind:string;code:string;name:string;currency:string;status:string;tax_registration:string|null;payment_terms_days:number;credit_limit_minor:number;source:string};
async function references(database:SqlDatabase,actor:WorkContext){
 const parties=(await database.prepare('SELECT p.*,to_jsonb(p)::text source FROM finance_parties p WHERE p.company_id=? ORDER BY p.id').bind(actor.companyId).all<Party>()).results;
 const money=(await database.prepare("SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=? UNION ALL SELECT 'scale:'||s.currency kind,to_jsonb(s)::text source FROM company_currency_scales s WHERE s.company_id=? ORDER BY kind").bind(actor.companyId,actor.companyId).all<{kind:string;source:string}>()).results;
 return {parties,money};
}
type References=Awaited<ReturnType<typeof references>>;
const image=(rows:References)=>JSON.stringify([rows.parties.map(row=>row.source),rows.money]);
const purchaseImage=(access:Awaited<ReturnType<typeof readPurchasedAccesses>>)=>JSON.stringify(capabilities.map(capability=>access[capability].revision));
const contextKey=(actor:WorkContext,rows:References,access:Awaited<ReturnType<typeof readPurchasedAccesses>>)=>createHash('sha256').update(JSON.stringify([stamp(actor),image(rows),purchaseImage(access)])).digest('hex');
async function receipt(database:SqlDatabase,key:string){return database.prepare('SELECT to_jsonb(r)::text source,r.record_id,r.operation,r.actor_id,r.authority_json FROM command_receipts r WHERE command_id=?').bind(key).first<{source:string;record_id:string;operation:string;actor_id:string;authority_json:string}>();}
function selectedReceipt(row:NonNullable<Awaited<ReturnType<typeof receipt>>>,actor:WorkContext){return row.operation===operation&&row.actor_id===actor.userId&&JSON.parse(row.authority_json).companyId===actor.companyId;}
function nativeResult(value:ServiceCustomerResult,input:ServiceCustomerInput['input']){return typeof value.id==='string'&&value.id.startsWith('party-')&&value.kind==='customer'&&value.code===input.code&&value.name===input.name&&value.currency===input.currency&&value.taxRegistration===input.taxRegistration&&value.paymentTermsDays===input.paymentTermsDays&&value.creditLimit===input.creditLimit&&value.creditLimitMinor===decimalToMinor(input.creditLimit);}
function bornParty(row:Party|undefined,actor:WorkContext,input:ServiceCustomerInput['input']){return !!row&&row.company_id===actor.companyId&&row.kind==='customer'&&row.status==='active'&&Number(row.version)===1&&row.person_id===null&&row.code===input.code&&row.name===input.name&&row.currency===input.currency&&row.tax_registration===(input.taxRegistration??null)&&Number(row.payment_terms_days)===input.paymentTermsDays&&Number(row.credit_limit_minor)===decimalToMinor(input.creditLimit);}
const empty=(companyId:string,unavailableReason:ServiceCustomerPreparation['unavailableReason']):ServiceCustomerPreparation=>({companyId,available:false,contextKey:null,canCreate:false,currencies:[],customers:[],unavailableReason});
export async function serviceCustomerPreparation(actor:WorkContext):Promise<ServiceCustomerPreparation>{
 if(!company(actor)||!hasPermission(actor,'records.read')||!hasPermission(actor,'finance.read'))return empty(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor);const current=await authority(database,actor),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return empty(actor.companyId,'purchase-required');
  const rows=await references(database,actor),scales=new Map(rows.money.filter(row=>row.kind.startsWith('scale:')).map(row=>{const value=JSON.parse(row.source) as {currency:string;scale:number};return[value.currency,value.scale];}));
  const currencies=currencyDefinitions.filter(row=>{try{return currencyScale(row.code,scales.get(row.code))===2;}catch{return false;}}).map(({code,name})=>({code,name}));
  const canCreate=hasPermission(current,'finance.write')&&capabilities.every(capability=>access[capability].newWork);
  if(canCreate)await authority(database,actor,true);
  if(stamp(current)!==stamp(await authority(database,actor))||image(rows)!==image(await references(database,actor))||purchaseImage(access)!==purchaseImage(await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities)))throw changed();
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return{companyId:actor.companyId,available:true,contextKey:contextKey(current,rows,access),canCreate,currencies,customers:rows.parties.filter(row=>row.kind==='customer'&&row.status==='active').map(({id,code,name,currency})=>({id,code,name,currency})),unavailableReason:null};
 });
}
/** Closed Finance composition. The caller supplies no SQL/permission/capability
 * selector; the original configuration service owns all native effects. */
export async function withServiceCustomerCommand(actor:WorkContext,request:ServiceCustomerInput,key:string,run?: (database:SqlDatabase)=>Promise<ServiceCustomerResult>):Promise<{result:ServiceCustomerResult;confirmation:ServiceCustomerConfirmation}>{
 if(!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Use the original Idempotency-Key.');
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor,true);const current=await authority(database,actor,true),prior=await receipt(database,key);
  if(prior&&!selectedReceipt(prior,actor))throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This key belongs to a different original request.');
  const purpose=prior?'history' as const:'new-work' as const,guard=`service-customer:${randomUUID()}`;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  const boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const before=await references(database,actor);let result:ServiceCustomerResult;
  if(prior){const replay=await replayCommand<ServiceCustomerResult>(database,actor,key,operation,prior.record_id,request);if(!replay)throw changed();result=replay;}
  else{
   if(!run)throw new ApiError(404,'SERVICE_CUSTOMER_RECEIPT_NOT_FOUND','The original customer creation receipt is not yet available. Retry the original request.');
   if(request.contextKey!==contextKey(current,before,access))throw new ApiError(409,'SERVICE_CUSTOMER_CONTEXT_CHANGED','Review the current Service customer references before submitting.');
   if(before.parties.some(row=>row.kind==='customer'&&row.code===request.input.code))throw new ApiError(409,'SERVICE_CUSTOMER_CODE_EXISTS','This customer code is already in use. Choose a different customer code.');
   result=await run(database);
  }
  const created=await references(database,actor),party=created.parties.find(row=>row.id===result.id),savedReceipt=await receipt(database,key);
  if(!nativeResult(result,request.input)||!party||party.company_id!==actor.companyId||party.kind!=='customer'||!['active','blocked'].includes(party.status)||!savedReceipt||!selectedReceipt(savedReceipt,actor)||savedReceipt.record_id!==result.id)throw changed();
  if(prior?(prior.source!==savedReceipt.source||image(before)!==image(created)):!bornParty(party,actor,request.input)||before.parties.some(row=>row.id===result.id)||JSON.stringify(before.parties.map(row=>row.source))!==JSON.stringify(created.parties.filter(row=>row.id!==result.id).map(row=>row.source))||JSON.stringify(before.money)!==JSON.stringify(created.money))throw changed();
  const replay=await replayCommand<ServiceCustomerResult>(database,actor,key,operation,result.id,request);if(JSON.stringify(result)!==JSON.stringify(replay)||savedReceipt.source!==(await receipt(database,key))?.source)throw changed();
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  if(image(created)!==image(await references(database,actor))||savedReceipt.source!==(await receipt(database,key))?.source||stamp(current)!==stamp(await authority(database,actor,true))||purchaseImage(access)!==purchaseImage(await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities)))throw changed();
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  return{result,confirmation:{commandId:key,companyId:actor.companyId,id:result.id,kind:'customer',status:party.status==='active'?'active':'blocked',code:party.code,name:party.name,currency:party.currency}};
 });}catch(error){throw nativeHistorySourceFailure(error)??purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export async function serviceCustomerConfirmation(actor:WorkContext,raw:unknown,key:string){return(await withServiceCustomerCommand(actor,serviceCustomerSchema.parse(raw),key)).confirmation;}
