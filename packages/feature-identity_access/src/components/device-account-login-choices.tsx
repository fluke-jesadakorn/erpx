'use client';
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {useEffect,useState} from 'react';
import {Button} from '@erpx/ui/components/button';
import type {DeviceAccounts} from '@erpx/feature-identity_access/lib/device-accounts';
async function load(){
 let response=await holdingFetch('/api/v1/account/device-accounts',{cache:'no-store'}),body=await response.json();
 if(!response.ok||!body.data)throw Error('Could not verify saved accounts.');
 if(!body.data.initialized){response=await holdingFetch('/api/v1/account/device-accounts/initialize',{method:'POST',headers:{'Content-Type':'application/json'},body:'{}'});body=await response.json();if(!response.ok||!body.data)throw Error('Could not verify saved accounts.');}
 return body.data as DeviceAccounts;
}
/** An expired active Account cookie must not hide other proven device sessions.
 * This is an additive picker on the existing login page, with the same facade. */
export function DeviceAccountLoginChoices({next}:{next:string}){
 const [state,setState]=useState<DeviceAccounts|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[unverified,setUnverified]=useState(false);
 useEffect(()=>{let active=true;void(async()=>{
  try{const next=await load();if(active)setState(next);
  }catch{/* Ordinary sign-in remains usable when device discovery is unavailable. */}
 })();return()=>{active=false;};},[]);
 if(!state?.accounts.length)return null;
 return <section aria-label="Saved accounts" className="ui-stack"><strong>Continue with a saved account</strong>{state.accounts.map(account=><Button appearance="standard" variant="ghost" key={account.handle} style={{whiteSpace:'normal',overflowWrap:'anywhere'}} disabled={busy||unverified} onClick={async()=>{
  setBusy(true);setError('');try{const response=await holdingFetch('/api/v1/account/device-accounts/active',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({handle:account.handle,revision:state.revision})}),body=await response.json();if(!response.ok)throw Error(body.error?.message??'This saved account is unavailable. Sign in again.');const destination=new URL(next,window.location.origin);if(destination.origin!==window.location.origin)throw Error('Open sign-in from your app.');window.location.assign(destination.href);}catch(cause){setError(cause instanceof Error?cause.message:'Could not select account.');try{setState(await load());}catch{setUnverified(true);}setBusy(false);}
 }}>{account.name} · {account.email}</Button>)}{error&&<p role="alert">{error}</p>}{unverified&&<Button appearance="standard" disabled={busy} onClick={async()=>{setBusy(true);try{setState(await load());setUnverified(false);setError('');}catch{setError('Check your connection and verify the account change before retrying.');}finally{setBusy(false);}}}>Check account status</Button>}</section>;
}
