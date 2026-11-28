"use client";
import {useRef,useState} from "react";
import {useRouter} from "@erpx/platform-shared-ui/lib/destination-router";
import {captureCRMForm,crmAcknowledgement,crmCommandContext,crmCommandFetch,crmFormUnchanged} from "@erpx/crm-ui/lib/command-fetch";
import {createCRMCommandAttempt,type CRMCommandAttempt} from "@erpx/crm-ui/lib/command-attempt";
import {crmCommandTransport} from "../lib/oasis-command-transport";
export function useCRMCommand(base:string,path:'crm'|'crm/activities'|'marketing'|'customer-followups'|'ecommerce'|'customer-access',allowed:(input:unknown)=>boolean,onSaved?:(receipt:{id:string;version:number}&Record<string,unknown>,input:Record<string,unknown>)=>void){
 const router=useRouter(),pendingRef=useRef(false),commands=useRef(createCRMCommandAttempt());
 const [error,setError]=useState(''),[notice,setNotice]=useState(''),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false);
 async function execute(requested:CRMCommandAttempt,retry=false){
  if(pendingRef.current)return;let attempt:CRMCommandAttempt;try{attempt=commands.current.select(requested,crmCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:'Review the original submitted customer change.');return;}
  const input=JSON.parse(attempt.body) as Record<string,unknown>;
  const transport=crmCommandTransport(window.location.origin,attempt.path,input,attempt.body);
  if(!transport){setError('This action is unavailable in the Oasis demo. Your draft is retained.');return;}
  const {path:url,body,receiptAtRoot}=transport;
  if(!allowed(input)&&!crmCommandFetch.hasAttempt(url,body)){setError('This action is unavailable for the current package. Your draft is retained.');return;}
  pendingRef.current=true;setBusy(true);setError('');setNotice('');const completion=retry||attempt!==requested?attempt.completion:captureCRMForm();attempt.completion=completion;let mayHaveCommitted=true;
  try{const response=await crmCommandFetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body},completion,receiptAtRoot),result=await response.json() as {error?:{message?:string;code?:string}};
   if(!response.ok){mayHaveCommitted=response.status>=500||result.error?.code==='WORK_CONTEXT_CHANGED';if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved(attempt);setUncertain(false);}throw Error(result.error?.message??'Unable to save the customer change.');}
   const receipt=crmAcknowledgement(result,receiptAtRoot);
   if(!receipt)throw Error('The acknowledgement is incomplete. Retry the submitted customer change.');commands.current.resolved(attempt);setUncertain(false);
   if(!crmFormUnchanged(completion)){setNotice('Submitted change saved. Your newer draft is retained.');return;}
   if(completion){window.dispatchEvent(new CustomEvent('erpx:crm-command-accepted',{detail:completion}));window.dispatchEvent(new CustomEvent('erpx:hub-saved',{detail:completion}));}
   onSaved?.(receipt,input);router.refresh();
  }catch(cause){if(mayHaveCommitted&&crmCommandFetch.hasAttempt(url,body)&&!commands.current.retry()){commands.current.unresolved(attempt);setUncertain(true);}setError(cause instanceof Error?cause.message:'Connection interrupted. Retry the exact submitted customer change.');}
  finally{pendingRef.current=false;setBusy(false);}
 }
 return {send:(input:unknown)=>execute({path:`/api/v1${base}/${path}`,body:JSON.stringify(input),context:crmCommandContext()}),retry:()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve();},error,setError,notice,busy,uncertain};
}
