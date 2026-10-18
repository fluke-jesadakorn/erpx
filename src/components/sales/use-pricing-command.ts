"use client";
import {useRef,useState} from 'react';
import {useRouter} from '@/lib/shared/destination-router';
import {captureCRMForm,crmCommandContext,crmCommandFetch,crmFormUnchanged,validCRMAcknowledgement} from '@/lib/crm/command-fetch';
import {createCRMCommandAttempt,type CRMCommandAttempt} from '@/lib/crm/command-attempt';

/** Pricing retains its original nested command URLs and payloads. An uncertain
 * command keeps its exact body, context and key; fresh UI flags cannot rewrite
 * it or acknowledge newer local edits. The server reauthorizes every retry. */
export function usePricingCommand(base:string,allowed:(path:string,input:Record<string,unknown>)=>boolean){
 const router=useRouter(),pending=useRef(false),attempts=useRef(createCRMCommandAttempt()),[error,setError]=useState(''),[notice,setNotice]=useState(''),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false);
 async function execute(requested:CRMCommandAttempt,retry=false){
  if(pending.current)return;let attempt:CRMCommandAttempt;try{attempt=attempts.current.select(requested,crmCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:'Review the original submitted price list.');return;}
  const input=JSON.parse(attempt.body) as Record<string,unknown>,prefix=`/api/v1${base}/commerce/pricing`,path=attempt.path.slice(prefix.length);if(!allowed(path,input)&&!crmCommandFetch.hasAttempt(attempt.path,attempt.body)){setError('This pricing action is unavailable under current access. Your draft is retained.');return;}
  pending.current=true;setBusy(true);setError('');setNotice('');const completion=retry||attempt!==requested?attempt.completion:captureCRMForm();attempt.completion=completion;let uncertainResult=true;
  try{const response=await crmCommandFetch(attempt.path,{method:'POST',headers:{'Content-Type':'application/json'},body:attempt.body},completion),result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(!response.ok){uncertainResult=response.status>=500||result.error?.code==='WORK_CONTEXT_CHANGED';if(uncertainResult){attempts.current.unresolved(attempt);setUncertain(true);}else{attempts.current.resolved(attempt);setUncertain(false);}throw Error(result.error?.message??'Unable to save the original pricing control.');}
   if(!validCRMAcknowledgement(result.data))throw Error('The acknowledgement is incomplete. Retry the submitted price list.');attempts.current.resolved(attempt);setUncertain(false);
   if(!crmFormUnchanged(completion)){setNotice('Submitted price list saved. Your newer draft is retained.');return;}
   if(completion){window.dispatchEvent(new CustomEvent('erpx:crm-command-accepted',{detail:completion}));window.dispatchEvent(new CustomEvent('erpx:hub-saved',{detail:completion}));}router.refresh();
  }catch(cause){if(uncertainResult&&crmCommandFetch.hasAttempt(attempt.path,attempt.body)&&!attempts.current.retry()){attempts.current.unresolved(attempt);setUncertain(true);}setError(cause instanceof Error?cause.message:'Retry the exact submitted price list.');}
  finally{pending.current=false;setBusy(false);}
 }
 return {send:(path:string,input:unknown)=>execute({path:`/api/v1${base}/commerce/pricing${path}`,body:JSON.stringify(input),context:crmCommandContext()}),retry:()=>{const attempt=attempts.current.retry();return attempt?execute(attempt,true):Promise.resolve();},error,notice,busy,uncertain};
}
