"use client";
import {useRef,useState} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {canonicalUiHref} from "@/lib/work_projection/destination-registry";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {landedCostCommandAllowed} from "@/lib/inventory/landed-cost-purchase-controls";
import {captureLandedCostForm,landedCostCommandContext,landedCostCommandFetch,landedCostFormUnchanged,validLandedCostAcknowledgement} from "@/lib/inventory/landed-cost-command-fetch";
import {createLandedCostCommandAttempt,type LandedCostCommandAttempt} from "@/lib/inventory/landed-cost-command-attempt";
import type {LandedCostPurchaseControls} from "@/lib/inventory/landed-cost-purchase-controls";
type Receipt={id:string;version:number};
export function useLandedCostCommand(base:string,purchase?:LandedCostPurchaseControls) {
  const router=useRouter(),pendingRef=useRef(false),commands=useRef(createLandedCostCommandAttempt());
  const [error,setError]=useState(""),[notice,setNotice]=useState(""),[pending,setPending]=useState(false),[uncertain,setUncertain]=useState(false);
  async function execute(attempt:LandedCostCommandAttempt,retry=false) {
    if(pendingRef.current)return false;
    const requested=attempt;
    try{attempt=commands.current.select(attempt,landedCostCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted landed cost.");return false;}
    const url=`/api/v1${base}/landed-costs${attempt.path}`;
    if(!landedCostCommandAllowed(purchase,attempt.path) && !landedCostCommandFetch.hasAttempt(url,attempt.body)){setError("This landed cost action is unavailable for the current package. Your draft is retained.");return false;}
    pendingRef.current=true;setPending(true);setError("");setNotice("");
    // A retry acknowledges the original submitted revision, never later edits.
    const completion=retry || attempt!==requested ? attempt.completion:captureLandedCostForm();attempt.completion=completion;
    let mayHaveCommitted=true;
    try {
      const assertCurrent=captureWorkCommandContext(),response=await landedCostCommandFetch(url,{method:"POST",headers:{"Content-Type":"application/json"},body:attempt.body},completion,"caller");
      const result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
      if(!response.ok){mayHaveCommitted=response.status>=500 || result.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved(attempt);setUncertain(false);}throw Error(result.error?.message ?? "Unable to save landed cost.");}
      if(!validLandedCostAcknowledgement(result.data))throw Error("LandedCost acknowledgement is incomplete. Retry the submitted change.");
      assertCurrent();
      if(!landedCostCommandFetch.acknowledge(response))throw Error("LandedCost acknowledgement is incomplete. Retry the submitted change.");
      mayHaveCommitted=false;
      commands.current.resolved(attempt);setUncertain(false);attempt.acknowledged?.(result.data);
      if(!landedCostFormUnchanged(completion) || !completion && captureLandedCostForm()){setNotice("Submitted landed cost change saved. Your newer draft is retained.");return false;}
      if(attempt.create)router.push(canonicalUiHref(`${base}/landed-costs/${result.data.id}`));
      router.refresh();return true;
    } catch(cause) {
      if(mayHaveCommitted && !commands.current.retry()){commands.current.unresolved(attempt);setUncertain(true);}
      setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted landed cost change.");return false;
    } finally {pendingRef.current=false;setPending(false);}
  }
  const send=(path:string,input:unknown,create=false,acknowledged?:(receipt:Receipt)=>void)=>execute({path,body:JSON.stringify(input),create,context:landedCostCommandContext(),acknowledged});
  const retry=()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve(false);};
  return {send,retry,error,notice,pending,uncertain,purchase,canPrepare:purchase?.newWork!==false || uncertain,canSettle:purchase?.settleExisting!==false || uncertain};
}
