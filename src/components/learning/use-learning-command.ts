"use client";
import {useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@/lib/shared/destination-router';
import {canonicalUiHref} from '@/lib/work_projection/destination-registry';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {createLearningCommands,type LearningAttempt,type LearningReceipt} from '@/lib/learning/command';
import {procurementFormUnchanged} from '@/lib/procurement/command';
export function useLearningCommand(base:string,allowed:(input:Record<string,unknown>)=>boolean,mayRetry:(attempt:LearningAttempt)=>boolean){
 const router=useRouter(),commands=useRef(createLearningCommands()),pending=useRef(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[retryAttempt,setRetryAttempt]=useState<LearningAttempt|null>(null),[uncertain,setUncertain]=useState(false),[savedDraft,setSavedDraft]=useState<(LearningReceipt&{entity:'course'|'assignment'})|null>(null);
 const live=useRef<{base:string;allowed:typeof allowed;mayRetry:typeof mayRetry}|null>({base,allowed,mayRetry});
 useLayoutEffect(()=>{live.current={base,allowed,mayRetry};return()=>{live.current=null;};},[base,allowed,mayRetry]);
 function savedHref(receipt:LearningReceipt,course:boolean,existing:boolean){const current=new URL(window.location.href);if(existing)return current.pathname+current.search+current.hash;if(current.pathname.includes('/knowledge')){current.searchParams.set('detailKind',course?'learning_course':'learning_assignment');current.searchParams.set('detailId',receipt.id);return current.pathname+current.search+current.hash;}return canonicalUiHref(`${base}/learning${course?'':`/${receipt.id}`}`);}
 async function execute(attempt:LearningAttempt,retry=false){
  if(pending.current)return null;if(savedDraft||!(allowed(attempt.input)||retry&&retryAttempt===attempt&&mayRetry(attempt))){setError('This Learning action is unavailable. Your draft remains here.');return null;}
  pending.current=true;setBusy(true);setError('');setNotice('');
  const assertCurrent=()=>{const current=live.current;if(!current||current.base!==base||!(current.allowed(attempt.input)||retry&&retryAttempt===attempt&&current.mayRetry(attempt)))throw Object.assign(Error('This Learning action changed. The original request and your draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  try{const {result,unchanged,acknowledge}=await commands.current.execute(base,attempt,undefined,'caller',assertCurrent);assertCurrent();if(!acknowledge())throw Error('Learning acknowledgement is incomplete. Retry the submitted action.');setRetryAttempt(null);const course=attempt.input.entity==='course';announceWorkChange();
   if(!unchanged||!procurementFormUnchanged(attempt.completion)){setSavedDraft({...result,entity:course?'course':'assignment'});setNotice('The submitted Learning action was saved. Your newer draft remains here for review.');return result;}
   if(attempt.completion){attempt.completion.form.reset();window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:attempt.completion}));}
   router.push(savedHref(result,course,typeof attempt.input.id==="string"),{scroll:false});router.refresh();return result;
  }catch(cause){setRetryAttempt(attempt);setError(cause instanceof Error?cause.message:'Connection interrupted. Retry the submitted Learning action.');return null;}
  finally{pending.current=false;setBusy(false);setUncertain(!!commands.current.pending());}
 }
 function send(input:Record<string,unknown>,form?:HTMLFormElement){try{return execute(commands.current.prepare(input,form));}catch(cause){setError(cause instanceof Error?cause.message:'Review the submitted action.');return Promise.resolve(null);}}
 return {send,retry:()=>retryAttempt?execute(retryAttempt,true):Promise.resolve(null),retryAttempt,busy,error,notice,uncertain,savedDraft,clearSaved:()=>setSavedDraft(null)};
}
