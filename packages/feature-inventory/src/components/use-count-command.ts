"use client";
import {useRef,useState} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {captureWorkCommandContext} from "@erpx/feature-work_projection/lib/command-context";
import {countCommandAllowed} from "@erpx/feature-inventory/lib/count-purchase-controls";
import {captureCountForm,countCommandContext,countCommandFetch,countFormUnchanged,validCountAcknowledgement} from "@erpx/feature-inventory/lib/count-command-fetch";
import {createCountCommandAttempt,type CountCommandAttempt} from "@erpx/feature-inventory/lib/count-command-attempt";
import {useCountPurchase} from "@erpx/feature-inventory/components/count-purchase-context";
type Receipt={id:string;version:number};
export function useCountCommand(base:string) {
  const router=useRouter(),purchase=useCountPurchase(),pendingRef=useRef(false),commands=useRef(createCountCommandAttempt());
  const [error,setError]=useState(""),[notice,setNotice]=useState(""),[pending,setPending]=useState(false),[uncertain,setUncertain]=useState(false);
  async function execute(attempt:CountCommandAttempt,retry=false) {
    if(pendingRef.current)return false;
    const requested=attempt;
    try{attempt=commands.current.select(attempt,countCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted count.");return false;}
    const url=`/api/v1${base}/stock-counts${attempt.path}`;
    if(!countCommandAllowed(purchase,attempt.path) && !countCommandFetch.hasAttempt(url,attempt.body)){setError("This count action is unavailable for the current package. Your draft is retained.");return false;}
    pendingRef.current=true;setPending(true);setError("");setNotice("");
    // A retry acknowledges the original submitted revision, never later edits.
    const completion=retry || attempt!==requested ? attempt.completion:captureCountForm();attempt.completion=completion;
    let mayHaveCommitted=true;
    try {
      const assertCurrent=captureWorkCommandContext(),response=await countCommandFetch(url,{method:"POST",headers:{"Content-Type":"application/json"},body:attempt.body},completion,"caller");
      const result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
      if(!response.ok){mayHaveCommitted=response.status>=500 || result.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved(attempt);setUncertain(false);}throw Error(result.error?.message ?? "Unable to save count.");}
      if(!validCountAcknowledgement(result.data))throw Error("Count acknowledgement is incomplete. Retry the submitted change.");
      assertCurrent();
      if(!countCommandFetch.acknowledge(response))throw Error("Count acknowledgement is incomplete. Retry the submitted change.");
      mayHaveCommitted=false;
      commands.current.resolved(attempt);setUncertain(false);attempt.acknowledged?.(result.data);
      if(!countFormUnchanged(completion) || !completion && captureCountForm()){setNotice("Submitted count change saved. Your newer draft is retained.");return false;}
      if(attempt.create)router.push(canonicalUiHref(`${base}/stock-counts/${result.data.id}`));
      router.refresh();return true;
    } catch(cause) {
      if(mayHaveCommitted && !commands.current.retry()){commands.current.unresolved(attempt);setUncertain(true);}
      setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted count change.");return false;
    } finally {pendingRef.current=false;setPending(false);}
  }
  const send=(path:string,input:unknown,create=false,acknowledged?:(receipt:Receipt)=>void)=>execute({path,body:JSON.stringify(input),create,context:countCommandContext(),acknowledged});
  const retry=()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve(false);};
  return {send,retry,error,notice,pending,uncertain,purchase,canPrepare:purchase?.newWork!==false || uncertain,canSettle:purchase?.settleExisting!==false || uncertain};
}
