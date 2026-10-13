/** Real Checkout rehearsal in disposable databases; never uses development booking records.
 * Run with the pinned Bun: bun --conditions=react-server tests/provider/pms/pms-stripe-sandbox-rehearsal.ts
 * Open the Checkout links saved under .scratch/dev/pms-2026-10-05/sandbox/ in Chrome.
 * No mail delivery is attempted. Provider and local fault-injection evidence stay distinct.
 */
import {runtimeSourceFingerprint,verificationSourceFingerprint} from "../../../scripts/local/source-fingerprint";
import {mkdir,writeFile} from 'node:fs/promises';
import {mock} from 'bun:test';
import {randomUUID} from 'node:crypto';
import {setupDatabases,cleanupDatabases,applyMigrations,env} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {record,records,pmsAccountKeys,plusDays,type Config} from "../../../src/server/modules/pms/public/rules/pms-model";
if(!process.env.POS_STRIPE_TEST_SECRET_KEY?.startsWith('sk_test_'))throw new Error('TEST_STRIPE_KEY_REQUIRED');
process.env.ERPX_TEST_STRIPE_SANDBOX='1';
mock.module('next/navigation',()=>({forbidden:()=>{throw new Error('CLI_FORBIDDEN');},redirect:()=>{throw new Error('CLI_REDIRECT');}}));
const delivery={secret:randomUUID()+randomUUID(),origin:'https://console-dev-erpx.jesadakorn.com'};
const actualRuntime=await import("../../../src/server/modules/runtime/adapters/runtime");
mock.module('../src/server/runtime',()=>({...actualRuntime,holdingResources:async()=>({write:env.H001_DB}),runtimeEnv:async()=>({BETTER_AUTH_SECRET:delivery.secret,ERPX_PUBLIC_ORIGIN:delivery.origin})}));
const {pmsCommand,loadPms}=await import("../../../src/server/modules/pms/public/pms");
const {publicQuote,createPublicReservation,bookingPaymentStatus,bookingSnapshot,persist}=await import("../../../src/server/modules/pms/public/pms-booking");
const {requestBookingRefund,approveBookingRefund,processBookingRefund}=await import("../../../src/server/modules/pms/public/pms-booking-refunds");
const {stripeCall}=await import("../../../src/server/modules/pos/public/pos-stripe");
const account=process.env.ERPX_PMS_SANDBOX_ACCOUNT??'acct_1ULRA83df28HzxB5';
const provider=await stripeCall(false,`accounts/${account}`);
if(provider.charges_enabled!==true)throw new Error('SANDBOX_ACCOUNT_NOT_READY');
const directory='.scratch/dev/pms-2026-10-05/sandbox/'+new Date().toISOString().replace(/[:.]/g,'-');await mkdir(directory,{recursive:true,mode:0o700});
const sourceFingerprint=await runtimeSourceFingerprint(process.cwd()),verificationFingerprint=await verificationSourceFingerprint(process.cwd(),sourceFingerprint);
const supported=['card-full','card-deposit','card-3ds','card-declined','promptpay-full','promptpay-deposit','promptpay-expired'];
const selectedArg=process.argv.find(arg=>arg.startsWith('--cases='));const selected=selectedArg?selectedArg.slice(8).split(','):supported.slice(0,6);if(!selected.length||selected.some(name=>!supported.includes(name)))throw new Error('CHOOSE_SUPPORTED_SANDBOX_CASES');
await setupDatabases();
const db=env.H001_DB;
try{
 await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);
 const f=await seedExpenseCompany(db,env.H001_FILES),owner=f.owner,reviewer=await f.member('sandbox-reviewer','finance_approver'),operationalReviewer=await f.member('sandbox-pms-reviewer','pms_manager');
 const date=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok'}).format(Date.now());
 const propertyId=(await pmsCommand(db,owner,{operation:'property.create',input:{name:'PMS isolated Stripe sandbox',branchId:f.branch,businessDate:date}},randomUUID())).id;
 const cmd=async(operation:string,input:unknown={},who=owner)=>{const s=await loadPms(db,owner,propertyId);return pmsCommand(db,who,{propertyId,expectedVersion:s.property.version,operation,input},randomUUID());};
 const accounts={} as Config['accounts'];
 for(const [i,k] of pmsAccountKeys.entries()){
  const id=randomUUID();accounts[k]=id;const category=['revenue','service'].includes(k)?'revenue':['advance','deposit','tax'].includes(k)?'liability':['consumption','commission','cashDifference'].includes(k)?'expense':'asset';
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(id,f.chart,f.company,`PMS-${i}`,k,category).run();
 }
 await cmd('accounting.save',{chartId:f.chart,accounts,taxBps:700,serviceBps:0,taxInclusive:false,advanceTax:true});await cmd('accounting.approve',{},reviewer);
 const liabilities=[randomUUID(),randomUUID()];for(const [i,id] of liabilities.entries())await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(id,f.chart,f.company,`ONLINE-${i}`,`Online liability ${i}`).run();
 await cmd('online_accounting.prepare',{refundPayable:liabilities[0],unallocated:liabilities[1]});await cmd('online_accounting.approve',{},reviewer);
 const typeId=(await cmd('room_type.save',{name:'Sandbox Deluxe',capacity:2,amenities:'Shower'})).id;
 for(let i=1;i<=8;i++)await cmd('room.save',{number:String(100+i),typeId,active:true});
 const rateIds:Record<string,string>={};for(const method of ['full','deposit'] as const)rateIds[method]=(await cmd('rate.save',{name:method,typeId,from:date,to:plusDays(date,90),price:10000,collection:{method,depositPercent:30,changesAllowed:true}})).id;
 await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,state,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,requirements_json,thb_bank_ready,payout_interval) VALUES(?,?,false,'ready',true,true,true,true,'{}',true,'daily')").bind(f.company,account).run();
 await cmd('property.enable');
 // Fixture publication, not evidence of browser setup or mail readiness.
 const s=await bookingSnapshot(db,propertyId);const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
 s.property.config.booking={published:true,description:'Isolated provider fixture',address:'Bangkok',contactEmail:'sandbox@example.invalid',contactPhone:'0000000000',cancellationDays:1,cancellationTime:'18:00',rateIds:Object.values(rateIds),roomContent:[{typeId,description:'Sandbox room'}]};await persist(db,s,before);await db.prepare('UPDATE pms_properties SET config_json=? WHERE id=?').bind(JSON.stringify(s.property.config),propertyId).run();
 const cases:Array<{name:string;policy:string;reservationId:string;attemptToken:string;checkoutUrl:string;state:string;refund?:string}>=[];
 for(const [i,name] of selected.entries()){
  const policy=name.endsWith('deposit')?'deposit':'full',now=Date.now();
  const q=await publicQuote(db,propertyId,[{typeId,rateId:rateIds[policy],arrival:plusDays(date,7+i*3),departure:plusDays(date,9+i*3),adults:2,quantity:1,childAges:[]}],delivery,now);
  const response=await createPublicReservation(db,'acme',{propertyId,quoteToken:q.token,guest:{name:`PMS Sandbox ${name}`,email:process.env.ERPX_PMS_SANDBOX_EMAIL??'pms-sandbox@example.invalid',phone:'0000000000',nationality:'TH',documentRef:''}},randomUUID(),delivery,now) as {id:string;attemptToken:string;checkoutUrl:string};
  cases.push({name,policy,reservationId:response.id,attemptToken:response.attemptToken,checkoutUrl:response.checkoutUrl,state:'payment_pending'});
 }
 const proof={sourceFingerprint,verificationFingerprint,browserEvidence:'Recorded separately; provider polling does not attest to return-page or signed-webhook E2E',checkedAt:new Date().toISOString(),realStripeApi:true,realHostedCheckout:true,mailDelivered:false,disposableDatabase:true,account,cases,assertions:[] as string[]};
 const save=()=>writeFile(`${directory}/verification.json`,JSON.stringify(proof,null,2),{mode:0o600});await save();await writeFile('.scratch/dev/pms-2026-10-05/sandbox/latest.json',JSON.stringify({directory}),{mode:0o600});console.log('Checkout links ready: '+directory+'/verification.json');
 const expired=cases.find(c=>c.name==='promptpay-expired');if(expired){const checkoutId=new URL(expired.checkoutUrl).pathname.split('/').at(-1)!;await stripeCall(false,`checkout/sessions/${checkoutId}/expire`,{method:'POST',account,key:`rehearsal-expire-${checkoutId}`,body:{}});const response=await bookingPaymentStatus(db,expired.attemptToken,delivery,Date.now()+31*60*1000);if(response.state!=='expired')throw new Error('PROVIDER_EXPIRY_NOT_REFLECTED');expired.state='provider_expired';proof.assertions.push('promptpay-expired: real Stripe-hosted session expiry, local 30-minute hold clock fault injection; no receipt or revenue');await save();}
 const deadline=Date.now()+25*60*1000;
 while(Date.now()<deadline){
  for(const item of cases.filter(c=>c.state==='payment_pending'&&c.name!=='card-declined')){
   const response=await bookingPaymentStatus(db,item.attemptToken,delivery) as {state:string};
   if(response.state!=='confirmed')continue;
   const state=await bookingSnapshot(db,propertyId);const st=records(state,'stay').find(st=>st.data.reservationId===item.reservationId)!;
   const payment=await db.prepare('SELECT id,amount_minor,provider_succeeded_at FROM pms_booking_payments WHERE reservation_id=?').bind(item.reservationId).first<{id:string;amount_minor:number;provider_succeeded_at:number}>();
   if(!payment?.provider_succeeded_at||record(state,'guest_account',st.data.guestAccountId).data.advance!==Number(payment.amount_minor))throw new Error('PMS_PROVIDER_RECEIPT_MISMATCH');
   await bookingPaymentStatus(db,item.attemptToken,delivery);
   const count=await db.prepare('SELECT count(*) n FROM pms_booking_allocations WHERE payment_id=?').bind(payment.id).first<{n:number}>();if(Number(count?.n)!==1)throw new Error('DUPLICATE_PROVIDER_RECEIPT');
   item.state='confirmed';proof.assertions.push(`${item.name}: real Checkout payment, verified provider timestamp, receipt and immutable allocation; repeat reconciliation no duplicate`);
   // PromptPay refunds require an explicitly supplied controlled email. No instructions are sent to a guessed recipient.
   if(item.name.startsWith('card-')||process.env.ERPX_PMS_SANDBOX_EMAIL){
    let refundId='';await db.transaction(async tx=>{const current=await bookingSnapshot(tx,propertyId,undefined,true);const request=await requestBookingRefund(tx,current,payment.id,[{stayId:st.id,amount:Math.floor(Number(payment.amount_minor)/2)}],'Real sandbox partial refund',owner.userId);refundId=request.id;});
    await approveBookingRefund(db,operationalReviewer,refundId);await processBookingRefund(db,refundId);await processBookingRefund(db,refundId);
    const refund=await db.prepare('SELECT state,provider_id FROM pms_booking_refund_requests WHERE id=?').bind(refundId).first<{state:string;provider_id:string}>();item.refund=refund?.state;proof.assertions.push(`${item.name}: independently approved 50% refund ${refund?.state}; provider id ${refund?.provider_id}`);
   }
   console.log(item.name+': '+item.state+(item.refund?' / refund '+item.refund:''));await save();
  }
  if(cases.every(c=>c.state==='confirmed'||c.state==='provider_declined'||c.state==='provider_expired'))break;
  const declined=cases.find(c=>c.name==='card-declined'&&c.state==='payment_pending');
  if(declined){const checkoutId=new URL(declined.checkoutUrl).pathname.split('/').at(-1)!,session=await stripeCall(false,`checkout/sessions/${checkoutId}`,{account});if(session.payment_intent){const pi=await stripeCall(false,`payment_intents/${String(session.payment_intent)}`,{account});const error=pi.last_payment_error as {code?:string;decline_code?:string}|undefined;if(error?.code==='card_declined'){declined.state='provider_declined';proof.assertions.push('card-declined: real Stripe PaymentIntent reports card_declined; booking stays unpaid, no receipt');await save();}}}
  await Bun.sleep(2000);
 }
 const amounts=await db.prepare('SELECT SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journal_lines l JOIN pms_postings p ON p.journal_id=l.journal_id WHERE p.property_id=?').bind(propertyId).first<{debit:number;credit:number}>();
 if(Number(amounts?.debit)!==Number(amounts?.credit))throw new Error('PMS_UNBALANCED_GL');proof.assertions.push('All resulting PMS journals balance');await save();
 if(cases.some(c=>!['confirmed','provider_declined','provider_expired'].includes(c.state)))process.exitCode=2;
 if(await runtimeSourceFingerprint(process.cwd())!==sourceFingerprint)proof.assertions.push('Source changed during rehearsal; this run is supplementary evidence and cannot attest to the final release candidate');await save();
}finally{await cleanupDatabases();}
