'use client';
import {useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {createRoutineCommands,routineCommandContext,type RoutineAttempt,type RoutineReceipt} from '@erpx/feature-operations/lib/routine-command';
import type {RoutinePurchase} from '@erpx/server-operations/public/rules/routine-purchase-controls';
export function useRoutineCommand(base:string,purchase:RoutinePurchase|undefined,renderedContext:string,originalRole:boolean|((attempt:RoutineAttempt)=>boolean)=true){
 const router=useRouter(),commands=useRef(createRoutineCommands()),current=useRef(renderedContext),inFlight=useRef(false),recovery=useRef<{attempt:RoutineAttempt;destination?:string;onSaved?:(result:RoutineReceipt)=>void}|null>(null);useLayoutEffect(()=>{current.current=renderedContext;},[renderedContext]);
 const [busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[retryAttempt,setRetryAttempt]=useState<RoutineAttempt|null>(null),[saved,setSaved]=useState<{receipt:RoutineReceipt;href:string}|null>(null),context=()=>routineCommandContext(current.current);
 async function execute(attempt:RoutineAttempt,retry=false,destination?:string,onSaved?:(result:RoutineReceipt)=>void){
  if(inFlight.current)return null;if(!(typeof originalRole==='function'?originalRole(attempt):originalRole)||saved||purchase&&!(retry?purchase.history:attempt.existing?purchase.settleExisting:purchase.newWork)){setError('This routine action is unavailable. Your draft remains here.');return null;}inFlight.current=true;setBusy(true);setError('');setNotice('');
  try{const {result,unchanged}=await commands.current.execute(base,attempt,context);setRetryAttempt(null);onSaved?.(result);announceWorkChange();const href=destination?canonicalUiHref(`${base}/${destination}/${result.id}`):window.location.pathname+window.location.search+window.location.hash;
   if(!unchanged){setSaved({receipt:result,href});setNotice('The submitted routine action was saved. Your newer draft remains here for review.');return result;}
   if(attempt.completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...attempt.completion,routineAcknowledged:true}}));if(destination)router.push(href,{scroll:false});router.refresh();return result;
  }catch(cause){setRetryAttempt(commands.current.pending()?attempt:null);setError(cause instanceof Error?cause.message:'Connection interrupted. Retry the submitted routine action.');return null;}finally{inFlight.current=false;setBusy(false);}
 }
 function send(path:string,input:Record<string,unknown>,form?:HTMLFormElement,existing=false,destination?:string,onSaved?:(result:RoutineReceipt)=>void){try{const attempt=commands.current.prepare(path,input,form,context(),existing);recovery.current={attempt,destination,onSaved};return execute(attempt,false,destination,onSaved);}catch(cause){setError(cause instanceof Error?cause.message:'Review the submitted routine action.');return Promise.resolve(null);}}
 return {send,busy,error,notice,retryAttempt,retry:()=>retryAttempt?execute(retryAttempt,true,recovery.current?.attempt===retryAttempt?recovery.current.destination:undefined,recovery.current?.attempt===retryAttempt?recovery.current.onSaved:undefined):Promise.resolve(null),saved,clearSaved:()=>setSaved(null),retryAllowed:(typeof originalRole==='function'?!!retryAttempt&&originalRole(retryAttempt):originalRole)&&(!purchase||purchase.history)};
}
