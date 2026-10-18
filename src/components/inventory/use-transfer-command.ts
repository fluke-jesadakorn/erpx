"use client";
import {useRef,useState} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {canonicalUiHref} from "@/lib/work_projection/destination-registry";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {transferCommandAllowed} from "@/lib/inventory/transfer-purchase-controls";
import {captureTransferForm,transferCommandContext,transferCommandFetch,transferFormUnchanged,validTransferAcknowledgement} from "@/lib/inventory/transfer-command-fetch";
import {createTransferCommandAttempt,type TransferCommandAttempt} from "@/lib/inventory/transfer-command-attempt";
import {useTransferPurchase} from "./transfer-purchase-context";
type Receipt={id:string;version:number};
export function useTransferCommand(base:string,space:"stock-transfers"|"stock-transit"="stock-transfers") {
  const router=useRouter(),purchase=useTransferPurchase(),pendingRef=useRef(false),commands=useRef(createTransferCommandAttempt());
  const [error,setError]=useState(""),[notice,setNotice]=useState(""),[pending,setPending]=useState(false),[uncertain,setUncertain]=useState(false);
  async function execute(attempt:TransferCommandAttempt,retry=false) {
    if(pendingRef.current)return false;
    const requested=attempt;
    try{attempt=commands.current.select(attempt,transferCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted transfer.");return false;}
    const url=`/api/v1${base}${attempt.path}`;
    if(!transferCommandAllowed(purchase,attempt.path,JSON.parse(attempt.body)) && !transferCommandFetch.hasAttempt(url,attempt.body)){setError("This transfer action is unavailable for the current package. Your draft is retained.");return false;}
    pendingRef.current=true;setPending(true);setError("");setNotice("");
    // A retry acknowledges the original submitted revision, never later edits.
    const completion=retry || attempt!==requested ? attempt.completion:captureTransferForm();attempt.completion=completion;
    let mayHaveCommitted=true;
    try {
      const assertCurrent=captureWorkCommandContext(),response=await transferCommandFetch(url,{method:"POST",headers:{"Content-Type":"application/json"},body:attempt.body},completion,"caller");
      const result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
      if(!response.ok){mayHaveCommitted=response.status>=500 || result.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved(attempt);setUncertain(false);}throw Error(result.error?.message ?? "Unable to save transfer.");}
      if(!validTransferAcknowledgement(result.data))throw Error("Transfer acknowledgement is incomplete. Retry the submitted change.");
      assertCurrent();
      if(!transferCommandFetch.acknowledge(response))throw Error("Transfer acknowledgement is incomplete. Retry the submitted change.");
      mayHaveCommitted=false;
      commands.current.resolved(attempt);setUncertain(false);attempt.acknowledged?.(result.data);
      if(!transferFormUnchanged(completion) || !completion && captureTransferForm()){setNotice("Submitted transfer change saved. Your newer draft is retained.");return false;}
      if(attempt.create)router.push(canonicalUiHref(`${base}${attempt.path.startsWith("/stock-transit/")?"/stock-transit":"/stock-transfers"}/${result.data.id}`));
      router.refresh();return true;
    } catch(cause) {
      if(mayHaveCommitted && !commands.current.retry()){commands.current.unresolved(attempt);setUncertain(true);}
      setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted transfer change.");return false;
    } finally {pendingRef.current=false;setPending(false);}
  }
  const send=(path:string,input:unknown,create=false,acknowledged?:(receipt:Receipt)=>void)=>execute({path:`/${space}${path}`,body:JSON.stringify(input),create,context:transferCommandContext(),acknowledged});
  const retry=()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve(false);};
  return {send,retry,error,notice,pending,uncertain,purchase,canPrepare:purchase?.newWork!==false || uncertain,canSettle:purchase?.settleExisting!==false || uncertain};
}
