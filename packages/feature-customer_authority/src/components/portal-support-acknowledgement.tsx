'use client';
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {contextGeneration,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import type {PortalSupportThreadDisplay} from '@erpx/feature-customer_authority/lib/portal-display';
import {createPortalActionAttempt,type PortalActionAttempt,type PortalPurchaseControls} from '@erpx/feature-customer_authority/lib/portal-command-attempt';
import {Button,InlineNotice,Card} from '@erpx/ui/components/foundation';
import {TimeText} from '@erpx/feature-people_payroll/components/time/time-preference-context';
export function PortalSupportAcknowledgement({holdingSlug,bindingId,bindingVersion,thread,purchase}:{holdingSlug:string;bindingId:string;bindingVersion:number;thread:PortalSupportThreadDisplay;purchase?:PortalPurchaseControls}){
 const router=useRouter(),attempts=useRef(createPortalActionAttempt()),pending=useRef(false),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),context=JSON.stringify([holdingSlug,bindingId,thread.requestId,purchase?.identityRevision??'legacy-verified-binding']);
 const source=JSON.stringify([context,bindingVersion,thread,purchase]),live=useRef({context,source,purchase,response:thread.response}),alive=useRef(true);
 useLayoutEffect(()=>{live.current={context,source,purchase,response:thread.response};},[context,source,purchase,thread.response]);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
 async function execute(attempt:PortalActionAttempt){
  if(pending.current)return;
  const captured=live.current,generation=contextGeneration(),current=()=>alive.current&&!contextIsSwitching()&&contextGeneration()===generation&&live.current.context===attempt.context&&live.current.source===captured.source;
  if(attempt.context!==captured.context||!current()){setError('Return to the original verified customer account before retrying.');return;}
  if(captured.purchase?.history===false||captured.purchase?.settleExisting===false&&!attempt.uncertain){setError('This document is currently available for history.');return;}
  pending.current=true;setBusy(true);setError('');setNotice('');let unknown=true;
  try{
   const response=await holdingFetch(`/api/customer-portal/${encodeURIComponent(holdingSlug)}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body}),body=await response.json() as {data?:unknown;error?:{message?:string}};
   // ORIGINAL JSON may complete after a new response, binding or account was
   // rendered. Keep that exact attempt until it is reviewed in a live source.
   if(!current())throw Error('The customer account or response changed. Retry the original acknowledgement in the current document.');
   if(!response.ok){unknown=response.status>=500||attempt.uncertain;if(unknown){attempts.current.uncertain(attempt);setUncertain(true);}else{attempts.current.resolved(attempt);setUncertain(false);}throw Error(body.error?.message??'Unable to record the customer acknowledgement.');}
   const expected=JSON.parse(attempt.body) as {bindingId:string;requestId:string;responseId:string;responseVersion:number},value=body.data;
   if(!value||typeof value!=='object'||Array.isArray(value)||!('id'in value)||typeof value.id!=='string'||!value.id||!('bindingId'in value)||value.bindingId!==expected.bindingId||!('requestId'in value)||value.requestId!==expected.requestId||!('responseId'in value)||value.responseId!==expected.responseId||!('responseVersion'in value)||value.responseVersion!==expected.responseVersion||!('status'in value)||value.status!=='acknowledged'||!('createdAt'in value)||typeof value.createdAt!=='number'||!Number.isSafeInteger(value.createdAt)||value.createdAt<0)throw Error('The acknowledgement is incomplete. Retry the exact submitted response acknowledgement.');
   unknown=false;attempts.current.resolved(attempt);setUncertain(false);
   const sameResponse=captured.response?.id===expected.responseId&&captured.response.version===expected.responseVersion;
   setNotice(`Your acknowledgement of response revision ${expected.responseVersion} was recorded.`+(sameResponse?'':' The newer Company response remains displayed.'));router.refresh();
  }catch(cause){
   if(unknown)attempts.current.uncertain(attempt);
   if(alive.current){setUncertain(unknown);if(live.current.context===attempt.context)setError(cause instanceof Error?cause.message:'Retry the exact submitted response acknowledgement.');}
  }finally{pending.current=false;if(alive.current){setBusy(false);}}
 }
 function submit(){if(!thread.response)return;try{void execute(attempts.current.select(JSON.stringify({action:'acknowledge_support',requestId:thread.requestId,bindingId,expectedBindingVersion:bindingVersion,responseId:thread.response.id,responseVersion:thread.response.version,note:''}),context));}catch(cause){setError(cause instanceof Error?cause.message:'Retry the submitted acknowledgement first.');}}
 return <Card appearance="standard" as="section"><h3>Support request</h3><p>{thread.note||'Help requested with this document.'}</p>{thread.response?<><h4>Company response · revision {thread.response.version}</h4><p>{thread.response.message}</p><p>Published <TimeText appearance="standard" value={thread.response.publishedAt}/></p>{thread.closedAt!==null?<p>Closed without customer acknowledgement <TimeText appearance="standard" value={thread.closedAt}/></p>:thread.acknowledgedAt!==null?<p>Acknowledged <TimeText appearance="standard" value={thread.acknowledgedAt}/></p>:<Button appearance="standard" type="button" disabled={busy||purchase?.settleExisting===false} onClick={submit}>Acknowledge this response</Button>}</>:<p>The Company has not published a response yet.</p>}{error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice&&<InlineNotice tone="info" role="status">{notice}</InlineNotice>}{uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>{const attempt=attempts.current.retry();if(attempt)void execute(attempt);}}>Retry submitted acknowledgement</Button>}</Card>;
}
