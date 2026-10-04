import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {z} from 'zod';
import {record,type Snapshot} from '@/lib/pms';
import {executePms} from '@/lib/pms-engine';
import {hasPermission} from '@/lib/permissions';
import type {WorkContext} from './authorization';
import {ApiError} from './http';
import {snapshotMoney} from './company-money';
import {bookingSnapshot} from './pms-booking';
import {effects as persistEffects,persistPmsRecords} from './pms';
import {stripeCall} from './pos-stripe';
import type {TransactionalSqlDatabase,SqlDatabase} from './runtime/types';

type Payment={id:string;request_hash:string;property_id:string;folio_id:string;company_id:string;account_id:string;livemode:boolean;kind:'receipt'|'deposit';amount_minor:number;payer_email:string|null;provider_id:string|null;qr_data:string|null;state:'requested'|'pending'|'succeeded'|'cancelled'|'refund_review';expires_at:number;entry_id:string|null;refund_state:string|null;refund_amount_minor:number|null;refund_reason:string|null;refund_requested_by:string|null;refund_id:string|null};
const inputSchema=z.object({propertyId:z.string().min(1),folioId:z.string().min(1),kind:z.enum(['receipt','deposit']),amountMinor:z.number().int().positive(),payerEmail:z.union([z.email(),z.literal('')]).default('')});
const run=(db:SqlDatabase,sql:string,...args:unknown[])=>db.prepare(sql).bind(...args).run();
function visible(row:Payment){return {id:row.id,state:row.state,kind:row.kind,amountMinor:Number(row.amount_minor),qrData:row.state==='pending'?row.qr_data:null,expiresAt:Number(row.expires_at),entryId:row.entry_id,refundState:row.refund_state,payerEmailProvided:Boolean(row.payer_email)};}
function scope(actor:WorkContext,s:Snapshot){if(s.property.companyId!==actor.companyId||actor.branchId&&s.property.branchId!==actor.branchId||actor.departmentId)throw new ApiError(404,'PMS_NOT_FOUND','Hotel not found in this workspace');}
export async function createPmsStripePayment(db:TransactionalSqlDatabase,actor:WorkContext,raw:unknown,key:string,now=Date.now()){
 if(!hasPermission(actor,'pms.cashier'))throw new ApiError(403,'PMS_CASHIER_REQUIRED','Cashier access is required');
 z.string().uuid().parse(key);const input=inputSchema.parse(raw),digest=createHash('sha256').update(JSON.stringify({actor:actor.userId,company:actor.companyId,input})).digest('hex');
 const id=await db.transaction(async tx=>{
  const s=await bookingSnapshot(tx,input.propertyId,undefined,true);scope(actor,s);
  const old=await tx.prepare('SELECT id,request_hash FROM pms_stripe_payments WHERE id=?').bind(key).first<{id:string;request_hash:string}>();
  if(old){if(old.request_hash!==digest)throw new ApiError(409,'PMS_PAYMENT_KEY_REUSED','Use a new payment request key');return old.id;}
  if(s.property.state!=='open'||s.property.config.currency!=='THB')throw new ApiError(409,'PMS_PAYMENT_NOT_READY','Hotel must be open and use THB');
  const folio=record(s,'folio',input.folioId);
  if(folio.data.state!=='open'&&folio.data.state!=='credit')throw new ApiError(409,'PMS_FOLIO_CLOSED','Choose an open folio');
  const connection=await tx.prepare("SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'").bind(actor.companyId).first<{account_id:string;livemode:boolean}>();
  if(!connection?.account_id||connection.livemode&&process.env.PMS_STRIPE_LIVE_ENABLED!=='true')throw new ApiError(503,'PMS_PAYMENT_UNAVAILABLE','Company Stripe is not ready for PromptPay');
  await run(tx,"INSERT INTO pms_stripe_payments(id,request_hash,property_id,folio_id,company_id,account_id,livemode,kind,amount_minor,payer_email,state,expires_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,'requested',?,?,?,?)",key,digest,input.propertyId,input.folioId,actor.companyId,connection.account_id,connection.livemode,input.kind,input.amountMinor,input.payerEmail||null,now+30*60*1000,actor.userId,now,now);
  const money=await snapshotMoney(tx,actor,{sourceType:'pms_stripe_payment',sourceId:key,amountKind:'requested',transactionDate:s.property.businessDate,currency:'THB',amountUnits:String(input.amountMinor)});
  await money.statement.run();
  return key;
 });
 return ensurePmsStripeQr(db,id,now);
}
async function ensurePmsStripeQr(db:TransactionalSqlDatabase,id:string,now=Date.now()){
 const row=await db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(id).first<Payment>();
 if(!row)throw new ApiError(404,'PMS_PAYMENT_NOT_FOUND','Payment request not found');
 if(row.state!=='requested'||row.provider_id)return visible(row);
 if(Number(row.expires_at)<=now)throw new ApiError(409,'PMS_PAYMENT_EXPIRED','Create a new payment request');
 const pi=await stripeCall(Boolean(row.livemode),'payment_intents',{method:'POST',account:row.account_id,key:`pms-counter-${id}`,body:{amount:String(row.amount_minor),currency:'thb','payment_method_types[0]':'promptpay','payment_method_data[type]':'promptpay',confirm:'true','metadata[pms_counter_payment_id]':id,...(row.payer_email?{receipt_email:row.payer_email}:{})}});
 const qr=((pi.next_action as Record<string,unknown>|undefined)?.promptpay_display_qr_code as Record<string,unknown>|undefined)?.data;
 if(pi.status!=='requires_action'||typeof qr!=='string'||!qr)throw new ApiError(502,'PMS_PROMPTPAY_QR_UNAVAILABLE','Stripe did not return a PromptPay QR');
 await run(db,"UPDATE pms_stripe_payments SET provider_id=?,qr_data=?,state='pending',updated_at=? WHERE id=? AND state='requested'",pi.id,qr,now,id);
 return visible({...row,provider_id:pi.id,qr_data:qr,state:'pending'});
}
export async function pmsStripePaymentStatus(db:TransactionalSqlDatabase,actor:WorkContext,id:string,now=Date.now()){
 if(!hasPermission(actor,'pms.cashier'))throw new ApiError(403,'PMS_CASHIER_REQUIRED','Cashier access is required');
 const row=await db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=?').bind(id,actor.companyId).first<Payment>();
 if(!row)throw new ApiError(404,'PMS_PAYMENT_NOT_FOUND','Payment request not found');
 const s=await bookingSnapshot(db,row.property_id);scope(actor,s);
 await reconcilePmsStripePayment(db,id,now);
 const latest=(await db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(id).first<Payment>())!;
 return visible(latest);
}
export async function reconcilePmsStripePayment(db:TransactionalSqlDatabase,id:string,now=Date.now()){
 const row=await db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(id).first<Payment>();
 if(!row||!['pending','requested','cancelled'].includes(row.state))return;
 if(!row.provider_id){if(row.state==='requested'&&Number(row.expires_at)<=now)await run(db,"UPDATE pms_stripe_payments SET state='cancelled',updated_at=? WHERE id=?",now,id);return;}
 const pi=await stripeCall(Boolean(row.livemode),`payment_intents/${row.provider_id}`,{account:row.account_id});
 if(pi.status==='succeeded'){
  if(pi.amount_received!==Number(row.amount_minor)||pi.currency!=='thb'||(pi.metadata as Record<string,string>|undefined)?.pms_counter_payment_id!==id)throw new ApiError(409,'PMS_PROVIDER_PAYMENT_MISMATCH','Provider payment needs review');
  await db.transaction(async tx=>{
   const s=await bookingSnapshot(tx,row.property_id,undefined,true);
   await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
   const current=await tx.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(id).first<Payment>();
   if(!current||current.state==='succeeded'||current.state==='refund_review')return;
   if(current.state==='cancelled'||now>=Number(current.expires_at)){await run(tx,"UPDATE pms_stripe_payments SET state='refund_review',received_at=?,refund_state=?,refund_amount_minor=?,refund_reason='Payment received after the QR hold expired',refund_requested_by='system:stripe',updated_at=? WHERE id=?",now,current.payer_email?'requested':null,current.payer_email?current.amount_minor:null,now,id);return;}
   const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
   const settings=await tx.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(current.company_id).first<{accounts_json:string}>();
   const oldEdc=s.property.config.accounts.edc;
   s.property.config.accounts.edc=settings?JSON.parse(settings.accounts_json).stripe as string:oldEdc;
   const outcome=executePms(s,current.kind==='deposit'?'payment.stripe_deposit':'payment.stripe',{folioId:current.folio_id,amount:Number(current.amount_minor),reference:String(current.provider_id)},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
   record(s,'entry',outcome.id).data.sourceKey=`counter-payment:${current.id}`;
   await persistPmsRecords(tx,s,before);
   await persistEffects(tx,{companyId:current.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);
   s.property.config.accounts.edc=oldEdc;
   await run(tx,'UPDATE pms_properties SET version=?,updated_at=? WHERE id=?',s.property.version,now,s.property.id);
   await run(tx,"UPDATE pms_stripe_payments SET state='succeeded',entry_id=?,received_at=?,updated_at=? WHERE id=?",outcome.id,now,now,id);
  });
  return;
 }
 if(pi.status==='canceled'){await run(db,"UPDATE pms_stripe_payments SET state='cancelled',updated_at=? WHERE id=? AND state IN ('requested','pending')",now,id);return;}
 if(now>=Number(row.expires_at)||pi.status==='requires_payment_method'){
  await stripeCall(Boolean(row.livemode),`payment_intents/${row.provider_id}/cancel`,{method:'POST',account:row.account_id,key:`pms-counter-cancel-${id}`,body:{}});
  await reconcilePmsStripePayment(db,id,now);
 }
}
export async function drainPmsStripePayments(db:TransactionalSqlDatabase,now=Date.now()){
 const due=(await db.prepare("SELECT id FROM pms_stripe_payments WHERE state IN ('requested','pending') AND expires_at<=? ORDER BY expires_at LIMIT 20").bind(now).all<{id:string}>()).results;
 for(const row of due){try{await reconcilePmsStripePayment(db,row.id,now);await run(db,'UPDATE pms_stripe_payments SET last_error=NULL WHERE id=?',row.id);}catch(error){await run(db,'UPDATE pms_stripe_payments SET last_error=? WHERE id=?',error instanceof Error?error.message.slice(0,100):'PMS_PAYMENT_REVIEW',row.id);}}
 const refunds=(await db.prepare("SELECT id FROM pms_stripe_payments WHERE refund_state IN ('approved','processing') ORDER BY updated_at LIMIT 20").all<{id:string}>()).results;
 for(const row of refunds){try{await processPmsStripeRefund(db,row.id,now);await run(db,'UPDATE pms_stripe_payments SET last_error=NULL WHERE id=?',row.id);}catch(error){await run(db,'UPDATE pms_stripe_payments SET last_error=? WHERE id=?',error instanceof Error?error.message.slice(0,100):'PMS_REFUND_REVIEW',row.id);}}
}
export async function requestPmsStripeRefund(db:TransactionalSqlDatabase,actor:WorkContext,paymentId:string,amountMinor:number,reason:string){
 if(!hasPermission(actor,'pms.cashier'))throw new ApiError(403,'PMS_CASHIER_REQUIRED','Cashier access is required');
 if(!Number.isSafeInteger(amountMinor)||amountMinor<=0||reason.trim().length<3)throw new ApiError(422,'PMS_REFUND_INPUT','Enter a valid amount and reason');
 return db.transaction(async tx=>{
  const row=await tx.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE').bind(paymentId,actor.companyId).first<Payment>();
  if(!row||row.state!=='succeeded'||row.refund_state)throw new ApiError(409,'PMS_REFUND_NOT_AVAILABLE','Choose an unsettled Stripe receipt');
  const s=await bookingSnapshot(tx,row.property_id);scope(actor,s);
  if(!row.payer_email)throw new ApiError(409,'PMS_OFFLINE_REFUND_REQUIRED','Use the independently approved cash or bank refund with payout evidence');
  if(amountMinor>Number(row.amount_minor))throw new ApiError(422,'PMS_REFUND_EXCEEDS_PAYMENT','Refund exceeds the original payment');
  const f=record(s,'folio',row.folio_id);
  if(amountMinor>f.data[row.kind==='deposit'?'deposit':'advance'])throw new ApiError(409,'PMS_REFUND_EXCEEDS_BALANCE','Refund exceeds the remaining folio balance');
  await run(tx,"UPDATE pms_stripe_payments SET refund_state='requested',refund_amount_minor=?,refund_reason=?,refund_requested_by=?,updated_at=? WHERE id=?",amountMinor,reason.trim(),actor.userId,Date.now(),paymentId);
  return {id:paymentId,state:'requested'};
 });
}
export async function approvePmsStripeRefund(db:TransactionalSqlDatabase,actor:WorkContext,paymentId:string){
 if(!hasPermission(actor,'finance.approve')||actor.branchId||actor.departmentId)throw new ApiError(403,'PMS_FINANCE_REQUIRED','Company Finance approval is required');
 return db.transaction(async tx=>{
  const row=await tx.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE').bind(paymentId,actor.companyId).first<Payment>();
  if(!row||row.refund_state!=='requested'||row.refund_requested_by===actor.userId)throw new ApiError(409,'PMS_INDEPENDENT_REFUND_APPROVAL','A different Finance user must approve a pending refund');
  await run(tx,"UPDATE pms_stripe_payments SET refund_state='approved',refund_approved_by=?,updated_at=? WHERE id=?",actor.userId,Date.now(),paymentId);
  return {id:paymentId,state:'approved'};
 });
}
export async function processPmsStripeRefund(db:TransactionalSqlDatabase,id:string,now=Date.now()){
 const row=await db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(id).first<Payment>();
 if(!row||!['approved','processing'].includes(row.refund_state??''))return;
 if(!row.provider_id||!row.refund_amount_minor||!row.payer_email)throw new ApiError(409,'PMS_REFUND_PROVIDER_MISSING','Provider payment or refund email is missing');
 let refundId=row.refund_id;
 if(!refundId){
  const pi=await stripeCall(Boolean(row.livemode),`payment_intents/${row.provider_id}`,{account:row.account_id});
  if(pi.status!=='succeeded'||pi.amount_received!==Number(row.amount_minor)||(pi.metadata as Record<string,string>|undefined)?.pms_counter_payment_id!==id)throw new ApiError(409,'PMS_REFUND_PAYMENT_MISMATCH','Provider payment does not match');
  const refund=await stripeCall(Boolean(row.livemode),'refunds',{method:'POST',account:row.account_id,key:`pms-counter-refund-${id}`,body:{payment_intent:pi.id,amount:String(row.refund_amount_minor),instructions_email:row.payer_email,'metadata[pms_counter_payment_id]':id}});
  refundId=refund.id;
  await run(db,"UPDATE pms_stripe_payments SET refund_id=?,refund_state='processing',updated_at=? WHERE id=? AND refund_id IS NULL",refundId,now,id);
 }
 const refund=await stripeCall(Boolean(row.livemode),`refunds/${refundId}`,{account:row.account_id});
 if(refund.amount!==Number(row.refund_amount_minor)||(refund.metadata as Record<string,string>|undefined)?.pms_counter_payment_id!==id)throw new ApiError(409,'PMS_REFUND_PROVIDER_MISMATCH','Provider refund does not match');
 if(refund.status==='failed'||refund.status==='canceled'){await run(db,"UPDATE pms_stripe_payments SET refund_state='failed',updated_at=? WHERE id=?",now,id);return;}
 if(refund.status!=='succeeded')return;
 await db.transaction(async tx=>{
  const s=await bookingSnapshot(tx,row.property_id,undefined,true);
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const current=await tx.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(id).first<Payment>();
  if(current?.refund_state==='succeeded')return;
  if(!current||current.refund_state!=='processing')throw new ApiError(409,'PMS_REFUND_STATE','Refund state changed');
  if(current.state==='succeeded'){
   const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
   const settings=await tx.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(current.company_id).first<{accounts_json:string}>();
   const oldEdc=s.property.config.accounts.edc;
   s.property.config.accounts.edc=settings?JSON.parse(settings.accounts_json).stripe as string:oldEdc;
   const outcome=executePms(s,current.kind==='deposit'?'payment.stripe_deposit_refund':'payment.stripe_refund',{folioId:current.folio_id,amount:Number(current.refund_amount_minor),reference:String(refundId),reason:current.refund_reason??'Stripe refund'},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
   await persistPmsRecords(tx,s,before);
   await persistEffects(tx,{companyId:current.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);
   s.property.config.accounts.edc=oldEdc;
   await run(tx,'UPDATE pms_properties SET version=?,updated_at=? WHERE id=?',s.property.version,now,s.property.id);
  }
  await run(tx,"UPDATE pms_stripe_payments SET refund_state='succeeded',updated_at=? WHERE id=?",now,id);
 });
}
export async function pmsStripeRefundsForFinance(actor:WorkContext){
 if(!hasPermission(actor,'finance.approve')||actor.branchId||actor.departmentId)return [];
 const {holdingResources}=await import('./runtime');
 const {write}=await holdingResources(actor.holdingId);
 return (await write.prepare("SELECT id,property_id,folio_id,amount_minor,refund_amount_minor,refund_state,last_error,refund_reason,payer_email FROM pms_stripe_payments WHERE company_id=? AND refund_state IS NOT NULL ORDER BY updated_at DESC LIMIT 100").bind(actor.companyId).all<{id:string;property_id:string;folio_id:string;amount_minor:number;refund_amount_minor:number|null;refund_state:string;last_error:string|null;refund_reason:string|null;payer_email:string|null}>()).results;
}
