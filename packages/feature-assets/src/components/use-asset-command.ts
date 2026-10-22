"use client";
import {useLayoutEffect,useRef,useState} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {announceWorkChange} from "@erpx/feature-work_projection/lib/work-refresh";
import {captureWorkCommandContext} from "@erpx/feature-work_projection/lib/command-context";
import {assetCommandAllowed,assetCommandContext,assetFormUnchanged,createAssetAttempts,sendAssetCommand,validAssetReceipt,type AssetCommandAttempt,type AssetCommandPath,type AssetPurchases} from "@erpx/feature-assets/lib/command";
type Source={base:string;path:AssetCommandPath;record?:{id:string;version:number}};
export function useAssetCommand(base:string,path:AssetCommandPath,purchases:AssetPurchases,record?:{id:string;version:number}){
 const renderedContext=assetCommandContext(),recordId=record?.id,recordVersion=record?.version,purchaseRef=useRef(purchases);useLayoutEffect(()=>{purchaseRef.current=purchases;},[purchases]);
 const sourceRef=useRef<Source|null>({base,path,record:recordId&&recordVersion!==undefined?{id:recordId,version:recordVersion}:undefined}),owners=useRef(new WeakMap<AssetCommandAttempt,Source>());
 useLayoutEffect(()=>{sourceRef.current={base,path,record:recordId&&recordVersion!==undefined?{id:recordId,version:recordVersion}:undefined};return()=>{sourceRef.current=null;};},[base,path,recordId,recordVersion]);
 const router=useRouter(),attempts=useRef(createAssetAttempts()),pending=useRef(false),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState<{text:string;saved:{id:string;href:string;source:string}}|null>(null),[uncertain,setUncertain]=useState(false);
 async function execute(requested:AssetCommandAttempt,retry=false){
  if(pending.current)return false;let attempt:AssetCommandAttempt;try{attempt=attempts.current.select(requested,assetCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted change.");return false;}
  const wasUnresolved=attempts.current.retry()===attempt;
  if(!assetCommandAllowed(attempt.path,JSON.parse(attempt.body),purchaseRef.current)&&!retry&&!wasUnresolved){setError("This action is unavailable for the current package. Your draft is retained.");return false;}
  const original=owners.current.get(attempt)??{base,path,record:recordId&&recordVersion!==undefined?{id:recordId,version:recordVersion}:undefined},live=sourceRef.current;
  if(!live||original.base!==live.base||original.path!==live.path||original.record?.id!==live.record?.id||!wasUnresolved&&original.record?.version!==live.record?.version){setError("Return to the original Asset or Maintenance record and role before confirming its request.");return false;}
  owners.current.set(attempt,original);const captured={...live,record:live.record?{...live.record}:undefined},assertWork=captureWorkCommandContext(),assertCurrent=()=>{assertWork();const now=sourceRef.current;if(!now||now.base!==captured.base||now.path!==captured.path||now.record?.id!==captured.record?.id||now.record?.version!==captured.record?.version)throw Object.assign(Error("The Asset or Maintenance source changed. Your original request and draft are retained."),{code:"WORK_CONTEXT_CHANGED"});};
  pending.current=true;setBusy(true);setError("");setNotice(null);let mayHaveCommitted=wasUnresolved;
  try{assertCurrent();mayHaveCommitted=true;const response=await sendAssetCommand(captured.base,attempt),result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};assertCurrent();
   if(!response.ok){mayHaveCommitted=wasUnresolved||response.status>=500||result.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){attempts.current.unresolved(attempt);setUncertain(true);}else{attempts.current.resolved();setUncertain(false);}throw Error(result.error?.message??"Unable to record this action.");}
   if(!validAssetReceipt(attempt,result.data))throw Error("The acknowledgement is incomplete. Retry the submitted change.");assertCurrent();attempts.current.resolved();setUncertain(false);const unchanged=assetFormUnchanged(attempt.completion);setNotice({text:unchanged?"":"Submitted change saved. Your newer draft is retained.",saved:{id:result.data.id,href:canonicalUiHref(`${captured.base}/${attempt.path}/${result.data.id}`),source:JSON.stringify([captured.base,attempt.path,captured.record?.id??null,attempt.context])}});if(attempt.completion)window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:attempt.completion}));announceWorkChange();
   if(!unchanged)return false;
   const current=new URL(window.location.href);if(current.pathname===canonicalUiHref(`${captured.base}/assets`)&&current.searchParams.has("view")){current.searchParams.set("detailKind",attempt.path==="assets"?"asset":attempt.path==="maintenance/plans"?"plan":"maintenance");current.searchParams.set("detailId",result.data.id);router.push(current.pathname+current.search,{scroll:false});}else router.push(canonicalUiHref(`${captured.base}/${attempt.path}/${result.data.id}`));router.refresh();return true;
  }catch(cause){if(mayHaveCommitted){attempts.current.unresolved(attempt);setUncertain(true);}setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted change.");return false;}finally{pending.current=false;setBusy(false);}
 }
 const send=(input:unknown,form?:HTMLFormElement)=>execute({path,body:JSON.stringify(input),context:renderedContext,key:crypto.randomUUID(),completion:form?{form,formEditVersion:form.getAttribute("data-erpx-edit-version")}:undefined});
 const retry=()=>{const attempt=attempts.current.retry();return attempt?execute(attempt,true):Promise.resolve(false);};const ownedNotice=notice?.saved.source===JSON.stringify([base,path,recordId??null,renderedContext])?notice:null;return {send,retry,busy,error,notice:ownedNotice?.text??"",saved:ownedNotice?.saved??null,uncertain};
}
