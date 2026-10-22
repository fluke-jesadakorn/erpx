import 'server-only';
import {randomUUID} from 'node:crypto';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {JobEnv} from '@erpx/server-workflow/public/job-projector';
import {ApiError} from '@erpx/server-runtime/public/http';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {inSalesNotificationOwnerUnit,salesNotificationSource,salesNotificationSourceHash,salesNotificationInvalid,salesNotificationAuthorization,requireSalesNotificationExecution,type SalesNotificationSource,type SalesNotificationAuthorization} from '@erpx/server-sales/adapters/notification-purchases';
type Attempt={id:string;notification_id:string;company_id:string;authorization_command_id:string;attempt:number;lease_until:number;source_hash:string;state:string};
type Stage={mail:SalesNotificationSource;authorization:SalesNotificationAuthorization;attempt:Attempt};
const heldUntil=253402300799000;
async function terminal(database:SqlDatabase,stage:Stage,state:'sent'|'failed'|'unknown'|'cancelled',code:string|null,providerId:string|null,now:number){
 const changed=await database.prepare("UPDATE commerce_customer_notifications SET status=?,version=version+1,lease_until=NULL,next_attempt_at=?,last_error=?,provider_message_id=?,sent_at=CASE WHEN ?='sent' THEN ? ELSE sent_at END,updated_at=? WHERE id=? AND company_id=? AND status='sending' AND attempts=? AND lease_until=? RETURNING id").bind(state,heldUntil,code,providerId,state,now,now,stage.mail.id,stage.mail.company_id,stage.attempt.attempt,stage.attempt.lease_until).first();
 if(!changed)throw new ApiError(409,'SALES_NOTIFICATION_LEASE_CHANGED','The original customer-notice attempt changed.');
 if(!await database.prepare("UPDATE sales_notification_execution_attempts SET state=?,provider_message_id=?,result_code=?,updated_at=? WHERE id=? AND notification_id=? AND company_id=? AND state='sending' RETURNING id").bind(state,providerId,code,now,stage.attempt.id,stage.mail.id,stage.mail.company_id).first())throw new ApiError(409,'SALES_NOTIFICATION_LEASE_CHANGED','The original durable send attempt changed.');
}
async function stageAttempt(owner:SqlDatabase,companyId:string,id:string,now:number):Promise<Stage|null>{return inSalesNotificationOwnerUnit(owner,companyId,async database=>{
 const mail=await salesNotificationSource(database,id);if(!mail||mail.company_id!==companyId||!mail.release_ready||!['pending','failed','sending'].includes(mail.status))return null;
 if(mail.status==='sending'){
  if((mail.lease_until??0)>=now)return null;
  // A process may have died after the provider accepted the message. Even a
  // legacy sending lease cannot safely be converted into a fresh automatic send.
  await database.prepare("UPDATE commerce_customer_notifications SET status='unknown',version=version+1,lease_until=NULL,next_attempt_at=?,last_error='CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN',updated_at=? WHERE id=? AND status='sending'").bind(heldUntil,now,id).run();
  await database.prepare("UPDATE sales_notification_execution_attempts SET state='unknown',result_code='CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN',updated_at=? WHERE notification_id=? AND state='sending'").bind(now,id).run();return null;
 }
 if(mail.attempts>=8||mail.next_attempt_at>now)return null;
 const authorization=await salesNotificationAuthorization(database,id);
 // A populated legacy queue has no authenticated queued context. It needs an
 // explicit normal retry by a current actor; never invent a worker assignment.
 try{if(!authorization)throw new ApiError(409,'SALES_NOTIFICATION_AUTHORIZATION_REQUIRED','Review this retained notice through its existing retry workflow.');await requireSalesNotificationExecution(database,mail,authorization);}catch{
  await database.prepare("UPDATE commerce_customer_notifications SET status='failed',version=version+1,next_attempt_at=?,lease_until=NULL,last_error='CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED',updated_at=? WHERE id=? AND status IN ('pending','failed')").bind(heldUntil,now,id).run();return null;
 }
 if(!authorization)return null;
 const attempt:Attempt={id:`sales-notification-attempt-${randomUUID()}`,notification_id:id,company_id:companyId,authorization_command_id:authorization.command_id,attempt:mail.attempts+1,lease_until:now+90000,source_hash:salesNotificationSourceHash(mail),state:'sending'};
 await database.prepare("UPDATE commerce_customer_notifications SET status='sending',attempts=attempts+1,lease_until=?,updated_at=? WHERE id=? AND status IN ('pending','failed')").bind(attempt.lease_until,now,id).run();
 await database.prepare("INSERT INTO sales_notification_execution_attempts(id,notification_id,authorization_command_id,company_id,attempt,lease_until,source_hash,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'sending',?,?)").bind(attempt.id,id,authorization.command_id,companyId,attempt.attempt,attempt.lease_until,attempt.source_hash,now,now).run();
 const stage={mail,authorization,attempt},invalid=salesNotificationInvalid(mail);
 if(invalid){await terminal(database,stage,'cancelled',invalid,null,now);return null;}
 await flushFinanceDeferredEffects(database);const final=await salesNotificationSource(database,id);if(!final||salesNotificationSourceHash(final)!==attempt.source_hash||salesNotificationInvalid(final))throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The exact original customer notice changed.');await requireSalesNotificationExecution(database,final,authorization);return stage;
});}
async function currentAttempt(database:SqlDatabase,stage:Stage){
 const mail=await salesNotificationSource(database,stage.mail.id),attempt=await database.prepare("SELECT * FROM sales_notification_execution_attempts WHERE id=? AND notification_id=? AND company_id=? AND state='sending'").bind(stage.attempt.id,stage.mail.id,stage.mail.company_id).first<Attempt>();
 if(!mail||!attempt||mail.status!=='sending'||mail.attempts!==attempt.attempt||mail.lease_until!==attempt.lease_until||attempt.lease_until<Date.now()||attempt.authorization_command_id!==stage.authorization.command_id||attempt.source_hash!==salesNotificationSourceHash(mail)||!mail.release_ready||salesNotificationInvalid(mail))throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The exact notice lease, source or consent changed before delivery.');
 await requireSalesNotificationExecution(database,mail,stage.authorization);return mail;
}
/** Durable phase one commits before I/O. A closure outside the retryable SQL
 * callback permits at most one provider invocation even if COMMIT is retried.
 * A lost response, crash or later DB/authority failure remains reconciliation
 * work; no failure path fabricates successful delivery or schedules a resend. */
export async function deliverPurchasedCustomerNotification(owner:SqlDatabase,env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,id:string,send:(input:RequestInfo|URL,init?:RequestInit)=>Promise<Response>,now:number){
 const companyId=await owner.prepare('SELECT company_id FROM commerce_customer_notifications WHERE id=?').bind(id).first<string>('company_id');if(!companyId)return;
 const stage=await stageAttempt(owner,companyId,id,now);if(!stage)return;let invoked=false,providerId:string|null=null;
 try{await inSalesNotificationOwnerUnit(owner,companyId,async database=>{
  const mail=await currentAttempt(database,stage);await flushFinanceDeferredEffects(database);await currentAttempt(database,stage);
  if(invoked)throw new ApiError(409,'CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN','Reconcile the original provider attempt before another delivery.');
  if(mail.adapter==='sandbox')providerId=`sandbox-${mail.id}`;
  else{
   if(!env.RESEND_API_KEY||!env.RESEND_FROM)throw new ApiError(503,'CUSTOMER_EMAIL_NOT_CONFIGURED','Configure the original customer-notice provider before retrying.');
   invoked=true;const response=await send('https://api.resend.com/emails',{method:'POST',headers:{Authorization:`Bearer ${String(env.RESEND_API_KEY)}`,'Content-Type':'application/json','Idempotency-Key':mail.id},body:JSON.stringify({from:String(env.RESEND_FROM),to:[mail.recipient_email],subject:mail.subject,text:mail.body_text}),signal:AbortSignal.timeout(10000)});
   if(!response.ok)throw new ApiError(409,'CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN','Reconcile the original provider response before another delivery.');
   const data:unknown=await response.json();if(!data||typeof data!=='object'||!('id' in data)||typeof data.id!=='string'||!data.id.trim())throw new ApiError(409,'CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN','The provider did not confirm an original message identity.');providerId=data.id;
  }
  await terminal(database,stage,'sent',null,providerId,Date.now());await flushFinanceDeferredEffects(database);
  const final=await salesNotificationSource(database,id);if(!final||salesNotificationSourceHash(final)!==stage.attempt.source_hash||salesNotificationInvalid(final))throw new ApiError(409,'SALES_NOTIFICATION_SOURCE_CHANGED','The exact customer notice changed before final delivery commit.');await requireSalesNotificationExecution(database,final,stage.authorization);
 });}catch(error){
  await inSalesNotificationOwnerUnit(owner,companyId,async database=>{
   const attempt=await database.prepare("SELECT 1 FROM sales_notification_execution_attempts WHERE id=? AND state='sending'").bind(stage.attempt.id).first();if(!attempt)return;
   await terminal(database,stage,invoked?'unknown':'failed',invoked?'CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN':'CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED',providerId,Date.now());await flushFinanceDeferredEffects(database);
  });throw error;
 }
}
export async function drainPurchasedCustomerNotifications(database:SqlDatabase,env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,now:number){for(let index=0;index<10;index++){
 const id=await database.prepare(`SELECT n.id FROM commerce_customer_notifications n WHERE ((n.status IN ('pending','failed') AND n.attempts<8 AND n.next_attempt_at<=?) OR (n.status='sending' AND n.lease_until<?)) AND ${scheduledFeaturePredicate('n.company_id','sales')} ORDER BY n.next_attempt_at,n.created_at,n.id LIMIT 1`).bind(now,now).first<string>('id');if(!id)break;try{await deliverPurchasedCustomerNotification(database,env,id,fetch,now);}catch{/* Durable original attempt remains visible to reconciliation. */}
}}
