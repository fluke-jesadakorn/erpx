"use client";
import {useRef,useState,useSyncExternalStore} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {announceWorkChange} from "@erpx/feature-work_projection/lib/work-refresh";
import {createProcurementAttempts,procurementCommandContext,procurementFormUnchanged,sendProcurementCommand,type ProcurementAttempt} from "@erpx/feature-procurement/lib/command";
import {validSupplierBillReceipt,type SupplierBillPurchase} from "@erpx/feature-procurement/lib/supplier-bill-command";
const subscribe=()=>()=>{},hydrated=()=>true,server=()=>false;
export function useSupplierBillCommand(base:string,purchase:SupplierBillPurchase,ownerId?:string){
 const router=useRouter(),commands=useRef(createProcurementAttempts()),pending=useRef(false),ready=useSyncExternalStore(subscribe,hydrated,server),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState(""),[uncertain,setUncertain]=useState(false),[savedHref,setSavedHref]=useState("");
 async function execute(requested:ProcurementAttempt,retry=false){
  if(pending.current)return false;let attempt:ProcurementAttempt;try{attempt=commands.current.select(requested,procurementCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted supplier invoice change.");return false;}
  const input=JSON.parse(attempt.body) as {expectedVersion?:number;action?:string;receipts?:unknown[];matchingMode?:string},allowed=attempt.path==="supplier-bills/policy"?purchase?.newWork!==false:purchase?.settleExisting!==false;
  if(!allowed&&!retry&&!commands.current.retry()){setError("This action is currently unavailable. Your draft is retained.");return false;}
  pending.current=true;setBusy(true);setError("");setNotice("");let mayHaveCommitted=true;
  try{
   const response=await sendProcurementCommand(base,attempt),body=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(!response.ok){mayHaveCommitted=response.status>=500||body.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved();setUncertain(false);}throw Error(body.error?.message??"Unable to save the supplier invoice change.");}
   if(!validSupplierBillReceipt(attempt.path,body.data,attempt.key,input,ownerId))throw Error("Supplier invoice acknowledgement is incomplete. Retry the submitted change.");
   commands.current.resolved();setUncertain(false);const unchanged=procurementFormUnchanged(attempt.completion),href=canonicalUiHref(`${base}/${attempt.order?`finance/invoices/${body.data.invoiceId}`:`supplier-bills/${body.data.id}`}`);if(attempt.path!=="supplier-bills/policy")setSavedHref(href);
   if(attempt.completion)window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:attempt.completion}));announceWorkChange();
   if(!unchanged){setNotice("Submitted supplier invoice change saved. Your newer draft is retained.");return false;}
   if(attempt.path==="supplier-bills"||attempt.order)router.push(href);router.refresh();return true;
  }catch(cause){if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted supplier invoice change.");return false;}
  finally{pending.current=false;setBusy(false);}
 }
 const send=(path:string,input:unknown,invoice=false,form?:HTMLFormElement)=>execute({path,body:JSON.stringify(input),order:invoice,context:procurementCommandContext(),key:crypto.randomUUID(),completion:form?{form,formEditVersion:form.getAttribute("data-erpx-edit-version")}:undefined});
 return {send,retry:()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve(false);},busy,ready,error,notice,uncertain,savedHref};
}
