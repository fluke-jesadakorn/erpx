import 'server-only';
import { randomUUID } from 'node:crypto';
import { record, records, type Snapshot } from '@/lib/pms';
import { executePms } from '@/lib/pms-engine';
import { samePerson } from '@/lib/iam';
import { bookingSnapshot, persist, queueMail, bookingEnv, type BookingPaymentRow } from './pms-booking';
import { effects } from './pms';
import { stripeCall } from './pos-stripe';
import {verifiedStripePaymentTime} from './stripe-payment-time';
import { ApiError } from './http';
import { requirePermission } from './iam-permissions';
import { holdingResources } from './runtime';
import type { WorkContext } from './authorization';
import type { SqlDatabase, TransactionalSqlDatabase } from './runtime/types';
type Part = {stayId:string;folioId:string;entryId:string;amount:number};
type Request = {id:string;payment_id:string;amount_minor:number;allocations_json:string;reason:string;requested_by:string;approved_by:string|null;provider_id:string|null;state:string;generation:number;legacy_review?:boolean};
const run=(db:SqlDatabase,sql:string,...args:unknown[])=>db.prepare(sql).bind(...args).run();
const providerActor=(now:number)=>({userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});

/** Called while the Property lock is held. Reserve original receipts before another command can consume them. */
export async function requestBookingRefund(tx:SqlDatabase,s:Snapshot,paymentId:string,wanted:Array<{stayId:string;amount:number}>,reason:string,requestedBy:string,now=Date.now(),requestId=randomUUID()) {
 const payment=await tx.prepare('SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE').bind(paymentId,s.property.id).first<BookingPaymentRow>();
 if(!payment||!['confirmed','cancelled'].includes(payment.status))throw new ApiError(409,'PMS_REFUND_NOT_AVAILABLE','Choose a confirmed original payment');
 const prior=await tx.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=?').bind(requestId).first<Request>();
 if(prior){
  const old=JSON.parse(prior.allocations_json) as Part[];
  if(prior.payment_id!==paymentId||prior.reason!==reason||prior.requested_by!==requestedBy||JSON.stringify(old.map(p=>({stayId:p.stayId,amount:p.amount})))!==JSON.stringify(wanted))throw new ApiError(409,'PMS_REFUND_KEY_REUSED','Use a new refund request key');
  return prior;
 }
 const sources=(await tx.prepare('SELECT stay_id,folio_id,entry_id,amount_minor FROM pms_booking_allocations WHERE payment_id=? ORDER BY stay_id').bind(paymentId).all<{stay_id:string;folio_id:string;entry_id:string;amount_minor:number}>()).results;
 const previous=(await tx.prepare('SELECT amount_minor,allocations_json FROM pms_booking_refund_requests WHERE payment_id=?').bind(paymentId).all<{amount_minor:number;allocations_json:string}>()).results;
 const used=previous.flatMap(r=>JSON.parse(r.allocations_json) as Part[]);
 const parts:Part[]=[];
 for(const selection of wanted){
  if(!Number.isSafeInteger(selection.amount)||selection.amount<=0)throw new ApiError(422,'PMS_REFUND_INPUT','Enter a positive amount');
  const source=sources.find(p=>p.stay_id===selection.stayId);
  const consumed=used.filter(p=>p.stayId===selection.stayId).reduce((n,p)=>n+p.amount,0)+parts.filter(p=>p.stayId===selection.stayId).reduce((n,p)=>n+p.amount,0);
  if(!source||selection.amount>Number(source.amount_minor)-consumed)throw new ApiError(409,'PMS_REFUND_EXCEEDS_PAYMENT','Refund exceeds the remaining original receipt');
  parts.push({stayId:source.stay_id,folioId:source.folio_id,entryId:source.entry_id,amount:selection.amount});
 }
 const amount=parts.reduce((n,p)=>n+p.amount,0);
 if(!amount||previous.reduce((n,p)=>n+Number(p.amount_minor),0)+amount>Number(payment.amount_minor))throw new ApiError(409,'PMS_REFUND_EXCEEDS_PAYMENT','Cumulative refunds exceed the payment');
 const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
 for(const part of parts)executePms(s,'payment.refund_reserve',{folioId:part.folioId,amount:part.amount,sourceEntryId:part.entryId},providerActor(now));
 await persist(tx,s,before);
 await run(tx,"INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,?,?,?,'requested',?,?)",requestId,paymentId,amount,JSON.stringify(parts),reason,requestedBy,now,now);
 await run(tx,"UPDATE pms_booking_payments SET refund_state='requested',refund_requested_at=?,updated_at=? WHERE id=?",now,now,paymentId);
 return {id:requestId,state:'requested',amount_minor:amount};
}
export async function approveBookingRefund(db:TransactionalSqlDatabase,actor:WorkContext,id:string){
 requirePermission(actor,'finance.approve');
 if(actor.branchId||actor.departmentId)throw new ApiError(403,'PMS_COMPANY_FINANCE','Use the company Finance workspace');
 const found=await db.prepare("SELECT r.id,p.property_id FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE (r.id=? OR p.id=?) AND p.company_id=? AND r.state='requested' ORDER BY r.created_at LIMIT 1").bind(id,id,actor.companyId).first<{id:string;property_id:string}>();
 if(!found)throw new ApiError(409,'PMS_REFUND_NOT_REQUESTED','No refund request awaits approval');
 return db.transaction(async tx=>{
  const s=await bookingSnapshot(tx,found.property_id,undefined,true);
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const request=await tx.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(found.id).first<Request>();
  if(!request||request.state!=='requested'||samePerson(actor,request.requested_by))throw new ApiError(409,'PMS_INDEPENDENT_REFUND_APPROVAL','A different Finance user must approve the refund');
  if(!s.property.config.onlineAccounting?.approvedBy)throw new ApiError(409,'PMS_ONLINE_ACCOUNTING_REQUIRED','Finance must approve the online liability accounts');
  if(!(JSON.parse(request.allocations_json) as Part[]).length && !records(s,'entry').some(e=>e.data.sourceKey===`online-payment:${request.payment_id}`&&e.data.description==='payment.unallocated'))
   throw new ApiError(409,'PMS_UNALLOCATED_RECEIPT_MISSING','Recover and account for the original provider receipt before approving its refund');
  const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
  for(const part of JSON.parse(request.allocations_json) as Part[]){
   if(request.legacy_review)executePms(s,'payment.refund_reserve',{folioId:part.folioId,amount:part.amount,sourceEntryId:part.entryId},providerActor(Date.now()));
   const outcome=executePms(s,'payment.refund_liability',{folioId:part.folioId,amount:part.amount,reference:request.id,sourceEntryId:part.entryId},providerActor(Date.now()));
   await persist(tx,s,before);await effects(tx,actor,s,outcome.effects);
  }
  await run(tx,"UPDATE pms_booking_refund_requests SET state='approved',legacy_review=false,approved_by=?,updated_at=? WHERE id=?",actor.userId,Date.now(),request.id);
  await run(tx,"UPDATE pms_booking_payments SET refund_state='approved',refund_approved_by=?,updated_at=? WHERE id=?",actor.userId,Date.now(),request.payment_id);
  return {id:request.id,state:'approved'};
 });
}
export async function bookingRefundsForFinance(actor:WorkContext){
 requirePermission(actor,'finance.read');if(actor.branchId||actor.departmentId)return [];
 const {write}=await holdingResources(actor.holdingId);
 return (await write.prepare(`SELECT r.id,p.id payment_id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.last_error,r.reason,r.created_at, (p.status='refund_review' AND r.state='requested' AND r.allocations_json::jsonb='[]'::jsonb AND NOT EXISTS(SELECT 1 FROM pms_records e WHERE e.property_id=p.property_id AND e.kind='entry' AND e.data_json::jsonb->>'sourceKey'='online-payment:'||p.id)) needs_receipt_recovery FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100`).bind(actor.companyId).all<{id:string;payment_id:string;property_id:string;amount_minor:number;status:string;refund_state:string;last_error:string|null;reason:string;created_at:number;needs_receipt_recovery:boolean}>()).results;
}

/** Older refund-review rows can lack their clearing liability. Recover the original verified
 * money once before Finance may approve a provider refund; never collect again. */
export async function recoverBookingReceipt(db:TransactionalSqlDatabase,actor:WorkContext,requestId:string,reason:string){
 requirePermission(actor,'finance.approve');
 if(actor.branchId||actor.departmentId||reason.trim().length<3)throw new ApiError(422,'PMS_RECEIPT_RECOVERY_INPUT','Use company Finance and enter a recovery reason');
 const found=await db.prepare(`SELECT p.* FROM pms_booking_payments p JOIN pms_booking_refund_requests r ON r.payment_id=p.id WHERE r.id=? AND p.company_id=? AND p.status='refund_review' AND r.state='requested'`).bind(requestId,actor.companyId).first<BookingPaymentRow>();
 if(!found?.checkout_id)throw new ApiError(409,'PMS_RECEIPT_RECOVERY_UNAVAILABLE','Select an original unassigned payment awaiting Finance review');
 const session=await stripeCall(Boolean(found.livemode),`checkout/sessions/${found.checkout_id}`,{account:found.account_id});
 const pi=await stripeCall(Boolean(found.livemode),`payment_intents/${String(session.payment_intent)}`,{account:found.account_id});
 if(session.payment_status!=='paid'||pi.status!=='succeeded'||pi.amount_received!==Number(found.amount_minor)||pi.currency!=='thb'||(pi.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==found.id)throw new ApiError(409,'PMS_PAYMENT_MISMATCH','The original provider receipt does not match');
 const now=Date.now(),succeededAt=await verifiedStripePaymentTime({livemode:Boolean(found.livemode),accountId:found.account_id,paymentIntentId:String(pi.id),known:found.provider_succeeded_at?Number(found.provider_succeeded_at):null,createdAt:Number(found.created_at),now});
 return db.transaction(async tx=>{
  const s=await bookingSnapshot(tx,found.property_id,undefined,true);await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const current=await tx.prepare("SELECT status FROM pms_booking_payments WHERE id=? FOR UPDATE").bind(found.id).first<{status:string}>();
  if(current?.status!=='refund_review')throw new ApiError(409,'PMS_PAYMENT_CHANGED','Reload the original payment before recovery');
  const existing=records(s,'entry').find(e=>e.data.sourceKey===`online-payment:${found.id}`);
  if(existing){if(existing.data.description!=='payment.unallocated')throw new ApiError(409,'PMS_RECEIPT_RECOVERY_UNAVAILABLE','Recover the original allocated receipt through its owner');return {id:requestId,state:'receipt_recovered'};}
  const stay=records(s,'stay').find(st=>st.data.reservationId===found.reservation_id);if(!stay)throw new ApiError(409,'PMS_RECEIPT_TARGET_MISSING','The original booking needs owner review');
  const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)])),clearing=s.property.config.accounts.edc;s.property.config.accounts.edc=found.clearing_account;
  const result=executePms(s,'payment.unallocated',{folioId:stay.data.folioId,amount:Number(found.amount_minor),reference:String(pi.id)},providerActor(now));record(s,'entry',result.id).data.sourceKey=`online-payment:${found.id}`;
  await persist(tx,s,before);await effects(tx,actor,s,result.effects);s.property.config.accounts.edc=clearing;
  const reservation=record(s,'reservation',found.reservation_id);reservation.data.history=[...(reservation.data.history??[]),{at:now,actor:actor.userId,action:`Recovered original unallocated receipt: ${reason.trim()}`}];reservation.version++;reservation.updatedAt=now;await persist(tx,s,before);
  await run(tx,'UPDATE pms_booking_payments SET paid_at=?,provider_succeeded_at=?,payment_intent_id=?,last_error=NULL,updated_at=? WHERE id=?',succeededAt,succeededAt,pi.id,now,found.id);
  return {id:requestId,state:'receipt_recovered'};
 });
}
export async function processBookingRefund(db:TransactionalSqlDatabase,id:string,now=Date.now()){
 const requests=(await db.prepare("SELECT * FROM pms_booking_refund_requests WHERE (id=? OR payment_id=?) AND state IN ('approved','processing','requires_action') ORDER BY created_at").bind(id,id).all<Request>()).results;
 for(const request of requests){
  const payment=await db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(request.payment_id).first<BookingPaymentRow>();
  if(!payment?.checkout_id)throw new ApiError(409,'PMS_REFUND_PROVIDER_MISSING','Original Checkout session is missing');
  let refundId=request.provider_id;
  if(!refundId){
   const session=await stripeCall(Boolean(payment.livemode),`checkout/sessions/${payment.checkout_id}`,{account:payment.account_id});
   const pi=await stripeCall(Boolean(payment.livemode),`payment_intents/${String(session.payment_intent)}`,{account:payment.account_id});
   if(pi.status!=='succeeded'||pi.amount_received!==Number(payment.amount_minor)||(pi.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==payment.id)throw new ApiError(409,'PMS_REFUND_PAYMENT_MISMATCH','Original provider payment needs review');
   const guest=record(await bookingSnapshot(db,payment.property_id),'reservation',payment.reservation_id).data.guestId;
   const email=record(await bookingSnapshot(db,payment.property_id),'guest',guest).data.email;
   const charge=typeof pi.latest_charge==='string'?await stripeCall(Boolean(payment.livemode),`charges/${pi.latest_charge}`,{account:payment.account_id}):null;
   const promptpay=(charge?.payment_method_details as {type?:string}|undefined)?.type==='promptpay'||JSON.parse(payment.methods_json).join(',')==='promptpay';
   const refund=await stripeCall(Boolean(payment.livemode),'refunds',{method:'POST',account:payment.account_id,key:`pms-refund-${request.id}-${request.generation}`,body:{payment_intent:pi.id,amount:String(request.amount_minor),...(promptpay?{instructions_email:email}:{}),'metadata[pms_booking_payment_id]':payment.id,'metadata[pms_booking_refund_id]':request.id}});
   refundId=String(refund.id);
   await run(db,'INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING',request.id,request.generation,refundId,now);
   await run(db,"UPDATE pms_booking_refund_requests SET provider_id=?,state='processing',updated_at=? WHERE id=? AND provider_id IS NULL",refundId,now,request.id);
   await run(db,"UPDATE pms_booking_payments SET refund_id=?,refund_state='processing',updated_at=? WHERE id=?",refundId,now,payment.id);
  }
  const refund=await stripeCall(Boolean(payment.livemode),`refunds/${refundId}`,{account:payment.account_id});
  if(refund.amount!==Number(request.amount_minor)||refund.currency!=='thb'&&refund.currency!==undefined||(refund.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==payment.id||((refund.metadata as Record<string,string>|undefined)?.pms_booking_refund_id&&((refund.metadata as Record<string,string>).pms_booking_refund_id!==request.id)))throw new ApiError(409,'PMS_REFUND_PROVIDER_MISMATCH','Refund amount or payment reference does not match');
  if(refund.status==='failed'||refund.status==='canceled'){
   await run(db,"UPDATE pms_booking_refund_requests SET state='failed',last_error='Provider refund failed; liability remains outstanding',updated_at=? WHERE id=?",now,request.id);
   await run(db,"UPDATE pms_booking_payments SET refund_state='failed',updated_at=? WHERE id=?",now,payment.id);continue;
  }
  if(refund.status!=='succeeded'){
   const state=refund.status==='requires_action'?'requires_action':'processing';
   await run(db,'UPDATE pms_booking_refund_requests SET state=?,updated_at=? WHERE id=?',state,now,request.id);continue;
  }
  await db.transaction(async tx=>{
   const s=await bookingSnapshot(tx,payment.property_id,undefined,true);
   await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
   const current=await tx.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(request.id).first<Request>();
   if(current?.state==='succeeded')return;
   if(!current||!['processing','requires_action'].includes(current.state))throw new ApiError(409,'PMS_REFUND_STATE','Refund state changed');
   const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)])),oldEdc=s.property.config.accounts.edc;
   s.property.config.accounts.edc=payment.clearing_account;
   const parts=JSON.parse(current.allocations_json) as Part[];
   if(!parts.length){const st=records(s,'stay').find(st=>st.data.reservationId===payment.reservation_id)!;const receipt=records(s,'entry').find(e=>e.data.sourceKey===`online-payment:${payment.id}`&&e.data.description==='payment.unallocated');if(!receipt)throw new ApiError(409,'PMS_UNALLOCATED_RECEIPT_MISSING','Unallocated receipt requires recovery');parts.push({stayId:st.id,folioId:st.data.folioId,entryId:receipt.id,amount:Number(current.amount_minor)});}
   for(const part of parts){
    const outcome=executePms(s,'payment.refund_settle',{folioId:part.folioId,amount:part.amount,reference:refundId,sourceEntryId:part.entryId},providerActor(now));
    await persist(tx,s,before);await effects(tx,{companyId:payment.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);
   }
   s.property.config.accounts.edc=oldEdc;await persist(tx,s,before);
   await run(tx,"UPDATE pms_booking_refund_requests SET state='succeeded',last_error=NULL,updated_at=? WHERE id=?",now,current.id);
   await run(tx,"UPDATE pms_booking_payments SET refund_state='succeeded',updated_at=? WHERE id=?",now,payment.id);
   await queueMail(tx,s,payment.reservation_id,payment.holding_slug,'refund',await bookingEnv(),now);
  });
 }
}

/** Retry only after Stripe confirms that the previous refund failed or was cancelled. */
export async function retryBookingRefund(db:TransactionalSqlDatabase,actor:WorkContext,id:string,reason:string){
 requirePermission(actor,'finance.approve');
 if(actor.branchId||actor.departmentId||reason.trim().length<3)throw new ApiError(422,'PMS_REFUND_RETRY_INPUT','Use company Finance and enter a recovery reason');
 const found=await db.prepare("SELECT r.*,p.property_id,p.account_id,p.livemode FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE r.id=? AND p.company_id=?").bind(id,actor.companyId).first<Request&{property_id:string;account_id:string;livemode:boolean}>();
 if(!found||found.state!=='failed'||!found.provider_id||samePerson(actor,found.requested_by))throw new ApiError(409,'PMS_REFUND_RETRY_UNAVAILABLE','An independent Finance reviewer must select a failed provider refund');
 const provider=await stripeCall(Boolean(found.livemode),`refunds/${found.provider_id}`,{account:found.account_id});
 if(!['failed','canceled'].includes(String(provider.status)))throw new ApiError(409,'PMS_REFUND_PROVIDER_PENDING','Recover the current provider refund without creating another');
 return db.transaction(async tx=>{
  await bookingSnapshot(tx,found.property_id,undefined,true);
  const current=await tx.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(id).first<Request>();
  if(current?.state!=='failed'||current.provider_id!==found.provider_id)throw new ApiError(409,'PMS_REFUND_CHANGED','Refund changed; refresh before retrying');
  await run(tx,'INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING',id,current.generation,current.provider_id,Date.now());
  await run(tx,"UPDATE pms_booking_refund_requests SET generation=generation+1,provider_id=NULL,state='approved',last_error=?,updated_at=? WHERE id=?",`Recovery approved by ${actor.userId}: ${reason.trim()}`.slice(0,500),Date.now(),id);
  await run(tx,"UPDATE pms_booking_payments SET refund_state='approved',updated_at=? WHERE id=?",Date.now(),current.payment_id);
  return {id,state:'approved'};
 });
}
