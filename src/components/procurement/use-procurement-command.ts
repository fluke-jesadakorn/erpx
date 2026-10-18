"use client";
import {useRef,useState} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {canonicalUiHref} from "@/lib/work_projection/destination-registry";
import {announceWorkChange} from "@/lib/work_projection/work-refresh";
import {createProcurementAttempts,procurementCommandContext,procurementFormUnchanged,sendProcurementCommand,validProcurementReceipt,type ProcurementAttempt} from "@/lib/procurement/command";
type Purchase={newWork:boolean;settleExisting:boolean;history:boolean;revision:string}|undefined;
export function useProcurementCommand(base:string,purchase:Purchase){
 const router=useRouter(),commands=useRef(createProcurementAttempts()),pending=useRef(false),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState(""),[uncertain,setUncertain]=useState(false),[savedHref,setSavedHref]=useState("");
 async function execute(requested:ProcurementAttempt,retry=false){
  if(pending.current)return false;let attempt:ProcurementAttempt;try{attempt=commands.current.select(requested,procurementCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted purchasing change.");return false;}
  const input=JSON.parse(attempt.body) as {action?:string;expectedVersion?:number},settlement=attempt.path.startsWith("supplier-promises/")||input.action==="cancel",allowed=settlement?purchase?.settleExisting!==false:purchase?.newWork!==false;
  if(!allowed&&!retry&&!commands.current.retry()){setError("This action is unavailable for the current package. Your draft is retained.");return false;}
  pending.current=true;setBusy(true);setError("");setNotice("");let mayHaveCommitted=true;
  try{
   const response=await sendProcurementCommand(base,attempt),result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(!response.ok){mayHaveCommitted=response.status>=500||result.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved();setUncertain(false);}throw Error(result.error?.message??"Unable to save the purchasing change.");}
   if(!validProcurementReceipt(attempt.path,result.data,attempt.key,input.expectedVersion))throw Error("Purchasing acknowledgement is incomplete. Retry the submitted change.");
   commands.current.resolved();setUncertain(false);const unchanged=procurementFormUnchanged(attempt.completion),href=canonicalUiHref(`${base}/${attempt.order?"commerce/orders":attempt.path.startsWith("supplier-promises/")?"commerce/orders":"procurement-rfqs"}/${result.data.id}`);setSavedHref(href);
   if(attempt.completion)window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:attempt.completion}));announceWorkChange();
   if(!unchanged){setNotice("Submitted purchasing change saved. Your newer draft is retained.");return false;}
   if(attempt.path==="procurement-rfqs"||attempt.order)router.push(href);router.refresh();return true;
  }catch(cause){if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted purchasing change.");return false;}
  finally{pending.current=false;setBusy(false);}
 }
 const send=(path:string,input:unknown,order=false,form?:HTMLFormElement)=>execute({path,body:JSON.stringify(input),order,context:procurementCommandContext(),key:crypto.randomUUID(),completion:form?{form,formEditVersion:form.getAttribute("data-erpx-edit-version")}:undefined});
 const retry=()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve(false);};
 return {send,retry,busy,error,notice,uncertain,savedHref,setError};
}
