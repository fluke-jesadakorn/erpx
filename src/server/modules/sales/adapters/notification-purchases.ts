import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {commerceCompleteOwnerSourceReadLocks} from '../../commerce/public/owner-source-locks';
import {salesNotificationDeliverySources} from '../../commerce/public/sales-notification-sources';
import {salesNotificationCustomerSources} from '../../finance/public/sales-notification-sources';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {commercePackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {financialConstraintError} from '../../workflow/public/commands';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import type {CustomerMail,CustomerNotification,CustomerNotificationRetry,CustomerNotificationSettings} from '../domain/notifications';
type Command={key:string}&({kind:'settings';input:CustomerNotificationSettings}|{kind:'queue';id:string;input:CustomerNotification}|{kind:'retry';id:string;input:CustomerNotificationRetry});
export type SalesNotificationSource=CustomerMail&{version:number;status:string;created_by:string;provider_message_id:string|null;lease_until:number|null;next_attempt_at:number;branch_id:string|null;department_id:string|null;order_state:string|null;customer_status:string|null;notification_email:string|null;notification_consent:string|null;current_adapter:string|null;source_company:string|null;customer_company:string|null;release_ready:boolean;image:string};
export type SalesNotificationAuthorization={command_id:string;notification_id:string;company_id:string;kind:'queue'|'retry';actor_id:string;assignment_id:string;authority_json:string;source_hash:string;notification_version:number};
const digest=(image:string)=>createHash('sha256').update(image).digest('hex');
export const salesNotificationSourceHash=(source:SalesNotificationSource)=>digest(source.image);
function authoritySnapshot(actor:WorkContext){const {holdingId,holdingSlug,holdingName,companyId,branchId,departmentId,userId,assignmentId,role,authority,iam,name,email,scope,assignmentScope,inherited,modules}=actor;return {holdingId,holdingSlug,holdingName,companyId,branchId,departmentId,userId,assignmentId,role,authority,iam,name,email,scope,assignmentScope,inherited,modules,assignments:[],workspaces:[]};}
/** Actual same-Company fulfillment and independently managed customer, never a
 * supplied Sales label or a private snapshot sent to the browser. */
export function salesNotificationSource(database:SqlDatabase,id:string){return database.prepare(`SELECT n.*,delivery.department_id,delivery.order_state,delivery.company_id source_company,customer.company_id customer_company,customer.status customer_status,customer.notification_email,customer.notification_consent,settings.adapter current_adapter,${scheduledFeaturePredicate('n.company_id','sales')} release_ready,
 jsonb_build_array(n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,n.party_version,n.recipient_email,n.subject,n.body_text,n.adapter,n.created_by,n.created_at,delivery.fulfillment_id,delivery.order_id,delivery.company_id,delivery.branch_id,delivery.department_id,delivery.party_id,delivery.order_kind,customer.id,customer.company_id,customer.kind)::text image
 FROM commerce_customer_notifications n LEFT JOIN (${salesNotificationDeliverySources}) delivery ON delivery.fulfillment_id=n.fulfillment_id AND delivery.order_id=n.order_id AND delivery.company_id=n.company_id AND delivery.party_id=n.party_id
 LEFT JOIN (${salesNotificationCustomerSources}) customer ON customer.id=n.party_id AND customer.company_id=n.company_id
 LEFT JOIN commerce_customer_notification_settings settings ON settings.company_id=n.company_id WHERE n.id=?`).bind(id).first<SalesNotificationSource>();}
export function salesNotificationLocalReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_customer_notification_settings,commerce_customer_notifications,sales_notification_execution_authorizations,sales_notification_execution_attempts IN SHARE ROW EXCLUSIVE MODE')];}
/** Complete source-leased units, shared by original commands and queued work.
 * All LOCKs precede the first advisory SELECT/SERIALIZABLE snapshot. */
export async function inSalesNotificationOwnerUnit<Result>(owner:SqlDatabase,companyId:string,run:(database:SqlDatabase)=>Promise<Result>,reconciliation=false){
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SALES_NOTIFICATION_TRANSACTION_REQUIRED','Customer notices require their complete owned Holding transaction.');
 try{return await owner.transaction(async database=>{const locks=companyPurchaseReadLocks(database,companyId);await database.batch([locks[0],...commerceCompleteOwnerSourceReadLocks(database),...salesNotificationLocalReadLocks(database),...(reconciliation?[database.prepare('LOCK TABLE sales_notification_reconciliations IN SHARE ROW EXCLUSIVE MODE')]:[])]);await lockQueuedCommandAuthority(database);await database.batch(locks.slice(1));return run(database);});}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function salesNotificationInvalid(source:SalesNotificationSource){return source.source_company!==source.company_id||source.customer_company!==source.company_id||!['confirmed','partially_fulfilled','fulfilled'].includes(source.order_state??'')?'CUSTOMER_NOTIFICATION_SOURCE_CHANGED':source.customer_status!=='active'||source.notification_consent!=='granted'||source.notification_email?.toLowerCase()!==source.recipient_email?'CUSTOMER_CONSENT_OR_EMAIL_CHANGED':source.current_adapter!==source.adapter?'CUSTOMER_NOTIFICATION_PROVIDER_CHANGED':null;}
export async function salesNotificationAuthorization(database:SqlDatabase,id:string){return database.prepare('SELECT * FROM sales_notification_execution_authorizations WHERE notification_id=? ORDER BY notification_version DESC,created_at DESC,command_id DESC LIMIT 1').bind(id).first<SalesNotificationAuthorization>();}
export async function requireSalesNotificationExecution(database:SqlDatabase,source:SalesNotificationSource,authorization:SalesNotificationAuthorization){
 if(authorization.notification_id!==source.id||authorization.company_id!==source.company_id||authorization.source_hash!==salesNotificationSourceHash(source)||authorization.notification_version>source.version)throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','Review this exact retained notice through its original retry workflow.');
 const actor=JSON.parse(authorization.authority_json) as WorkContext;
 if(actor.userId!==authorization.actor_id||actor.assignmentId!==authorization.assignment_id||actor.companyId!==source.company_id||actor.branchId&&actor.branchId!==source.branch_id||actor.departmentId&&actor.departmentId!==source.department_id)throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The exact queued identity or source binding changed.');
 const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=? AND result_json::jsonb->>'id'=?").bind(authorization.command_id,`commerce.customer-notification.${authorization.kind}`,authorization.kind==='queue'?source.fulfillment_id:source.id,actor.userId,source.company_id,source.id).first();
 if(!receipt)throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The original notice command receipt is unavailable.');
 await requireQueuedCommandAuthority(database,actor,'commerce.sales');await checkPurchasedCapability(database,await commercePackageEnforcementMode(),{companyId:source.company_id,capability:'sales.orders',purpose:'settle-existing'});return actor;
}
export async function withSalesNotificationPurchase(actor:WorkContext,command:Command,run:(database:SqlDatabase)=>Promise<Record<string,unknown>>){
 const mode=await commercePackageEnforcementMode(),owner=(await holdingResources(actor.holdingId)).write;if(mode==='disabled')return run(owner);
 return inSalesNotificationOwnerUnit(owner,actor.companyId,async database=>{
  const permission=command.kind==='settings'?'commerce.approve':'commerce.sales',bound=await requireQueuedCommandAuthority(database,actor,permission),operation=command.kind==='settings'?'commerce.customer-notification-settings.update':`commerce.customer-notification.${command.kind}`,recordId=command.kind==='settings'?actor.companyId:command.id;
  const prior=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,operation,recordId,actor.userId,actor.companyId).first(),purpose=prior?'history' as const:command.kind==='retry'||command.kind==='settings'&&command.input.adapter==='disabled'?'settle-existing' as const:'new-work' as const,scope={companyId:actor.companyId,capability:'sales.orders',purpose},key=`sales-notification:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);const result=await run(database);
  const id=command.kind==='settings'?null:typeof result.id==='string'?result.id:null;
  if(command.kind!=='settings'&&!id)throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The original notice result is unavailable.');
  const original=id?await salesNotificationSource(database,id):null;
  if(original&&(original.company_id!==actor.companyId||original.source_company!==original.company_id||original.customer_company!==original.company_id||!prior&&salesNotificationInvalid(original)||command.kind==='queue'&&original.fulfillment_id!==command.id||command.kind==='retry'&&original.id!==command.id))throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The original notice source changed before commit.');
  if(original&&!prior)await database.prepare('INSERT INTO sales_notification_execution_authorizations(command_id,notification_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,notification_version,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(command.key,original.id,actor.companyId,command.kind,actor.userId,actor.assignmentId,JSON.stringify(authoritySnapshot(bound)),salesNotificationSourceHash(original),original.version,Date.now()).run();
  await flushFinanceDeferredEffects(database);await requireQueuedCommandAuthority(database,bound,permission);
  if(original){const final=await salesNotificationSource(database,original.id);if(!final||salesNotificationSourceHash(final)!==salesNotificationSourceHash(original)||!prior&&salesNotificationInvalid(final))throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The captured customer notice source changed before commit.');}
  if(command.kind==='settings'&&!prior&&!await database.prepare('SELECT 1 FROM commerce_customer_notification_settings WHERE company_id=? AND adapter=? AND version=?').bind(actor.companyId,command.input.adapter,command.input.expectedVersion+1).first())throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The original notification settings changed before commit.');
  const observed=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary.observe?.(observed);return result;
 });
}
export async function requireSalesNotificationRead(database:SqlDatabase,actor:WorkContext){const mode=await commercePackageEnforcementMode();if(mode==='disabled')return;await requireQueuedCommandAuthority(database,actor,'commerce.read');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'history'});}
