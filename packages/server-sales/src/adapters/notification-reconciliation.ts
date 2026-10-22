import 'server-only';
import {z} from 'zod';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {samePerson} from '@erpx/feature-identity_access/lib/iam';
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary} from '@erpx/server-commercial/public/purchase-command-boundary';
import {holdingResources,runtimeEnv} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {JobEnv} from '@erpx/server-workflow/public/job-projector';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {inSalesNotificationOwnerUnit,salesNotificationSource,salesNotificationSourceHash,type SalesNotificationSource} from '@erpx/server-sales/adapters/notification-purchases';
export const customerNotificationReconciliationSchema=z.object({expectedVersion:z.number().int().positive(),attemptId:z.string().trim().min(1).max(256),reason:z.string().trim().min(5).max(1000)});
type Input=z.infer<typeof customerNotificationReconciliationSchema>;
type Provider={env:Pick<JobEnv,'RESEND_API_KEY'>;read:(input:RequestInfo|URL,init?:RequestInit)=>Promise<Response>};
type Attempt={id:string;notification_id:string;company_id:string;authorization_command_id:string;actor_id:string;source_hash:string;state:string;provider_message_id:string|null;attempt:number;created_at:number;updated_at:number};
const observationSchema=z.object({object:z.literal('email'),id:z.string().min(1),to:z.array(z.string()).length(1),subject:z.string(),text:z.string(),created_at:z.string(),last_event:z.enum(['sent','delivered','opened','clicked','bounced','canceled','failed','suppressed'])});
async function providerObservation(source:SalesNotificationSource,attempt:Attempt,provider:Provider){
 if(source.adapter!=='resend'||!attempt.provider_message_id||source.provider_message_id!==attempt.provider_message_id)throw new ApiError(409,'CUSTOMER_NOTIFICATION_PROVIDER_ID_REQUIRED','The actual uncertain attempt must already contain its original provider message identity.');
 if(!provider.env.RESEND_API_KEY)throw new ApiError(503,'CUSTOMER_EMAIL_NOT_CONFIGURED','Configure the original read-only provider credential before reconciliation.');
 const response=await provider.read(`https://api.resend.com/emails/${encodeURIComponent(attempt.provider_message_id)}`,{method:'GET',headers:{Authorization:`Bearer ${String(provider.env.RESEND_API_KEY)}`},signal:AbortSignal.timeout(10000)});
 if(!response.ok)throw new ApiError(409,'CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED','The original provider did not confirm this uncertain message.');
 const parsed=observationSchema.safeParse(await response.json().catch(()=>null));if(!parsed.success)throw new ApiError(409,'CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED','The original provider observation is incomplete or not final.');const observed=parsed.data,created=Date.parse(observed.created_at);
 if(observed.id!==attempt.provider_message_id||observed.to[0].toLowerCase()!==source.recipient_email||observed.subject!==source.subject||observed.text!==source.body_text||!Number.isFinite(created)||created<attempt.created_at-1000||created>attempt.updated_at+1000)throw new ApiError(409,'CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH','The actual provider message does not match the immutable original attempt.');
 return {...observed,to:observed.to.map(recipient=>recipient.toLowerCase())};
}
/** A normal independent settlement command. It never sends email, replaces an
 * immutable attempt, or accepts a supplied provider identity/receipt as truth. */
export async function reconcileCustomerNotification(actor:WorkContext,id:string,raw:Input,key:string,provider?:Provider){
 const input=customerNotificationReconciliationSchema.parse(raw),mode=await commercePackageEnforcementMode();if(mode==='disabled')throw new ApiError(409,'CUSTOMER_NOTIFICATION_RECONCILIATION_DISABLED','Enable the reviewed purchase boundary before reconciling durable attempts.');
 const owner=(await holdingResources(actor.holdingId)).write;
 return inSalesNotificationOwnerUnit(owner,actor.companyId,async database=>{
  const bound=await requireQueuedCommandAuthority(database,actor,'commerce.approve'),source=await salesNotificationSource(database,id);
  if(!source||source.company_id!==actor.companyId||source.source_company!==actor.companyId||source.customer_company!==actor.companyId||actor.branchId&&actor.branchId!==source.branch_id||actor.departmentId&&actor.departmentId!==source.department_id)throw new ApiError(404,'CUSTOMER_NOTIFICATION_NOT_FOUND','The original customer notice is unavailable in this work scope.');
  const attempt=await database.prepare('SELECT attempt.*,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.id=? AND attempt.notification_id=? AND attempt.company_id=?').bind(input.attemptId,id,actor.companyId).first<Attempt>();
  if(!attempt||attempt.source_hash!==salesNotificationSourceHash(source))throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The actual original notice attempt is unavailable.');
  if(samePerson(bound,source.created_by)||samePerson(bound,attempt.actor_id))throw new ApiError(403,'INDEPENDENT_NOTIFICATION_REVIEWER_REQUIRED','A different current Sales approver must reconcile the original provider attempt.');
  const operation='commerce.customer-notification.reconcile',prior=await replayCommand<Record<string,unknown>>(database,bound,key,operation,id,input),scope={companyId:actor.companyId,capability:'sales.orders',purpose:prior?'history' as const:'settle-existing' as const};await checkPurchasedCapability(database,mode,scope);
  if(prior){if(!await database.prepare('SELECT 1 FROM sales_notification_reconciliations WHERE command_id=? AND notification_id=? AND attempt_id=? AND actor_id=? AND source_hash=?').bind(key,id,attempt.id,bound.userId,attempt.source_hash).first())throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The original reconciliation witness is unavailable.');return prior;}
  if(!source.release_ready)throw new ApiError(409,'PRODUCTION_MODULE_DISABLED','The original Sales owner release is unavailable.');
  if(source.version!==input.expectedVersion)throw new ApiError(409,'VERSION_CONFLICT','The customer notice changed. Refresh before reconciliation.');
  if(source.status!=='unknown'||attempt.state!=='unknown'||source.attempts!==attempt.attempt)throw new ApiError(409,'CUSTOMER_NOTIFICATION_NOT_UNCERTAIN','Only the exact original uncertain provider attempt can be reconciled.');
  const observed=await providerObservation(source,attempt,provider??{env:await runtimeEnv(),read:fetch}),outcome=['sent','delivered','opened','clicked'].includes(observed.last_event)?'sent':'cancelled',now=Date.now(),version=source.version+1,eventId=`sales-notification-reconciliation-${key}`,boundaryKey=`sales-notification-reconciliation:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,boundaryKey,scope)!,result={id,orderId:source.order_id,status:outcome,version,reconciliationId:eventId};
  await database.batch(boundary.before);
  await executeCommand(database,bound,key,operation,input,id,version,result,[
   assertion(database,key,'notice-version',"EXISTS(SELECT 1 FROM commerce_customer_notifications WHERE id=? AND company_id=? AND version=? AND status='unknown' AND provider_message_id=?)",[id,actor.companyId,input.expectedVersion,attempt.provider_message_id]),
   database.prepare("UPDATE commerce_customer_notifications SET status=?,version=version+1,lease_until=NULL,last_error=NULL,sent_at=CASE WHEN ?='sent' THEN ? ELSE sent_at END,updated_at=? WHERE id=? AND version=? AND status='unknown'").bind(outcome,outcome,now,now,id,input.expectedVersion),
   database.prepare('INSERT INTO sales_notification_reconciliations(id,command_id,notification_id,attempt_id,company_id,actor_id,assignment_id,expected_version,notification_version,source_hash,provider_message_id,provider_event,outcome,provider_observation_json,provider_observed_at,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(eventId,key,id,attempt.id,actor.companyId,bound.userId,bound.assignmentId,input.expectedVersion,version,attempt.source_hash,attempt.provider_message_id,observed.last_event,outcome,JSON.stringify(observed),now,input.reason,now)
  ],{companyId:actor.companyId,branchId:source.branch_id,departmentId:source.department_id});
  await flushFinanceDeferredEffects(database);await requireQueuedCommandAuthority(database,bound,'commerce.approve');const final=await salesNotificationSource(database,id);
  if(!final||salesNotificationSourceHash(final)!==attempt.source_hash||final.status!==outcome||final.version!==version||!await database.prepare('SELECT 1 FROM sales_notification_reconciliations event JOIN sales_notification_execution_attempts attempt ON attempt.id=event.attempt_id WHERE event.id=? AND event.command_id=? AND event.source_hash=? AND event.outcome=? AND event.notification_version=? AND attempt.state=\'unknown\' AND attempt.provider_message_id=event.provider_message_id').bind(eventId,key,attempt.source_hash,outcome,version).first())throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The original reconciliation source changed before commit.');
  const results=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(boundaryKey)]);boundary.observe?.(results);return result;
 },true);
}

/** Minimal presentation flags after the original Sales record reader. No queued
 * authority, provider response, private message or capability snapshot crosses
 * the server/client boundary. Disabled deployments keep their original shape. */
export async function customerNotificationReconciliationControls(actor:WorkContext,id:string){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;const database=(await holdingResources(actor.holdingId)).write;await requireQueuedCommandAuthority(database,actor,'commerce.read');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'history'});const source=await salesNotificationSource(database,id);
 if(!source||source.company_id!==actor.companyId||source.source_company!==actor.companyId||source.customer_company!==actor.companyId||actor.branchId&&actor.branchId!==source.branch_id||actor.departmentId&&actor.departmentId!==source.department_id)throw new ApiError(404,'CUSTOMER_NOTIFICATION_NOT_FOUND','The original customer notice is unavailable in this work scope.');
 const attempt=source.status==='unknown'?await database.prepare("SELECT attempt.id,attempt.source_hash,attempt.provider_message_id,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.notification_id=? AND attempt.company_id=? AND attempt.attempt=? AND attempt.state='unknown'").bind(id,actor.companyId,source.attempts).first<{id:string;source_hash:string;provider_message_id:string|null;actor_id:string}>():null;
 if(!attempt)return undefined;const hasProviderIdentity=source.adapter==='resend'&&!!attempt.provider_message_id&&attempt.provider_message_id===source.provider_message_id&&attempt.source_hash===salesNotificationSourceHash(source);let canReview=false;
 if(hasProviderIdentity&&source.release_ready&&!samePerson(actor,source.created_by)&&!samePerson(actor,attempt.actor_id)&&await database.prepare("SELECT to_regclass('sales_notification_reconciliations') IS NOT NULL installed").first<boolean>('installed'))try{await requireQueuedCommandAuthority(database,actor,'commerce.approve');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'settle-existing'});canReview=true;}catch(error){if(!(error instanceof ApiError)||![403,409,402].includes(error.status))throw error;}
 return {attemptId:attempt.id,hasProviderIdentity,canReview};
}
