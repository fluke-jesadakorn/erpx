"use client";
import {useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {benefitCommandContext,createBenefitCommands,type BenefitAttempt,type BenefitReceipt} from '@erpx/feature-benefits/lib/command';
import {benefitActionAllowed,benefitRetryAllowed,type BenefitPresentation} from '@erpx/feature-benefits/lib/presentation';
export function benefitSavedHref(base:string,input:Readonly<Record<string,unknown>>,receipt:BenefitReceipt,url:string){
 const current=new URL(url);if(typeof input.id==='string')return current.pathname+current.search+current.hash;
 if(input.entity==='election'){if(current.pathname===canonicalUiHref(`${base}/benefits`)){current.searchParams.set('detailKind','benefit_enrollment');current.searchParams.set('detailId',receipt.id);return current.pathname+current.search+current.hash;}return canonicalUiHref(`${base}/benefits/${receipt.id}`);}
 return current.pathname+current.search+current.hash;
}
export function useBenefitCommand(base:string,view:BenefitPresentation,renderedContext:string){
 const router=useRouter(),commands=useRef(createBenefitCommands()),pending=useRef(false),rendered=useRef(renderedContext);useLayoutEffect(()=>{rendered.current=renderedContext;},[renderedContext]);
 const [busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[retryAttempt,setRetryAttempt]=useState<BenefitAttempt|null>(null),[savedDraft,setSavedDraft]=useState<{receipt:BenefitReceipt;href:string}|null>(null);
 const context=()=>benefitCommandContext(rendered.current);
 async function execute(attempt:BenefitAttempt,retry=false){
  if(pending.current)return null;if(savedDraft||!(benefitActionAllowed(view,attempt.input)||retry&&retryAttempt===attempt&&benefitRetryAllowed(view,attempt))){setError('This Benefits action is unavailable. Your draft remains here.');return null;}
  pending.current=true;setBusy(true);setError('');setNotice('');
  try{const {result,unchanged}=await commands.current.execute(base,attempt,context);setRetryAttempt(null);announceWorkChange();const href=benefitSavedHref(base,attempt.input,result,window.location.href);
   if(!unchanged){setSavedDraft({receipt:result,href});setNotice('The submitted Benefits action was saved. Your newer draft remains here for review.');return result;}
   if(attempt.completion){attempt.completion.form.reset();window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...attempt.completion,benefitAcknowledged:true}}));}
   router.push(href,{scroll:false});router.refresh();return result;
  }catch(cause){setRetryAttempt(commands.current.pending()?attempt:null);setError(cause instanceof Error?cause.message:'Connection interrupted. Retry the submitted Benefits action.');return null;}
  finally{pending.current=false;setBusy(false);}
 }
 function send(input:Record<string,unknown>,form:HTMLFormElement){try{return execute(commands.current.prepare(input,form,context()));}catch(cause){setError(cause instanceof Error?cause.message:'Review the submitted Benefits action.');return Promise.resolve(null);}}
 return {send,retry:()=>retryAttempt?execute(retryAttempt,true):Promise.resolve(null),retryAttempt,busy,error,notice,savedDraft,clearSaved:()=>setSavedDraft(null)};
}
