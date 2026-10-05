import 'server-only';
import { createHmac, createHash, timingSafeEqual, randomUUID } from 'node:crypto';
import { z } from 'zod';
import { record, records, dates, plusDays, type Snapshot } from '@/lib/pms';
import { bookingQuote, roomSelectionSchema, type RoomSelection, type BookingQuote } from '@/lib/pms-booking';
import { availability } from '@/lib/pms-inventory';
import {availableReceiptSources} from '@/lib/pms-receipt-funding';
import { executePms } from '@/lib/pms-engine';
import { authorizedBooking, bookingSummary, bookingSnapshot, persist, queueMail, bookingCheckout, type BookingEnv, type BookingPaymentRow } from './pms-booking';
import { requestBookingRefund } from './pms-booking-refunds';
import { effects, persistPmsRecords } from './pms';
import { snapshotMoney } from './company-money';
import { ApiError } from './http';
import { requirePermission } from './iam-permissions';
import { assertProductionModuleEnabled } from './hospitality-rollout';
import type { WorkContext } from './authorization';
import type { SqlDatabase, TransactionalSqlDatabase } from './runtime/types';
type StaffAuthority={actor:WorkContext;reservationId:string;reason:string;override:boolean};
type Proposal={staff?:{userId:string;reason:string;override:boolean};reservationId:string;version:number;kind:'change'|'cancel';items:Array<{stayId:string;room?:RoomSelection;quote?:BookingQuote;oldTotal:number;newTotal:number;dueNow:number;refund:number;fee:number;dueAtHotel:number}>;expiresAt:number;dueNow:number;refund:number;fee:number};
const run=(db:SqlDatabase,sql:string,...args:unknown[])=>db.prepare(sql).bind(...args).run();
const signature=(env:BookingEnv,payload:string)=>createHmac('sha256',env.secret).update(`pms-change:${payload}`).digest('base64url');
const actor=(now:number)=>({userId:'guest:self-service',now,uuid:randomUUID,independent:()=>false});
const providerActor=(now:number)=>({...actor(now),userId:'system:stripe'});
function signed(p:Proposal,env:BookingEnv){const payload=Buffer.from(JSON.stringify(p)).toString('base64url');return `${payload}.${signature(env,payload)}`;}
function read(token:string,env:BookingEnv):Proposal{const [body,sig,extra]=token.split('.'),expected=signature(env,body??'');if(extra||!body||!sig||sig.length!==expected.length||!timingSafeEqual(Buffer.from(sig),Buffer.from(expected)))throw new ApiError(422,'PMS_PROPOSAL_INVALID','Review a new proposal');return JSON.parse(Buffer.from(body,'base64url').toString());}
function eligible(s:Snapshot,id:string,now:number,change:boolean,override=false){
 const stay=record(s,'stay',id),reservation=record(s,'reservation',stay.data.reservationId),policy=stay.data.bookingPolicy??reservation.data.bookingPolicy;
 if(!override&&policy&&now>=policy.deadline)throw new ApiError(409,'PMS_CONTACT_HOTEL','The free-cancellation deadline has passed. Contact the hotel');
 if(s.property.state!=='open'||stay.data.state!=='reserved'||reservation.data.state!=='confirmed'||!policy||!override&&(now>=policy.deadline||change&&policy.collection?.changesAllowed===false||!change&&policy.collection?.cancellationsAllowed===false))throw new ApiError(409,'PMS_CONTACT_HOTEL','This room can no longer be changed online. Contact the hotel');
 if(records(s,'entry').some(e=>e.data.folioId===stay.data.folioId&&(['charge','adjustment'].includes(e.data.kind)||e.data.kind==='allocation'&&e.data.description!=='payment.refund_liability')))throw new ApiError(409,'PMS_CONTACT_HOTEL','This room has financial activity. Contact the hotel');
 return stay;
}
async function bookingAuthority(tx:SqlDatabase,token:string,now:number,lock:boolean,staff?:StaffAuthority){
 if(!staff)return authorizedBooking(tx,token,now,lock);
 requirePermission(staff.actor,'pms.reservations');
 if(staff.override)requirePermission(staff.actor,'pms.approve');
 if(staff.actor.beta||staff.actor.departmentId||staff.reason.trim().length<3)throw new ApiError(422,'PMS_REVISION_REASON','Use an authorized hotel workspace and enter a change reason');
 const row=await tx.prepare("SELECT property_id FROM pms_records WHERE id=? AND kind='reservation'").bind(staff.reservationId).first<{property_id:string}>();
 if(!row)throw new ApiError(404,'PMS_BOOKING_NOT_FOUND','Booking not found');
 const s=await bookingSnapshot(tx,row.property_id,undefined,lock);
 if(s.property.companyId!==staff.actor.companyId||staff.actor.branchId&&s.property.branchId!==staff.actor.branchId)throw new ApiError(404,'PMS_BOOKING_NOT_FOUND','Booking not found in this workspace');
 await assertProductionModuleEnabled(tx,staff.actor.companyId,'pms');
 return {s,id:staff.reservationId};
}
export async function publicChangeQuote(db:TransactionalSqlDatabase,token:string,kind:'change'|'cancel',raw:unknown,env:BookingEnv,now=Date.now(),staff?:StaffAuthority){
 const input=z.object({items:z.array(z.object({stayId:z.string().min(1),room:roomSelectionSchema.optional()})).min(1).max(20)}).parse(raw);
 if(new Set(input.items.map(i=>i.stayId)).size!==input.items.length)throw new ApiError(422,'PMS_DUPLICATE_ROOM','Choose each room once');
 return db.transaction(async tx=>{
  const {s,id}=await bookingAuthority(tx,token,now,false,staff);s.asOf=now;
  const trial=structuredClone(s);
  for(const item of input.items){const st=eligible(s,item.stayId,now,kind==='change',staff?.override);if(st.data.reservationId!==id)throw new ApiError(404,'PMS_STAY_NOT_FOUND','Room does not belong to this booking');record(trial,'stay',st.id).data.state='cancelled';}
  const items:Proposal['items']=[];
  for(const item of input.items){
   const st=record(s,'stay',item.stayId),oldTotal=st.data.prices.reduce((n,p)=>n+p.total,0);
   const sourceRows=(await tx.prepare('SELECT a.*,COALESCE((SELECT SUM((part->>\'amount\')::bigint) FROM pms_booking_refund_requests r CROSS JOIN LATERAL jsonb_array_elements(r.allocations_json::jsonb) part WHERE r.payment_id=a.payment_id AND part->>\'entryId\'=a.entry_id),0) refunded FROM pms_booking_allocations a WHERE a.stay_id=?').bind(st.id).all<{amount_minor:number;refunded:number}>()).results;
   const retained=sourceRows.reduce((n,p)=>n+Number(p.amount_minor)-Number(p.refunded),0);
   let quote:BookingQuote|undefined;
   if(kind==='change'){
    if(!item.room||item.room.quantity!==1||item.room.arrival<new Intl.DateTimeFormat('en-CA',{timeZone:s.property.config.timezone}).format(now)||!s.property.config.booking?.rateIds.includes(item.room.rateId))throw new ApiError(422,'PMS_CHANGE_INPUT','Choose one available public room with current dates');
    quote=bookingQuote(trial,[item.room]);
    // Add each proposed room to the trial so multi-room proposals cannot oversell.
    Object.assign(record(trial,'stay',st.id).data,{...item.room,state:'reserved',prices:quote.lines[0].nights});
   }
   const accepted=st.data.bookingPolicy??record(s,'reservation',id).data.bookingPolicy;
   const fee=kind==='cancel'?Number((BigInt(oldTotal)*BigInt(accepted?.collection?.cancellationFeePercent??0)+BigInt(99))/BigInt(100)):0;
   if(fee&&!s.property.config.bookingFees?.approvedBy)throw new ApiError(409,'PMS_FEE_ACCOUNT_REQUIRED','Contact the hotel to complete cancellation accounting setup');
   const target=quote?.dueNow??fee,newTotal=quote?.total??fee;
   items.push({stayId:st.id,room:kind==='change'?item.room:undefined,quote,oldTotal,newTotal,fee,dueAtHotel:kind==='cancel'?Math.max(0,fee-retained):quote!.dueAtHotel,dueNow:kind==='cancel'?0:Math.max(0,target-retained),refund:Math.max(0,retained-target)});
  }
  const proposal:Proposal={...(staff?{staff:{userId:staff.actor.userId,reason:staff.reason.trim(),override:staff.override}}:{}),reservationId:id,version:record(s,'reservation',id).version,kind,items,expiresAt:now+15*60*1000,dueNow:items.reduce((n,p)=>n+p.dueNow,0),refund:items.reduce((n,p)=>n+p.refund,0),fee:items.reduce((n,p)=>n+p.fee,0)};
  return {proposal,token:signed(proposal,env)};
 });
}
function releaseHolds(s:Snapshot,id:string,now:number){for(const b of records(s,'block').filter(b=>b.data.bookingChangeId===id&&b.data.state==='active')){b.data.state='released';b.version++;b.updatedAt=now;}}
async function apply(tx:SqlDatabase,s:Snapshot,proposal:Proposal,slug:string,env:BookingEnv,now:number,acceptedAt=now,original=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]))){
 for(const item of proposal.items)eligible(s,item.stayId,acceptedAt,proposal.kind==='change',proposal.staff?.override);
 // Free every selected original before applying swaps, then persist only the final state.
 if(proposal.kind==='change')for(const item of proposal.items)record(s,'stay',item.stayId).data.state='cancelled';
 for(const item of proposal.items){
  if(proposal.kind==='change')record(s,'stay',item.stayId).data.state='reserved';
  if(item.fee){const before=original,result=executePms(s,'booking.cancellation_fee',{stayId:item.stayId,amount:item.fee,reference:proposal.version.toString()},actor(now));await persistPmsRecords(tx,s,before);await effects(tx,{companyId:s.property.companyId,userId:proposal.staff?.userId??'guest:self-service'} as WorkContext,s,result.effects);}
  if(proposal.kind==='cancel')executePms(s,'stay.cancel',{id:item.stayId,reason:proposal.staff?.reason??'Guest cancelled online within accepted terms',fee:0},actor(now));
  else executePms(s,'stay.replace',{id:item.stayId,room:item.room,policy:item.quote!.lines[0].policy,nightPrices:item.quote!.lines[0].nights,reason:proposal.staff?.reason??'Guest changed booking online'},actor(now));
 }
 for(const item of proposal.items){
  let refund=item.refund;
  if(refund){
   const sources=(await tx.prepare('SELECT a.*,COALESCE((SELECT SUM((part->>\'amount\')::bigint) FROM pms_booking_refund_requests r CROSS JOIN LATERAL jsonb_array_elements(r.allocations_json::jsonb) part WHERE r.payment_id=a.payment_id AND part->>\'entryId\'=a.entry_id),0) refunded FROM pms_booking_allocations a WHERE a.stay_id=? ORDER BY a.created_at,a.id').bind(item.stayId).all<{entry_id:string;payment_id:string;amount_minor:number;refunded:number}>()).results;
   for(const source of sources){const n=Math.min(refund,Number(source.amount_minor)-Number(source.refunded),availableReceiptSources(s,record(s,'stay',item.stayId).data.folioId,'advance').find(p=>p.entryId===source.entry_id)?.amount??0);if(n>0){await requestBookingRefund(tx,s,source.payment_id,[{stayId:item.stayId,amount:n}],proposal.staff?.reason??'Guest booking revision',proposal.staff?.userId??'guest:self-service',now);refund-=n;}}
   if(refund)throw new ApiError(409,'PMS_REFUND_FUNDING_CHANGED','Original refund sources changed');
  }
 }
 const reservation=record(s,'reservation',proposal.reservationId);
 reservation.data.history=[...(reservation.data.history??[]),{at:now,actor:proposal.staff?.userId??'guest:self-service',action:`${proposal.staff?proposal.staff.reason+'; ':''}${proposal.staff?.override?'Policy override; ':''}${proposal.kind==='cancel'?'Cancelled':'Changed'} ${proposal.items.length} room(s); additional payment ${proposal.dueNow}; refund requested ${proposal.refund}`}];reservation.version++;reservation.updatedAt=now;
 await queueMail(tx,s,reservation.id,slug,proposal.kind==='cancel'?'cancellation':'change',env,now);
}
export async function confirmBookingChange(db:TransactionalSqlDatabase,slug:string,token:string,proposalToken:string,key:string,env:BookingEnv,now=Date.now(),staff?:StaffAuthority,expectedKind?:'change'|'cancel'){
 z.string().uuid().parse(key);const proposal=read(proposalToken,env),digest=createHash('sha256').update(proposalToken).digest('hex');
 if(expectedKind&&proposal.kind!==expectedKind)throw new ApiError(422,'PMS_PROPOSAL_KIND','Review the selected booking action');
 if((proposal.staff?.userId??null)!==(staff?.actor.userId??null)||proposal.staff&&(!staff||proposal.staff.reason!==staff.reason.trim()||proposal.staff.override!==staff.override))throw new ApiError(403,'PMS_PROPOSAL_AUTHORITY','Review the proposal using the same authorized person');
 const result=await db.transaction(async tx=>{
  const {s,id}=await bookingAuthority(tx,token,now,true,staff);s.asOf=now;
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  if(id!==proposal.reservationId)throw new ApiError(404,'PMS_PROPOSAL_INVALID','Proposal belongs to another booking');
  const prior=await tx.prepare('SELECT request_hash,state FROM pms_booking_changes WHERE id=? AND reservation_id=?').bind(key,id).first<{request_hash:string;state:string}>();
  if(prior){if(prior.request_hash!==digest)throw new ApiError(409,'PMS_KEY_REUSED','Use a new request key');const p=await tx.prepare('SELECT id FROM pms_booking_payments WHERE proposal_id=?').bind(key).first<{id:string}>();return {summary:bookingSummary(s,id,now),attemptId:prior.state==='payment_pending'?p?.id:undefined};}
  if(proposal.expiresAt<=now||record(s,'reservation',id).version!==proposal.version)throw new ApiError(409,'PMS_PROPOSAL_STALE','Booking or quote changed. Review a new proposal');
  if(await tx.prepare("SELECT id FROM pms_booking_changes WHERE reservation_id=? AND state='payment_pending' AND expires_at>? LIMIT 1").bind(id,now).first())throw new ApiError(409,'PMS_CHANGE_PENDING','Complete the pending change first');
  await run(tx,"UPDATE pms_booking_changes SET state='expired' WHERE reservation_id=? AND state='payment_pending' AND expires_at<=?",id,now);
  for(const item of proposal.items)eligible(s,item.stayId,now,proposal.kind==='change',proposal.staff?.override);
  // Revalidate availability and price under the same Property lock as staff bookings.
  const trial=structuredClone(s);for(const item of proposal.items)record(trial,'stay',item.stayId).data.state='cancelled';
  for(const item of proposal.items)if(item.room&&proposal.kind==='change'){const q=bookingQuote(trial,[item.room]);if(JSON.stringify(q)!==JSON.stringify(item.quote))throw new ApiError(409,'PMS_QUOTE_CHANGED','Price or terms changed. Review again');Object.assign(record(trial,'stay',item.stayId).data,{...item.room,state:'reserved',prices:q.lines[0].nights});}
  const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
  const expiresAt=now+30*60*1000;
  await run(tx,'INSERT INTO pms_booking_changes(id,property_id,reservation_id,request_hash,reservation_version,proposal_json,state,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)',key,s.property.id,id,digest,proposal.version,JSON.stringify(proposal),proposal.dueNow?'payment_pending':'applied',expiresAt,now);
  if(!proposal.dueNow){await apply(tx,s,proposal,slug,env,now,now,before);await persist(tx,s,before);return {summary:bookingSummary(s,id,now)};}
  const connection=await tx.prepare("SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true").bind(s.property.companyId).first<{account_id:string;livemode:boolean;card_enabled:boolean;promptpay_enabled:boolean}>();
  if(!connection||!connection.card_enabled&&!connection.promptpay_enabled||connection.livemode&&process.env.PMS_STRIPE_LIVE_ENABLED!=='true'||!s.property.config.onlineAccounting?.approvedBy)throw new ApiError(409,'PMS_PAYMENT_UNAVAILABLE','Online payment is unavailable');
  const delta=new Map<string,number>();
  for(const item of proposal.items){const old=record(s,'stay',item.stayId).data;for(const d of dates(old.arrival,old.departure)){const k=`${old.typeId}|${d}`;delta.set(k,(delta.get(k)??0)-1);}if(item.room)for(const d of dates(item.room.arrival,item.room.departure)){const k=`${item.room.typeId}|${d}`;delta.set(k,(delta.get(k)??0)+1);}}
  for(const [k,n] of delta)if(n>0){const [typeId,date]=k.split('|');if(availability(s,typeId,date).available<n)throw new ApiError(409,'PMS_SOLD_OUT','The additional room capacity is unavailable');s.records.push({id:randomUUID(),kind:'block',version:1,updatedAt:now,data:{name:'Pending guest change',typeId,roomId:'',from:date,to:plusDays(date,1),quantity:n,releaseDate:plusDays(s.property.businessDate,366),kind:'group',state:'active',pickedStayIds:[],bookingChangeId:key,expiresAt}});}
  const attemptId=randomUUID(),allocations=proposal.items.filter(i=>i.dueNow>0).map(i=>({stayId:i.stayId,folioId:record(s,'stay',i.stayId).data.folioId,amount:i.dueNow}));
  const settings=await tx.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(s.property.companyId).first<{accounts_json:string}>();
  await persist(tx,s,before);
  await run(tx,"INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,status,expires_at,created_at,updated_at,allocation_json,methods_json,clearing_account,purpose,proposal_id) VALUES(?,?,?,?,?,?,?,?,'THB','payment_pending',?,?,?,?,?,?,'change',?)",attemptId,s.property.id,id,s.property.companyId,slug,connection.account_id,connection.livemode,proposal.dueNow,expiresAt,now,now,JSON.stringify(allocations),JSON.stringify([...(connection.card_enabled?['card']:[]),...(connection.promptpay_enabled?['promptpay']:[])]),settings?(JSON.parse(settings.accounts_json).stripe||s.property.config.accounts.edc):s.property.config.accounts.edc,key);
  await (await snapshotMoney(tx,{companyId:s.property.companyId},{sourceType:'pms_booking_payment',sourceId:attemptId,amountKind:'requested',transactionDate:s.property.businessDate,currency:'THB',amountUnits:String(proposal.dueNow)})).statement.run();
  return {summary:bookingSummary(s,id,now),attemptId};
 });
 return result.attemptId?bookingCheckout(db,slug,result.attemptId,env,now):result.summary;
}
export async function settleBookingChange(tx:SqlDatabase,s:Snapshot,payment:BookingPaymentRow,result:{paid:boolean;providerSucceededAt:number|null;now:number;env:BookingEnv}){
 const {paid,providerSucceededAt,now,env}=result;s.asOf=now;
 const change=await tx.prepare('SELECT * FROM pms_booking_changes WHERE id=? FOR UPDATE').bind(payment.proposal_id).first<{id:string;state:string;proposal_json:string;reservation_version:number}>();
 if(!change)throw new ApiError(409,'PMS_CHANGE_MISSING','Change proposal is missing');
 const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));releaseHolds(s,change.id,now);
 const proposal=JSON.parse(change.proposal_json) as Proposal;
 let valid=paid&&(providerSucceededAt??now)<Number(payment.expires_at)&&record(s,'reservation',payment.reservation_id).version===Number(change.reservation_version);
 if(valid){const trial=structuredClone(s);for(const item of proposal.items)record(trial,'stay',item.stayId).data.state='cancelled';try{for(const item of proposal.items){eligible(s,item.stayId,providerSucceededAt??now,true,proposal.staff?.override);Object.assign(record(trial,'stay',item.stayId).data,{...item.room,state:'reserved',prices:item.quote!.lines[0].nights});}valid=proposal.items.every(item=>item.quote!.lines[0].nights.every(n=>availability(trial,item.room!.typeId,n.date).available>=0));}catch{valid=false;}}
 const oldEdc=s.property.config.accounts.edc;s.property.config.accounts.edc=payment.clearing_account;
 if(valid){
  let offset=0;
  for(const part of JSON.parse(payment.allocation_json) as Array<{stayId:string;folioId:string;amount:number}>){const outcome=executePms(s,'payment.stripe',{folioId:part.folioId,amount:part.amount,reference:payment.payment_intent_id??payment.checkout_id},providerActor(now));record(s,'entry',outcome.id).data.sourceKey=`online-payment:${payment.id}`;record(s,'entry',outcome.id).data.providerPaymentOffset=offset;offset+=part.amount;await persistPmsRecords(tx,s,before);await effects(tx,{companyId:payment.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);await run(tx,'INSERT INTO pms_booking_allocations(id,payment_id,stay_id,folio_id,entry_id,amount_minor,created_at) VALUES(?,?,?,?,?,?,?)',randomUUID(),payment.id,part.stayId,part.folioId,outcome.id,part.amount,now);}
  await apply(tx,s,proposal,payment.holding_slug,env,now,providerSucceededAt??now,before);
 } else if(paid){const st=record(s,'stay',proposal.items[0].stayId);const outcome=executePms(s,'payment.unallocated',{folioId:st.data.folioId,amount:Number(payment.amount_minor),reference:payment.checkout_id},providerActor(now));record(s,'entry',outcome.id).data.sourceKey=`online-payment:${payment.id}`;await persistPmsRecords(tx,s,before);await effects(tx,{companyId:payment.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);await run(tx,"INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,'[]','Change payment could not be assigned','system:stripe','requested',?,?)",randomUUID(),payment.id,payment.amount_minor,now,now);}
 s.property.config.accounts.edc=oldEdc;await persist(tx,s,before);
 await run(tx,'UPDATE pms_booking_changes SET state=? WHERE id=?',valid?'applied':'expired',change.id);
 await run(tx,'UPDATE pms_booking_payments SET status=?,refund_state=?,updated_at=? WHERE id=?',valid?'confirmed':paid?'refund_review':'expired',paid&&!valid?'requested':null,now,payment.id);
}
