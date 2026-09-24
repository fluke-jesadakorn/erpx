'use client';

import {useEffect,useState} from 'react';
import {useRouter} from 'next/navigation';
import {workFetch} from '@/lib/work-fetch';
import styles from './company-stripe.module.css';

type Connection={account_id:string|null;state:string;livemode:boolean;charges_enabled:boolean;payouts_enabled:boolean;promptpay_enabled:boolean;thb_bank_ready:boolean;payout_interval:string|null;requirements_json:string};
type Job={id:string;kind:string;state:string;last_error:string|null;payload_json:string};
type Requirements={currently_due:string[];pending_verification:string[]};

function requirementsFrom(value:string):Requirements{
 try{
  const parsed:unknown=JSON.parse(value||'[]');
  if(Array.isArray(parsed))return {currently_due:parsed.filter((item):item is string=>typeof item==='string'),pending_verification:[]};
  if(parsed&&typeof parsed==='object'){
   const record=parsed as Record<string,unknown>;
   return {
    currently_due:Array.isArray(record.currently_due)?record.currently_due.filter((item):item is string=>typeof item==='string'):[],
    pending_verification:Array.isArray(record.pending_verification)?record.pending_verification.filter((item):item is string=>typeof item==='string'):[],
   };
  }
 }catch{}
 return {currently_due:[],pending_verification:[]};
}

function requirementLabel(value:string,pending:boolean){
 if(value.includes('verification.document'))return pending?'Stripe is reviewing a verification document.':'Upload a verification document in Stripe.';
 if(value.startsWith('tos_acceptance.'))return 'Accept the connected account agreement in Stripe.';
 if(value.includes('external_account'))return 'Add a THB bank account in Stripe.';
 return pending?'Stripe is reviewing submitted information.':'Stripe needs more account information.';
}

function latestOnboardingUrl(jobs:Job[]){
 for(const job of jobs){
  if(job.state!=='completed')continue;
  try{const payload:unknown=JSON.parse(job.payload_json||'{}');if(payload&&typeof payload==='object'&&'url' in payload&&typeof payload.url==='string'&&payload.url.startsWith('https://connect.stripe.com/'))return payload.url;}catch{}
 }
 return null;
}

function StatusTile({label,ready,detail}:{label:string;ready:boolean;detail:string}){
 return <div className={styles.tile}><span className={styles.tileLabel}>{label}</span><strong className={ready?styles.good:styles.waiting}><span className={styles.dot} aria-hidden="true"/>{ready?'Ready':'Pending'}</strong><small>{detail}</small></div>;
}

export function CompanyStripe({base,connection,jobs,liveEnabled}:{base:string;connection:Connection|null;jobs:Job[];liveEnabled:boolean}){
 const router=useRouter(),[busy,setBusy]=useState(false),[error,setError]=useState('');
 useEffect(()=>{const timer=window.setInterval(()=>router.refresh(),5000);return()=>window.clearInterval(timer);},[router]);
 async function connect(action:string,livemode:boolean){
  setBusy(true);setError('');
  try{const response=await workFetch(`/api/v1${base}/pos/connection/${action}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':crypto.randomUUID()},body:JSON.stringify({livemode})});const body=await response.json();if(!response.ok)throw new Error(body.error?.message??'Stripe setup failed');if(body.data?.url)window.location.assign(body.data.url);else router.refresh();}
  catch(error){setError(error instanceof Error?error.message:'Stripe setup failed');}finally{setBusy(false);}
 }

 const connected=Boolean(connection?.account_id),requirements=requirementsFrom(connection?.requirements_json??'[]');
 const paymentsReady=Boolean(connection?.charges_enabled&&connection.promptpay_enabled);
 const payoutReady=Boolean(connection?.payouts_enabled&&connection.thb_bank_ready&&connection.payout_interval==='daily');
 const complete=paymentsReady&&payoutReady;
 const onboardingUrl=latestOnboardingUrl(jobs);
 const latestError=jobs[0]?.state==='failed'?jobs[0].last_error:null;
 const due=[...new Set(requirements.currently_due.map(item=>requirementLabel(item,false)))];
 const pending=[...new Set(requirements.pending_verification.map(item=>requirementLabel(item,true)))];

 return <section className={styles.panel} aria-labelledby="stripe-account-title">
  <div className={styles.header}>
   <div>
    <div className={styles.eyebrow}>Company payment account <span className={styles.mode}>{connection?.livemode?'Live':'Sandbox'}</span></div>
    <h2 id="stripe-account-title">Stripe Connect</h2>
    <p>One connected account for this company’s POS, hotel payments and online bookings.</p>
   </div>
   {connected&&<button className="button secondary" disabled={busy} onClick={()=>void connect('refresh',Boolean(connection?.livemode))}>{busy?'Checking…':'Check Stripe status'}</button>}
  </div>

  {error&&<p className={styles.error} role="alert">{error}</p>}
  {latestError&&!error&&<p className={styles.error} role="alert">Stripe sync: {latestError}</p>}

  {connected?<>
   <div className={`${styles.summary} ${complete?styles.summaryReady:paymentsReady?styles.summaryPartial:styles.summaryWaiting}`} role="status">
    <span className={styles.summaryIcon} aria-hidden="true">{complete?'✓':paymentsReady?'◷':'!'}</span>
    <div><strong>{complete?'Payments and payouts are ready':paymentsReady?'PromptPay is ready for payments':'Account setup is in progress'}</strong><p>{complete?'Stripe will pay the available THB balance on its daily schedule.':paymentsReady?'Payouts are not ready yet. Review the account requirements below.':'Complete the account requirements in Stripe before taking payments.'}</p></div>
   </div>
   <div className={styles.accountLine}><span>Connected account</span><code>{connection?.account_id}</code></div>
   <div className={styles.tiles}>
    <StatusTile label="PromptPay" ready={paymentsReady} detail="Direct charges for this company"/>
    <StatusTile label="Payouts" ready={Boolean(connection?.payouts_enabled)} detail="Stripe to company bank"/>
    <StatusTile label="THB bank" ready={Boolean(connection?.thb_bank_ready)} detail="Bank account on file"/>
    <StatusTile label="Daily schedule" ready={connection?.payout_interval==='daily'} detail="Available balance only"/>
   </div>
   {(due.length>0||pending.length>0)&&<div className={styles.requirements}>
    <h3>{due.length?'Action needed in Stripe':'Verification in progress'}</h3>
    {due.length>0&&<ul>{due.map(item=><li key={item}>{item}</li>)}</ul>}
    {pending.length>0&&<ul>{pending.map(item=><li key={item}>{item}</li>)}</ul>}
    <p>Stripe controls the review. Refresh this page after its account status changes.</p>
    {due.length>0&&onboardingUrl&&<a className="button" href={onboardingUrl}>Continue in Stripe</a>}
   </div>}
   {!complete&&due.length===0&&pending.length===0&&<p className={styles.note}>Stripe has no outstanding requirements in this snapshot. Check Stripe status to update payment and payout readiness.</p>}
   <div className={styles.footer}><p>Finance reconciles each payout after Stripe sends it. Payout date can differ from the POS or hotel business date.</p></div>
   <details className={styles.advanced}><summary>Account options and activity</summary><div className={styles.advancedBody}>
    <p>Connect another account only when this company is changing its Stripe account.</p>
    <div className={styles.actions}><button className="button secondary" disabled={busy} onClick={()=>void connect('oauth',false)}>Connect existing test account</button>{liveEnabled&&<><button className="button secondary" disabled={busy} onClick={()=>void connect('create',true)}>Set up live account</button><button className="button secondary" disabled={busy} onClick={()=>void connect('oauth',true)}>Connect existing live account</button></>}</div>
    {jobs.length>0&&<ul className={styles.activity}>{jobs.slice(0,5).map(job=><li key={job.id}><span>{job.kind==='connect_refresh'?'Status check':'Connection'}</span><strong>{job.state}</strong></li>)}</ul>}
   </div></details>
  </>:<div className={styles.empty}><h3>Connect a Stripe account</h3><p>Set up a company account in Stripe sandbox to try PromptPay payments and payout reporting.</p><div className={styles.actions}><button className="button" disabled={busy} onClick={()=>void connect('create',false)}>Set up test account</button><button className="button secondary" disabled={busy} onClick={()=>void connect('oauth',false)}>Connect existing test account</button>{liveEnabled&&<><button className="button secondary" disabled={busy} onClick={()=>void connect('create',true)}>Set up live account</button><button className="button secondary" disabled={busy} onClick={()=>void connect('oauth',true)}>Connect existing live account</button></>}</div></div>}
 </section>;
}
