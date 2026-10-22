import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import {salesDeliveryNotificationFacts,salesDeliveryNotificationLines} from "@erpx/server-commerce/public/sales-order-sources";
import 'server-only';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {JobEnv} from "@erpx/server-workflow/public/job-projector";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {assertProductionModuleEnabled,productionFeatureGuard,scheduledFeaturePredicate} from "@erpx/server-pms/public/hospitality-rollout";
import {commercePackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {withSalesNotificationPurchase,requireSalesNotificationRead} from '@erpx/server-sales/adapters/notification-purchases';
import {deliverPurchasedCustomerNotification,drainPurchasedCustomerNotifications} from '@erpx/server-sales/adapters/notification-execution';


import {BusinessRuleError} from '@erpx/server-shared/index';
import {createCustomerNotificationApplication,type CustomerNotificationRepository,type CustomerNotificationMutation} from '@erpx/server-sales/application/notifications';
import {createCustomerDeliveryApplication,type CustomerDeliveryRepository} from '@erpx/server-sales/application/notification-delivery';
import type {CustomerNotification,CustomerNotificationRetry,CustomerNotificationSettings,CustomerMail} from '@erpx/server-sales/domain/notifications';
import {customerNotificationSchema,customerNotificationRetrySchema,customerNotificationSettingsSchema} from '@erpx/server-sales/domain/notifications';
export {customerNotificationSchema,customerNotificationRetrySchema,customerNotificationSettingsSchema} from '@erpx/server-sales/domain/notifications';
type Row=Record<string,string|number|null>;
function scoped(actor:WorkContext){return {sql:"o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)",args:[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]};}
function notificationSourcePredicate(alias:string) {
  return `EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_fulfillments}) f
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id
    JOIN commerce_customer_notification_settings ns ON ns.company_id=o.company_id
    WHERE f.id=${alias}.fulfillment_id AND o.id=${alias}.order_id AND o.company_id=${alias}.company_id
      AND o.party_id=${alias}.party_id AND o.kind='sales' AND o.state IN ('confirmed','partially_fulfilled','fulfilled')
      AND p.status='active' AND p.notification_consent='granted' AND lower(p.notification_email)=${alias}.recipient_email
      AND ns.adapter=${alias}.adapter AND ns.adapter IN ('sandbox','resend'))`;
}
function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);return createCustomerNotificationApplication({companyId:actor.companyId,authorize:permission=>{requirePermission(actor,permission);},clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},repository:async()=>repository(await database(),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):CustomerNotificationRepository{const s=scoped(actor);return {
 productionEnabled:()=>assertProductionModuleEnabled(write,actor.companyId,'sales'),settings:async()=> write.prepare("SELECT * FROM commerce_customer_notification_settings WHERE company_id=?").bind(actor.companyId).first<Row>(),delivery:async fulfillmentId=>salesDeliveryNotificationFacts(write,actor,fulfillmentId),existing:async fulfillmentId=>await write.prepare('SELECT id,status FROM commerce_customer_notifications WHERE fulfillment_id=?').bind(fulfillmentId).first<Row>(),lines:async fulfillmentId=>salesDeliveryNotificationLines(write,actor,fulfillmentId),notification:async id=>await write.prepare(`SELECT n.* FROM commerce_customer_notifications n JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id WHERE n.id=? AND ${s.sql}`).bind(id,...s.args).first<Row>(),replay:(key,operation,id,input)=>replayCommand(write,actor,key,operation,id,input),commit:m=>commit(write,actor,m),
};}
async function commit(write:SqlDatabase,actor:WorkContext,m:CustomerNotificationMutation){const {key,id,now,result}=m;switch(m.action){
 case 'settings':{const input=m.input,version=input.expectedVersion+1;return executeCommand(write,actor,key,'commerce.customer-notification-settings.update',input,actor.companyId,version,result,[...productionFeatureGuard(write,key,actor.companyId,'sales'),input.expectedVersion===0?assertion(write,key,'settings-new','NOT EXISTS(SELECT 1 FROM commerce_customer_notification_settings WHERE company_id=?)',[actor.companyId]):assertion(write,key,'settings-version','EXISTS(SELECT 1 FROM commerce_customer_notification_settings WHERE company_id=? AND version=?)',[actor.companyId,input.expectedVersion]),write.prepare("INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) VALUES(?,?,1,?,?) ON CONFLICT(company_id) DO UPDATE SET adapter=excluded.adapter,version=commerce_customer_notification_settings.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(actor.companyId,input.adapter,actor.userId,now)]);}
 case 'queue':{const input=m.input,{fulfillmentId,subject,body}=m,row=m.record;return executeCommand(write,actor,key,'commerce.customer-notification.queue',input,fulfillmentId,1,result,[
    ...productionFeatureGuard(write,key,actor.companyId,'sales'),
    assertion(write,key,'provider-settings',"EXISTS(SELECT 1 FROM commerce_customer_notification_settings WHERE company_id=? AND version=? AND adapter=?)",[actor.companyId,row.settings_version,row.adapter]),
    assertion(write,key,'delivery-version',`EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_fulfillments}) f JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id WHERE f.id=? AND f.version=? AND o.company_id=? AND p.version=? AND p.notification_email=? AND p.notification_consent='granted')`,[fulfillmentId,input.expectedVersion,actor.companyId,row.party_version,row.notification_email]),
    write.prepare('INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,fulfillment_id,party_id,party_version,recipient_email,subject,body_text,adapter,next_attempt_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,actor.companyId,row.branch_id,row.order_id,fulfillmentId,row.party_id,row.party_version,String(row.notification_email).toLowerCase(),subject,body,String(row.adapter),now,actor.userId,now,now),
  ],{companyId:actor.companyId,branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null});
}
 case 'retry':{const input=m.input,row=m.record,updatedAt=m.updatedAt;return executeCommand(write,actor,key,'commerce.customer-notification.retry',input,id,input.expectedVersion+1,result,[...productionFeatureGuard(write,key,actor.companyId,'sales'),assertion(write,key,'current-customer-binding',`EXISTS(SELECT 1 FROM commerce_customer_notifications n WHERE n.id=? AND ${notificationSourcePredicate('n')})`,[id]),assertion(write,key,'notification-version',"EXISTS(SELECT 1 FROM commerce_customer_notifications WHERE id=? AND version=? AND status='failed')",[id,input.expectedVersion]),write.prepare("UPDATE commerce_customer_notifications SET status='pending',version=version+1,attempts=0,next_attempt_at=?,lease_until=NULL,last_error=NULL,updated_at=? WHERE id=?").bind(now,updatedAt,id)],{companyId:actor.companyId,branchId:row.branch_id?String(row.branch_id):null,departmentId:null});
}
}}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerNotificationSettings(actor:WorkContext){return translated(async()=>{const database=(await holdingResources(actor.holdingId)).write;await requireSalesNotificationRead(database,actor);return application(actor,database).settings();});}
export function updateCustomerNotificationSettings(actor:WorkContext,raw:CustomerNotificationSettings,key:string){return translated(()=>{const input=customerNotificationSettingsSchema.parse(raw);return withSalesNotificationPurchase(actor,{kind:'settings',input,key},database=>application(actor,database).updateSettings(input,key));});}
export function queueDeliveryNotification(actor:WorkContext,id:string,raw:CustomerNotification,key:string){return translated(()=>{const input=customerNotificationSchema.parse(raw);return withSalesNotificationPurchase(actor,{kind:'queue',id,input,key},database=>application(actor,database).queue(id,input,key));});}
export function retryCustomerNotification(actor:WorkContext,id:string,raw:CustomerNotificationRetry,key:string){return translated(()=>{const input=customerNotificationRetrySchema.parse(raw);return withSalesNotificationPurchase(actor,{kind:'retry',id,input,key},database=>application(actor,database).retry(id,input,key));});}

function deliveryRepository(db:SqlDatabase):CustomerDeliveryRepository{return {
 lease:async(id,now,leaseUntil)=>await db.prepare(`UPDATE commerce_customer_notifications n SET status='sending',attempts=attempts+1,lease_until=?,updated_at=?
    WHERE n.id=? AND n.attempts<8 AND ((n.status IN ('pending','failed') AND n.next_attempt_at<=?) OR (n.status='sending' AND n.lease_until<?))
      AND ${scheduledFeaturePredicate('n.company_id','sales')}
    RETURNING n.id,n.company_id,n.fulfillment_id,n.order_id,n.party_id,n.party_version,n.recipient_email,n.subject,n.body_text,n.adapter,n.attempts`)
    .bind(leaseUntil,now,id,now,now).first<CustomerMail>(),
 async current(mail,leaseUntil){const leasePredicate=`n.id=? AND n.status='sending' AND n.attempts=? AND n.lease_until=? AND ${scheduledFeaturePredicate('n.company_id','sales')}`;const leaseArgs=[mail.id,mail.attempts,leaseUntil];return await db.prepare(`SELECT p.status,p.notification_email,p.notification_consent,ns.adapter
    FROM commerce_customer_notifications n JOIN (${commerceProjectionSources.commerce_fulfillments}) f ON f.id=n.fulfillment_id
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id AND o.id=n.order_id AND o.company_id=n.company_id
    JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.id=n.party_id AND p.company_id=o.company_id
    LEFT JOIN commerce_customer_notification_settings ns ON ns.company_id=o.company_id
    WHERE ${leasePredicate} AND o.kind='sales' AND o.state IN ('confirmed','partially_fulfilled','fulfilled')`)
    .bind(...leaseArgs).first<Row>();},
 async cancel(mail,leaseUntil,invalid,now){const leasePredicate=`n.id=? AND n.status='sending' AND n.attempts=? AND n.lease_until=? AND ${scheduledFeaturePredicate('n.company_id','sales')}`;const leaseArgs=[mail.id,mail.attempts,leaseUntil];await db.prepare(`UPDATE commerce_customer_notifications n SET status='cancelled',version=version+1,lease_until=NULL,last_error=?,updated_at=? WHERE ${leasePredicate}`)
    .bind(invalid,now,...leaseArgs).run();},
 async sent(mail,leaseUntil,providerId,now){const leasePredicate=`n.id=? AND n.status='sending' AND n.attempts=? AND n.lease_until=? AND ${scheduledFeaturePredicate('n.company_id','sales')}`;const leaseArgs=[mail.id,mail.attempts,leaseUntil];await db.prepare(`UPDATE commerce_customer_notifications n SET status='sent',version=version+1,provider_message_id=?,sent_at=?,lease_until=NULL,last_error=NULL,updated_at=?
      WHERE ${leasePredicate} AND ${notificationSourcePredicate('n')}`)
      .bind(providerId,now,now,...leaseArgs).run();},
 async failed(mail,leaseUntil,retryAt,code,now){const leasePredicate=`n.id=? AND n.status='sending' AND n.attempts=? AND n.lease_until=? AND ${scheduledFeaturePredicate('n.company_id','sales')}`;const leaseArgs=[mail.id,mail.attempts,leaseUntil];await db.prepare(`UPDATE commerce_customer_notifications n SET status='failed',version=version+1,next_attempt_at=?,lease_until=NULL,last_error=?,updated_at=? WHERE ${leasePredicate}`)
    .bind(retryAt,code,now,...leaseArgs).run();},
 next:async now=>await db.prepare(`SELECT n.id FROM commerce_customer_notifications n WHERE n.attempts<8
      AND ((n.status IN ('pending','failed') AND n.next_attempt_at<=?) OR (n.status='sending' AND n.lease_until<?))
      AND ${scheduledFeaturePredicate('n.company_id','sales')} ORDER BY n.next_attempt_at,n.created_at,n.id FOR UPDATE SKIP LOCKED LIMIT 1`)
      .bind(now,now).first<string>('id'),
};}
function deliveryApplication(db:SqlDatabase,env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,send:(input:RequestInfo | URL, init?:RequestInit)=>Promise<Response>){return createCustomerDeliveryApplication({repository:deliveryRepository(db),async send(mail){if(!env.RESEND_API_KEY||!env.RESEND_FROM)throw new Error('CUSTOMER_EMAIL_NOT_CONFIGURED');
const response=await send('https://api.resend.com/emails',{method:'POST',headers:{Authorization:`Bearer ${String(env.RESEND_API_KEY)}`,'Content-Type':'application/json','Idempotency-Key':mail.id},body:JSON.stringify({from:String(env.RESEND_FROM),to:[mail.recipient_email],subject:mail.subject,text:mail.body_text}),signal:AbortSignal.timeout(10000)});
if(!response.ok)throw new Error('CUSTOMER_EMAIL_DELIVERY_FAILED');
const data=await response.json().catch(()=>({})) as {id?:string};
return data.id??`resend-${mail.id}`;}});}
export async function deliverCustomerNotification(db:SqlDatabase,env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,id:string,send:(input:RequestInfo | URL, init?:RequestInit)=>Promise<Response>=fetch,now=Date.now()){return await commercePackageEnforcementMode()==='disabled'?deliveryApplication(db,env,send).deliver(id,now):deliverPurchasedCustomerNotification(db,env,id,send,now);}
export async function drainCustomerNotifications(db:SqlDatabase,env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,now=Date.now()){return await commercePackageEnforcementMode()==='disabled'?deliveryApplication(db,env,fetch).drain(now):drainPurchasedCustomerNotifications(db,env,now);}

