'use client';
import {useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {createHandbookCommands,handbookCommandContext,type HandbookAttempt,type HandbookReceipt} from '@erpx/feature-collaboration/lib/handbook-command';
import type {HandbookPurchaseControls} from '@erpx/server-collaboration/public/handbook-purchases';
export function useHandbookCommand(base:string,purchase:HandbookPurchaseControls|undefined,renderedContext:string){
 const router=useRouter(),commands=useRef(createHandbookCommands()),current=useRef(renderedContext),inFlight=useRef(false),recovery=useRef<{attempt:HandbookAttempt;onSaved?:(result:HandbookReceipt)=>void}|null>(null);useLayoutEffect(()=>{current.current=renderedContext;},[renderedContext]);
 const [busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[retryAttempt,setRetryAttempt]=useState<HandbookAttempt|null>(null),context=()=>handbookCommandContext(current.current);
 async function execute(attempt:HandbookAttempt,retry=false,onSaved?:(result:HandbookReceipt)=>void){
  if(inFlight.current)return null;if(purchase&&!(retry?purchase.history:attempt.settlement?purchase.settleExisting:purchase.newWork)){setError('This handbook action is unavailable. Your draft remains here.');return null;}inFlight.current=true;setBusy(true);setError('');setNotice('');
  try{const {result,unchanged}=await commands.current.execute(base,attempt,context);setRetryAttempt(null);announceWorkChange();
   if(!unchanged){setNotice('The submitted handbook version was saved. Your newer draft remains here for review.');return null;}
   if(attempt.completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...attempt.completion,handbookAcknowledged:true}}));onSaved?.(result);router.refresh();return result;
  }catch(cause){setRetryAttempt(commands.current.pending()?attempt:null);setError(cause instanceof Error?cause.message:'Retry the submitted handbook action.');return null;}finally{inFlight.current=false;setBusy(false);}
 }
 function send(path:string,input:Record<string,unknown>,form?:HTMLFormElement,settlement=false,onSaved?:(result:HandbookReceipt)=>void){try{const attempt=commands.current.prepare(path,input,context(),form,settlement);recovery.current={attempt,onSaved};return execute(attempt,false,onSaved);}catch(cause){setError(cause instanceof Error?cause.message:'Review the submitted handbook action.');return Promise.resolve(null);}}
 return {send,busy,error,notice,retryAttempt,retry:()=>retryAttempt?execute(retryAttempt,true,recovery.current?.attempt===retryAttempt?recovery.current.onSaved:undefined):Promise.resolve(null),retryAllowed:!purchase||purchase.history,canonicalArticle:(id:string)=>canonicalUiHref(`${base}/handbook/${id}?edit=1`)};
}
