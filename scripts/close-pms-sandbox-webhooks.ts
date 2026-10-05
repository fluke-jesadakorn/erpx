/** Resolve only webhook jobs for the exact disposable rehearsal in the supplied evidence file.
 * Provider events remain intact. No development PMS/Finance receipt or ledger is manufactured.
 * This utility refuses live mode, non-development databases and unknown payment identities.
 */
import {createHash} from 'node:crypto';
import {readFile,writeFile} from 'node:fs/promises';
import {assertHospitalityDevelopmentTarget} from '../src/lib/hospitality-seed-target';
import {PostgresDatabase} from '../src/server/runtime/postgres';
import {stripeCall} from '../src/server/pos-stripe';
const path=process.argv[2];
if(!path?.startsWith('.scratch/dev/')||!path.endsWith('/verification.json'))throw new Error('EXACT_REHEARSAL_EVIDENCE_REQUIRED');
const bytes=await readFile(path),proof=JSON.parse(bytes.toString()) as {disposableDatabase:boolean;realStripeApi:boolean;account:string;checkedAt:string;cases:Array<{name:string;attemptToken:string;checkoutUrl:string}>};
if(!proof.disposableDatabase||!proof.realStripeApi||!proof.account.startsWith('acct_'))throw new Error('DISPOSABLE_PROVIDER_PROOF_REQUIRED');
const manifest=await Bun.file(process.env.ERPX_DATABASE_MANIFEST!).json();assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT);
const db=new PostgresDatabase({connectionString:manifest.databases.H001_DB,max:1}),digest=createHash('sha256').update(bytes).digest('hex'),resolved:string[]=[];
try{
 const jobs=(await db.prepare(`SELECT j.id,j.payload_json,e.id event_id,e.event_type,e.payload_json event_json,e.received_at FROM pos_jobs j JOIN pos_stripe_events e ON e.id=j.record_id
   WHERE j.kind='event' AND j.state='exception' AND j.last_error IN ('PMS_PROVIDER_BOOKING_MISMATCH','PMS_PROVIDER_REFUND_MISMATCH') AND e.account_id=? AND e.livemode=false`).bind(proof.account).all<{id:string;payload_json:string;event_id:string;event_type:string;event_json:string;received_at:number}>()).results;
 for(const job of jobs){
  const object=JSON.parse(job.event_json) as {id:string;metadata?:Record<string,string>;payment_intent?:string};
  const item=proof.cases.find(c=>c.attemptToken.split('.')[0]===object.metadata?.pms_booking_payment_id);if(!item)continue;
  const sessionId=new URL(item.checkoutUrl).pathname.split('/').at(-1)!;
  const session=await stripeCall(false,`checkout/sessions/${sessionId}`,{account:proof.account});
  if((session.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==item.attemptToken.split('.')[0]||session.livemode!==false)throw new Error('REHEARSAL_PROVIDER_IDENTITY_MISMATCH');
  if(job.event_type.startsWith('refund.')){
   const refund=await stripeCall(false,`refunds/${object.id}`,{account:proof.account});
   if(refund.payment_intent!==session.payment_intent||refund.livemode===true)throw new Error('REHEARSAL_REFUND_IDENTITY_MISMATCH');
  }else if(job.event_type.startsWith('payment_intent.')&&object.id!==session.payment_intent)throw new Error('REHEARSAL_PAYMENT_IDENTITY_MISMATCH');
  else if(job.event_type.startsWith('checkout.session.')&&object.id!==session.id)throw new Error('REHEARSAL_CHECKOUT_IDENTITY_MISMATCH');
  if(await db.prepare('SELECT id FROM pms_booking_payments WHERE id=?').bind(item.attemptToken.split('.')[0]).first())throw new Error('DEVELOPMENT_PAYMENT_OWNER_EXISTS');
  await db.transaction(async tx=>{
   const current=await tx.prepare("SELECT state FROM pos_jobs WHERE id=? FOR UPDATE").bind(job.id).first<{state:string}>();if(current?.state!=='exception')return;
   await tx.prepare("UPDATE pos_jobs SET state='completed',last_error=NULL,lease_until=NULL,lease_token=NULL,payload_json=? WHERE id=?").bind(JSON.stringify({...JSON.parse(job.payload_json),externalSandboxRehearsal:{evidence:path,sha256:digest,scenario:item.name,resolution:'Verified external disposable fixture; no development payment owner',resolvedAt:Date.now()}}),job.id).run();
   await tx.prepare('UPDATE pos_stripe_events SET applied_at=? WHERE id=?').bind(Date.now(),job.event_id).run();
  });resolved.push(job.event_id);
 }
 await writeFile(path.replace('/verification.json','/webhook-resolution.json'),JSON.stringify({checkedAt:new Date().toISOString(),evidenceSha256:digest,providerVerified:true,liveMode:false,resolvedEvents:resolved},null,2),{mode:0o600});
 console.log(`Resolved ${resolved.length} verified disposable sandbox webhook events; original events retained`);
}finally{await db.close();}
