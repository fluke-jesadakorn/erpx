import 'server-only';
import { createHmac, timingSafeEqual } from 'node:crypto';
import { z } from 'zod';
import { businessDate, type PosAccounts } from '@/lib/pos';
import { resourceSlots, registeredResources } from '@/lib/holding-registry';
import { runtimeEnv, holdingResources } from './runtime';
import { PostgresDatabase } from './runtime/postgres';
import type { SqlDatabase } from './runtime/types';
import type { WorkContext } from './authorization';
import { loadIamAuthority } from './iam-store';
import { hasPermission } from '@/lib/permissions';
import { id, one, run, fail, str, num, json, posting, pair, audit, enqueue, receivePayment, completeSale, completeRefundPayment, releaseUnpaidReservations, type PosRow } from './pos-core';
const objectSchema=z.object({id:z.string()}).passthrough();
type StripeObject=z.infer<typeof objectSchema>;
const hasCompanyStripePermission=(actor:WorkContext)=>hasPermission(actor,'pos.payments.configure')||hasPermission(actor,'finance.approve');
function secret(live:boolean){const key=process.env[live?'POS_STRIPE_LIVE_SECRET_KEY':'POS_STRIPE_TEST_SECRET_KEY'];if(!key?.startsWith(live?'sk_live_':'sk_test_'))fail('POS_STRIPE_NOT_CONFIGURED','Platform has not configured Stripe POS');return key;}
function origin(){const value=process.env.ERPX_PUBLIC_ORIGIN;if(!value)fail('POS_ORIGIN_REQUIRED','Configure the public origin before connecting Stripe');const u=new URL(value);if(u.protocol!=='https:'&&u.hostname!=='localhost')fail('POS_HTTPS_REQUIRED','Stripe requires HTTPS');return u.origin;}
type StripeOptions={account?:string;method?:'POST'|'GET';body?:Record<string,string>;key?:string};
async function stripeRequest(live:boolean,path:string,options:StripeOptions={}):Promise<unknown>{
 if(!/^[a-z0-9_/?=&.%+-]+$/i.test(path))throw new Error('STRIPE_PATH_INVALID');
 const response=await fetch(`https://api.stripe.com/v1/${path}`,{method:options.method??'GET',headers:{Authorization:`Bearer ${secret(live)}`,'Stripe-Version':'2025-02-24.acacia',...(options.account?{'Stripe-Account':options.account}:{}),...(options.key?{'Idempotency-Key':options.key}:{}),...(options.body?{'Content-Type':'application/x-www-form-urlencoded'}:{})},body:options.body?new URLSearchParams(options.body):undefined,signal:AbortSignal.timeout(20000),cache:'no-store'});
 if(!response.ok)throw new Error(`STRIPE_HTTP_${response.status}`);
 return response.json();
}
export async function stripeCall(live:boolean,path:string,options:StripeOptions={}):Promise<StripeObject>{return objectSchema.parse(await stripeRequest(live,path,options));}
export function verifyPosStripeSignature(raw:string,signature:string,signingSecrets:string[],now=Date.now()){
 const parts=signature.split(','),timestamp=parts.find(p=>p.startsWith('t='))?.slice(2),values=parts.filter(p=>p.startsWith('v1=')).map(p=>p.slice(3));
 if(!timestamp||!/^[0-9]+$/.test(timestamp)||Math.abs(now/1000-Number(timestamp))>300)return false;
 return signingSecrets.some(s=>{const expected=createHmac('sha256',s).update(`${timestamp}.${raw}`).digest();return values.some(value=>/^[a-f0-9]{64}$/.test(value)&&timingSafeEqual(expected,Buffer.from(value,'hex')));});
}
export async function posStripeWebhook(raw:string,signature:string){
 const secrets=[process.env.POS_STRIPE_TEST_WEBHOOK_SECRET,process.env.POS_STRIPE_LIVE_WEBHOOK_SECRET].filter((s):s is string=>Boolean(s));
 if(!verifyPosStripeSignature(raw,signature,secrets))fail('POS_WEBHOOK_SIGNATURE','Invalid signature',400);
 const event=z.object({id:z.string(),type:z.string(),account:z.string(),livemode:z.boolean(),data:z.object({object:z.record(z.string(),z.unknown())})}).parse(JSON.parse(raw));
 const modeSecret=process.env[event.livemode?'POS_STRIPE_LIVE_WEBHOOK_SECRET':'POS_STRIPE_TEST_WEBHOOK_SECRET'];
 if(!modeSecret||!verifyPosStripeSignature(raw,signature,[modeSecret]))fail('POS_WEBHOOK_MODE','Webhook signing mode mismatch',400);
 const env=await runtimeEnv(),route=await one(env.CONTROL_DB,'SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?',event.account,event.livemode);
 if(!route)return {received:true,ignored:true};
 const db=registeredResources(env,str(route,'holding_id')).write as PostgresDatabase;
 await db.transaction(async tx=>{
  await run(tx,'INSERT INTO pos_stripe_events(id,account_id,livemode,company_id,event_type,payload_json,received_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING',event.id,event.account,event.livemode,route.company_id,event.type,JSON.stringify(event.data.object),Date.now());
  await enqueue(tx,str(route,'company_id'),'event',event.id);
 });return {received:true};
}
export function oauthUrl(state:string,holdingSlug:string,live:boolean){
 if(process.env.NODE_ENV==='production'&&process.env.POS_STRIPE_LIVE_ENABLED!=='true')fail('POS_PROVIDER_COMING_SOON','Enable the Stripe production gate before connecting live accounts',403);
 secret(live);const client=process.env[live?'POS_STRIPE_LIVE_CLIENT_ID':'POS_STRIPE_TEST_CLIENT_ID'];if(!client)fail('POS_STRIPE_OAUTH_NOT_CONFIGURED','Platform has not configured Connect OAuth');
 const params=new URLSearchParams({response_type:'code',client_id:client,scope:'read_write',state,redirect_uri:`${origin()}/api/v1/holdings/${holdingSlug}/pos/connect/callback`});return `https://connect.stripe.com/oauth/authorize?${params}`;
}
async function bindAccount(db:PostgresDatabase,holdingId:string,companyId:string,accountId:string,live:boolean){
 const account=await stripeCall(live,`accounts/${accountId}`),env=await runtimeEnv();
 if(account.country!=='TH')fail('POS_STRIPE_COUNTRY','This version supports Stripe accounts in Thailand');
 const banks=z.object({data:z.array(z.record(z.string(),z.unknown()))}).parse(await stripeRequest(live,`accounts/${accountId}/external_accounts?object=bank_account&limit=20`));
 const thbBankReady=Array.isArray(banks.data)&&banks.data.some((bank:unknown)=>{const b=bank as Record<string,unknown>;return b.currency==='thb'&&b.status!=='errored'&&b.status!=='verification_failed';});
 const payoutInterval=(((account.settings as Record<string,unknown>|undefined)?.payouts as Record<string,unknown>|undefined)?.schedule as Record<string,unknown>|undefined)?.interval;
 const existing=await one(env.CONTROL_DB,'SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?',accountId,live);
 if(existing&&(existing.holding_id!==holdingId||existing.company_id!==companyId))fail('POS_STRIPE_ACCOUNT_BOUND','This Stripe account is already connected to another company');
 await run(env.CONTROL_DB,'INSERT INTO pos_stripe_routes VALUES(?,?,?,?,?) ON CONFLICT(account_id,livemode) DO NOTHING',accountId,live,holdingId,companyId,Date.now());
 const route=await one(env.CONTROL_DB,'SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?',accountId,live);if(route?.holding_id!==holdingId||route.company_id!==companyId)fail('POS_STRIPE_ACCOUNT_BOUND','This Stripe account is already connected to another company');
 await db.transaction(async tx=>{
  await run(tx,'INSERT INTO pos_connection_history VALUES(?,?,?) ON CONFLICT DO NOTHING',companyId,accountId,live);
  const capabilities=(account.capabilities??{}) as Record<string,unknown>,requirements=(account.requirements??{}) as Record<string,unknown>;
  await run(tx,'INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,requirements_json,state,payout_interval,thb_bank_ready,last_checked_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET account_id=excluded.account_id,livemode=excluded.livemode,charges_enabled=excluded.charges_enabled,payouts_enabled=excluded.payouts_enabled,card_enabled=excluded.card_enabled,promptpay_enabled=excluded.promptpay_enabled,requirements_json=excluded.requirements_json,state=excluded.state,payout_interval=excluded.payout_interval,thb_bank_ready=excluded.thb_bank_ready,last_checked_at=excluded.last_checked_at,version=pos_connections.version+1',companyId,accountId,live,account.charges_enabled===true,account.payouts_enabled===true,capabilities.card_payments==='active',capabilities.promptpay_payments==='active',JSON.stringify({currently_due:requirements.currently_due??[],pending_verification:requirements.pending_verification??[]}),account.charges_enabled&&account.payouts_enabled&&thbBankReady&&payoutInterval==='daily'?'ready':'onboarding',typeof payoutInterval==='string'?payoutInterval:null,thbBankReady,Date.now());
 });
}
export async function finishPosOAuth(actor:WorkContext,stateId:string,code:string){
 if(process.env.NODE_ENV==='production'&&process.env.POS_STRIPE_LIVE_ENABLED!=='true')fail('POS_PROVIDER_COMING_SOON','Enable the Stripe production gate before connecting live accounts',403);
 if(!hasCompanyStripePermission(actor))fail('POS_CONNECTION_FORBIDDEN','Company Finance access is required',403);if(actor.branchId||actor.departmentId)fail('POS_COMPANY_SETTINGS','Select a company area',403);
 const db=(await holdingResources(actor.holdingId)).write as PostgresDatabase;
 // Claim once before the non-idempotent OAuth exchange. A failed exchange requires fresh authorization.
 const live=await db.transaction(async tx=>{const state=await one(tx,'SELECT * FROM pos_oauth_states WHERE id=? AND company_id=? AND user_id=? AND used_at IS NULL AND expires_at>? FOR UPDATE',stateId,actor.companyId,actor.userId,Date.now());if(!state)fail('POS_OAUTH_STATE','The confirmation link expired. Connect again.',400);await run(tx,'UPDATE pos_oauth_states SET used_at=? WHERE id=?',Date.now(),stateId);return Boolean(state.livemode);});
 const response=await fetch('https://connect.stripe.com/oauth/token',{method:'POST',headers:{Authorization:`Bearer ${secret(live)}`,'Content-Type':'application/x-www-form-urlencoded'},body:new URLSearchParams({grant_type:'authorization_code',code}),signal:AbortSignal.timeout(20000)});
 if(!response.ok)fail('POS_OAUTH_FAILED','Connection failed. Start again.');
 const value=z.object({stripe_user_id:z.string().regex(/^acct_/),livemode:z.boolean()}).parse(await response.json());if(value.livemode!==live)fail('POS_STRIPE_MODE','Account mode does not match');
 await bindAccount(db,actor.holdingId,actor.companyId,value.stripe_user_id,live);
 await audit(db,actor.companyId,null,actor.userId,'connection.connected',value.stripe_user_id,{livemode:live});
}
async function checkPayment(db:PostgresDatabase,p:PosRow,date?:string){
 if(!p.provider_id)throw new Error('POS_CHECKOUT_NOT_CREATED');
 if(p.method==='stripe_promptpay'&&String(p.provider_id).startsWith('pi_')){
  const pi=await stripeCall(Boolean(p.livemode),`payment_intents/${p.provider_id}`,{account:str(p,'stripe_account')});
  if(pi.status==='succeeded'){
   if(pi.amount_received!==num(p,'amount_minor')||pi.currency!=='thb'||(pi.metadata as Record<string,string>)?.pos_payment_id!==p.id)throw new Error('POS_PROVIDER_PAYMENT_MISMATCH');
   await run(db,'UPDATE pos_payments SET provider_received_at=COALESCE(provider_received_at,?) WHERE id=?',Date.now(),p.id);
   await db.transaction(async tx=>{const current=(await one(tx,'SELECT * FROM pos_payments WHERE id=? FOR UPDATE',p.id))!;await receivePayment(tx,current,'system:stripe',date);await enqueue(tx,str(p,'company_id'),'complete_sale',str(p,'sale_id'));});return false;
  }
  if(pi.status==='canceled'){
   await db.transaction(async tx=>{const current=(await one(tx,'SELECT * FROM pos_payments WHERE id=? FOR UPDATE',p.id))!;if(current.state==='succeeded')return;await run(tx,"UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?",Date.now(),p.id);await releaseUnpaidReservations(tx,str(p,'sale_id'));});return false;
  }
  if((p.expires_at&&Date.now()>=num(p,'expires_at'))||pi.status==='requires_payment_method'){
   await stripeCall(Boolean(p.livemode),`payment_intents/${p.provider_id}/cancel`,{method:'POST',account:str(p,'stripe_account'),key:`pos-cancel-${p.id}`,body:{}});
   return checkPayment(db,p,date);
  }
  return true;
 }
 const session=await stripeCall(Boolean(p.livemode),`checkout/sessions/${p.provider_id}`,{account:str(p,'stripe_account')});
 if(session.payment_status==='paid'){
  const pi=await stripeCall(Boolean(p.livemode),`payment_intents/${String(session.payment_intent)}`,{account:str(p,'stripe_account')});
  if(pi.status!=='succeeded'||pi.amount_received!==num(p,'amount_minor')||pi.currency!=='thb'||(pi.metadata as Record<string,string>)?.pos_payment_id!==p.id)throw new Error('POS_PROVIDER_PAYMENT_MISMATCH');
  await run(db,'UPDATE pos_payments SET provider_received_at=COALESCE(provider_received_at,?) WHERE id=?',Date.now(),p.id);
  await db.transaction(async tx=>{const current=(await one(tx,'SELECT * FROM pos_payments WHERE id=? FOR UPDATE',p.id))!;await receivePayment(tx,current,'system:stripe',date);await enqueue(tx,str(p,'company_id'),'complete_sale',str(p,'sale_id'));});return false;
 }
 if(session.status==='expired'){
  await db.transaction(async tx=>{
   const current=(await one(tx,'SELECT * FROM pos_payments WHERE id=? FOR UPDATE',p.id))!;if(current.state==='succeeded')return;
   await run(tx,"UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?",Date.now(),p.id);
   await releaseUnpaidReservations(tx,str(p,'sale_id'));
  });return false;
 }
 return true;
}
async function checkRefund(db:PostgresDatabase,rp:PosRow,p:PosRow,date?:string){
 const refund=await stripeCall(Boolean(p.livemode),`refunds/${rp.provider_id}`,{account:str(p,'stripe_account')});
 if(refund.amount!==num(rp,'amount_minor')||(refund.metadata as Record<string,string>)?.pos_refund_payment_id!==rp.id)throw new Error('POS_PROVIDER_REFUND_MISMATCH');
 if(refund.status==='succeeded'){await db.transaction(tx=>completeRefundPayment(tx,str(rp,'id'),'system:stripe',date));return false;}
 if(refund.status==='failed'||refund.status==='canceled'){await run(db,"UPDATE pos_refund_payments SET state='failed' WHERE id=?",rp.id);throw new Error('POS_PROVIDER_REFUND_FAILED');}
 return true;
}
async function stripeEvent(db:PostgresDatabase,job:PosRow,holdingId:string){
 const event=(await one(db,'SELECT * FROM pos_stripe_events WHERE id=?',job.record_id))!;if(event.applied_at)return;
 const obj=json<Record<string,unknown>>(event.payload_json),type=str(event,'event_type');
 if(type==='account.updated'){
  const current=await one(db,'SELECT * FROM pos_connections WHERE company_id=?',job.company_id);if(current?.account_id===event.account_id&&current.livemode===event.livemode)await bindAccount(db,holdingId,str(job,'company_id'),str(event,'account_id'),Boolean(event.livemode));
 }else if(type==='account.application.deauthorized')await run(db,"UPDATE pos_connections SET state='disconnected',charges_enabled=false,payouts_enabled=false,version=version+1 WHERE company_id=? AND account_id=? AND livemode=?",job.company_id,event.account_id,event.livemode);
 else if(type.startsWith('account.external_account.'))await bindAccount(db,holdingId,str(job,'company_id'),str(event,'account_id'),Boolean(event.livemode));
 else if(type.startsWith('checkout.session.')||type.startsWith('payment_intent.')){
  const counterId=(obj.metadata as Record<string,string>|undefined)?.pms_counter_payment_id;
  if(counterId){
   const payment=await one(db,'SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?',counterId,job.company_id,event.account_id,event.livemode);
   if(!payment)throw new Error('PMS_PROVIDER_PAYMENT_MISMATCH');
   await (await import('./pms-stripe')).reconcilePmsStripePayment(db,counterId);
  }
  const bookingId=(obj.metadata as Record<string,string>|undefined)?.pms_booking_payment_id;
  if(bookingId){
   const payment=await one(db,'SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?',bookingId,job.company_id,event.account_id,event.livemode);
   if(!payment)throw new Error('PMS_PROVIDER_BOOKING_MISMATCH');
   await (await import('./pms-booking')).reconcileBookingPayment(db,bookingId,await (await import('./pms-booking')).bookingEnv());
  }
  const pid=(obj.metadata as Record<string,string>|undefined)?.pos_payment_id;
  const p=pid?await one(db,'SELECT * FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?',pid,job.company_id,event.account_id,event.livemode):null;
  if(p?.provider_id)await checkPayment(db,p,json<{accountingDate?:string}>(job.payload_json).accountingDate);
 }else if(type.startsWith('refund.')){
  const counterId=(obj.metadata as Record<string,string>|undefined)?.pms_counter_payment_id;
  if(counterId){
   const payment=await one(db,'SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?',counterId,job.company_id,event.account_id,event.livemode);
   if(!payment)throw new Error('PMS_PROVIDER_REFUND_MISMATCH');
   await (await import('./pms-stripe')).processPmsStripeRefund(db,counterId);
   await run(db,'UPDATE pos_stripe_events SET applied_at=? WHERE id=?',Date.now(),event.id);
   return;
  }
  const bookingId=(obj.metadata as Record<string,string>|undefined)?.pms_booking_payment_id;
  if(bookingId){
   const payment=await one(db,'SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?',bookingId,job.company_id,event.account_id,event.livemode);
   if(!payment)throw new Error('PMS_PROVIDER_REFUND_MISMATCH');
   await (await import('./pms-booking')).processBookingRefund(db,bookingId);
   await run(db,'UPDATE pos_stripe_events SET applied_at=? WHERE id=?',Date.now(),event.id);
   return;
  }
  const rpid=(obj.metadata as Record<string,string>|undefined)?.pos_refund_payment_id;
  const rp=rpid?await one(db,'SELECT rp.* FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?',rpid,job.company_id,event.account_id,event.livemode):null;
  if(!rp)throw new Error('POS_EXTERNAL_REFUND_REQUIRES_REVIEW');
  if(rp.provider_id)await checkRefund(db,rp,(await one(db,'SELECT * FROM pos_payments WHERE id=?',rp.payment_id))!,json<{accountingDate?:string}>(job.payload_json).accountingDate);
 }else if(type.startsWith('payout.')||type.startsWith('charge.dispute.')){
  await settlement(db,job,event,obj);
 }else if(type==='charge.succeeded'){
  const charge=await stripeCall(Boolean(event.livemode),`charges/${String(obj.id)}`,{account:str(event,'account_id')});
  if(charge.balance_transaction)await balanceTransaction(db,event,String(charge.balance_transaction),json<{accountingDate?:string}>(job.payload_json).accountingDate);
 }
 await run(db,'UPDATE pos_stripe_events SET applied_at=? WHERE id=?',Date.now(),event.id);
}
async function balanceTransaction(db:PostgresDatabase,event:PosRow,btId:string,date?:string){
 const bt=await stripeCall(Boolean(event.livemode),`balance_transactions/${btId}`,{account:str(event,'account_id')});
 if(bt.currency!=='thb')throw new Error('POS_SETTLEMENT_CURRENCY');
 const amount=z.number().int().parse(bt.amount),fee=z.number().int().parse(bt.fee);
 const source=typeof bt.source==='string'?bt.source:'';
 let owned=bt.type==='stripe_fee';
 if(source.startsWith('ch_')){
  const charge=await stripeCall(Boolean(event.livemode),`charges/${source}`,{account:str(event,'account_id')});
  const paymentId=(charge.metadata as Record<string,string>|undefined)?.pos_payment_id;
  const bookingId=(charge.metadata as Record<string,string>|undefined)?.pms_booking_payment_id;
  const counterId=(charge.metadata as Record<string,string>|undefined)?.pms_counter_payment_id;
  owned=Boolean(paymentId&&await one(db,'SELECT id FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?',paymentId,event.company_id,event.account_id,event.livemode))||Boolean(bookingId&&await one(db,"SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=? AND status IN ('confirmed','cancelled')",bookingId,event.company_id,event.account_id,event.livemode))||Boolean(counterId&&await one(db,"SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'",counterId,event.company_id,event.account_id,event.livemode));
 }else if(source.startsWith('re_'))owned=Boolean(await one(db,'SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.provider_id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?',source,event.company_id,event.account_id,event.livemode))||Boolean(await one(db,"SELECT id FROM pms_booking_payments WHERE refund_id=? AND company_id=? AND account_id=? AND livemode=? AND status='cancelled'",source,event.company_id,event.account_id,event.livemode))||Boolean(await one(db,"SELECT id FROM pms_stripe_payments WHERE refund_id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'",source,event.company_id,event.account_id,event.livemode));
 else if(source.startsWith('dp_'))owned=true;
 if(!owned){
  await run(db,"INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,created_at) VALUES(?,?,?,?,'balance',?,?,?,'THB','unmapped',?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO NOTHING",id(),event.company_id,event.account_id,event.livemode,btId,amount,fee,JSON.stringify({source,net:bt.net,type:bt.type}),Date.now());return;
 }
 await db.transaction(async tx=>{
  const settings=await settlementSettings(tx,str(event,'company_id')),a=json<PosAccounts>(settings.accounts_json);
  const journal=await posting(tx,str(event,'company_id'),null,a,date??businessDate(),str(settings,'currency'),`stripe-fee:${event.livemode}:${event.account_id}:${btId}`,btId,'system:stripe',pair(fee>=0?a.fees:a.stripe,fee>=0?a.stripe:a.fees,Math.abs(fee)));
  await run(tx,"INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,journal_id,created_at) VALUES(?,?,?,?,'balance',?,?,?,'THB','recorded',?,?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO NOTHING",id(),event.company_id,event.account_id,event.livemode,btId,amount,fee,JSON.stringify({source:bt.source,net:bt.net,type:bt.type,created:bt.created}),journal,Date.now());
 });
}
async function settlementSettings(db:SqlDatabase,companyId:string){
 const pos=await one(db,'SELECT currency,accounts_json FROM pos_settings WHERE company_id=?',companyId);
 if(pos)return pos;
 const hotel=await one(db,"SELECT p.config_json FROM pms_properties p JOIN pms_booking_payments b ON b.property_id=p.id WHERE b.company_id=? ORDER BY b.created_at LIMIT 1",companyId)??await one(db,"SELECT p.config_json FROM pms_properties p JOIN pms_stripe_payments m ON m.property_id=p.id WHERE m.company_id=? ORDER BY m.created_at LIMIT 1",companyId);
 if(!hotel)throw new Error('STRIPE_COMPANY_ACCOUNTS_REQUIRED');
 const c=json<{currency:string;chartId:string;accounts:{edc:string;bank:string;commission:string}}>(hotel.config_json);
 return {currency:c.currency,accounts_json:JSON.stringify({chartId:c.chartId,stripe:c.accounts.edc,bank:c.accounts.bank,fees:c.accounts.commission,disputes:c.accounts.commission})};
}
async function settlement(db:PostgresDatabase,job:PosRow,event:PosRow,obj:Record<string,unknown>){
 const payout=str(event,'event_type').startsWith('payout.'),resource=await stripeCall(Boolean(event.livemode),`${payout?'payouts':'disputes'}/${String(obj.id)}`,{account:str(event,'account_id')});
 if(resource.currency!=='thb')throw new Error('POS_SETTLEMENT_CURRENCY');
 const amount=z.number().int().positive().parse(resource.amount),date=json<{accountingDate?:string}>(job.payload_json).accountingDate??businessDate();
 const pendingTransactions=(resource.balance_transactions??[]) as Array<{id:string}>;
 for(const bt of pendingTransactions)await balanceTransaction(db,event,bt.id,date);
 let details:unknown={status:resource.status};
 if(payout&&resource.status==='paid'){
  const items:unknown[]=[];let after='';
  for(let page=0;page<100;page++){
   const response=await fetch(`https://api.stripe.com/v1/balance_transactions?${new URLSearchParams({payout:resource.id,limit:'100',...(after?{starting_after:after}:{})})}`,{headers:{Authorization:`Bearer ${secret(Boolean(event.livemode))}`,'Stripe-Account':str(event,'account_id')},signal:AbortSignal.timeout(20000)});
   if(!response.ok)throw new Error('POS_PAYOUT_ITEMS_UNAVAILABLE');
   const list=z.object({data:z.array(objectSchema),has_more:z.boolean()}).parse(await response.json());
   for(const item of list.data){await balanceTransaction(db,event,item.id,date);items.push({id:item.id,amount:item.amount,fee:item.fee,net:item.net,source:item.source});}
   if(!list.has_more)break;if(!list.data.length||page===99)throw new Error('POS_PAYOUT_TOO_LARGE');after=list.data.at(-1)!.id;
  }
  details={items,status:resource.status};
  if(items.reduce<number>((sum,item)=>sum+Number((item as {net:number}).net),0)!==amount)throw new Error('POS_PAYOUT_NET_MISMATCH');
  for(const item of items){const entry=await one(db,"SELECT state FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?",event.account_id,event.livemode,(item as {id:string}).id);if(entry?.state==='unmapped')throw new Error('POS_EXTERNAL_BALANCE_REVIEW');}
 
 }
 await db.transaction(async tx=>{
  const settings=await settlementSettings(tx,str(event,'company_id')),a=json<PosAccounts>(settings.accounts_json);let journal:string|null=null;
  if(payout&&resource.status==='failed'&&await one(tx,'SELECT journal_id FROM pos_postings WHERE source_key=?',`payout:${event.livemode}:${event.account_id}:${resource.id}`))await posting(tx,str(event,'company_id'),null,a,date,'THB',`payout-return:${event.livemode}:${event.account_id}:${resource.id}`,resource.id,'system:stripe',pair(a.stripe,a.bank,amount));
  if(payout&&resource.status==='paid')journal=await posting(tx,str(event,'company_id'),null,a,date,'THB',`payout:${event.livemode}:${event.account_id}:${resource.id}`,resource.id,'system:stripe',pair(a.bank,a.stripe,amount));
  // Disputes are tracked separately from sales/refunds. Post actual balance movements only.
  if(!payout){
   for(const bt of pendingTransactions){
    const row=await one(tx,"SELECT * FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?",event.account_id,event.livemode,bt.id);if(row){const value=num(row,'amount_minor');journal=await posting(tx,str(event,'company_id'),null,a,date,'THB',`dispute-move:${event.account_id}:${bt.id}`,resource.id,'system:stripe',pair(value<0?a.disputes:a.stripe,value<0?a.stripe:a.disputes,Math.abs(value)));}
   }
   if(resource.status==='lost')journal=await posting(tx,str(event,'company_id'),null,a,date,'THB',`dispute-loss:${event.account_id}:${resource.id}`,resource.id,'system:stripe',pair(a.fees,a.disputes,amount));
  }
  await run(tx,'INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,currency,state,details_json,journal_id,created_at) VALUES(?,?,?,?,?,?,?,\'THB\',?,?,?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO UPDATE SET state=excluded.state,details_json=excluded.details_json,journal_id=COALESCE(excluded.journal_id,pos_settlements.journal_id)',id(),event.company_id,event.account_id,event.livemode,payout?'payout':'dispute',resource.id,amount,String(resource.status),JSON.stringify(details),journal,Date.now());
 });
}
async function processJob(db:PostgresDatabase,holdingId:string,job:PosRow):Promise<boolean>{
 const payload=json<{livemode?:boolean;holdingSlug?:string;userId?:string;assignmentId?:string;accountingDate?:string;providerAttemptStartedAt?:number}>(job.payload_json);
 if(job.kind==='connect'||job.kind==='connect_refresh'){
  const assignment=await one(db,"SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?",payload.assignmentId,payload.userId,job.company_id,Date.now(),Date.now());
  if(!assignment)throw new Error('POS_CONNECTION_AUTHORITY_CHANGED');
  const authority=await loadIamAuthority(db,str(job,'company_id'),String(payload.userId),str(assignment,'role'));
  if(!authority.permissions.includes('pos.payments.configure')&&!authority.permissions.includes('finance.approve'))throw new Error('POS_CONNECTION_AUTHORITY_CHANGED');
  let connection=await one(db,'SELECT * FROM pos_connections WHERE company_id=?',job.company_id),accountId=connection?.account_id;
  if(job.kind==='connect'&&(!accountId||connection?.livemode!==Boolean(payload.livemode))){
   if(Date.now()-num(job,'created_at')>23*60*60*1000)throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
   const account=await stripeCall(Boolean(payload.livemode),'accounts',{method:'POST',key:`pos-connect-${job.id}-v2`,body:{country:'TH','controller[fees][payer]':'account','controller[losses][payments]':'stripe','controller[requirement_collection]':'stripe','controller[stripe_dashboard][type]':'full','capabilities[card_payments][requested]':'true','capabilities[transfers][requested]':'true','capabilities[promptpay_payments][requested]':'true','metadata[erpx_company_id]':str(job,'company_id')}});accountId=account.id;
  }
  if(!accountId)throw new Error('POS_CONNECTION_MISSING');
  await bindAccount(db,holdingId,str(job,'company_id'),String(accountId),Boolean(payload.livemode));connection=await one(db,'SELECT * FROM pos_connections WHERE company_id=?',job.company_id);
  const url=`${origin()}/holdings/${payload.holdingSlug}/settings/stripe`;
  const link=z.object({url:z.string().url()}).parse(await stripeRequest(Boolean(connection!.livemode),'account_links',{method:'POST',key:`pos-link-${job.id}`,body:{account:String(accountId),type:'account_onboarding',return_url:url,refresh_url:url}}));
  if(typeof link.url!=='string'||new URL(link.url).hostname!=='connect.stripe.com')throw new Error('POS_CONNECT_URL_INVALID');
  await run(db,'UPDATE pos_jobs SET payload_json=? WHERE id=?',JSON.stringify({...payload,url:link.url}),job.id);return false;
 }
 if(job.kind==='checkout'){
  const p=(await one(db,'SELECT * FROM pos_payments WHERE id=? AND company_id=?',job.record_id,job.company_id))!;if(!p)throw new Error('POS_PAYMENT_MISSING');if(p.state==='succeeded'||p.state==='cancelled')return false;
  if(!p.provider_id){
   if(p.method==='stripe_promptpay'){
    const sale=(await one(db,'SELECT number FROM pos_sales WHERE id=?',p.sale_id))!;
    const pi=await stripeCall(Boolean(p.livemode),'payment_intents',{method:'POST',account:str(p,'stripe_account'),key:`pos-promptpay-${p.id}`,body:{amount:String(p.amount_minor),currency:'thb','payment_method_types[0]':'promptpay','payment_method_data[type]':'promptpay',confirm:'true',description:String(sale.number),'metadata[pos_payment_id]':str(p,'id'),...(p.payer_email?{receipt_email:String(p.payer_email)}:{})}});
    const qr=((pi.next_action as Record<string,unknown>|undefined)?.promptpay_display_qr_code as Record<string,unknown>|undefined)?.data;
    if(pi.status!=='requires_action'||typeof qr!=='string'||!qr)throw new Error('POS_PROMPTPAY_QR_UNAVAILABLE');
    await db.transaction(async tx=>{await run(tx,"UPDATE pos_payments SET provider_id=?,qr_data=?,expires_at=?,state='pending',version=version+1,updated_at=? WHERE id=? AND state IN ('requested','exception')",pi.id,qr,Date.now()+30*60*1000,Date.now(),p.id);await enqueue(tx,str(p,'company_id'),'check_payment',str(p,'id'));});
    return false;
   }
   const sale=(await one(db,'SELECT * FROM pos_sales WHERE id=?',p.sale_id))!,env=await runtimeEnv(),binding=await one(env.CONTROL_DB,'SELECT slug FROM holding_bindings WHERE holding_id=?',holdingId);
   const returnUrl=`${origin()}/holdings/${binding?.slug}/pos?sale=${sale.id}`;
   const session=await stripeCall(Boolean(p.livemode),'checkout/sessions',{method:'POST',account:str(p,'stripe_account'),key:`pos-checkout-${p.id}`,body:{mode:'payment','payment_method_types[0]':p.method==='stripe_promptpay'?'promptpay':'card','line_items[0][price_data][currency]':'thb','line_items[0][price_data][unit_amount]':String(p.amount_minor),'line_items[0][price_data][product_data][name]':String(sale.number),'line_items[0][quantity]':'1','metadata[pos_payment_id]':str(p,'id'),'payment_intent_data[metadata][pos_payment_id]':str(p,'id'),success_url:returnUrl,cancel_url:returnUrl,expires_at:String(Math.floor(Number(p.created_at)/1000)+1800)}});
   if(typeof session.url!=='string'||new URL(session.url).hostname!=='checkout.stripe.com')throw new Error('POS_CHECKOUT_URL_INVALID');
   await db.transaction(async tx=>{await run(tx,"UPDATE pos_payments SET provider_id=?,checkout_url=?,state='pending',version=version+1,updated_at=? WHERE id=? AND state IN ('requested','exception')",session.id,session.url,Date.now(),p.id);await enqueue(tx,str(p,'company_id'),'check_payment',str(p,'id'));});
  }
  return false;
 }
 if(job.kind==='check_payment'||job.kind==='cancel_checkout'){
  const p=(await one(db,'SELECT * FROM pos_payments WHERE id=? AND company_id=?',job.record_id,job.company_id))!;
  if(!p)throw new Error('POS_PAYMENT_MISSING');if(['succeeded','cancelled'].includes(str(p,'state')))return false;
  if(job.kind==='cancel_checkout'){
   if(!p.provider_id)return true;
   if(p.method==='stripe_promptpay'&&String(p.provider_id).startsWith('pi_')){
    const pi=await stripeCall(Boolean(p.livemode),`payment_intents/${p.provider_id}`,{account:str(p,'stripe_account')});
    if(pi.status!=='succeeded'&&pi.status!=='canceled')await stripeCall(Boolean(p.livemode),`payment_intents/${p.provider_id}/cancel`,{method:'POST',account:str(p,'stripe_account'),key:`pos-cancel-${p.id}`,body:{}});
    return checkPayment(db,p,payload.accountingDate);
   }
   const session=await stripeCall(Boolean(p.livemode),`checkout/sessions/${p.provider_id}`,{account:str(p,'stripe_account')});
   if(session.status==='open')await stripeCall(Boolean(p.livemode),`checkout/sessions/${p.provider_id}/expire`,{method:'POST',account:str(p,'stripe_account'),key:`pos-expire-${p.id}`,body:{}});
  }
  return checkPayment(db,p,payload.accountingDate);
 }
 if(job.kind==='complete_sale'){
  return db.transaction(async tx=>{const sale=await one(tx,'SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE',job.record_id,job.company_id);if(!sale||sale.state==='completed')return false;
   const paid=await one(tx,"SELECT COALESCE(SUM(amount_minor-refunded_minor),0) n FROM pos_payments WHERE sale_id=? AND state='succeeded'",sale.id);
   if(num(paid!,'n')!==num(sale,'total_minor'))return true;
   await completeSale(tx,sale,'system:stripe',payload.accountingDate);return false;
  });
 }
 if(job.kind==='refund'){
  const rp=(await one(db,'SELECT rp.* FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=? AND r.company_id=?',job.record_id,job.company_id))!;if(!rp)throw new Error('POS_REFUND_MISSING');if(rp.state==='succeeded')return false;
  const p=(await one(db,'SELECT * FROM pos_payments WHERE id=?',rp.payment_id))!;
  if(!rp.provider_id){
   if(Date.now()-(payload.providerAttemptStartedAt??num(job,'created_at'))>23*60*60*1000)throw new Error('POS_PROVIDER_LOOKUP_REQUIRED');
   const paymentIntent=String(p.provider_id).startsWith('pi_')?String(p.provider_id):String((await stripeCall(Boolean(p.livemode),`checkout/sessions/${p.provider_id}`,{account:str(p,'stripe_account')})).payment_intent);
   const refund=await stripeCall(Boolean(p.livemode),'refunds',{method:'POST',account:str(p,'stripe_account'),key:`pos-refund-${rp.id}-${rp.provider_attempt}`,body:{payment_intent:paymentIntent,amount:String(rp.amount_minor),'metadata[pos_refund_payment_id]':str(rp,'id'),...(p.method==='stripe_promptpay'&&p.payer_email?{instructions_email:String(p.payer_email)}:{})}});
   await run(db,"UPDATE pos_refund_payments SET provider_id=?,state='pending' WHERE id=?",refund.id,rp.id);rp.provider_id=refund.id;
  }
  const again=await checkRefund(db,rp,p,payload.accountingDate);
  if(!again)await db.transaction(tx=>releaseUnpaidReservations(tx,str(p,'sale_id')));
  return again;
 }
 if(job.kind==='event'){await stripeEvent(db,job,holdingId);return false;}
 throw new Error('POS_JOB_UNKNOWN');
}
/** Durable leases allow the jobs service to recover work after a process restart. */
export async function drainPos(db:PostgresDatabase,holdingId:string,limit=8){
 for(let i=0;i<limit;i++){
  const token=id(),now=Date.now();
  const job=await one(db,"UPDATE pos_jobs SET state='running',lease_token=?,lease_until=?,attempts=attempts+1 WHERE id=(SELECT id FROM pos_jobs WHERE (state='pending' OR state='running' AND lease_until<?) AND next_attempt_at<=? ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *",token,now+120000,now,now);if(!job)break;
  const renew=setInterval(()=>{void run(db,"UPDATE pos_jobs SET lease_until=? WHERE id=? AND lease_token=? AND state='running'",Date.now()+120000,job.id,token).catch(()=>{});},30000);
  try {
   const again=await processJob(db,holdingId,job);
   await run(db,'UPDATE pos_jobs SET state=?,next_attempt_at=?,attempts=0,lease_until=NULL,lease_token=NULL,last_error=NULL WHERE id=? AND lease_token=?',again?'pending':'completed',Date.now()+15000,job.id,token);
  }catch(error){
   const raw=(error as {code?:string;message?:string}).code??(error as Error).message;
   const code=/^[A-Z0-9_]+$/.test(raw)?raw:'POS_JOB_FAILED',exception=num(job,'attempts')>=6||['POS_PERIOD_CLOSED','POS_PROVIDER_PAYMENT_MISMATCH','POS_PROVIDER_REFUND_FAILED','POS_STRIPE_NOT_CONFIGURED','POS_PROVIDER_LOOKUP_REQUIRED'].includes(code);
   await run(db,'UPDATE pos_jobs SET state=?,next_attempt_at=?,last_error=?,lease_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?',exception?'exception':'pending',Date.now()+Math.min(60000,1000*2**num(job,'attempts')),code,job.id,token);
   if(exception)await audit(db,str(job,'company_id'),null,'system:stripe','job.exception',str(job,'record_id'),{jobId:job.id,code});
  }finally{clearInterval(renew);}
 }
}
export async function drainAllPos(){const env=await runtimeEnv();for(const slot of resourceSlots(env)){const db=registeredResources(env,slot.id).write as PostgresDatabase;if((await one(db,"SELECT to_regclass('pos_jobs') name"))?.name)await drainPos(db,slot.id);}}
