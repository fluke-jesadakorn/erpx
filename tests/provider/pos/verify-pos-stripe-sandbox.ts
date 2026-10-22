/** Real provider boundary rehearsal; no synthetic webhook/provider success. */
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {mock} from 'bun:test';
import {assertHospitalityDevelopmentTarget} from "@erpx/feature-shared/lib/hospitality-seed-target";
import {posDemoCompany as company} from "../../../fixtures/pos/catalog";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
mock.module('next/navigation',()=>({forbidden:()=>{throw new Error('CLI_PAGE_FORBIDDEN');},redirect:()=>{throw new Error('CLI_REDIRECT_FORBIDDEN');}}));
const path=process.env.ERPX_DATABASE_MANIFEST;if(!path)throw new Error('DEVELOPMENT_MANIFEST_REQUIRED');
const app=JSON.parse(await readFile(path,'utf8')),jobsPath=join(dirname(path),'databases-jobs.json'),jobs=JSON.parse(await readFile(jobsPath,'utf8'));
assertHospitalityDevelopmentTarget(app,process.env.ERPX_ENVIRONMENT);assertHospitalityDevelopmentTarget(jobs,process.env.ERPX_ENVIRONMENT);
if(!process.env.POS_STRIPE_TEST_SECRET_KEY?.startsWith('sk_test_'))throw new Error('STRIPE_TEST_KEYS_REQUIRED');
process.env.ERPX_DATABASE_MANIFEST=jobsPath;process.env.ERPX_PROCESS_ROLE='jobs';
const {closeRuntime}=await import("@erpx/server-runtime/adapters/runtime/env");
const {posCommand,one,all}=await import("@erpx/server-pos/public/pos-core");
const {drainPos,stripeCall}=await import("@erpx/server-pos/public/pos-stripe");
const db=new PostgresDatabase({connectionString:jobs.databases.H001_DB,max:2});
const actor={userId:'pos-thb-v1-admin',assignmentId:'pos-thb-v1-admin-access',role:'holding_owner',companyId:company,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'acme',holdingName:'ACME',name:'POS demo administrator',email:'admin@pos-thb-v1.example',assignmentScope:'holding',inherited:false,modules:['pos','finance'],assignments:[],workspaces:[],scope:{kind:'holding',holdingId:'h001',holdingSlug:'acme',holdingName:'ACME',companyId:company}} as WorkContext;
const directory=resolve('.scratch/dev/pos-stripe-sandbox');await mkdir(directory,{recursive:true,mode:0o700});
try{
 const platform=await stripeCall(false,'account');
 // v1 reached Stripe before Connect activation and Stripe cached its rejection.
 // This distinct attempt is safe only after verifying that it created no account.
 const refreshKey=process.argv.find(arg=>arg.startsWith('--refresh-key='))?.slice('--refresh-key='.length);
 const result=await posCommand(db,actor,refreshKey?'connection.refresh':'connection.create',{livemode:false},refreshKey??'pos-sandbox-real-connect-after-activation-v2');
 const previous=await one(db,'SELECT id,state,last_error FROM pos_jobs WHERE record_id=? AND company_id=?',result.id,company);
 if(previous?.state==='exception'&&['STRIPE_HTTP_401','STRIPE_HTTP_400'].includes(String(previous.last_error)))await posCommand(db,actor,'exception.retry',{id:previous.id,accountingDate:new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok'}).format(new Date()),reason:'Stripe key verified and Connect platform setup confirmed'},`pos-sandbox-connect-configured-v2:${previous.id}`);
 if(!process.argv.includes('--worker-only'))await drainPos(db,'h001',4,company);
 const job=await one(db,'SELECT state,last_error,payload_json FROM pos_jobs WHERE record_id=? AND company_id=?',result.id,company);
 const connection=await one(db,'SELECT account_id,livemode,state,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,requirements_json,thb_bank_ready,payout_interval FROM pos_connections WHERE company_id=?',company);
 const jobsStatus=await all(db,'SELECT kind,state,last_error FROM pos_jobs WHERE company_id=?',company);
 const events=await all(db,'SELECT id,event_type,applied_at FROM pos_stripe_events WHERE company_id=? ORDER BY received_at DESC LIMIT 20',company);
 const eventSources=await all(db,"SELECT e.id,e.event_type,e.applied_at,COUNT(j.id)::int job_count,COUNT(j.id) FILTER (WHERE j.state='completed')::int completed_jobs FROM pos_stripe_events e LEFT JOIN pos_jobs j ON j.company_id=e.company_id AND j.kind='event' AND j.record_id=e.id WHERE e.company_id=? GROUP BY e.id,e.event_type,e.applied_at ORDER BY e.id",company);
 if(eventSources.some(event=>event.job_count!==1))throw new Error('DUPLICATE_OR_MISSING_PROVIDER_EVENT_JOB');
 let provider:Record<string,unknown>|null=null;
 if(connection){if(connection.livemode)throw new Error('LIVE_PROVIDER_FORBIDDEN');const account=await stripeCall(false,`accounts/${connection.account_id}`);provider={id:account.id,country:account.country,charges_enabled:account.charges_enabled,payouts_enabled:account.payouts_enabled,capabilities:account.capabilities,controller:account.controller,requirements:account.requirements};}
 const payload=JSON.parse(String(job?.payload_json??'{}'));
 const proof={schema:1,realStripeApi:true,syntheticWebhook:false,company,checkedAt:new Date().toISOString(),platformAccount:platform.id,connection,provider,jobs:jobsStatus,events,eventSources,onboardingUrl:payload.url??null,acceptance:{hostedOnboarding:connection?.state==='ready'?'passed':'awaiting merchant onboarding',accountWebhook:events.some(event=>event.event_type==='account.updated'&&event.applied_at)?'verified provider delivery processed by worker':'not received',card:'not run',promptpay:'not run',refund:'not run',webhookReplay:'payment/refund not run; account-event source counts recorded separately',recovery:'not run'}};
 await writeFile(join(directory,'verification.json'),JSON.stringify(proof,null,2),{mode:0o600});
 console.log(JSON.stringify({...proof,onboardingUrl:proof.onboardingUrl?'saved in private verification file':null},null,2));
 if(!connection||connection.state!=='ready')process.exitCode=2;
}finally{await db.close();await closeRuntime();}
