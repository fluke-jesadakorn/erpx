import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import {holdingResources} from './runtime';
import type {SqlDatabase} from './runtime/types';
import type {JobEnv} from './job-projector';
import {requirePermission} from './iam-permissions';
import {ApiError} from './http';
import {assertion,executeCommand,replayCommand} from './commands';
import {assertProductionModuleEnabled,productionFeatureGuard,scheduledFeaturePredicate} from './hospitality-rollout';

type Row=Record<string,string|number|null>;
export const customerNotificationSchema=z.object({expectedVersion:z.number().int().positive(),note:z.string().trim().min(3).max(500)});
export const customerNotificationRetrySchema=z.object({expectedVersion:z.number().int().positive()});
export const customerNotificationSettingsSchema=z.object({expectedVersion:z.number().int().min(0),adapter:z.enum(['disabled','sandbox','resend'])});

function scoped(actor:WorkContext){return {sql:"o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)",args:[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]};}

export async function customerNotificationSettings(actor:WorkContext){requirePermission(actor,'commerce.read');const {write}=await holdingResources(actor.holdingId);return await write.prepare("SELECT * FROM commerce_customer_notification_settings WHERE company_id=?").bind(actor.companyId).first<Row>()??{company_id:actor.companyId,adapter:'disabled',version:0,updated_by:null,updated_at:null};}

export async function updateCustomerNotificationSettings(actor:WorkContext,input:z.infer<typeof customerNotificationSettingsSchema>,key:string){requirePermission(actor,'commerce.approve');const {write}=await holdingResources(actor.holdingId);await assertProductionModuleEnabled(write,actor.companyId,'sales');const previous=await replayCommand(write,actor,key,'commerce.customer-notification-settings.update',actor.companyId,input);if(previous)return previous;const current=await write.prepare('SELECT * FROM commerce_customer_notification_settings WHERE company_id=?').bind(actor.companyId).first<Row>();if(Number(current?.version??0)!==input.expectedVersion)throw new ApiError(409,'VERSION_CONFLICT','Customer notification settings changed. Refresh before saving.');const version=input.expectedVersion+1,result={companyId:actor.companyId,adapter:input.adapter,version};return executeCommand(write,actor,key,'commerce.customer-notification-settings.update',input,actor.companyId,version,result,[...productionFeatureGuard(write,key,actor.companyId,'sales'),input.expectedVersion===0?assertion(write,key,'settings-new','NOT EXISTS(SELECT 1 FROM commerce_customer_notification_settings WHERE company_id=?)',[actor.companyId]):assertion(write,key,'settings-version','EXISTS(SELECT 1 FROM commerce_customer_notification_settings WHERE company_id=? AND version=?)',[actor.companyId,input.expectedVersion]),write.prepare("INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) VALUES(?,?,1,?,?) ON CONFLICT(company_id) DO UPDATE SET adapter=excluded.adapter,version=commerce_customer_notification_settings.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(actor.companyId,input.adapter,actor.userId,Date.now())]);}

export async function queueDeliveryNotification(actor:WorkContext,fulfillmentId:string,input:z.infer<typeof customerNotificationSchema>,key:string){
  requirePermission(actor,'commerce.sales');
  const {write}=await holdingResources(actor.holdingId);await assertProductionModuleEnabled(write,actor.companyId,'sales');const previous=await replayCommand(write,actor,key,'commerce.customer-notification.queue',fulfillmentId,input);if(previous)return previous;
  const s=scoped(actor),row=await write.prepare(`SELECT f.id fulfillment_id,f.version fulfillment_version,f.number fulfillment_number,f.fulfillment_date,o.id order_id,o.number order_number,o.branch_id,o.department_id,p.id party_id,p.name party_name,p.version party_version,p.notification_email,p.notification_consent,ns.adapter,ns.version settings_version FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id JOIN finance_parties p ON p.id=o.party_id LEFT JOIN commerce_customer_notification_settings ns ON ns.company_id=o.company_id WHERE f.id=? AND o.kind='sales' AND ${s.sql}`).bind(fulfillmentId,...s.args).first<Row>();
  if(!row)throw new ApiError(404,'DELIVERY_NOT_FOUND','The customer delivery is unavailable in this work scope.');
  if(Number(row.fulfillment_version)!==input.expectedVersion)throw new ApiError(409,'VERSION_CONFLICT','The delivery changed. Refresh before notifying the customer.');
  if(row.notification_consent!=='granted'||!row.notification_email)throw new ApiError(422,'CUSTOMER_NOTIFICATION_CONSENT_REQUIRED','Record a verified customer notification email and granted consent before sending.');
  if(row.adapter!=='sandbox'&&row.adapter!=='resend')throw new ApiError(422,'CUSTOMER_NOTIFICATION_PROVIDER_REQUIRED','Configure a sandbox or live customer email adapter before sending.');
  const existing=await write.prepare('SELECT id,status FROM commerce_customer_notifications WHERE fulfillment_id=?').bind(fulfillmentId).first<Row>();
  if(existing)throw new ApiError(409,'CUSTOMER_ALREADY_NOTIFIED',`This delivery already has a ${String(existing.status)} customer notification.`);
  const lines=(await write.prepare('SELECT p.sku,fl.quantity_milli,p.unit FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal').bind(fulfillmentId).all<Row>()).results;
  const id=`customer-notification-${crypto.randomUUID()}`,now=Date.now(),subject=`Delivery ${String(row.fulfillment_number)} for order ${String(row.order_number)}`,body=[`Hello ${String(row.party_name)},`,`Your delivery ${String(row.fulfillment_number)} for order ${String(row.order_number)} was recorded on ${String(row.fulfillment_date)}.`,...lines.map(line=>`${String(line.sku)}: ${Number(line.quantity_milli)/1000} ${String(line.unit)}`),`Update from our team: ${input.note}`].join('\n\n');
  const result={id,orderId:String(row.order_id),status:'pending',version:1};
  return executeCommand(write,actor,key,'commerce.customer-notification.queue',input,fulfillmentId,1,result,[
    ...productionFeatureGuard(write,key,actor.companyId,'sales'),
    assertion(write,key,'provider-settings',"EXISTS(SELECT 1 FROM commerce_customer_notification_settings WHERE company_id=? AND version=? AND adapter=?)",[actor.companyId,row.settings_version,row.adapter]),
    assertion(write,key,'delivery-version','EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id JOIN finance_parties p ON p.id=o.party_id WHERE f.id=? AND f.version=? AND o.company_id=? AND p.version=? AND p.notification_email=? AND p.notification_consent=\'granted\')',[fulfillmentId,input.expectedVersion,actor.companyId,row.party_version,row.notification_email]),
    write.prepare('INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,fulfillment_id,party_id,party_version,recipient_email,subject,body_text,adapter,next_attempt_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,actor.companyId,row.branch_id,row.order_id,fulfillmentId,row.party_id,row.party_version,String(row.notification_email).toLowerCase(),subject,body,String(row.adapter),now,actor.userId,now,now),
  ],{companyId:actor.companyId,branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null});
}

export async function retryCustomerNotification(actor:WorkContext,id:string,input:z.infer<typeof customerNotificationRetrySchema>,key:string){
  requirePermission(actor,'commerce.sales');const {write}=await holdingResources(actor.holdingId);await assertProductionModuleEnabled(write,actor.companyId,'sales');const previous=await replayCommand(write,actor,key,'commerce.customer-notification.retry',id,input);if(previous)return previous;const s=scoped(actor),row=await write.prepare(`SELECT n.* FROM commerce_customer_notifications n JOIN commerce_orders o ON o.id=n.order_id WHERE n.id=? AND ${s.sql}`).bind(id,...s.args).first<Row>();if(!row)throw new ApiError(404,'CUSTOMER_NOTIFICATION_NOT_FOUND','The customer notification is unavailable in this work scope.');if(row.status!=='failed')throw new ApiError(409,'CUSTOMER_NOTIFICATION_NOT_FAILED','Only a failed customer notification can be retried.');if(Number(row.version)!==input.expectedVersion)throw new ApiError(409,'VERSION_CONFLICT','The notification changed. Refresh before retrying.');const result={id,orderId:String(row.order_id),status:'pending',version:input.expectedVersion+1};return executeCommand(write,actor,key,'commerce.customer-notification.retry',input,id,input.expectedVersion+1,result,[...productionFeatureGuard(write,key,actor.companyId,'sales'),assertion(write,key,'current-customer-binding',`EXISTS(SELECT 1 FROM commerce_customer_notifications n WHERE n.id=? AND ${notificationSourcePredicate('n')})`,[id]),assertion(write,key,'notification-version',"EXISTS(SELECT 1 FROM commerce_customer_notifications WHERE id=? AND version=? AND status='failed')",[id,input.expectedVersion]),write.prepare("UPDATE commerce_customer_notifications SET status='pending',version=version+1,attempts=0,next_attempt_at=?,lease_until=NULL,last_error=NULL,updated_at=? WHERE id=?").bind(Date.now(),Date.now(),id)],{companyId:actor.companyId,branchId:row.branch_id?String(row.branch_id):null,departmentId:null});
}

/** The immutable message still belongs to its current delivery, customer consent and configured adapter. */
function notificationSourcePredicate(alias:string) {
  return `EXISTS(SELECT 1 FROM commerce_fulfillments f
    JOIN commerce_orders o ON o.id=f.order_id JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id
    JOIN commerce_customer_notification_settings ns ON ns.company_id=o.company_id
    WHERE f.id=${alias}.fulfillment_id AND o.id=${alias}.order_id AND o.company_id=${alias}.company_id
      AND o.party_id=${alias}.party_id AND o.kind='sales' AND o.state IN ('confirmed','partially_fulfilled','fulfilled')
      AND p.status='active' AND p.notification_consent='granted' AND lower(p.notification_email)=${alias}.recipient_email
      AND ns.adapter=${alias}.adapter AND ns.adapter IN ('sandbox','resend'))`;
}

type Mail={id:string;company_id:string;fulfillment_id:string;order_id:string;party_id:string;party_version:number;recipient_email:string;subject:string;body_text:string;adapter:string;attempts:number};
export async function deliverCustomerNotification(db:SqlDatabase,env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,id:string,send:typeof fetch=fetch,now=Date.now()){
  const leaseUntil=now+90000;
  const mail=await db.prepare(`UPDATE commerce_customer_notifications n SET status='sending',attempts=attempts+1,lease_until=?,updated_at=?
    WHERE n.id=? AND n.attempts<8 AND ((n.status IN ('pending','failed') AND n.next_attempt_at<=?) OR (n.status='sending' AND n.lease_until<?))
      AND ${scheduledFeaturePredicate('n.company_id','sales')}
    RETURNING n.id,n.company_id,n.fulfillment_id,n.order_id,n.party_id,n.party_version,n.recipient_email,n.subject,n.body_text,n.adapter,n.attempts`)
    .bind(leaseUntil,now,id,now,now).first<Mail>();
  if(!mail)return;
  const leasePredicate=`n.id=? AND n.status='sending' AND n.attempts=? AND n.lease_until=? AND ${scheduledFeaturePredicate('n.company_id','sales')}`;
  const leaseArgs=[mail.id,mail.attempts,leaseUntil];
  const current=await db.prepare(`SELECT p.status,p.notification_email,p.notification_consent,ns.adapter
    FROM commerce_customer_notifications n JOIN commerce_fulfillments f ON f.id=n.fulfillment_id
    JOIN commerce_orders o ON o.id=f.order_id AND o.id=n.order_id AND o.company_id=n.company_id
    JOIN finance_parties p ON p.id=o.party_id AND p.id=n.party_id AND p.company_id=o.company_id
    LEFT JOIN commerce_customer_notification_settings ns ON ns.company_id=o.company_id
    WHERE ${leasePredicate} AND o.kind='sales' AND o.state IN ('confirmed','partially_fulfilled','fulfilled')`)
    .bind(...leaseArgs).first<Row>();
  const invalid=!current?'CUSTOMER_NOTIFICATION_SOURCE_CHANGED':current.status!=='active'||current.notification_consent!=='granted'||String(current.notification_email??'').toLowerCase()!==mail.recipient_email
    ?'CUSTOMER_CONSENT_OR_EMAIL_CHANGED':current.adapter!==mail.adapter?'CUSTOMER_NOTIFICATION_PROVIDER_CHANGED':null;
  if(invalid){await db.prepare(`UPDATE commerce_customer_notifications n SET status='cancelled',version=version+1,lease_until=NULL,last_error=?,updated_at=? WHERE ${leasePredicate}`)
    .bind(invalid,now,...leaseArgs).run();return;}
  try{
    let providerId:string;
    if(mail.adapter==='sandbox')providerId=`sandbox-${mail.id}`;
    else {if(!env.RESEND_API_KEY||!env.RESEND_FROM)throw new Error('CUSTOMER_EMAIL_NOT_CONFIGURED');const response=await send('https://api.resend.com/emails',{method:'POST',headers:{Authorization:`Bearer ${String(env.RESEND_API_KEY)}`,'Content-Type':'application/json','Idempotency-Key':mail.id},body:JSON.stringify({from:String(env.RESEND_FROM),to:[mail.recipient_email],subject:mail.subject,text:mail.body_text}),signal:AbortSignal.timeout(10000)});if(!response.ok)throw new Error('CUSTOMER_EMAIL_DELIVERY_FAILED');const data=await response.json().catch(()=>({})) as {id?:string};providerId=data.id??`resend-${mail.id}`;}
    await db.prepare(`UPDATE commerce_customer_notifications n SET status='sent',version=version+1,provider_message_id=?,sent_at=?,lease_until=NULL,last_error=NULL,updated_at=?
      WHERE ${leasePredicate} AND ${notificationSourcePredicate('n')}`)
      .bind(providerId,now,now,...leaseArgs).run();
  }catch(error){const code=error instanceof Error?error.message:'CUSTOMER_EMAIL_DELIVERY_FAILED';await db.prepare(`UPDATE commerce_customer_notifications n SET status='failed',version=version+1,next_attempt_at=?,lease_until=NULL,last_error=?,updated_at=? WHERE ${leasePredicate}`)
    .bind(now+Math.min(3600000,1000*2**Math.min(mail.attempts,10)),code.slice(0,100),now,...leaseArgs).run();throw error;}
}

export async function drainCustomerNotifications(db:SqlDatabase,env:Pick<JobEnv,'RESEND_API_KEY'|'RESEND_FROM'>,now=Date.now()){
  for(let index=0;index<10;index++){
    const id=await db.prepare(`SELECT n.id FROM commerce_customer_notifications n WHERE n.attempts<8
      AND ((n.status IN ('pending','failed') AND n.next_attempt_at<=?) OR (n.status='sending' AND n.lease_until<?))
      AND ${scheduledFeaturePredicate('n.company_id','sales')} ORDER BY n.next_attempt_at,n.created_at,n.id FOR UPDATE SKIP LOCKED LIMIT 1`)
      .bind(now,now).first<string>('id');
    if(!id)break;
    try{await deliverCustomerNotification(db,env,id,fetch,now);}catch{}
  }
}
